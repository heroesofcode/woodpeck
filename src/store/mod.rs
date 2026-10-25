//! SQLite repositories. Recording an attempt and changing review membership are atomic.
mod attempts;
mod library;
mod puzzles;
mod schema;
mod sets;
mod sync;
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
        ensure_external_data_dir(path)?;
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
    pub(crate) fn immediate_transaction(&self) -> Result<rusqlite::Transaction<'_>> {
        Ok(rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?)
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
                "Puzzle rating must be 400–3000 and sound volume 0–100%.".into(),
            ));
        }
        let tx = self.connection.unchecked_transaction()?;
        let previous = serde_json::to_value(self.settings()?)?;
        let values = serde_json::to_value(settings)?;
        for (key, value) in values
            .as_object()
            .ok_or_else(|| StoreError::InvalidData("Invalid preferences.".into()))?
        {
            if key != "sound_volume" && previous.get(key) != Some(value) {
                tx.execute("INSERT INTO sync_fields(key,value,updated_at) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=max(sync_fields.updated_at+1,excluded.updated_at)",rusqlite::params![key,value.to_string(),chrono::Utc::now().timestamp_millis()])?;
            }
        }
        self.set_setting("preferences", &serde_json::to_string(settings)?)?;
        tx.execute("UPDATE sync_changes SET generation=generation+1", [])?;
        tx.commit()?;
        Ok(())
    }
}
/// Platform data discovery for the application. Directory creation happens when opening the DB.
pub fn database_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("WOODPECK_DATA_DIR") {
        return Some(PathBuf::from(path).join("woodpeck.db"));
    }
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

/// Refuse databases and sync state inside the public application's source checkout.
pub fn ensure_external_data_dir(path: &Path) -> Result<()> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut ancestor = absolute.as_path();
    let mut suffix = Vec::new();
    while !ancestor.exists() {
        if let Some(name) = ancestor.file_name() {
            suffix.push(name.to_os_string());
        }
        ancestor = ancestor
            .parent()
            .ok_or_else(|| StoreError::InvalidData("Invalid data directory.".into()))?;
    }
    let mut resolved = ancestor.canonicalize()?;
    for component in suffix.into_iter().rev() {
        resolved.push(component);
    }
    let mut normalized = PathBuf::new();
    for component in resolved.components() {
        match component {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    let resolved = normalized;
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = source_path
        .canonicalize()
        .unwrap_or_else(|_| source_path.to_path_buf());
    if resolved.starts_with(source) {
        return Err(StoreError::InvalidData(
            "The data directory must be outside the woodpeck source tree.".into(),
        ));
    }
    Ok(())
}
