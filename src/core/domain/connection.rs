use crate::core::enums::tls_mode::TlsMode;

pub const DEFAULT_PORT: u16 = 5432;

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Connection {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub tls: TlsMode,
    pub environment: Option<String>,
    pub id: Option<String>,
}
