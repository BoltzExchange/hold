use diesel::prelude::*;
use diesel::r2d2::ConnectionManager;
use diesel::{r2d2, PgConnection, SqliteConnection};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use log::{debug, info, trace};
use std::error::Error;

pub mod helpers;
pub mod model;

mod schema;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

#[derive(diesel::MultiConnection)]
pub enum AnyConnection {
    Postgresql(PgConnection),
    Sqlite(SqliteConnection),
}

pub type Pool = r2d2::Pool<ConnectionManager<AnyConnection>>;

// TODO: postgres migrations
pub fn connect(url: &str) -> Result<Pool, Box<dyn Error + Send + Sync>> {
    debug!("Connecting to database");
    let manager: ConnectionManager<AnyConnection> = ConnectionManager::new(url);
    let pool = Pool::builder().build(manager)?;

    info!("Connected to database");

    debug!("Running migrations");
    let mut con = pool.get()?;
    con.run_pending_migrations(MIGRATIONS)?;

    trace!("Ran migrations");

    Ok(pool)
}
