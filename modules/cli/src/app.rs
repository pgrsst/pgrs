use std::env;
use std::path::PathBuf;

use pgrs_core::{Core, QueryApi};

use crate::cli::Cli;
use crate::repl::Repl;

pub fn run() -> Result<(), String> {
    let data_dir = dirs::home_dir()
        .ok_or("could not determine home directory")?
        .join(".pgrs");

    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;

    let args: Vec<String> = env::args().skip(1).collect();
    run_with_dir(data_dir, args)
}

fn run_with_dir(data_dir: PathBuf, args: Vec<String>) -> Result<(), String> {
    let db_path = data_dir.join("pgrs.db");
    let db_path = db_path
        .to_str()
        .ok_or_else(|| format!("data directory path is not valid UTF-8: {}", db_path.display()))?;
    let core = Core::init(db_path).map_err(|e| format!("pgrs: {e}"))?;

    match args.first().map(String::as_str) {
        Some("shell") => run_shell(&core, &args[1..]),
        Some("test") => run_test(&core, &args[1..]),
        _ => {
            let cli = Cli::new(core.connection);
            cli.run(args)
        }
    }
}

fn run_shell(core: &Core, args: &[String]) -> Result<(), String> {
    let name = args.first().ok_or("usage: pgrs shell <connection-name>")?;
    let conn = core.connection.find(name).map_err(|e| e.to_string())?;
    let query = QueryApi::connect(&conn)?;

    Repl::new(
        query,
        &conn.database,
        &conn.name,
        conn.environment.as_deref(),
        core.analytics_api(),
        core.schema_api(),
    )
    .run()
}

fn run_test(core: &Core, args: &[String]) -> Result<(), String> {
    let name = args.first().ok_or("usage: pgrs test <connection-name>")?;
    let conn = core.connection.find(name).map_err(|e| e.to_string())?;
    let conn_name = conn.name.clone();
    let query = QueryApi::connect(&conn)
        .map_err(|e| format!("connection '{}' failed: {}", conn_name, e))?;
    query
        .execute("SELECT 1")
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
        )
        .unwrap_err();
        assert!(err.contains("not found"), "error should say not found, got: {err}");
    }

    #[test]
    fn run_with_dir_test_unknown_connection_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = run_with_dir(
            dir.path().to_path_buf(),
            vec!["test".to_string(), "ghost".to_string()],
        )
        .unwrap_err();
        assert!(err.contains("not found"), "error should say not found, got: {err}");
    }
}
