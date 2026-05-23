use crate::core::domain::connection::{Connection, TlsMode};
use crate::core::ports::connection_repository::ConnectionRepository;

pub struct ConnectionService<R>
where
    R: ConnectionRepository,
{
    repository: R,
}

pub struct EditConnectionInput {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub database: Option<String>,
    pub tls: Option<TlsMode>,
}

pub struct AddConnectionInput {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub tls: TlsMode,
}

fn require_field(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{label} is required"))
    } else {
        Ok(())
    }
}

impl<R> ConnectionService<R>
where
    R: ConnectionRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn add_connection(&self, input: AddConnectionInput) -> Result<(), String> {
        require_field("connection name", &input.name)?;
        require_field("host", &input.host)?;
        require_field("database", &input.database)?;
        require_field("username", &input.username)?;
        require_field("password", &input.password)?;

        let connection = Connection {
            name: input.name,
            host: input.host,
            port: input.port,
            username: input.username,
            password: input.password,
            database: input.database,
            tls: input.tls,
        };

        self.repository.add(connection)
    }

    pub fn list_connections(&self) -> Result<Vec<Connection>, String> {
        self.repository.list()
    }

    pub fn delete_connection(&self, name: &str) -> Result<(), String> {
        require_field("connection name", name)?;
        self.repository.delete(name)
    }

    pub fn get_connection(&self, name: &str) -> Result<Connection, String> {
        require_field("connection name", name)?;
        self.repository.get_connection(name)
    }

    pub fn edit_connection(&self, name: &str, input: EditConnectionInput) -> Result<(), String> {
        require_field("connection name", name)?;

        if input.host.is_none()
            && input.port.is_none()
            && input.username.is_none()
            && input.password.is_none()
            && input.database.is_none()
            && input.tls.is_none()
        {
            return Err("at least one field must be specified".to_string());
        }

        if let Some(ref v) = input.host { require_field("host", v)?; }
        if let Some(ref v) = input.username { require_field("username", v)?; }
        if let Some(ref v) = input.password { require_field("password", v)?; }
        if let Some(ref v) = input.database { require_field("database", v)?; }

        let mut conn = self.repository.get_connection(name)?;
        if let Some(v) = input.host { conn.host = v; }
        if let Some(v) = input.port { conn.port = v; }
        if let Some(v) = input.username { conn.username = v; }
        if let Some(v) = input.password { conn.password = v; }
        if let Some(v) = input.database { conn.database = v; }
        if let Some(v) = input.tls { conn.tls = v; }
        self.repository.update(conn)
    }

    pub fn rename_connection(&self, old_name: &str, new_name: &str) -> Result<(), String> {
        require_field("old connection name", old_name)?;
        require_field("new connection name", new_name)?;
        self.repository.rename(old_name, new_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::connection::{Connection, TlsMode};
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

    fn valid_input(name: &str) -> AddConnectionInput {
        AddConnectionInput {
            name: name.to_string(),
            host: "localhost".to_string(),
            port: 5432,
            username: "admin".to_string(),
            password: "secret".to_string(),
            database: "mydb".to_string(),
            tls: TlsMode::Disable,
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

    #[test]
    fn get_connection_returns_existing_connection() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        let conn = svc.get_connection("prod").unwrap();
        assert_eq!(conn.name, "prod");
    }

    #[test]
    fn get_connection_returns_error_when_not_found() {
        let svc = service();
        let result = svc.get_connection("missing");
        assert_eq!(result, Err("connection 'missing' not found".to_string()));
    }

    #[test]
    fn get_connection_rejects_empty_name() {
        let svc = service();
        let result = svc.get_connection("  ");
        assert_eq!(result, Err("connection name is required".to_string()));
    }

    fn edit_input() -> EditConnectionInput {
        EditConnectionInput {
            host: None,
            port: None,
            username: None,
            password: None,
            database: None,
            tls: None,
        }
    }

    #[test]
    fn edit_connection_updates_single_field() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        svc.edit_connection("prod", EditConnectionInput {
            database: Some("newdb".to_string()),
            ..edit_input()
        }).unwrap();
        let conn = svc.get_connection("prod").unwrap();
        assert_eq!(conn.database, "newdb");
        assert_eq!(conn.host, "localhost"); // unchanged
    }

    #[test]
    fn edit_connection_updates_multiple_fields() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        svc.edit_connection("prod", EditConnectionInput {
            host: Some("db.example.com".to_string()),
            password: Some("newpass".to_string()),
            ..edit_input()
        }).unwrap();
        let conn = svc.get_connection("prod").unwrap();
        assert_eq!(conn.host, "db.example.com");
        assert_eq!(conn.password, "newpass");
        assert_eq!(conn.database, "mydb"); // unchanged
    }

    #[test]
    fn edit_connection_rejects_no_fields() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        let result = svc.edit_connection("prod", edit_input());
        assert_eq!(result, Err("at least one field must be specified".to_string()));
    }

    #[test]
    fn edit_connection_rejects_empty_name() {
        let svc = service();
        let result = svc.edit_connection("  ", EditConnectionInput {
            host: Some("h".to_string()),
            ..edit_input()
        });
        assert_eq!(result, Err("connection name is required".to_string()));
    }

    #[test]
    fn edit_connection_rejects_empty_host_value() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        let result = svc.edit_connection("prod", EditConnectionInput {
            host: Some("".to_string()),
            ..edit_input()
        });
        assert_eq!(result, Err("host is required".to_string()));
    }

    #[test]
    fn edit_connection_returns_error_when_not_found() {
        let svc = service();
        let result = svc.edit_connection("missing", EditConnectionInput {
            host: Some("h".to_string()),
            ..edit_input()
        });
        assert_eq!(result, Err("connection 'missing' not found".to_string()));
    }

    #[test]
    fn edit_connection_updates_tls_mode() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        svc.edit_connection("prod", EditConnectionInput {
            tls: Some(TlsMode::Require),
            ..edit_input()
        }).unwrap();
        assert_eq!(svc.get_connection("prod").unwrap().tls, TlsMode::Require);
    }

    #[test]
    fn edit_connection_updates_port() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        svc.edit_connection("prod", EditConnectionInput {
            port: Some(5433),
            ..edit_input()
        }).unwrap();
        assert_eq!(svc.get_connection("prod").unwrap().port, 5433);
    }

    #[test]
    fn rename_connection_succeeds() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        svc.rename_connection("prod", "production").unwrap();
        assert!(svc.get_connection("production").is_ok());
        assert!(svc.get_connection("prod").is_err());
    }

    #[test]
    fn rename_connection_returns_error_when_not_found() {
        let svc = service();
        let result = svc.rename_connection("missing", "new");
        assert_eq!(result, Err("connection 'missing' not found".to_string()));
    }

    #[test]
    fn rename_connection_returns_error_when_new_name_exists() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        svc.add_connection(valid_input("staging")).unwrap();
        let result = svc.rename_connection("prod", "staging");
        assert_eq!(result, Err("connection 'staging' already exists".to_string()));
    }

    #[test]
    fn rename_connection_rejects_empty_old_name() {
        let svc = service();
        let result = svc.rename_connection("  ", "new");
        assert_eq!(result, Err("old connection name is required".to_string()));
    }

    #[test]
    fn rename_connection_rejects_empty_new_name() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        let result = svc.rename_connection("prod", "  ");
        assert_eq!(result, Err("new connection name is required".to_string()));
    }
}
