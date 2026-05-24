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
            Some("list") | Some("ls") => self.list_connections(&args[1..]),
            Some("delete") | Some("del") | Some("rm") => self.delete_connection(&args[1..]),
            Some("edit") => self.edit_connection(&args[1..]),
            Some("rename") => self.rename_connection(&args[1..]),
            Some("connect") => self.connect_to(&args[1..]),
            Some("completions") => self.print_completions(&args[1..]),
            Some("--version") | Some("-V") => {
                println!("pgrs {}", env!("CARGO_PKG_VERSION"));
                Ok(())
            }
            Some(cmd @ "shell") | Some(cmd @ "test") => Err(format!(
                "'{cmd}' requires a connection — run 'pgrs {cmd} <connection-name>'"
            )),
            Some(cmd) => Err(format!("unknown command '{cmd}'. Run 'pgrs' for help.")),
        }
    }

    fn add_connection(&self, args: &[String]) -> Result<(), String> {
        let name = args.first().ok_or(
            "usage: pgrs add <name> [--url=<postgresql://...>] [--host=<host>] [--username=<user>] [--password=<pass>] [--database=<db>] [--port=<port>] [--tls=disable|require|verify-full]"
        )?.trim().to_string();

        let url = optional_option(args, "--url")
            .map(|u| parse_connection_url(&u))
            .transpose()?
            .unwrap_or_default();

        let host = optional_option(args, "--host")
            .or(url.host)
            .ok_or("host is required")?;
        let username = optional_option(args, "--username")
            .or(url.username)
            .ok_or("username is required")?;
        let password = optional_option(args, "--password")
            .or(url.password)
            .ok_or("password is required")?;
        let database = optional_option(args, "--database")
            .or(url.database)
            .ok_or("database is required")?;
        let port = optional_option(args, "--port")
            .map(|value| {
                value
                    .parse::<u16>()
                    .map_err(|_| "port must be a number".to_string())
            })
            .transpose()?
            .or(url.port)
            .unwrap_or(crate::core::domain::connection::DEFAULT_PORT);

        let tls = match optional_option(args, "--tls") {
            None => TlsMode::Disable,
            Some(s) => parse_tls_mode(&s)?,
        };

        self.connection_service.add_connection(AddConnectionInput {
            name: name.clone(),
            host,
            port,
            username,
            password,
            database,
            tls,
            environment: optional_option(args, "--env"),
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
        let env_w = connections
            .iter()
            .map(|c| c.environment.as_deref().unwrap_or("").len())
            .max()
            .unwrap_or(3)
            .max(3);

        println!(
            "{:<8}  {:<name_w$}  {:<host_w$}  {:<6}  {:<db_w$}  {:<env_w$}  {:<user_w$}  {:<tls_w$}  PASSWORD",
            "ID", "NAME", "HOST", "PORT", "DATABASE", "ENV", "USERNAME", "TLS",
        );

        for c in &connections {
            println!(
                "{:<8}  {:<name_w$}  {:<host_w$}  {:<6}  {:<db_w$}  {:<env_w$}  {:<user_w$}  {:<tls_w$}  ****",
                c.id.as_deref().unwrap_or("-"),
                c.name, c.host, c.port, c.database,
                c.environment.as_deref().unwrap_or(""),
                c.username, c.tls,
            );
        }

        Ok(())
    }

    fn connect_to(&self, args: &[String]) -> Result<(), String> {
        let name = args
            .first()
            .ok_or("usage: pgrs connect <connection-name>")?
            .trim()
            .to_string();

        let connection = self.connection_service.find_connection(&name)?;

        println!(
            "handing off to psql for '{}' — you'll return to your shell, not pgrs, on exit",
            connection.name
        );

        let mut cmd = std::process::Command::new("psql");
        cmd.env("PGPASSWORD", &connection.password)
            .arg("-h")
            .arg(&connection.host)
            .arg("-p")
            .arg(connection.port.to_string())
            .arg("-U")
            .arg(&connection.username)
            .arg("-d")
            .arg(&connection.database);

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let error = cmd.exec();
            return Err(if error.kind() == std::io::ErrorKind::NotFound {
                "psql not found — is it installed?".to_string()
            } else {
                error.to_string()
            });
        }

        #[cfg(not(unix))]
        {
            match cmd.status() {
                Ok(_) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    Err("psql not found — is it installed?".to_string())
                }
                Err(e) => Err(e.to_string()),
            }
        }
    }

    fn edit_connection(&self, args: &[String]) -> Result<(), String> {
        let name = args
            .first()
            .ok_or("usage: pgrs edit <name> [--host=...] [--port=...] [--username=...] [--password=...] [--database=...] [--tls=...] [--env=...]")?
            .trim()
            .to_string();

        let port = optional_option(args, "--port")
            .map(|v| v.parse::<u16>().map_err(|_| "port must be a number".to_string()))
            .transpose()?;

        let tls = match optional_option(args, "--tls") {
            None => None,
            Some(s) => Some(parse_tls_mode(&s)?),
        };

        let environment = args.iter()
            .find(|a| a.starts_with("--env="))
            .map(|arg| {
                let val = arg.strip_prefix("--env=").unwrap();
                if val.is_empty() { None } else { Some(val.to_string()) }
            });

        let resolved_name = self.connection_service.find_connection(&name)?.name;
        self.connection_service.edit_connection(&resolved_name, EditConnectionInput {
            host: optional_option(args, "--host"),
            port,
            username: optional_option(args, "--username"),
            password: optional_option(args, "--password"),
            database: optional_option(args, "--database"),
            tls,
            environment,
        })?;

        println!("connection '{resolved_name}' updated");
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
        let resolved_old = self.connection_service.find_connection(&old_name)?.name;
        self.connection_service.rename_connection(&resolved_old, &new_name)?;
        println!("connection '{resolved_old}' renamed to '{new_name}'");
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

        let resolved_name = self.connection_service.find_connection(&name)?.name;
        self.connection_service.delete_connection(&resolved_name)?;
        println!("connection '{resolved_name}' deleted");
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

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut decoded: Vec<u8> = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            )
        {
            decoded.push((hi * 16 + lo) as u8);
            i += 3;
            continue;
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(decoded).unwrap_or_else(|e| {
        // Invalid UTF-8: return the lossy conversion so the caller gets a
        // user-readable error from the service layer rather than a panic.
        String::from_utf8_lossy(e.as_bytes()).into_owned()
    })
}

#[derive(Debug, Default)]
struct ParsedUrl {
    host: Option<String>,
    port: Option<u16>,
    username: Option<String>,
    password: Option<String>,
    database: Option<String>,
}

// Individual CLI flags take precedence over URL-parsed values.
fn parse_connection_url(url: &str) -> Result<ParsedUrl, String> {
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
                (
                    Some(percent_decode(&ui[..colon])),
                    Some(percent_decode(&ui[colon + 1..])),
                )
            } else {
                (Some(percent_decode(ui)), None)
            }
        }
        None => (None, None),
    };

    let (hostport, database) = if let Some(slash) = hostinfo.find('/') {
        let db = &hostinfo[slash + 1..];
        (
            &hostinfo[..slash],
            if db.is_empty() { None } else { Some(percent_decode(db)) },
        )
    } else {
        (hostinfo, None)
    };

    let (host, port) = if hostport.starts_with('[') {
        // IPv6 bracket notation: [::1] or [::1]:5432
        let bracket_end = hostport
            .find(']')
            .ok_or_else(|| format!("unclosed '[' in URL host '{}'", hostport))?;
        let h = &hostport[1..bracket_end];
        let rest = &hostport[bracket_end + 1..];
        let port = if let Some(port_str) = rest.strip_prefix(':') {
            let p = port_str
                .parse::<u16>()
                .map_err(|_| format!("invalid port '{}' in URL", port_str))?;
            Some(p)
        } else if rest.is_empty() {
            None
        } else {
            return Err(format!("unexpected content after IPv6 address: '{}'", rest));
        };
        (if h.is_empty() { None } else { Some(h.to_string()) }, port)
    } else if let Some(colon) = hostport.rfind(':') {
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

    Ok(ParsedUrl { host, port, username, password, database })
}

fn parse_tls_mode(value: &str) -> Result<TlsMode, String> {
    match value {
        "disable" => Ok(TlsMode::Disable),
        "require" => Ok(TlsMode::Require),
        "verify-full" => Ok(TlsMode::VerifyFull),
        other => Err(format!(
            "unknown tls mode '{other}' — supported: disable, require, verify-full"
        )),
    }
}

fn optional_option(args: &[String], key: &str) -> Option<String> {
    args.iter()
        .find_map(|arg| arg.strip_prefix(key).and_then(|r| r.strip_prefix('=')).map(ToString::to_string))
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
            "           [--env=<environment>]",
        ],
        desc_lines: &[
            "Add a new named connection. Use --url to specify all fields at once;",
            "individual flags override URL-parsed values.",
            "--tls: disable (no encryption), require (encrypt, no cert check),",
            "       verify-full (encrypt + verify server certificate)",
        ],
    },
    CmdDoc {
        usage_lines: &["list|ls [--names-only]"],
        desc_lines: &[
            "List all saved connections.",
            "--names-only: print only names, one per line (handy for scripts and shell completion)",
        ],
    },
    CmdDoc {
        usage_lines: &[
            "edit <name> [--host=...] [--port=...] [--username=...] [--password=...]",
            "            [--database=...] [--tls=...] [--env=...]",
        ],
        desc_lines: &["Update one or more fields of a saved connection"],
    },
    CmdDoc {
        usage_lines: &["delete|del|rm <name> [--yes]"],
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
    use crate::core::ports::connection_repository::test_support::StubConnectionRepository;

    fn cli_with(names: &[&str]) -> Cli<StubConnectionRepository> {
        Cli::new(ConnectionService::new(StubConnectionRepository::with_names(names)))
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
        let parsed =
            parse_connection_url("postgresql://user:pass@localhost:5432/mydb").unwrap();
        assert_eq!(parsed.host, Some("localhost".to_string()));
        assert_eq!(parsed.port, Some(5432));
        assert_eq!(parsed.username, Some("user".to_string()));
        assert_eq!(parsed.password, Some("pass".to_string()));
        assert_eq!(parsed.database, Some("mydb".to_string()));
    }

    #[test]
    fn parse_url_postgres_scheme() {
        let parsed = parse_connection_url("postgres://user:pass@localhost/db").unwrap();
        assert_eq!(parsed.host, Some("localhost".to_string()));
        assert_eq!(parsed.username, Some("user".to_string()));
        assert_eq!(parsed.database, Some("db".to_string()));
    }

    #[test]
    fn parse_url_without_port_returns_none() {
        let parsed = parse_connection_url("postgresql://user:pass@localhost/db").unwrap();
        assert!(parsed.port.is_none());
    }

    #[test]
    fn parse_url_ipv6_with_port() {
        let parsed = parse_connection_url("postgresql://user:pass@[::1]:5432/mydb").unwrap();
        assert_eq!(parsed.host, Some("::1".to_string()));
        assert_eq!(parsed.port, Some(5432));
        assert_eq!(parsed.database, Some("mydb".to_string()));
    }

    #[test]
    fn parse_url_ipv6_without_port() {
        let parsed = parse_connection_url("postgresql://user:pass@[::1]/mydb").unwrap();
        assert_eq!(parsed.host, Some("::1".to_string()));
        assert!(parsed.port.is_none());
    }

    #[test]
    fn parse_url_ipv6_full_address() {
        let parsed = parse_connection_url("postgresql://user:pass@[2001:db8::1]:5433/db").unwrap();
        assert_eq!(parsed.host, Some("2001:db8::1".to_string()));
        assert_eq!(parsed.port, Some(5433));
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
    fn parse_url_decodes_percent_encoded_password() {
        let parsed =
            parse_connection_url("postgresql://user:p%40ss%23word@localhost/db").unwrap();
        assert_eq!(parsed.password, Some("p@ss#word".to_string()));
    }

    #[test]
    fn parse_url_decodes_percent_encoded_username() {
        let parsed =
            parse_connection_url("postgresql://admin%40corp:pass@localhost/db").unwrap();
        assert_eq!(parsed.username, Some("admin@corp".to_string()));
    }

    #[test]
    fn parse_url_decodes_percent_encoded_database() {
        let parsed =
            parse_connection_url("postgresql://user:pass@localhost/my%20db").unwrap();
        assert_eq!(parsed.database, Some("my db".to_string()));
    }

    #[test]
    fn parse_tls_mode_disable_returns_disable() {
        assert_eq!(parse_tls_mode("disable"), Ok(TlsMode::Disable));
    }

    #[test]
    fn parse_tls_mode_require_returns_require() {
        assert_eq!(parse_tls_mode("require"), Ok(TlsMode::Require));
    }

    #[test]
    fn parse_tls_mode_verify_full_returns_verify_full() {
        assert_eq!(parse_tls_mode("verify-full"), Ok(TlsMode::VerifyFull));
    }

    #[test]
    fn parse_tls_mode_unknown_returns_error_mentioning_value() {
        let err = parse_tls_mode("starttls").unwrap_err();
        assert!(err.contains("starttls"), "got: {err}");
    }

    #[test]
    fn shell_command_does_not_say_unknown_command() {
        let cli = cli_with(&[]);
        let err = cli
            .run(["shell".to_string(), "prod".to_string()].into_iter())
            .unwrap_err();
        assert!(!err.contains("unknown command"), "should give specific error for shell, got: {err}");
    }

    #[test]
    fn test_command_does_not_say_unknown_command() {
        let cli = cli_with(&[]);
        let err = cli
            .run(["test".to_string(), "prod".to_string()].into_iter())
            .unwrap_err();
        assert!(!err.contains("unknown command"), "should give specific error for test, got: {err}");
    }

    #[test]
    fn percent_decode_handles_uppercase_hex() {
        assert_eq!(percent_decode("hello%2Fworld"), "hello/world");
    }

    #[test]
    fn percent_decode_leaves_plain_text_unchanged() {
        assert_eq!(percent_decode("plaintext"), "plaintext");
    }

    #[test]
    fn percent_decode_multibyte_utf8_accent() {
        // %C3%A9 is the UTF-8 encoding of é (U+00E9)
        assert_eq!(percent_decode("%C3%A9"), "é");
    }

    #[test]
    fn percent_decode_multibyte_utf8_in_password() {
        // postgresql://user:caf%C3%A9@host/db — password should decode to "café"
        let parsed = parse_connection_url("postgresql://user:caf%C3%A9@host/db").unwrap();
        assert_eq!(parsed.password, Some("café".to_string()));
    }

    #[test]
    fn percent_decode_multibyte_three_byte_sequence() {
        // %E2%82%AC is the UTF-8 encoding of € (U+20AC)
        assert_eq!(percent_decode("%E2%82%AC"), "€");
    }

    #[test]
    fn completions_fish_returns_ok() {
        let cli = cli_with(&[]);
        assert!(
            cli.run(["completions".to_string(), "fish".to_string()].into_iter())
                .is_ok()
        );
    }

    #[test]
    fn add_with_env_flag_saves_environment() {
        let cli = cli_with(&[]);
        cli.run(add_args("prod", &["--env=production"])).unwrap();
        assert_eq!(
            cli.get_connection("prod").unwrap().environment,
            Some("production".to_string())
        );
    }

    #[test]
    fn add_without_env_flag_leaves_environment_none() {
        let cli = cli_with(&[]);
        cli.run(add_args("prod", &[])).unwrap();
        assert_eq!(cli.get_connection("prod").unwrap().environment, None);
    }

    #[test]
    fn edit_env_flag_sets_environment() {
        let cli = cli_with(&["prod"]);
        cli.run(edit_args("prod", &["--env=staging"])).unwrap();
        assert_eq!(
            cli.get_connection("prod").unwrap().environment,
            Some("staging".to_string())
        );
    }

    #[test]
    fn edit_empty_env_flag_clears_environment() {
        let cli = cli_with(&["prod"]);
        // first set env
        cli.run(edit_args("prod", &["--env=prod"])).unwrap();
        // then clear it
        cli.run(edit_args("prod", &["--env="])).unwrap();
        assert_eq!(cli.get_connection("prod").unwrap().environment, None);
    }
}
