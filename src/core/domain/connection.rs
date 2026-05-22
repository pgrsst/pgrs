use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TlsMode {
    #[default]
    Disable,
    Require,
    VerifyFull,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Connection {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    #[serde(default)]
    pub tls: TlsMode,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_without_tls_field_deserializes_to_disable() {
        let json = r#"{"name":"prod","host":"localhost","port":5432,"username":"u","password":"p","database":"db"}"#;
        let conn: Connection = serde_json::from_str(json).unwrap();
        assert_eq!(conn.tls, TlsMode::Disable);
    }

    #[test]
    fn connection_with_tls_require_deserializes_correctly() {
        let json = r#"{"name":"prod","host":"localhost","port":5432,"username":"u","password":"p","database":"db","tls":"require"}"#;
        let conn: Connection = serde_json::from_str(json).unwrap();
        assert_eq!(conn.tls, TlsMode::Require);
    }
}
