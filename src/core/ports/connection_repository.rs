use crate::core::domain::connection::Connection;

pub trait ConnectionRepository {
    fn add(&self, connection: Connection) -> Result<(), String>;
    fn list(&self) -> Result<Vec<Connection>, String>;
    fn delete(&self, name: &str) -> Result<(), String>;
    fn get_connection(&self, name: &str) -> Result<Connection, String>;
    fn rename(&self, old_name: &str, new_name: &str) -> Result<(), String>;
    fn update(&self, connection: Connection) -> Result<(), String>;
}

#[cfg(test)]
pub mod test_support {
    use crate::core::domain::connection::{Connection, TlsMode, DEFAULT_PORT};
    use crate::core::ports::connection_repository::ConnectionRepository;
    use std::cell::RefCell;

    pub struct StubConnectionRepository {
        connections: RefCell<Vec<Connection>>,
    }

    impl StubConnectionRepository {
        pub fn new() -> Self {
            Self { connections: RefCell::new(vec![]) }
        }

        pub fn with_names(names: &[&str]) -> Self {
            let connections = names
                .iter()
                .map(|&n| Connection {
                    name: n.to_string(),
                    host: "localhost".to_string(),
                    port: DEFAULT_PORT,
                    username: "user".to_string(),
                    password: "pass".to_string(),
                    database: "db".to_string(),
                    tls: TlsMode::Disable,
                })
                .collect();
            Self { connections: RefCell::new(connections) }
        }
    }

    impl ConnectionRepository for StubConnectionRepository {
        fn add(&self, connection: Connection) -> Result<(), String> {
            let mut connections = self.connections.borrow_mut();
            if connections.iter().any(|c| c.name == connection.name) {
                return Err(format!("connection '{}' already exists", connection.name));
            }
            connections.push(connection);
            Ok(())
        }

        fn list(&self) -> Result<Vec<Connection>, String> {
            Ok(self.connections.borrow().clone())
        }

        fn delete(&self, name: &str) -> Result<(), String> {
            let mut connections = self.connections.borrow_mut();
            let initial_len = connections.len();
            connections.retain(|c| c.name != name);
            if connections.len() == initial_len {
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
}
