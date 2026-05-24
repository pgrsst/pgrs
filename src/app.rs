use std::env;
use std::path::PathBuf;

use crate::adapters::driven::file_connection_repository::FileConnectionRepository;
use crate::adapters::driven::postgres_db::PostgresDb;
use crate::adapters::driving::cli::Cli;
use crate::adapters::driving::repl;
use crate::core::ports::connection_repository::ConnectionRepository;
use crate::core::ports::db_connection::DbConnection;
use crate::core::services::connection::service::ConnectionService;

pub fn run() -> Result<(), String> {
    let data_dir = dirs::home_dir()
        .ok_or("could not determine home directory")?
        .join(".pgrs");

    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;

    let args: Vec<String> = env::args().skip(1).collect();
    run_with_dir(data_dir, args)
}

fn run_with_dir(data_dir: PathBuf, args: Vec<String>) -> Result<(), String> {
    let repository = FileConnectionRepository::new(data_dir.join("connections.json"));
    let connection_service = ConnectionService::new(repository);

    match args.first().map(String::as_str) {
        Some("shell") => run_shell(&args[1..], &connection_service),
        Some("test") => run_test(&args[1..], &connection_service),
        _ => {
            let cli = Cli::new(connection_service);
            cli.run(args)
        }
    }
}

fn run_shell<R: ConnectionRepository>(
    args: &[String],
    service: &ConnectionService<R>,
) -> Result<(), String> {
    let name = args.first().ok_or("usage: pgrs shell <connection-name>")?;
    let conn = service.find_connection(name)?;
    let db = PostgresDb::new(&conn)?;
    repl::run(Box::new(db), &conn.database, conn.environment.as_deref())
}

fn run_test<R: ConnectionRepository>(
    args: &[String],
    service: &ConnectionService<R>,
) -> Result<(), String> {
    let name = args.first().ok_or("usage: pgrs test <connection-name>")?;
    let conn = service.find_connection(name)?;
    let conn_name = &conn.name;
    let db = PostgresDb::new(&conn)
        .map_err(|e| format!("connection '{}' failed: {}", conn_name, e))?;
    db.execute("SELECT 1")
        .map_err(|e| format!("connection '{}' failed: {}", conn_name, e))?;
    println!("connection '{}' ok", conn_name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_with_dir_no_args_returns_ok() {
        let dir = tempfile::tempdir().unwrap();
        assert!(run_with_dir(dir.path().to_path_buf(), vec![]).is_ok());
    }

    #[test]
    fn run_with_dir_unknown_command_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = run_with_dir(dir.path().to_path_buf(), vec!["badcmd".to_string()]).unwrap_err();
        assert!(err.contains("badcmd"), "error should mention the unknown command, got: {err}");
    }

    #[test]
    fn run_with_dir_shell_without_name_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = run_with_dir(dir.path().to_path_buf(), vec!["shell".to_string()]).unwrap_err();
        assert!(err.contains("usage"), "error should show usage hint, got: {err}");
    }

    #[test]
    fn run_with_dir_test_without_name_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = run_with_dir(dir.path().to_path_buf(), vec!["test".to_string()]).unwrap_err();
        assert!(err.contains("usage"), "error should show usage hint, got: {err}");
    }

    #[test]
    fn run_with_dir_shell_unknown_connection_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = run_with_dir(
            dir.path().to_path_buf(),
            vec!["shell".to_string(), "ghost".to_string()],
        ).unwrap_err();
        assert!(err.contains("not found"), "error should say not found, got: {err}");
    }

    #[test]
    fn run_with_dir_test_unknown_connection_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = run_with_dir(
            dir.path().to_path_buf(),
            vec!["test".to_string(), "ghost".to_string()],
        ).unwrap_err();
        assert!(err.contains("not found"), "error should say not found, got: {err}");
    }
}
