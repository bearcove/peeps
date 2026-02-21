use std::path::{Path, PathBuf};

use rusqlite::Connection;

mod query;
mod schema;

pub use query::{fetch_scope_entity_links_blocking, query_named_blocking, sql_query_blocking};
pub use schema::{init_sqlite, load_next_connection_id};

#[derive(Debug, Clone)]
pub struct Db {
    path: PathBuf,
}

impl Db {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn open(&self) -> Result<Connection, String> {
        Connection::open(&self.path).map_err(|error| format!("open sqlite: {error}"))
    }
}
