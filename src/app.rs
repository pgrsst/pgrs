use std::env;

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

    let repository = FileConnectionRepository::new(data_dir.join("connections.json"));
    let connection_service = ConnectionService::new(repository);

    let args: Vec<String> = env::args().skip(1).collect();

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
    let conn = service.get_connection(name)?;
    let db = PostgresDb::new(&conn)?;
    repl::run(Box::new(db), &conn.database)
}

fn run_test<R: ConnectionRepository>(
    args: &[String],
    service: &ConnectionService<R>,
) -> Result<(), String> {
    let name = args.first().ok_or("usage: pgrs test <connection-name>")?;
    let conn = service.get_connection(name)?;
    let db = PostgresDb::new(&conn)
        .map_err(|e| format!("connection '{}' failed: {}", name, e))?;
    db.execute("SELECT 1")
        .map_err(|e| format!("connection '{}' failed: {}", name, e))?;
    println!("connection '{}' ok", name);
    Ok(())
}
