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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ports::connection_repository::ConnectionRepository;
    use std::os::unix::fs::PermissionsExt;

    fn sample_connection(name: &str) -> Connection {
        Connection {
            name: name.to_string(),
            host: "localhost".to_string(),
            port: 5432,
            username: "admin".to_string(),
            password: "secret".to_string(),
            database: "mydb".to_string(),
        }
    }

    fn repo() -> (FileConnectionRepository, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let repo = FileConnectionRepository::new(dir.path().join("connections.json"));
        (repo, dir)
    }

    #[test]
    fn list_returns_empty_when_file_does_not_exist() {
        let (repo, _dir) = repo();
        assert!(repo.list().unwrap().is_empty());
    }

    #[test]
    fn add_persists_connection() {
        let (repo, _dir) = repo();
        repo.add(sample_connection("prod")).unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "prod");
    }

    #[test]
    fn add_returns_error_on_duplicate_name() {
        let (repo, _dir) = repo();
        repo.add(sample_connection("prod")).unwrap();
        let result = repo.add(sample_connection("prod"));
        assert_eq!(result, Err("connection 'prod' already exists".to_string()));
    }

    #[test]
    fn list_returns_all_connections() {
        let (repo, _dir) = repo();
        repo.add(sample_connection("prod")).unwrap();
        repo.add(sample_connection("staging")).unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn delete_removes_connection() {
        let (repo, _dir) = repo();
        repo.add(sample_connection("prod")).unwrap();
        repo.delete("prod").unwrap();
        assert!(repo.list().unwrap().is_empty());
    }

    #[test]
    fn delete_returns_error_when_not_found() {
        let (repo, _dir) = repo();
        let result = repo.delete("nonexistent");
        assert_eq!(result, Err("connection 'nonexistent' not found".to_string()));
    }

    #[test]
    fn write_sets_file_permissions_to_0600() {
        let (repo, dir) = repo();
        repo.add(sample_connection("prod")).unwrap();
        let mode = std::fs::metadata(dir.path().join("connections.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
