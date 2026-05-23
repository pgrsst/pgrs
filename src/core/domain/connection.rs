use serde::{Deserialize, Serialize};

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
        assert_eq!(conn.tls, TlsMode::Disable);
    }

    #[test]
    fn connection_with_tls_require_deserializes_correctly() {
        let json = r#"{"name":"prod","host":"localhost","port":5432,"username":"u","password":"p","database":"db","tls":"require"}"#;
        let conn: Connection = serde_json::from_str(json).unwrap();
        assert_eq!(conn.tls, TlsMode::Require);
    }
}
