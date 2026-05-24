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
    pub environment: Option<Option<String>>,
}

pub struct AddConnectionInput {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub tls: TlsMode,
    pub environment: Option<String>,
}

fn require_field(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{label} is required"))
    } else {
        Ok(())
    }
}

fn generate_id() -> Result<String, String> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .subsec_nanos();

    let thread_id = std::thread::current().id();
    let mut hasher = DefaultHasher::new();
    nanos.hash(&mut hasher);
    thread_id.hash(&mut hasher);
    let hash = hasher.finish();

    Ok(format!("{:08x}", hash & 0xFFFF_FFFF))
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
            environment: input.environment,
            id: Some(generate_id()?),
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

    #[cfg(test)]
    pub fn get_connection(&self, name: &str) -> Result<Connection, String> {
        require_field("connection name", name)?;
        self.repository.get_connection(name)
    }

    pub fn find_connection(&self, input: &str) -> Result<Connection, String> {
        let connections = self.repository.list()?;
        connections
            .into_iter()
            .find(|c| c.id.as_deref() == Some(input) || c.name == input)
            .ok_or_else(|| format!("connection '{}' not found", input))
    }

    pub fn edit_connection(&self, name: &str, input: EditConnectionInput) -> Result<(), String> {
        require_field("connection name", name)?;

        if input.host.is_none()
            && input.port.is_none()
            && input.username.is_none()
            && input.password.is_none()
            && input.database.is_none()
            && input.tls.is_none()
            && input.environment.is_none()
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
        if let Some(v) = input.environment { conn.environment = v; }
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
    use crate::core::domain::connection::{TlsMode, DEFAULT_PORT};
    use crate::core::ports::connection_repository::test_support::StubConnectionRepository;

    fn valid_input(name: &str) -> AddConnectionInput {
        AddConnectionInput {
            name: name.to_string(),
            host: "localhost".to_string(),
            port: DEFAULT_PORT,
            username: "admin".to_string(),
            password: "secret".to_string(),
            database: "mydb".to_string(),
            tls: TlsMode::Disable,
            environment: None,
        }
    }

    fn service() -> ConnectionService<StubConnectionRepository> {
        ConnectionService::new(StubConnectionRepository::new())
    }

    #[test]
    fn add_connection_succeeds() {
        let svc = service();
        assert!(svc.add_connection(valid_input("prod")).is_ok());
    }

    #[test]
    fn add_connection_returns_error_on_duplicate_name() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        let result = svc.add_connection(valid_input("prod"));
        assert_eq!(result, Err("connection 'prod' already exists".to_string()));
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
            environment: None,
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

    #[test]
    fn add_connection_saves_environment() {
        let svc = service();
        svc.add_connection(AddConnectionInput {
            environment: Some("production".to_string()),
            ..valid_input("prod")
        }).unwrap();
        assert_eq!(
            svc.get_connection("prod").unwrap().environment,
            Some("production".to_string())
        );
    }

    #[test]
    fn add_connection_without_environment_saves_none() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        assert_eq!(svc.get_connection("prod").unwrap().environment, None);
    }

    #[test]
    fn edit_connection_sets_environment() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        svc.edit_connection("prod", EditConnectionInput {
            environment: Some(Some("staging".to_string())),
            ..edit_input()
        }).unwrap();
        assert_eq!(
            svc.get_connection("prod").unwrap().environment,
            Some("staging".to_string())
        );
    }

    #[test]
    fn edit_connection_clears_environment() {
        let svc = service();
        svc.add_connection(AddConnectionInput {
            environment: Some("production".to_string()),
            ..valid_input("prod")
        }).unwrap();
        svc.edit_connection("prod", EditConnectionInput {
            environment: Some(None),
            ..edit_input()
        }).unwrap();
        assert_eq!(svc.get_connection("prod").unwrap().environment, None);
    }

    #[test]
    fn edit_connection_with_only_environment_succeeds() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        let result = svc.edit_connection("prod", EditConnectionInput {
            environment: Some(Some("dev".to_string())),
            ..edit_input()
        });
        assert!(result.is_ok());
    }

    #[test]
    fn edit_connection_without_environment_does_not_change_it() {
        let svc = service();
        svc.add_connection(AddConnectionInput {
            environment: Some("prod".to_string()),
            ..valid_input("prod")
        }).unwrap();
        svc.edit_connection("prod", EditConnectionInput {
            database: Some("otherdb".to_string()),
            ..edit_input()
        }).unwrap();
        assert_eq!(
            svc.get_connection("prod").unwrap().environment,
            Some("prod".to_string())
        );
    }

    #[test]
    fn add_connection_assigns_non_none_id() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        let conn = svc.get_connection("prod").unwrap();
        assert!(conn.id.is_some(), "id should be assigned on add");
    }

    #[test]
    fn add_connection_assigns_unique_ids() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        svc.add_connection(valid_input("staging")).unwrap();
        let id1 = svc.get_connection("prod").unwrap().id;
        let id2 = svc.get_connection("staging").unwrap().id;
        assert_ne!(id1, id2, "each connection should get a unique id");
    }

    #[test]
    fn add_connection_assigns_8_char_hex_id() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        let id = svc.get_connection("prod").unwrap().id.unwrap();
        assert_eq!(id.len(), 8, "id should be 8 characters, got: {id}");
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()), "id should be hex, got: {id}");
    }

    #[test]
    fn find_connection_by_name() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        let conn = svc.find_connection("prod").unwrap();
        assert_eq!(conn.name, "prod");
    }

    #[test]
    fn find_connection_by_id() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        let id = svc.get_connection("prod").unwrap().id.unwrap();
        let conn = svc.find_connection(&id).unwrap();
        assert_eq!(conn.name, "prod");
    }

    #[test]
    fn find_connection_returns_error_when_not_found() {
        let svc = service();
        let result = svc.find_connection("ghost");
        assert_eq!(result, Err("connection 'ghost' not found".to_string()));
    }
}
