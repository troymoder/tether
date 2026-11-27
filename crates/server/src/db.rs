//! tetherd's state, in one SQLite database. Large things such as packages,
//! icons and the emulators' disks stay in files.
//!
//! Whatever is kept for a device refers to the device's row and is deleted with
//! it. Most tables are small, so the module that works with one holds all of it
//! in memory and writes it back when it changes. The tables are created from
//! the entities when they are missing.

use std::path::Path;

use anyhow::Result;
use protocol::Last;
use sea_orm::sea_query::Expr;
use sea_orm::sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sea_orm::{ConnectionTrait, DatabaseConnection, Schema};
use tokio::sync::{Mutex, MutexGuard};

mod apps;
mod apps_auto;
mod clients;
mod devices;
mod emulators;
mod events;
mod instances;
mod places;
mod variants;

pub(crate) struct Db {
    connection: DatabaseConnection,
    writing: Mutex<()>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Known {
    pub name: String,
    pub given: Option<String>,
    /// None for an emulator that never connected.
    pub last: Option<Last>,
}

/// SQLite's rowid is the order rows were first added in, and a row that is
/// saved again keeps its place.
fn added() -> Expr {
    Expr::cust("rowid")
}

impl Db {
    pub(crate) async fn open(file: &Path) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(file)
            .create_if_missing(true);

        Self::set_up(options).await
    }

    #[cfg(test)]
    pub(crate) async fn in_memory() -> Self {
        Self::set_up("sqlite::memory:".parse().unwrap())
            .await
            .unwrap()
    }

    async fn set_up(options: SqliteConnectOptions) -> Result<Self> {
        // An in-memory database belongs to a single connection, and SQLite
        // writes one at a time anyway.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options.foreign_keys(true))
            .await?;
        let connection = DatabaseConnection::from(pool);
        let schema = Schema::new(connection.get_database_backend());
        let tables = [
            schema.create_table_from_entity(devices::Entity),
            schema.create_table_from_entity(emulators::Entity),
            schema.create_table_from_entity(apps::Entity),
            schema.create_table_from_entity(variants::Entity),
            schema.create_table_from_entity(apps_auto::Entity),
            schema.create_table_from_entity(places::Entity),
            schema.create_table_from_entity(events::Entity),
            schema.create_table_from_entity(instances::Entity),
            schema.create_table_from_entity(clients::Entity),
        ];

        for mut table in tables {
            connection.execute(table.if_not_exists()).await?;
        }

        Ok(Self {
            connection,
            writing: Mutex::new(()),
        })
    }

    /// Held from copying state out of memory until it is written, so that
    /// the last write carries the latest state.
    pub(crate) async fn writing(&self) -> MutexGuard<'_, ()> {
        self.writing.lock().await
    }
}
