//! Driven port for opening live database connections.
//!
//! Lets the application layer obtain a connection from a [`Connection`] config
//! without naming a concrete driver. The Postgres adapter implements this; the
//! composition root injects it, so `QueryApi`/`Core` never reference an adapter.

use crate::domain::connection::Connection;
use crate::domain::error::DomainError;
use crate::ports::repl_port::ReplPort;

pub trait DbConnector: Send + Sync {
    /// Open a live connection described by `connection`, returning a boxed
    /// [`ReplPort`] (query execution + schema/catalog reads).
    fn connect(&self, connection: &Connection) -> Result<Box<dyn ReplPort>, DomainError>;
}
