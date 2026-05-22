use std::env;

use crate::adapters::driven::file_connection_repository::FileConnectionRepository;
use crate::adapters::driven::postgres_db::PostgresDb;
use crate::adapters::driving::cli::Cli;
use crate::adapters::driving::repl;
use crate::core::services::connection::service::ConnectionService;

pub fn run() -> Result<(), String> {
    let data_dir = dirs::home_dir()
        .ok_or("could not determine home directory")?
        .join(".pgrs");

    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;

    let repository = FileConnectionRepository::new(data_dir.join("connections.json"));
    let connection_service = ConnectionService::new(repository);

    let args: Vec<String> = env::args().skip(1).collect();

    if args.first().map(String::as_str) == Some("shell") {
        let name = args.get(1).ok_or("usage: pgrs shell <connection-name>")?;
        let conn = connection_service.get_connection(name)?;
        let db = PostgresDb::new(&conn)?;
        return repl::run(Box::new(db), &conn.database);
    }

    let cli = Cli::new(connection_service);
    cli.run(args)
}
