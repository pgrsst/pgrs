use crate::adapters::driving::completions;
use crate::core::domain::connection::{Connection, TlsMode};
use crate::core::ports::connection_repository::ConnectionRepository;
use crate::core::services::connection::service::{AddConnectionInput, ConnectionService};

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
            None => {
                println!("{}", welcome());
                Ok(())
            }
            Some("add") => self.add_connection(&args[1..]),
            Some("list") => self.list_connections(&args[1..]),
            Some("delete") => self.delete_connection(&args[1..]),
            Some("connect") => self.connect_to(&args[1..]),
            Some("completions") => self.print_completions(&args[1..]),
            // "shell" is intercepted in app.rs before cli.run() is called
            _ => Err(usage().to_string()),
        }
    }

    fn add_connection(&self, args: &[String]) -> Result<(), String> {
        let name = args.first().ok_or_else(|| usage().to_string())?.trim().to_string();

        let host = required_option(args, "--host")?;
        let username = required_option(args, "--username")?;
        let password = required_option(args, "--password")?;
        let database = required_option(args, "--database")?;
        let port = optional_option(args, "--port")
            .map(|value| {
                value
                    .parse::<u16>()
                    .map_err(|_| "port must be a number".to_string())
            })
            .transpose()?
            .unwrap_or(5432);

        let tls = match optional_option(args, "--tls").as_deref() {
            None | Some("disable") => TlsMode::Disable,
            Some("require") => TlsMode::Require,
            Some(other) => return Err(format!("unknown tls mode '{other}' — supported: disable, require")),
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

        println!(
            "{:<name_w$}  {:<host_w$}  {:<6}  {:<db_w$}  {:<user_w$}  PASSWORD",
            "NAME", "HOST", "PORT", "DATABASE", "USERNAME",
        );

        for c in &connections {
            println!(
                "{:<name_w$}  {:<host_w$}  {:<6}  {:<db_w$}  {:<user_w$}  ****",
                c.name, c.host, c.port, c.database, c.username,
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

    fn delete_connection(&self, args: &[String]) -> Result<(), String> {
        let name = args
            .first()
            .ok_or("usage: pgrs delete <connection-name>")?
            .trim()
            .to_string();

        self.connection_service.delete_connection(&name)?;
        println!("connection '{name}' deleted");
        Ok(())
    }

    fn print_completions(&self, args: &[String]) -> Result<(), String> {
        let shell = args.first().ok_or("usage: pgrs completions <bash|zsh|fish>")?;
        let script = match shell.as_str() {
            "bash" => completions::bash_script(),
            "zsh" => completions::zsh_script(),
            "fish" => completions::fish_script(),
            other => return Err(format!("unknown shell '{}' — supported: bash, zsh, fish", other)),
        };
        print!("{}", script);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn get_connection(&self, name: &str) -> Result<Connection, String> {
        self.connection_service.get_connection(name)
    }
}

fn required_option(args: &[String], key: &str) -> Result<String, String> {
    optional_option(args, key).ok_or_else(|| format!("{key} is required"))
}

fn optional_option(args: &[String], key: &str) -> Option<String> {
    let prefix = format!("{key}=");

    args.iter()
        .find_map(|arg| arg.strip_prefix(&prefix).map(ToString::to_string))
}

fn welcome() -> &'static str {
    "pgrs — PostgreSQL connection manager built with Rust\n\nManage and store named PostgreSQL connections locally.\n\nCommands:\n  add <name> --host=<host> --username=<user> --password=<pass> --database=<db> [--port=<port>] [--tls=disable|require]\n             Add a new named connection\n  list         List all saved connections\n  list --names-only\n             Print connection names only, one per line\n  delete <name>\n             Delete a named connection\n  connect <name>\n             Open an interactive psql session using a saved connection\n  shell <name>\n             Open pgrs interactive SQL REPL with auto-completion\n  completions <bash|zsh|fish>\n             Print shell completion script\n\nRun `pgrs <command> --help` for more info on a specific command."
}

fn usage() -> &'static str {
    "usage: pgrs add <connection-name> --host=<host> --username=<username> --password=<password> --database=<database> [--port=<port>]"
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
            self.connections.borrow_mut().retain(|c| c.name != name);
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

    fn add_args(name: &str, extra: &[&str]) -> impl Iterator<Item = String> {
        let mut args = vec![
            "add".to_string(), name.to_string(),
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
}
