use std::env;

use crate::adapters::driven::file_connection_repository::FileConnectionRepository;
use crate::adapters::driving::cli::Cli;
use crate::core::services::connection::service::ConnectionService;

pub fn run() -> Result<(), String> {
    let data_dir = dirs::home_dir()
        .ok_or("could not determine home directory")?
        .join(".pgrs");

    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;

    let repository = FileConnectionRepository::new(data_dir.join("connections.json"));
    let connection_service = ConnectionService::new(repository);
    let cli = Cli::new(connection_service);

    cli.run(env::args().skip(1))
}
