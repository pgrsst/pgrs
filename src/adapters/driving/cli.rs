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
            Some("add") => self.add_connection(&args[1..]),
            _ => Err(usage()),
        }
    }

    fn add_connection(&self, args: &[String]) -> Result<(), String> {
        let name = args.first().ok_or_else(usage)?.trim().to_string();

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
}

fn required_option(args: &[String], key: &str) -> Result<String, String> {
    optional_option(args, key).ok_or_else(|| format!("{key} is required"))
}

fn optional_option(args: &[String], key: &str) -> Option<String> {
    let prefix = format!("{key}=");

    args.iter()
        .find_map(|arg| arg.strip_prefix(&prefix).map(ToString::to_string))
}

fn usage() -> String {
    "usage: pgrs add <connection-name> --host=<host> --username=<username> --password=<password> --database=<database> [--port=<port>]".to_string()
}
