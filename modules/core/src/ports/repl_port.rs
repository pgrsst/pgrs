use super::db_connection::DbConnection;
use super::schema_port::SchemaPort;

pub trait ReplPort: DbConnection + SchemaPort {}
impl<T: DbConnection + SchemaPort> ReplPort for T {}
