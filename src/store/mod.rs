//! SQLite repositories. Recording an attempt and changing review membership are atomic.
mod attempts;
mod puzzles;
mod schema;
mod sets;
#[cfg(test)]
mod tests;
pub use attempts::RecordedAttempt;
use rusqlite::Connection;
pub use sets::{CycleRecord, StoredSet};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Database schema version {0} is newer than this app supports.")]
    NewerSchema(u32),
    #[error("Invalid stored or requested data: {0}")]
    InvalidData(String),
    #[error("The requested record does not exist.")]
    NotFound,
    #[error(transparent)]
    Cycle(#[from] crate::stats::CycleError),
}
pub type Result<T> = std::result::Result<T, StoreError>;
pub struct Store {
    connection: Connection,
}
impl Store {
    pub fn memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        Self::from_connection(connection)
    }
    pub fn from_connection(mut connection: Connection) -> Result<Self> {
        schema::migrate(&mut connection)?;
        Ok(Self { connection })
    }
    pub fn schema_version(&self) -> Result<u32> {
        Ok(self
            .connection
            .pragma_query_value(None, "user_version", |r| r.get(0))?)
    }
    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        use rusqlite::OptionalExtension;
        Ok(self
            .connection
            .query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.connection.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[key,value])?;
        Ok(())
    }
    pub fn settings(&self) -> Result<crate::settings::Settings> {
        self.setting("preferences")?.map_or_else(
            || Ok(crate::settings::Settings::default()),
            |s| Ok(serde_json::from_str(&s)?),
        )
    }
    pub fn save_settings(&self, settings: &crate::settings::Settings) -> Result<()> {
        if !settings.validate() {
            return Err(StoreError::InvalidData(
                "Puzzle rating must be 400–3000.".into(),
            ));
        }
        self.set_setting("preferences", &serde_json::to_string(settings)?)
    }
}
/// Platform data discovery for the application. Directory creation happens when opening the DB.
pub fn database_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "woodpeck").map(|d| d.data_dir().join("woodpeck.db"))
}
/// Explicit XDG/HOME variant for tests and portable installations.
pub fn database_path_for(home: &Path, xdg_data_home: Option<&Path>) -> PathBuf {
    xdg_data_home
        .filter(|p| p.is_absolute())
        .map_or_else(|| home.join(".local/share"), Path::to_path_buf)
        .join("woodpeck/woodpeck.db")
}

fn unsigned(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(index)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
}
