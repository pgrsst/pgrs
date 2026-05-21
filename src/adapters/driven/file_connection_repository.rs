use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::core::domain::connection::Connection;
use crate::core::ports::connection_repository::ConnectionRepository;

pub struct FileConnectionRepository {
    path: PathBuf,
}

impl FileConnectionRepository {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    fn read_connections(&self) -> Result<Vec<Connection>, String> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&self.path).map_err(|error| error.to_string())?;

        if content.trim().is_empty() {
            return Ok(Vec::new());
        }

        serde_json::from_str(&content).map_err(|error| error.to_string())
    }

    fn write_connections(&self, connections: &[Connection]) -> Result<(), String> {
        let content =
            serde_json::to_string_pretty(connections).map_err(|error| error.to_string())?;

        fs::write(&self.path, content).map_err(|error| error.to_string())?;
        fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())
    }
}

impl ConnectionRepository for FileConnectionRepository {
    fn add(&self, connection: Connection) -> Result<(), String> {
        let mut connections = self.read_connections()?;

        if connections
            .iter()
            .any(|existing| existing.name == connection.name)
        {
            return Err(format!("connection '{}' already exists", connection.name));
        }

        connections.push(connection);
        self.write_connections(&connections)
    }

    fn list(&self) -> Result<Vec<Connection>, String> {
        self.read_connections()
    }

    fn delete(&self, name: &str) -> Result<(), String> {
        let mut connections = self.read_connections()?;
        let initial_len = connections.len();

        connections.retain(|connection| connection.name != name);

        if connections.len() == initial_len {
            return Err(format!("connection '{}' not found", name));
        }

        self.write_connections(&connections)
    }
}
