//! Persistance de silo (utilisateurs, sessions) dans une base SQLite
//! embarquée. Le schéma et les formats de date sont ceux de la version Go :
//! une base existante est reprise telle quelle.

mod sessions;
mod users;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use rusqlite::Connection;
use time::OffsetDateTime;
use time::macros::format_description;

pub use users::User;

/// Erreur de la couche de persistance.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("store: ressource introuvable")]
    NotFound,
    #[error("store: ressource déjà existante")]
    AlreadyExists,
    #[error("store: création du dossier de données {path}: {source}")]
    DataDir {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("store: {context}: {source}")]
    Db {
        context: &'static str,
        #[source]
        source: rusqlite::Error,
    },
}

fn db_err(context: &'static str) -> impl FnOnce(rusqlite::Error) -> StoreError {
    move |source| StoreError::Db { context, source }
}

/// Connexion à la base SQLite et opérations de persistance.
pub struct Store {
    // Une connexion unique sérialise les écritures, que SQLite gère mal en
    // concurrence ; les requêtes sont assez brèves pour que cela suffise.
    conn: Mutex<Connection>,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS users (
	id         INTEGER PRIMARY KEY AUTOINCREMENT,
	username   TEXT NOT NULL UNIQUE,
	password_hash TEXT NOT NULL,
	role       TEXT NOT NULL CHECK (role IN ('admin', 'user')),
	created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS sessions (
	token_hash TEXT PRIMARY KEY,
	user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
	created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
	expires_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at);
";

impl Store {
    /// Ouvre (ou crée) la base `silo.db` dans `data_dir` et applique le schéma.
    pub fn open(data_dir: &str) -> Result<Self, StoreError> {
        create_data_dir(Path::new(data_dir)).map_err(|source| StoreError::DataDir {
            path: data_dir.to_string(),
            source,
        })?;

        let conn = Connection::open(Path::new(data_dir).join("silo.db"))
            .map_err(db_err("ouverture sqlite"))?;
        // busy_timeout : la sous-commande reset-password peut écrire pendant
        // que le service tourne, sans qu'il faille l'arrêter.
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(db_err("configuration sqlite"))?;
        conn.pragma_update(None, "foreign_keys", true)
            .map_err(db_err("configuration sqlite"))?;
        conn.execute_batch(SCHEMA).map_err(db_err("migration"))?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        // Une connexion SQLite reste cohérente même si un appelant a paniqué
        // en la tenant : chaque requête est atomique côté SQLite.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(unix)]
fn create_data_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o750)
        .create(path)
}

#[cfg(not(unix))]
fn create_data_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)
}

/// Formate un instant comme le `time.RFC3339` de Go en UTC
/// (`2006-01-02T15:04:05Z`). Les sessions sont comparées en tant que
/// chaînes : ce format doit rester strictement identique à celui des
/// sessions déjà en base.
fn format_utc(t: OffsetDateTime) -> String {
    t.to_offset(time::UtcOffset::UTC)
        .format(format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second]Z"
        ))
        .expect("format de date statique valide")
}

#[cfg(test)]
pub(crate) fn test_store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_str().unwrap()).unwrap();
    (store, dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_utc_format_go_rfc3339_sans_fraction() {
        let t = time::macros::datetime!(2026-01-02 03:04:05.678 +02:00);
        assert_eq!(format_utc(t), "2026-01-02T01:04:05Z");
    }
}
