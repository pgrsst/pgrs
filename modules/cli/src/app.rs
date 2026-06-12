use std::env;
use std::path::PathBuf;

use pgrs_core::Core;

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
        Some("shell") => run_shell(&core, &args[1..], &data_dir),
        Some("test") => run_test(&core, &args[1..]),
        _ => {
            let cli = Cli::new(core.connection);
            cli.run(args)
        }
    }
}

fn run_shell(core: &Core, args: &[String], data_dir: &std::path::Path) -> Result<(), String> {
    let name = args.first().ok_or("usage: pgrs shell <connection-name>")?;
    let conn = core.connection.find(name).map_err(|e| e.to_string())?;
    let query = core.connect(&conn)?;

    // Per-connection reedline line-history file (separate from the analytics
    // query_history table). Sanitized so the connection name is filesystem-safe.
    let history_path = data_dir.join(format!("history-{}", sanitize_filename(&conn.name)));

    Repl::new(
        query,
        &conn.database,
        &conn.name,
        conn.environment.as_deref(),
        core.analytics_api(),
        core.saved_query_api(),
        core.schema_api(),
        history_path,
    )
    .run()
}

/// Map a connection name to a filesystem-safe filename fragment: anything that
/// isn't an ASCII alphanumeric, `.`, `-`, or `_` becomes `_`.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .collect()
}

fn run_test(core: &Core, args: &[String]) -> Result<(), String> {
    let name = args.first().ok_or("usage: pgrs test <connection-name>")?;
    let conn = core.connection.find(name).map_err(|e| e.to_string())?;
    let conn_name = conn.name.clone();
    let query = core
        .connect(&conn)
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
    fn sanitize_filename_replaces_unsafe_chars() {
        assert_eq!(sanitize_filename("prod"), "prod");
        assert_eq!(sanitize_filename("my-db_1.2"), "my-db_1.2");
        assert_eq!(sanitize_filename("a/b c:d"), "a_b_c_d");
    }

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
