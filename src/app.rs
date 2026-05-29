use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use crate::adapters::driven::postgres_db::PostgresDb;
use crate::adapters::driven::sqlite::SqliteRepository;
use crate::adapters::driving::cli::Cli;
use crate::adapters::driving::repl;
use crate::core::ports::column_access_repository::ColumnAccessRepository;
use crate::core::ports::connection_repository::ConnectionRepository;
use crate::core::ports::db_connection::DbConnection;
use crate::core::ports::query_history_repository::QueryHistoryRepository;
use crate::core::ports::schema_column_repository::SchemaColumnRepository;
use crate::core::ports::schema_table_repository::SchemaTableRepository;
use crate::core::ports::table_access_repository::TableAccessRepository;
use crate::core::services::analytics::service::{AnalyticsService, AnalyticsSvc};
use crate::core::services::column_access::service::{ColumnAccessService, ColumnAccessSvc};
use crate::core::services::connection::service::ConnectionService;
use crate::core::services::query_history::service::{QueryHistoryService, QueryHistorySvc};
use crate::core::services::schema_cache::service::{SchemaCacheService, SchemaCacheSvc};
use crate::core::services::schema_column::service::{SchemaColumnService, SchemaColumnSvc};
use crate::core::services::schema_table::service::{SchemaTableService, SchemaTableSvc};
use crate::core::services::table_access::service::{TableAccessService, TableAccessSvc};

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
    let sqlite = Arc::new(
        SqliteRepository::open(db_path.to_str().unwrap_or("pgrs.db"))
            .map_err(|e| format!("pgrs: could not open database: {e}"))?,
    );

    let connection_service = ConnectionService::new(Arc::clone(&sqlite) as Arc<dyn ConnectionRepository>);

    match args.first().map(String::as_str) {
        Some("shell") => {
            let connection_repo = Arc::clone(&sqlite) as Arc<dyn ConnectionRepository>;
            let query_history_svc = Arc::new(QueryHistoryService::new(
                Arc::clone(&connection_repo),
                Arc::clone(&sqlite) as Arc<dyn QueryHistoryRepository>,
            ));
            let table_access_svc = Arc::new(TableAccessService::new(
                Arc::clone(&connection_repo),
                Arc::clone(&sqlite) as Arc<dyn TableAccessRepository>,
            ));
            let column_access_svc = Arc::new(ColumnAccessService::new(
                Arc::clone(&connection_repo),
                Arc::clone(&sqlite) as Arc<dyn ColumnAccessRepository>,
            ));
            let analytics = Arc::new(AnalyticsService::new(
                Arc::clone(&query_history_svc) as Arc<dyn QueryHistorySvc>,
                Arc::clone(&table_access_svc) as Arc<dyn TableAccessSvc>,
                Arc::clone(&column_access_svc) as Arc<dyn ColumnAccessSvc>,
            ));
            let schema_table_svc = Arc::new(SchemaTableService::new(
                Arc::clone(&connection_repo),
                Arc::clone(&sqlite) as Arc<dyn SchemaTableRepository>,
            ));
            let schema_column_svc = Arc::new(SchemaColumnService::new(
                Arc::clone(&connection_repo),
                Arc::clone(&sqlite) as Arc<dyn SchemaColumnRepository>,
            ));
            let schema_cache = Arc::new(SchemaCacheService::new(
                Arc::clone(&schema_table_svc) as Arc<dyn SchemaTableSvc>,
                Arc::clone(&schema_column_svc) as Arc<dyn SchemaColumnSvc>,
            ));
            run_shell(
                &args[1..],
                &connection_service,
                Some(analytics as Arc<dyn AnalyticsSvc>),
                Some(schema_cache as Arc<dyn SchemaCacheSvc>),
            )
        }
        Some("test") => run_test(&args[1..], &connection_service),
        _ => {
            let cli = Cli::new(connection_service);
            cli.run(args)
        }
    }
}

fn run_shell(
    args: &[String],
    service: &ConnectionService,
    analytics: Option<Arc<dyn AnalyticsSvc>>,
    schema_cache: Option<Arc<dyn SchemaCacheSvc>>,
) -> Result<(), String> {
    let name = args.first().ok_or("usage: pgrs shell <connection-name>")?;
    let conn = service.find_connection(name)?;
    let db = PostgresDb::new(&conn)?;

    repl::run(
        Box::new(db),
        &conn.database,
        &conn.name,
        conn.environment.as_deref(),
        analytics,
        schema_cache,
    )
}

fn run_test(
    args: &[String],
    service: &ConnectionService,
) -> Result<(), String> {
    let name = args.first().ok_or("usage: pgrs test <connection-name>")?;
    let conn = service.find_connection(name)?;
    let conn_name = conn.name.clone();
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
