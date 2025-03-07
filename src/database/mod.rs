use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::r2d2::ConnectionManager;
use diesel::{r2d2, PgConnection, SqliteConnection};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use log::{debug, info, trace};
use std::error::Error;
use std::time::Duration;

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

#[derive(Debug)]
pub struct ConnectionOptions {
    pub busy_timeout: Option<Duration>,
}

impl diesel::r2d2::CustomizeConnection<AnyConnection, diesel::r2d2::Error> for ConnectionOptions {
    fn on_acquire(&self, conn: &mut AnyConnection) -> Result<(), diesel::r2d2::Error> {
        (|| {
            if let AnyConnection::Sqlite(conn) = conn {
                if let Some(d) = self.busy_timeout {
                    conn.batch_execute(&format!("PRAGMA busy_timeout = {};", d.as_millis()))?;
                }
            }
            Ok(())
        })()
        .map_err(diesel::r2d2::Error::QueryError)
    }
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
    let pool = Pool::builder()
        .connection_customizer(Box::new(ConnectionOptions {
            busy_timeout: Some(Duration::from_secs(5)),
        }))
        .build(manager)?;

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
