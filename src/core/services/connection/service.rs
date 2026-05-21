use crate::core::domain::connection::Connection;
use crate::core::ports::connection_repository::ConnectionRepository;

pub struct ConnectionService<R>
where
    R: ConnectionRepository,
{
    repository: R,
}

pub struct AddConnectionInput {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
}

impl<R> ConnectionService<R>
where
    R: ConnectionRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn add_connection(&self, input: AddConnectionInput) -> Result<(), String> {
        if input.name.trim().is_empty() {
            return Err("connection name is required".to_string());
        }

        if input.host.trim().is_empty() {
            return Err("host is required".to_string());
        }

        if input.database.trim().is_empty() {
            return Err("database is required".to_string());
        }

        let connection = Connection {
            name: input.name,
            host: input.host,
            port: input.port,
            username: input.username,
            password: input.password,
            database: input.database,
        };

        self.repository.add(connection)
    }

    pub fn list_connections(&self) -> Result<Vec<Connection>, String> {
        self.repository.list()
    }

    pub fn delete_connection(&self, name: &str) -> Result<(), String> {
        if name.trim().is_empty() {
            return Err("connection name is required".to_string());
        }

        self.repository.delete(name)
    }
}
