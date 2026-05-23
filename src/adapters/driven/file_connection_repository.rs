use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::core::domain::connection::Connection;
use crate::core::ports::connection_repository::ConnectionRepository;

struct LockGuard {
    path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

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

        let tmp_path = self.path.with_extension("tmp");
        fs::write(&tmp_path, &content).map_err(|error| error.to_string())?;
        fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
        fs::rename(&tmp_path, &self.path).map_err(|error| error.to_string())
    }

    // Acquires an exclusive lock file before calling f(), releases it after.
    // create_new is atomic on POSIX — only one process succeeds.
    // LockGuard ensures the lock file is removed even if f() panics.
    fn with_lock<F, T>(&self, f: F) -> Result<T, String>
    where
        F: FnOnce() -> Result<T, String>,
    {
        let lock_path = self.path.with_extension("lock");
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .map_err(|_| {
                format!(
                    "another pgrs process is running — if not, remove {:?} and try again",
                    lock_path
                )
            })?;
        let _guard = LockGuard { path: lock_path };
        f()
    }
}

impl ConnectionRepository for FileConnectionRepository {
    fn add(&self, connection: Connection) -> Result<(), String> {
        self.with_lock(|| {
            let mut connections = self.read_connections()?;

            if connections
                .iter()
                .any(|existing| existing.name == connection.name)
            {
                return Err(format!("connection '{}' already exists", connection.name));
            }

            connections.push(connection);
            self.write_connections(&connections)
        })
    }

    fn list(&self) -> Result<Vec<Connection>, String> {
        self.read_connections()
    }

    fn delete(&self, name: &str) -> Result<(), String> {
        self.with_lock(|| {
            let mut connections = self.read_connections()?;
            let initial_len = connections.len();

            connections.retain(|connection| connection.name != name);

            if connections.len() == initial_len {
                return Err(format!("connection '{}' not found", name));
            }

            self.write_connections(&connections)
        })
    }

    fn get_connection(&self, name: &str) -> Result<Connection, String> {
        let connections = self.read_connections()?;
        connections
            .into_iter()
            .find(|c| c.name == name)
            .ok_or_else(|| format!("connection '{}' not found", name))
    }

    fn update(&self, connection: Connection) -> Result<(), String> {
        self.with_lock(|| {
            let mut connections = self.read_connections()?;
            let pos = connections
                .iter()
                .position(|c| c.name == connection.name)
                .ok_or_else(|| format!("connection '{}' not found", connection.name))?;
            connections[pos] = connection;
            self.write_connections(&connections)
        })
    }

    fn rename(&self, old_name: &str, new_name: &str) -> Result<(), String> {
        self.with_lock(|| {
            let mut connections = self.read_connections()?;
            if connections.iter().any(|c| c.name == new_name) {
                return Err(format!("connection '{}' already exists", new_name));
            }
            let conn = connections
                .iter_mut()
                .find(|c| c.name == old_name)
                .ok_or_else(|| format!("connection '{}' not found", old_name))?;
            conn.name = new_name.to_string();
            self.write_connections(&connections)
        })
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
            tls: crate::core::domain::connection::TlsMode::Disable,
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

    #[test]
    fn get_connection_returns_connection_by_name() {
        let (repo, _dir) = repo();
        repo.add(sample_connection("prod")).unwrap();
        let conn = repo.get_connection("prod").unwrap();
        assert_eq!(conn.name, "prod");
        assert_eq!(conn.host, "localhost");
    }

    #[test]
    fn get_connection_returns_error_when_not_found() {
        let (repo, _dir) = repo();
        let result = repo.get_connection("nonexistent");
        assert_eq!(result, Err("connection 'nonexistent' not found".to_string()));
    }

    #[test]
    fn lock_file_is_removed_even_when_closure_panics() {
        use std::panic;
        let (repo, dir) = repo();
        let lock_path = dir.path().join("connections.lock");

        let _ = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            let _ = repo.with_lock(|| -> Result<(), String> {
                panic!("deliberate panic inside lock");
            });
        }));

        assert!(
            !lock_path.exists(),
            "lock file must be cleaned up after panic so future operations can proceed"
        );
    }

    #[test]
    fn lock_error_message_is_user_friendly() {
        let (repo, dir) = repo();
        let lock_path = dir.path().join("connections.lock");
        std::fs::write(&lock_path, b"").unwrap();

        let err = repo.add(sample_connection("prod")).unwrap_err();
        assert!(!err.contains("connections file is locked"), "old technical message should be gone, got: {err}");
        assert!(err.contains("pgrs"), "should mention pgrs, got: {err}");
    }

    #[test]
    fn lock_error_still_includes_path_for_recovery() {
        let (repo, dir) = repo();
        let lock_path = dir.path().join("connections.lock");
        std::fs::write(&lock_path, b"").unwrap();

        let err = repo.add(sample_connection("prod")).unwrap_err();
        assert!(err.contains("connections.lock"), "should include lock path so user can remove it, got: {err}");
    }

    #[test]
    fn update_changes_field_in_place() {
        let (repo, _dir) = repo();
        repo.add(sample_connection("prod")).unwrap();
        let mut updated = sample_connection("prod");
        updated.database = "newdb".to_string();
        repo.update(updated).unwrap();
        let conn = repo.get_connection("prod").unwrap();
        assert_eq!(conn.database, "newdb");
        assert_eq!(conn.host, "localhost"); // unchanged
    }

    #[test]
    fn update_returns_error_when_not_found() {
        let (repo, _dir) = repo();
        let result = repo.update(sample_connection("ghost"));
        assert_eq!(result, Err("connection 'ghost' not found".to_string()));
    }

    #[test]
    fn rename_updates_connection_name() {
        let (repo, _dir) = repo();
        repo.add(sample_connection("prod")).unwrap();
        repo.rename("prod", "production").unwrap();
        assert!(repo.get_connection("production").is_ok());
        assert_eq!(
            repo.get_connection("prod"),
            Err("connection 'prod' not found".to_string())
        );
    }

    #[test]
    fn rename_returns_error_when_not_found() {
        let (repo, _dir) = repo();
        let result = repo.rename("nonexistent", "new");
        assert_eq!(result, Err("connection 'nonexistent' not found".to_string()));
    }

    #[test]
    fn rename_returns_error_when_new_name_exists() {
        let (repo, _dir) = repo();
        repo.add(sample_connection("prod")).unwrap();
        repo.add(sample_connection("staging")).unwrap();
        let result = repo.rename("prod", "staging");
        assert_eq!(result, Err("connection 'staging' already exists".to_string()));
    }

    #[test]
    fn no_leftover_tmp_file_after_successful_write() {
        let (repo, dir) = repo();
        repo.add(sample_connection("prod")).unwrap();
        repo.add(sample_connection("staging")).unwrap();

        let tmp_path = dir.path().join("connections.tmp");
        assert!(!tmp_path.exists(), "tmp file should not remain after write completes");
    }
}
