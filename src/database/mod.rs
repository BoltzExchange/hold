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
pub const MIGRATIONS_POSTGRES: EmbeddedMigrations = embed_migrations!("./migrations_postgres");

#[derive(diesel::MultiConnection)]
pub enum AnyConnection {
    Postgresql(PgConnection),
    Sqlite(SqliteConnection),
}

pub type Pool = r2d2::Pool<ConnectionManager<AnyConnection>>;

pub fn connect(url: &str) -> Result<Pool, Box<dyn Error + Send + Sync>> {
    let db_name = if is_postgres_connection_url(url) {
        "PostgreSQL"
    } else {
        "SQLite"
    };

    debug!("Connecting to {} database", db_name);
    let manager: ConnectionManager<AnyConnection> = ConnectionManager::new(url);
    let pool = Pool::builder().build(manager)?;

    info!("Connected to {} database", db_name);

    debug!("Running migrations");
    let mut con = pool.get()?;
    con.run_pending_migrations(if is_postgres_connection_url(url) {
        MIGRATIONS_POSTGRES
    } else {
        MIGRATIONS
    })?;

    trace!("Ran migrations");

    Ok(pool)
}

fn is_postgres_connection_url(url: &str) -> bool {
    url.to_lowercase().starts_with("postgresql")
}
