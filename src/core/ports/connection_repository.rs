use crate::core::domain::connection::Connection;
use crate::core::domain::error::DomainError;

pub trait ConnectionRepository {
    fn add(&self, connection: Connection) -> Result<(), DomainError>;
    fn list(&self) -> Result<Vec<Connection>, DomainError>;
    fn delete(&self, name: &str) -> Result<(), DomainError>;
    fn get_connection(&self, name: &str) -> Result<Connection, DomainError>;
    fn rename(&self, old_name: &str, new_name: &str) -> Result<(), DomainError>;
    fn update(&self, connection: Connection) -> Result<(), DomainError>;
}

impl<T: ConnectionRepository> ConnectionRepository for std::sync::Arc<T> {
    fn add(&self, connection: Connection) -> Result<(), DomainError> {
        (**self).add(connection)
    }
    fn list(&self) -> Result<Vec<Connection>, DomainError> {
        (**self).list()
    }
    fn delete(&self, name: &str) -> Result<(), DomainError> {
        (**self).delete(name)
    }
    fn get_connection(&self, name: &str) -> Result<Connection, DomainError> {
        (**self).get_connection(name)
    }
    fn update(&self, connection: Connection) -> Result<(), DomainError> {
        (**self).update(connection)
    }
    fn rename(&self, old_name: &str, new_name: &str) -> Result<(), DomainError> {
        (**self).rename(old_name, new_name)
    }
}

#[cfg(test)]
pub mod test_support {
    use crate::core::domain::connection::{Connection, TlsMode, DEFAULT_PORT};
    use crate::core::domain::error::DomainError;
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
                .map(|&n| Connection::new(
                    n.to_string(),
                    "localhost".to_string(),
                    DEFAULT_PORT,
                    "user".to_string(),
                    "pass".to_string(),
                    "db".to_string(),
                    TlsMode::Disable,
                    None,
                ).expect("valid stub connection"))
                .collect();
            Self { connections: RefCell::new(connections) }
        }
    }

    impl ConnectionRepository for StubConnectionRepository {
        fn add(&self, connection: Connection) -> Result<(), DomainError> {
            let mut connections = self.connections.borrow_mut();
            if connections.iter().any(|c| c.name() == connection.name()) {
                return Err(DomainError::AlreadyExists(
                    format!("connection '{}' already exists", connection.name())
                ));
            }
            connections.push(connection);
            Ok(())
        }

        fn list(&self) -> Result<Vec<Connection>, DomainError> {
            Ok(self.connections.borrow().clone())
        }

        fn delete(&self, name: &str) -> Result<(), DomainError> {
            let mut connections = self.connections.borrow_mut();
            let initial_len = connections.len();
            connections.retain(|c| c.name() != name);
            if connections.len() == initial_len {
                return Err(DomainError::NotFound(format!("connection '{}' not found", name)));
            }
            Ok(())
        }

        fn get_connection(&self, name: &str) -> Result<Connection, DomainError> {
            self.connections
                .borrow()
                .iter()
                .find(|c| c.name() == name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(format!("connection '{}' not found", name)))
        }

        fn rename(&self, old_name: &str, new_name: &str) -> Result<(), DomainError> {
            let mut connections = self.connections.borrow_mut();
            if connections.iter().any(|c| c.name() == new_name) {
                return Err(DomainError::AlreadyExists(
                    format!("connection '{}' already exists", new_name)
                ));
            }
            let conn = connections
                .iter_mut()
                .find(|c| c.name() == old_name)
                .ok_or_else(|| DomainError::NotFound(format!("connection '{}' not found", old_name)))?;
            conn.set_name(new_name.to_string());
            Ok(())
        }

        fn update(&self, connection: Connection) -> Result<(), DomainError> {
            let mut connections = self.connections.borrow_mut();
            let pos = connections
                .iter()
                .position(|c| c.name() == connection.name())
                .ok_or_else(|| DomainError::NotFound(
                    format!("connection '{}' not found", connection.name())
                ))?;
            connections[pos] = connection;
            Ok(())
        }
    }
}
