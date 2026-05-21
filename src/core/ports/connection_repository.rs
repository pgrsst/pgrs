use crate::core::domain::connection::Connection;

pub trait ConnectionRepository {
    fn add(&self, connection: Connection) -> Result<(), String>;
    fn list(&self) -> Result<Vec<Connection>, String>;
    fn delete(&self, name: &str) -> Result<(), String>;
    fn get_connection(&self, name: &str) -> Result<Connection, String>;
}
