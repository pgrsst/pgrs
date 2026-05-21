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
            Some("list") => self.list_connections(),
            Some("delete") => self.delete_connection(&args[1..]),
            Some("connect") => self.connect_to(&args[1..]),
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

        self.connection_service.add_connection(AddConnectionInput {
            name: name.clone(),
            host,
            port,
            username,
            password,
            database,
        })?;

        println!("connection '{name}' added");
        Ok(())
    }

    fn list_connections(&self) -> Result<(), String> {
        let connections = self.connection_service.list_connections()?;

        if connections.is_empty() {
            println!("no connections saved");
            return Ok(());
        }

        let name_w = connections.iter().map(|c| c.name.len()).max().unwrap_or(4).max(4);
        let host_w = connections.iter().map(|c| c.host.len()).max().unwrap_or(4).max(4);
        let db_w = connections.iter().map(|c| c.database.len()).max().unwrap_or(8).max(8);
        let user_w = connections.iter().map(|c| c.username.len()).max().unwrap_or(8).max(8);

        println!(
            "{:<name_w$}  {:<host_w$}  {:<6}  {:<db_w$}  {:<user_w$}  {}",
            "NAME", "HOST", "PORT", "DATABASE", "USERNAME", "PASSWORD",
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
    "pgrs — PostgreSQL connection manager built with Rust\n\nManage and store named PostgreSQL connections locally.\n\nCommands:\n  add <name> --host=<host> --username=<user> --password=<pass> --database=<db> [--port=<port>]\n             Add a new named connection\n  list         List all saved connections\n  delete <name>\n             Delete a named connection\n  connect <name>\n             Open an interactive psql session using a saved connection\n\nRun `pgrs <command> --help` for more info on a specific command."
}

fn usage() -> &'static str {
    "usage: pgrs add <connection-name> --host=<host> --username=<username> --password=<password> --database=<database> [--port=<port>]"
}
