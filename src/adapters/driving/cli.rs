use crate::adapters::driving::completions;
use crate::core::domain::connection::TlsMode;
use crate::core::ports::connection_repository::ConnectionRepository;
use crate::core::services::connection::service::{AddConnectionInput, ConnectionService, EditConnectionInput};

pub struct Cli<R>
where
    R: ConnectionRepository,
{
    connection_service: ConnectionService<R>,
}

impl<R> Cli<R>
where
    R: ConnectionRepository,
{
    pub fn new(connection_service: ConnectionService<R>) -> Self {
        Self { connection_service }
    }

    pub fn run(&self, args: impl IntoIterator<Item = String>) -> Result<(), String> {
        let args: Vec<String> = args.into_iter().collect();

        match args.first().map(String::as_str) {
            None | Some("help") | Some("--help") | Some("-h") => {
                println!("{}", welcome());
                Ok(())
            }
            Some("add") => self.add_connection(&args[1..]),
            Some("list") => self.list_connections(&args[1..]),
            Some("delete") => self.delete_connection(&args[1..]),
            Some("edit") => self.edit_connection(&args[1..]),
            Some("rename") => self.rename_connection(&args[1..]),
            Some("connect") => self.connect_to(&args[1..]),
            Some("completions") => self.print_completions(&args[1..]),
            Some("--version") | Some("-V") => {
                println!("pgrs {}", env!("CARGO_PKG_VERSION"));
                Ok(())
            }
            // both intercepted in app.rs before cli.run() is called — unreachable in normal use
            Some("shell") => Err("usage: pgrs shell <connection-name>".to_string()),
            Some("test") => Err("usage: pgrs test <connection-name>".to_string()),
            Some(cmd) => Err(format!("unknown command '{cmd}'. Run 'pgrs' for help.")),
        }
    }

    fn add_connection(&self, args: &[String]) -> Result<(), String> {
        let name = args.first().ok_or(
            "usage: pgrs add <name> [--url=<postgresql://...>] [--host=<host>] [--username=<user>] [--password=<pass>] [--database=<db>] [--port=<port>] [--tls=disable|require|verify-full]"
        )?.trim().to_string();

        let (url_host, url_port, url_username, url_password, url_database) =
            optional_option(args, "--url")
                .map(|u| parse_connection_url(&u))
                .transpose()?
                .unwrap_or_default();

        let host = optional_option(args, "--host")
            .or(url_host)
            .ok_or("--host is required")?;
        let username = optional_option(args, "--username")
            .or(url_username)
            .ok_or("--username is required")?;
        let password = optional_option(args, "--password")
            .or(url_password)
            .ok_or("--password is required")?;
        let database = optional_option(args, "--database")
            .or(url_database)
            .ok_or("--database is required")?;
        let port = optional_option(args, "--port")
            .map(|value| {
                value
                    .parse::<u16>()
                    .map_err(|_| "port must be a number".to_string())
            })
            .transpose()?
            .or(url_port)
            .unwrap_or(5432);

        let tls = match optional_option(args, "--tls").as_deref() {
            None | Some("disable") => TlsMode::Disable,
            Some("require") => TlsMode::Require,
            Some("verify-full") => TlsMode::VerifyFull,
            Some(other) => {
                return Err(format!(
                    "unknown tls mode '{other}' — supported: disable, require, verify-full"
                ));
            }
        };

        self.connection_service.add_connection(AddConnectionInput {
            name: name.clone(),
            host,
            port,
            username,
            password,
            database,
            tls,
        })?;

        println!("connection '{name}' added");
        Ok(())
    }

    fn list_connections(&self, args: &[String]) -> Result<(), String> {
        let names_only = args.iter().any(|a| a == "--names-only");
        let connections = self.connection_service.list_connections()?;

        if names_only {
            for c in &connections {
                println!("{}", c.name);
            }
            return Ok(());
        }

        if connections.is_empty() {
            println!("no connections saved");
            return Ok(());
        }

        let name_w = connections.iter().map(|c| c.name.len()).max().unwrap_or(4).max(4);
        let host_w = connections.iter().map(|c| c.host.len()).max().unwrap_or(4).max(4);
        let db_w = connections.iter().map(|c| c.database.len()).max().unwrap_or(8).max(8);
        let user_w = connections.iter().map(|c| c.username.len()).max().unwrap_or(8).max(8);
        let tls_w = connections
            .iter()
            .map(|c| c.tls.to_string().len())
            .max()
            .unwrap_or(3)
            .max(3);

        println!(
            "{:<name_w$}  {:<host_w$}  {:<6}  {:<db_w$}  {:<user_w$}  {:<tls_w$}  PASSWORD",
            "NAME", "HOST", "PORT", "DATABASE", "USERNAME", "TLS",
        );

        for c in &connections {
            println!(
                "{:<name_w$}  {:<host_w$}  {:<6}  {:<db_w$}  {:<user_w$}  {:<tls_w$}  ****",
                c.name, c.host, c.port, c.database, c.username, c.tls,
            );
        }

        Ok(())
    }

    fn connect_to(&self, args: &[String]) -> Result<(), String> {
        use std::os::unix::process::CommandExt;

        let name = args
            .first()
            .ok_or("usage: pgrs connect <connection-name>")?
            .trim()
            .to_string();

        let connection = self.connection_service.get_connection(&name)?;

        println!(
            "handing off to psql for '{}' — you'll return to your shell, not pgrs, on exit",
            name
        );

        let error = std::process::Command::new("psql")
            .env("PGPASSWORD", &connection.password)
            .arg("-h")
            .arg(&connection.host)
            .arg("-p")
            .arg(connection.port.to_string())
            .arg("-U")
            .arg(&connection.username)
            .arg("-d")
            .arg(&connection.database)
            .exec();

        Err(if error.kind() == std::io::ErrorKind::NotFound {
            "psql not found — is it installed?".to_string()
        } else {
            error.to_string()
        })
    }

    fn edit_connection(&self, args: &[String]) -> Result<(), String> {
        let name = args
            .first()
            .ok_or("usage: pgrs edit <name> [--host=...] [--port=...] [--username=...] [--password=...] [--database=...] [--tls=...]")?
            .trim()
            .to_string();

        let port = optional_option(args, "--port")
            .map(|v| v.parse::<u16>().map_err(|_| "port must be a number".to_string()))
            .transpose()?;

        let tls = match optional_option(args, "--tls").as_deref() {
            None => None,
            Some("disable") => Some(TlsMode::Disable),
            Some("require") => Some(TlsMode::Require),
            Some("verify-full") => Some(TlsMode::VerifyFull),
            Some(other) => {
                return Err(format!(
                    "unknown tls mode '{other}' — supported: disable, require, verify-full"
                ));
            }
        };

        self.connection_service.edit_connection(&name, EditConnectionInput {
            host: optional_option(args, "--host"),
            port,
            username: optional_option(args, "--username"),
            password: optional_option(args, "--password"),
            database: optional_option(args, "--database"),
            tls,
        })?;

        println!("connection '{name}' updated");
        Ok(())
    }

    fn rename_connection(&self, args: &[String]) -> Result<(), String> {
        let old_name = args
            .first()
            .ok_or("usage: pgrs rename <old-name> <new-name>")?
            .trim()
            .to_string();
        let new_name = args
            .get(1)
            .ok_or("usage: pgrs rename <old-name> <new-name>")?
            .trim()
            .to_string();
        self.connection_service.rename_connection(&old_name, &new_name)?;
        println!("connection '{old_name}' renamed to '{new_name}'");
        Ok(())
    }

    fn delete_connection(&self, args: &[String]) -> Result<(), String> {
        let name = args
            .first()
            .ok_or("usage: pgrs delete <connection-name> [--yes]")?
            .trim()
            .to_string();

        let skip_confirmation = args.iter().any(|a| a == "--yes");

        if !skip_confirmation {
            use std::io::{self, IsTerminal, Write};
            if io::stdin().is_terminal() {
                print!("Delete connection '{name}'? [y/N] ");
                io::stdout().flush().map_err(|e| e.to_string())?;
                let mut input = String::new();
                io::stdin().read_line(&mut input).map_err(|e| e.to_string())?;
                if !matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
                    println!("Aborted.");
                    return Ok(());
                }
            } else {
                return Err(format!(
                    "cowardly refusing to delete '{name}' — pass --yes to confirm"
                ));
            }
        }

        self.connection_service.delete_connection(&name)?;
        println!("connection '{name}' deleted");
        Ok(())
    }

    fn print_completions(&self, args: &[String]) -> Result<(), String> {
        let shell = args
            .first()
            .ok_or("usage: pgrs completions <bash|zsh|fish>")?;
        let script = match shell.as_str() {
            "bash" => completions::bash_script(),
            "zsh" => completions::zsh_script(),
            "fish" => completions::fish_script(),
            other => {
                return Err(format!(
                    "unknown shell '{}' — supported: bash, zsh, fish",
                    other
                ));
            }
        };
        print!("{}", script);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn get_connection(
        &self,
        name: &str,
    ) -> Result<crate::core::domain::connection::Connection, String> {
        self.connection_service.get_connection(name)
    }
}

// Returns (host, port, username, password, database) parsed from a postgresql:// URL.
// Individual CLI flags take precedence over URL-parsed values.
// Note: percent-encoded characters (e.g. %40 in passwords) are not decoded —
// use individual flags for credentials containing special characters.
fn parse_connection_url(
    url: &str,
) -> Result<(Option<String>, Option<u16>, Option<String>, Option<String>, Option<String>), String> {
    let rest = url
        .strip_prefix("postgresql://")
        .or_else(|| url.strip_prefix("postgres://"))
        .ok_or_else(|| {
            format!("URL must start with postgresql:// or postgres://, got '{}'", url)
        })?;

    let (userinfo_str, hostinfo) = if let Some(at) = rest.rfind('@') {
        (Some(&rest[..at]), &rest[at + 1..])
    } else {
        (None, rest)
    };

    let (username, password) = match userinfo_str {
        Some(ui) => {
            if let Some(colon) = ui.find(':') {
                (Some(ui[..colon].to_string()), Some(ui[colon + 1..].to_string()))
            } else {
                (Some(ui.to_string()), None)
            }
        }
        None => (None, None),
    };

    let (hostport, database) = if let Some(slash) = hostinfo.find('/') {
        let db = &hostinfo[slash + 1..];
        (
            &hostinfo[..slash],
            if db.is_empty() { None } else { Some(db.to_string()) },
        )
    } else {
        (hostinfo, None)
    };

    let (host, port) = if let Some(colon) = hostport.rfind(':') {
        let h = &hostport[..colon];
        let p_str = &hostport[colon + 1..];
        let p = p_str
            .parse::<u16>()
            .map_err(|_| format!("invalid port '{}' in URL", p_str))?;
        (if h.is_empty() { None } else { Some(h.to_string()) }, Some(p))
    } else {
        (
            if hostport.is_empty() { None } else { Some(hostport.to_string()) },
            None,
        )
    };

    Ok((host, port, username, password, database))
}

fn optional_option(args: &[String], key: &str) -> Option<String> {
    let prefix = format!("{key}=");

    args.iter()
        .find_map(|arg| arg.strip_prefix(&prefix).map(ToString::to_string))
}

struct CmdDoc {
    usage_lines: &'static [&'static str],
    desc_lines: &'static [&'static str],
}

// To add a new command: append one CmdDoc block here — no string wrangling required.
const COMMAND_DOCS: &[CmdDoc] = &[
    CmdDoc {
        usage_lines: &[
            "add <name> [--url=<postgresql://user:pass@host:port/db>]",
            "           [--host=<host>] [--username=<user>] [--password=<pass>]",
            "           [--database=<db>] [--port=<port>] [--tls=disable|require|verify-full]",
        ],
        desc_lines: &[
            "Add a new named connection. Use --url to specify all fields at once;",
            "individual flags override URL-parsed values.",
            "--tls: disable (no encryption), require (encrypt, no cert check),",
            "       verify-full (encrypt + verify server certificate)",
        ],
    },
    CmdDoc {
        usage_lines: &["list [--names-only]"],
        desc_lines: &[
            "List all saved connections.",
            "--names-only: print only names, one per line (handy for scripts and shell completion)",
        ],
    },
    CmdDoc {
        usage_lines: &[
            "edit <name> [--host=...] [--port=...] [--username=...] [--password=...]",
            "            [--database=...] [--tls=...]",
        ],
        desc_lines: &["Update one or more fields of a saved connection"],
    },
    CmdDoc {
        usage_lines: &["delete <name> [--yes]"],
        desc_lines: &["Delete a named connection (prompts for confirmation without --yes)"],
    },
    CmdDoc {
        usage_lines: &["rename <old-name> <new-name>"],
        desc_lines: &["Rename a saved connection"],
    },
    CmdDoc {
        usage_lines: &["test <name>"],
        desc_lines: &["Verify a saved connection is reachable"],
    },
    CmdDoc {
        usage_lines: &["connect <name>"],
        desc_lines: &["Open an interactive psql session using a saved connection"],
    },
    CmdDoc {
        usage_lines: &["shell <name>"],
        desc_lines: &["Open pgrs interactive SQL REPL with auto-completion"],
    },
    CmdDoc {
        usage_lines: &["completions <bash|zsh|fish>"],
        desc_lines: &["Print shell completion script"],
    },
    CmdDoc {
        usage_lines: &["help, --help, -h"],
        desc_lines: &["Show this help"],
    },
];

fn welcome() -> String {
    let mut out = String::from(
        "pgrs — PostgreSQL connection manager built with Rust\n\n\
         Manage and store named PostgreSQL connections locally.\n\n\
         Commands:\n",
    );
    for doc in COMMAND_DOCS {
        for line in doc.usage_lines {
            out.push_str(&format!("  {line}\n"));
        }
        for line in doc.desc_lines {
            out.push_str(&format!("             {line}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::connection::Connection;
    use crate::core::ports::connection_repository::ConnectionRepository;
    use std::cell::RefCell;

    struct StubRepository {
        connections: RefCell<Vec<Connection>>,
    }

    impl StubRepository {
        fn with(names: &[&str]) -> Self {
            let connections = names
                .iter()
                .map(|n| Connection {
                    name: n.to_string(),
                    host: "localhost".to_string(),
                    port: 5432,
                    username: "user".to_string(),
                    password: "pass".to_string(),
                    database: "db".to_string(),
                    tls: crate::core::domain::connection::TlsMode::Disable,
                })
                .collect();
            Self {
                connections: RefCell::new(connections),
            }
        }
    }

    impl ConnectionRepository for StubRepository {
        fn add(&self, c: Connection) -> Result<(), String> {
            self.connections.borrow_mut().push(c);
            Ok(())
        }
        fn list(&self) -> Result<Vec<Connection>, String> {
            Ok(self.connections.borrow().clone())
        }
        fn delete(&self, name: &str) -> Result<(), String> {
            let mut connections = self.connections.borrow_mut();
            let before = connections.len();
            connections.retain(|c| c.name != name);
            if connections.len() == before {
                return Err(format!("connection '{}' not found", name));
            }
            Ok(())
        }
        fn get_connection(&self, name: &str) -> Result<Connection, String> {
            self.connections
                .borrow()
                .iter()
                .find(|c| c.name == name)
                .cloned()
                .ok_or_else(|| format!("connection '{}' not found", name))
        }
        fn rename(&self, old_name: &str, new_name: &str) -> Result<(), String> {
            let mut connections = self.connections.borrow_mut();
            if connections.iter().any(|c| c.name == new_name) {
                return Err(format!("connection '{}' already exists", new_name));
            }
            let conn = connections
                .iter_mut()
                .find(|c| c.name == old_name)
                .ok_or_else(|| format!("connection '{}' not found", old_name))?;
            conn.name = new_name.to_string();
            Ok(())
        }
        fn update(&self, connection: Connection) -> Result<(), String> {
            let mut connections = self.connections.borrow_mut();
            let pos = connections
                .iter()
                .position(|c| c.name == connection.name)
                .ok_or_else(|| format!("connection '{}' not found", connection.name))?;
            connections[pos] = connection;
            Ok(())
        }
    }

    fn cli_with(names: &[&str]) -> Cli<StubRepository> {
        Cli::new(ConnectionService::new(StubRepository::with(names)))
    }

    #[test]
    fn list_names_only_returns_ok() {
        let cli = cli_with(&["prod", "staging"]);
        let result = cli.run(vec!["list".to_string(), "--names-only".to_string()].into_iter());
        assert!(result.is_ok());
    }

    #[test]
    fn completions_bash_returns_ok() {
        let cli = cli_with(&[]);
        let result = cli.run(vec!["completions".to_string(), "bash".to_string()].into_iter());
        assert!(result.is_ok());
    }

    #[test]
    fn completions_unknown_shell_returns_err() {
        let cli = cli_with(&[]);
        let result = cli.run(vec!["completions".to_string(), "powershell".to_string()].into_iter());
        assert!(result.is_err());
    }

    #[test]
    fn get_connection_returns_correct_connection() {
        let cli = cli_with(&["prod"]);
        let conn = cli.get_connection("prod").unwrap();
        assert_eq!(conn.name, "prod");
    }

    #[test]
    fn add_without_name_shows_add_usage() {
        let cli = cli_with(&[]);
        let err = cli.run(["add".to_string()].into_iter()).unwrap_err();
        assert!(
            err.contains("--host"),
            "error should show add usage, got: {err}"
        );
        assert!(
            err.contains("add"),
            "error should mention add command, got: {err}"
        );
    }

    fn add_args(name: &str, extra: &[&str]) -> impl Iterator<Item = String> {
        let mut args = vec![
            "add".to_string(),
            name.to_string(),
            "--host=localhost".to_string(),
            "--username=user".to_string(),
            "--password=pass".to_string(),
            "--database=db".to_string(),
        ];
        args.extend(extra.iter().map(|s| s.to_string()));
        args.into_iter()
    }

    #[test]
    fn add_without_tls_flag_defaults_to_disable() {
        use crate::core::domain::connection::TlsMode;
        let cli = cli_with(&[]);
        cli.run(add_args("prod", &[])).unwrap();
        let conn = cli.get_connection("prod").unwrap();
        assert_eq!(conn.tls, TlsMode::Disable);
    }

    #[test]
    fn add_with_tls_require_saves_require_mode() {
        use crate::core::domain::connection::TlsMode;
        let cli = cli_with(&[]);
        cli.run(add_args("prod", &["--tls=require"])).unwrap();
        let conn = cli.get_connection("prod").unwrap();
        assert_eq!(conn.tls, TlsMode::Require);
    }

    #[test]
    fn add_with_tls_verify_full_saves_verify_full_mode() {
        use crate::core::domain::connection::TlsMode;
        let cli = cli_with(&[]);
        cli.run(add_args("prod", &["--tls=verify-full"])).unwrap();
        let conn = cli.get_connection("prod").unwrap();
        assert_eq!(conn.tls, TlsMode::VerifyFull);
    }

    #[test]
    fn no_args_returns_ok() {
        let cli = cli_with(&[]);
        assert!(cli.run(std::iter::empty()).is_ok());
    }

    #[test]
    fn help_command_returns_ok() {
        let cli = cli_with(&[]);
        assert!(cli.run(["help".to_string()].into_iter()).is_ok());
        assert!(cli.run(["--help".to_string()].into_iter()).is_ok());
        assert!(cli.run(["-h".to_string()].into_iter()).is_ok());
    }

    #[test]
    fn unknown_command_returns_error() {
        let cli = cli_with(&[]);
        assert!(cli.run(["unknown".to_string()].into_iter()).is_err());
    }

    #[test]
    fn unknown_command_error_mentions_command_name() {
        let cli = cli_with(&[]);
        let err = cli.run(["foobar".to_string()].into_iter()).unwrap_err();
        assert!(
            err.contains("foobar"),
            "error should mention the unknown command, got: {err}"
        );
    }

    #[test]
    fn unknown_command_error_does_not_show_add_usage() {
        let cli = cli_with(&[]);
        let err = cli.run(["foobar".to_string()].into_iter()).unwrap_err();
        assert!(
            !err.contains("--host"),
            "error should not show add usage, got: {err}"
        );
    }

    #[test]
    fn add_with_invalid_port_returns_error() {
        let cli = cli_with(&[]);
        let result = cli.run(add_args("prod", &["--port=abc"]));
        assert_eq!(result, Err("port must be a number".to_string()));
    }

    #[test]
    fn add_with_unknown_tls_returns_error() {
        let cli = cli_with(&[]);
        let result = cli.run(add_args("prod", &["--tls=starttls"]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown tls mode"));
    }

    #[test]
    fn list_empty_returns_ok() {
        let cli = cli_with(&[]);
        assert!(cli.run(["list".to_string()].into_iter()).is_ok());
    }

    #[test]
    fn list_with_connections_returns_ok() {
        let cli = cli_with(&["prod", "staging"]);
        assert!(cli.run(["list".to_string()].into_iter()).is_ok());
    }

    #[test]
    fn delete_succeeds_returns_ok() {
        let cli = cli_with(&["prod"]);
        assert!(
            cli.run(["delete".to_string(), "prod".to_string(), "--yes".to_string()].into_iter())
                .is_ok()
        );
    }

    #[test]
    fn delete_without_yes_in_non_tty_returns_error() {
        // test runner is not a TTY — --yes is required
        let cli = cli_with(&["prod"]);
        let err = cli
            .run(["delete".to_string(), "prod".to_string()].into_iter())
            .unwrap_err();
        assert!(err.contains("--yes"), "expected --yes hint, got: {err}");
    }

    #[test]
    fn delete_missing_name_arg_returns_error() {
        let cli = cli_with(&[]);
        let result = cli.run(["delete".to_string()].into_iter());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("delete"));
    }

    #[test]
    fn connect_missing_name_arg_returns_error() {
        let cli = cli_with(&[]);
        let result = cli.run(["connect".to_string()].into_iter());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("connect"));
    }

    #[test]
    fn connect_unknown_connection_returns_error() {
        let cli = cli_with(&[]);
        let result = cli.run(["connect".to_string(), "nonexistent".to_string()].into_iter());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn version_flag_returns_ok() {
        let cli = cli_with(&[]);
        assert!(cli.run(["--version".to_string()].into_iter()).is_ok());
    }

    #[test]
    fn version_short_flag_returns_ok() {
        let cli = cli_with(&[]);
        assert!(cli.run(["-V".to_string()].into_iter()).is_ok());
    }

    #[test]
    fn connect_to_nonexistent_connection_returns_not_found() {
        let cli = cli_with(&[]);
        let err = cli
            .run(["connect".to_string(), "ghost".to_string()].into_iter())
            .unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[test]
    fn welcome_lists_all_commands() {
        use super::welcome;
        let w = welcome();
        for cmd in &["add", "list", "edit", "delete", "rename", "test", "connect", "shell", "completions"] {
            assert!(w.contains(cmd), "welcome should mention '{cmd}', got:\n{w}");
        }
    }

    fn edit_args(name: &str, extra: &[&str]) -> impl Iterator<Item = String> {
        let mut args = vec!["edit".to_string(), name.to_string()];
        args.extend(extra.iter().map(|s| s.to_string()));
        args.into_iter()
    }

    #[test]
    fn edit_single_field_updates_database() {
        let cli = cli_with(&["prod"]);
        cli.run(edit_args("prod", &["--database=newdb"])).unwrap();
        assert_eq!(cli.get_connection("prod").unwrap().database, "newdb");
    }

    #[test]
    fn edit_multiple_fields_updates_all() {
        let cli = cli_with(&["prod"]);
        cli.run(edit_args("prod", &["--password=secret2", "--database=newdb"])).unwrap();
        let conn = cli.get_connection("prod").unwrap();
        assert_eq!(conn.password, "secret2");
        assert_eq!(conn.database, "newdb");
        assert_eq!(conn.host, "localhost"); // unchanged
    }

    #[test]
    fn edit_port_parses_correctly() {
        let cli = cli_with(&["prod"]);
        cli.run(edit_args("prod", &["--port=5433"])).unwrap();
        assert_eq!(cli.get_connection("prod").unwrap().port, 5433);
    }

    #[test]
    fn edit_tls_updates_mode() {
        let cli = cli_with(&["prod"]);
        cli.run(edit_args("prod", &["--tls=require"])).unwrap();
        assert_eq!(
            cli.get_connection("prod").unwrap().tls,
            crate::core::domain::connection::TlsMode::Require
        );
    }

    #[test]
    fn edit_without_fields_returns_error() {
        let cli = cli_with(&["prod"]);
        let err = cli.run(edit_args("prod", &[])).unwrap_err();
        assert!(err.contains("at least one field"), "got: {err}");
    }

    #[test]
    fn edit_missing_name_returns_error() {
        let cli = cli_with(&[]);
        let err = cli.run(["edit".to_string()].into_iter()).unwrap_err();
        assert!(err.contains("edit"), "got: {err}");
    }

    #[test]
    fn edit_not_found_returns_error() {
        let cli = cli_with(&[]);
        let err = cli
            .run(edit_args("ghost", &["--password=x"]))
            .unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[test]
    fn edit_with_invalid_port_returns_error() {
        let cli = cli_with(&["prod"]);
        let result = cli.run(edit_args("prod", &["--port=abc"]));
        assert_eq!(result, Err("port must be a number".to_string()));
    }

    #[test]
    fn edit_with_unknown_tls_returns_error() {
        let cli = cli_with(&["prod"]);
        let result = cli.run(edit_args("prod", &["--tls=starttls"]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown tls mode"));
    }

    #[test]
    fn edit_with_empty_host_returns_error() {
        let cli = cli_with(&["prod"]);
        let result = cli.run(edit_args("prod", &["--host="]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("host is required"));
    }

    #[test]
    fn rename_succeeds() {
        let cli = cli_with(&["prod"]);
        cli.run(["rename".to_string(), "prod".to_string(), "production".to_string()].into_iter())
            .unwrap();
        assert!(cli.get_connection("production").is_ok());
        assert!(cli.get_connection("prod").is_err());
    }

    #[test]
    fn rename_missing_new_name_returns_error() {
        let cli = cli_with(&["prod"]);
        let err = cli
            .run(["rename".to_string(), "prod".to_string()].into_iter())
            .unwrap_err();
        assert!(err.contains("rename"), "got: {err}");
    }

    #[test]
    fn rename_missing_args_returns_error() {
        let cli = cli_with(&[]);
        assert!(cli.run(["rename".to_string()].into_iter()).is_err());
    }

    #[test]
    fn rename_not_found_returns_error() {
        let cli = cli_with(&[]);
        let err = cli
            .run(["rename".to_string(), "ghost".to_string(), "new".to_string()].into_iter())
            .unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[test]
    fn shell_command_via_cli_run_returns_error_with_usage() {
        let cli = cli_with(&[]);
        let err = cli
            .run(["shell".to_string(), "prod".to_string()].into_iter())
            .unwrap_err();
        assert!(err.contains("shell"), "got: {err}");
    }

    #[test]
    fn add_with_url_sets_all_fields() {
        let cli = cli_with(&[]);
        cli.run(
            [
                "add".to_string(),
                "prod".to_string(),
                "--url=postgresql://user:pass@localhost:5432/mydb".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();
        let conn = cli.get_connection("prod").unwrap();
        assert_eq!(conn.host, "localhost");
        assert_eq!(conn.port, 5432);
        assert_eq!(conn.username, "user");
        assert_eq!(conn.password, "pass");
        assert_eq!(conn.database, "mydb");
    }

    #[test]
    fn add_with_url_and_flag_overrides_port() {
        let cli = cli_with(&[]);
        cli.run(
            [
                "add".to_string(),
                "prod".to_string(),
                "--url=postgresql://user:pass@localhost/mydb".to_string(),
                "--port=5433".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();
        assert_eq!(cli.get_connection("prod").unwrap().port, 5433);
    }

    #[test]
    fn add_with_url_postgres_scheme_accepted() {
        let cli = cli_with(&[]);
        assert!(cli
            .run(
                [
                    "add".to_string(),
                    "prod".to_string(),
                    "--url=postgres://user:pass@localhost/mydb".to_string(),
                ]
                .into_iter()
            )
            .is_ok());
    }

    #[test]
    fn add_with_invalid_url_scheme_returns_error() {
        let cli = cli_with(&[]);
        let err = cli
            .run(
                [
                    "add".to_string(),
                    "prod".to_string(),
                    "--url=mysql://user:pass@host/db".to_string(),
                ]
                .into_iter()
            )
            .unwrap_err();
        assert!(err.contains("postgresql://"), "got: {err}");
    }

    #[test]
    fn add_with_url_missing_password_requires_flag() {
        let cli = cli_with(&[]);
        // URL has no password, no --password flag → error
        let result = cli.run(
            [
                "add".to_string(),
                "prod".to_string(),
                "--url=postgresql://user@localhost/mydb".to_string(),
            ]
            .into_iter(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn completions_zsh_returns_ok() {
        let cli = cli_with(&[]);
        assert!(
            cli.run(["completions".to_string(), "zsh".to_string()].into_iter())
                .is_ok()
        );
    }

    #[test]
    fn parse_url_full_postgresql_scheme() {
        let (host, port, user, pass, db) =
            parse_connection_url("postgresql://user:pass@localhost:5432/mydb").unwrap();
        assert_eq!(host, Some("localhost".to_string()));
        assert_eq!(port, Some(5432));
        assert_eq!(user, Some("user".to_string()));
        assert_eq!(pass, Some("pass".to_string()));
        assert_eq!(db, Some("mydb".to_string()));
    }

    #[test]
    fn parse_url_postgres_scheme() {
        let (host, _, user, _, db) =
            parse_connection_url("postgres://user:pass@localhost/db").unwrap();
        assert_eq!(host, Some("localhost".to_string()));
        assert_eq!(user, Some("user".to_string()));
        assert_eq!(db, Some("db".to_string()));
    }

    #[test]
    fn parse_url_without_port_returns_none() {
        let (_, port, _, _, _) =
            parse_connection_url("postgresql://user:pass@localhost/db").unwrap();
        assert!(port.is_none());
    }

    #[test]
    fn parse_url_invalid_scheme_returns_error() {
        let result = parse_connection_url("mysql://user:pass@host/db");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("postgresql://"));
    }

    #[test]
    fn parse_url_invalid_port_returns_error() {
        let result = parse_connection_url("postgresql://user:pass@host:abc/db");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("port"));
    }

    #[test]
    fn completions_fish_returns_ok() {
        let cli = cli_with(&[]);
        assert!(
            cli.run(["completions".to_string(), "fish".to_string()].into_iter())
                .is_ok()
        );
    }
}
