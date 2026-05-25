use serde::{Deserialize, Serialize};

use crate::core::domain::error::DomainError;

pub const DEFAULT_PORT: u16 = 5432;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TlsMode {
    #[default]
    Disable,
    Require,
    VerifyFull,
}

impl std::fmt::Display for TlsMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TlsMode::Disable => write!(f, "disable"),
            TlsMode::Require => write!(f, "require"),
            TlsMode::VerifyFull => write!(f, "verify-full"),
        }
    }
}

// Shadow struct used only for serde serialization/deserialization.
#[derive(Serialize, Deserialize)]
struct ConnectionSerde {
    name: String,
    host: String,
    port: u16,
    username: String,
    password: String,
    database: String,
    #[serde(default)]
    tls: TlsMode,
    #[serde(default)]
    environment: Option<String>,
    #[serde(default)]
    id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ConnectionSerde", into = "ConnectionSerde")]
pub struct Connection {
    name: String,
    host: String,
    port: u16,
    username: String,
    password: String,
    database: String,
    tls: TlsMode,
    environment: Option<String>,
    id: Option<String>,
}

impl TryFrom<ConnectionSerde> for Connection {
    type Error = DomainError;

    fn try_from(data: ConnectionSerde) -> Result<Self, Self::Error> {
        let mut conn = Connection::new(
            data.name,
            data.host,
            data.port,
            data.username,
            data.password,
            data.database,
            data.tls,
            data.environment,
        )?;
        conn.id = data.id;
        Ok(conn)
    }
}

impl From<Connection> for ConnectionSerde {
    fn from(c: Connection) -> Self {
        ConnectionSerde {
            name: c.name,
            host: c.host,
            port: c.port,
            username: c.username,
            password: c.password,
            database: c.database,
            tls: c.tls,
            environment: c.environment,
            id: c.id,
        }
    }
}

fn require_not_empty(label: &str, value: &str) -> Result<(), DomainError> {
    if value.trim().is_empty() {
        Err(DomainError::ValidationError(format!("{label} is required")))
    } else {
        Ok(())
    }
}

impl Connection {
    /// Create a validated connection. Returns an error if any required field is empty.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        host: String,
        port: u16,
        username: String,
        password: String,
        database: String,
        tls: TlsMode,
        environment: Option<String>,
    ) -> Result<Self, DomainError> {
        require_not_empty("connection name", &name)?;
        require_not_empty("host", &host)?;
        require_not_empty("username", &username)?;
        require_not_empty("password", &password)?;
        require_not_empty("database", &database)?;
        Ok(Self { name, host, port, username, password, database, tls, environment, id: None })
    }

    /// Construct directly from trusted storage (DB rows, JSON files) — skips validation.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_storage(
        name: String,
        host: String,
        port: u16,
        username: String,
        password: String,
        database: String,
        tls: TlsMode,
        environment: Option<String>,
        id: Option<String>,
    ) -> Self {
        Self { name, host, port, username, password, database, tls, environment, id }
    }

    // --- Getters ---

    pub fn name(&self) -> &str { &self.name }
    pub fn host(&self) -> &str { &self.host }
    pub fn port(&self) -> u16 { self.port }
    pub fn username(&self) -> &str { &self.username }
    pub fn password(&self) -> &str { &self.password }
    pub fn database(&self) -> &str { &self.database }
    pub fn tls(&self) -> &TlsMode { &self.tls }
    pub fn environment(&self) -> Option<&str> { self.environment.as_deref() }
    pub fn id(&self) -> Option<&str> { self.id.as_deref() }

    // --- Setters (crate-internal, used by services and adapters) ---

    pub(crate) fn set_name(&mut self, v: String) { self.name = v; }
    pub(crate) fn set_host(&mut self, v: String) { self.host = v; }
    pub(crate) fn set_port(&mut self, v: u16) { self.port = v; }
    pub(crate) fn set_username(&mut self, v: String) { self.username = v; }
    pub(crate) fn set_password(&mut self, v: String) { self.password = v; }
    pub(crate) fn set_database(&mut self, v: String) { self.database = v; }
    pub(crate) fn set_tls(&mut self, v: TlsMode) { self.tls = v; }
    pub(crate) fn set_environment(&mut self, v: Option<String>) { self.environment = v; }
    pub(crate) fn set_id(&mut self, v: String) { self.id = Some(v); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tls_mode_displays_disable() {
        assert_eq!(TlsMode::Disable.to_string(), "disable");
    }

    #[test]
    fn tls_mode_displays_require() {
        assert_eq!(TlsMode::Require.to_string(), "require");
    }

    #[test]
    fn tls_mode_displays_verify_full() {
        assert_eq!(TlsMode::VerifyFull.to_string(), "verify-full");
    }

    #[test]
    fn connection_without_tls_field_deserializes_to_disable() {
        let json = r#"{"name":"prod","host":"localhost","port":5432,"username":"u","password":"p","database":"db"}"#;
        let conn: Connection = serde_json::from_str(json).unwrap();
        assert_eq!(conn.tls(), &TlsMode::Disable);
    }

    #[test]
    fn connection_with_tls_require_deserializes_correctly() {
        let json = r#"{"name":"prod","host":"localhost","port":5432,"username":"u","password":"p","database":"db","tls":"require"}"#;
        let conn: Connection = serde_json::from_str(json).unwrap();
        assert_eq!(conn.tls(), &TlsMode::Require);
    }

    #[test]
    fn connection_without_environment_field_deserializes_to_none() {
        let json = r#"{"name":"prod","host":"localhost","port":5432,"username":"u","password":"p","database":"db"}"#;
        let conn: Connection = serde_json::from_str(json).unwrap();
        assert_eq!(conn.environment(), None);
    }

    #[test]
    fn connection_with_environment_deserializes_correctly() {
        let json = r#"{"name":"prod","host":"localhost","port":5432,"username":"u","password":"p","database":"db","environment":"production"}"#;
        let conn: Connection = serde_json::from_str(json).unwrap();
        assert_eq!(conn.environment(), Some("production"));
    }

    #[test]
    fn connection_without_id_field_deserializes_to_none() {
        let json = r#"{"name":"prod","host":"localhost","port":5432,"username":"u","password":"p","database":"db"}"#;
        let conn: Connection = serde_json::from_str(json).unwrap();
        assert_eq!(conn.id(), None);
    }

    #[test]
    fn connection_with_id_deserializes_correctly() {
        let json = r#"{"name":"prod","host":"localhost","port":5432,"username":"u","password":"p","database":"db","id":"a3f9c2d1"}"#;
        let conn: Connection = serde_json::from_str(json).unwrap();
        assert_eq!(conn.id(), Some("a3f9c2d1"));
    }

    #[test]
    fn connection_new_rejects_empty_name() {
        let err = Connection::new("".to_string(), "h".to_string(), 5432, "u".to_string(), "p".to_string(), "db".to_string(), TlsMode::Disable, None);
        assert!(matches!(err, Err(DomainError::ValidationError(_))));
    }

    #[test]
    fn connection_new_rejects_blank_host() {
        let err = Connection::new("n".to_string(), "  ".to_string(), 5432, "u".to_string(), "p".to_string(), "db".to_string(), TlsMode::Disable, None);
        assert!(matches!(err, Err(DomainError::ValidationError(_))));
    }

    #[test]
    fn connection_new_succeeds_with_valid_fields() {
        let conn = Connection::new("prod".to_string(), "localhost".to_string(), 5432, "u".to_string(), "p".to_string(), "db".to_string(), TlsMode::Disable, None);
        assert!(conn.is_ok());
    }
}
