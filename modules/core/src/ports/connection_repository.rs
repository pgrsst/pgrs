use crate::domain::connection::Connection;
use crate::domain::error::DomainError;

pub trait ConnectionRepository: Send + Sync {
    fn add(&self, connection: Connection) -> Result<(), DomainError>;
    fn list(&self) -> Result<Vec<Connection>, DomainError>;
    fn delete(&self, name: &str) -> Result<(), DomainError>;
    fn get_connection(&self, name: &str) -> Result<Connection, DomainError>;
    fn rename(&self, old_name: &str, new_name: &str) -> Result<(), DomainError>;
    fn update(&self, connection: Connection) -> Result<(), DomainError>;
}

#[cfg(test)]
pub mod test_support {
    use crate::domain::connection::{Connection, DEFAULT_PORT};
    use crate::enums::tls_mode::TlsMode;
    use crate::domain::error::DomainError;
    use crate::ports::connection_repository::ConnectionRepository;
    use std::sync::Mutex;

    pub struct StubConnectionRepository {
        connections: Mutex<Vec<Connection>>,
    }

    impl StubConnectionRepository {
        pub fn new() -> Self {
            Self { connections: Mutex::new(vec![]) }
        }

        #[allow(dead_code)]
        pub fn with_names(names: &[&str]) -> Self {
            let connections = names
                .iter()
                .enumerate()
                .map(|(i, &n)| Connection {
                    name: n.to_string(),
                    host: "localhost".to_string(),
                    port: DEFAULT_PORT,
                    username: "user".to_string(),
                    password: "pass".to_string(),
                    database: "db".to_string(),
                    tls: TlsMode::Disable,
                    environment: None,
                    id: Some((i as i64) + 1),
                })
                .collect();
            Self { connections: Mutex::new(connections) }
        }
    }

    impl ConnectionRepository for StubConnectionRepository {
        fn add(&self, connection: Connection) -> Result<(), DomainError> {
            let mut connections = self.connections.lock().unwrap();
            if connections.iter().any(|c| c.name == connection.name) {
                return Err(DomainError::AlreadyExists(
                    format!("connection '{}' already exists", connection.name)
                ));
            }
            let mut conn = connection;
            conn.id = Some((connections.len() as i64) + 1);
            connections.push(conn);
            Ok(())
        }

        fn list(&self) -> Result<Vec<Connection>, DomainError> {
            Ok(self.connections.lock().unwrap().clone())
        }

        fn delete(&self, name: &str) -> Result<(), DomainError> {
            let mut connections = self.connections.lock().unwrap();
            let initial_len = connections.len();
            connections.retain(|c| c.name != name);
            if connections.len() == initial_len {
                return Err(DomainError::NotFound(format!("connection '{}' not found", name)));
            }
            Ok(())
        }

        fn get_connection(&self, name: &str) -> Result<Connection, DomainError> {
            self.connections
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.name == name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(format!("connection '{}' not found", name)))
        }

        fn rename(&self, old_name: &str, new_name: &str) -> Result<(), DomainError> {
            let mut connections = self.connections.lock().unwrap();
            if connections.iter().any(|c| c.name == new_name) {
                return Err(DomainError::AlreadyExists(
                    format!("connection '{}' already exists", new_name)
                ));
            }
            let conn = connections
                .iter_mut()
                .find(|c| c.name == old_name)
                .ok_or_else(|| DomainError::NotFound(format!("connection '{}' not found", old_name)))?;
            conn.name = new_name.to_string();
            Ok(())
        }

        fn update(&self, connection: Connection) -> Result<(), DomainError> {
            let mut connections = self.connections.lock().unwrap();
            let pos = connections
                .iter()
                .position(|c| c.name == connection.name)
                .ok_or_else(|| DomainError::NotFound(
                    format!("connection '{}' not found", connection.name)
                ))?;
            connections[pos] = connection;
            Ok(())
        }
    }
}
