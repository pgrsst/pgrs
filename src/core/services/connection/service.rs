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

        if input.username.trim().is_empty() {
            return Err("username is required".to_string());
        }

        if input.password.trim().is_empty() {
            return Err("password is required".to_string());
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
        fn new() -> Self {
            Self {
                connections: RefCell::new(vec![]),
            }
        }
    }

    impl ConnectionRepository for StubRepository {
        fn add(&self, connection: Connection) -> Result<(), String> {
            self.connections.borrow_mut().push(connection);
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
    }

    fn valid_input(name: &str) -> AddConnectionInput {
        AddConnectionInput {
            name: name.to_string(),
            host: "localhost".to_string(),
            port: 5432,
            username: "admin".to_string(),
            password: "secret".to_string(),
            database: "mydb".to_string(),
        }
    }

    fn service() -> ConnectionService<StubRepository> {
        ConnectionService::new(StubRepository::new())
    }

    #[test]
    fn add_connection_succeeds() {
        let svc = service();
        assert!(svc.add_connection(valid_input("prod")).is_ok());
    }

    #[test]
    fn add_connection_rejects_empty_name() {
        let svc = service();
        let result = svc.add_connection(AddConnectionInput {
            name: "  ".to_string(),
            ..valid_input("x")
        });
        assert_eq!(result, Err("connection name is required".to_string()));
    }

    #[test]
    fn add_connection_rejects_empty_host() {
        let svc = service();
        let result = svc.add_connection(AddConnectionInput {
            host: "".to_string(),
            ..valid_input("prod")
        });
        assert_eq!(result, Err("host is required".to_string()));
    }

    #[test]
    fn add_connection_rejects_empty_database() {
        let svc = service();
        let result = svc.add_connection(AddConnectionInput {
            database: "".to_string(),
            ..valid_input("prod")
        });
        assert_eq!(result, Err("database is required".to_string()));
    }

    #[test]
    fn add_connection_rejects_empty_username() {
        let svc = service();
        let result = svc.add_connection(AddConnectionInput {
            username: "".to_string(),
            ..valid_input("prod")
        });
        assert_eq!(result, Err("username is required".to_string()));
    }

    #[test]
    fn add_connection_rejects_empty_password() {
        let svc = service();
        let result = svc.add_connection(AddConnectionInput {
            password: "".to_string(),
            ..valid_input("prod")
        });
        assert_eq!(result, Err("password is required".to_string()));
    }

    #[test]
    fn list_connections_returns_all() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        svc.add_connection(valid_input("staging")).unwrap();
        let list = svc.list_connections().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "prod");
        assert_eq!(list[1].name, "staging");
    }

    #[test]
    fn delete_connection_succeeds() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        assert!(svc.delete_connection("prod").is_ok());
        assert!(svc.list_connections().unwrap().is_empty());
    }

    #[test]
    fn delete_connection_rejects_empty_name() {
        let svc = service();
        let result = svc.delete_connection("  ");
        assert_eq!(result, Err("connection name is required".to_string()));
    }
}
