//! Opt-in progress synchronization. The portable format is independent of Git and SQLite.
pub mod format;
mod git;
pub mod merge;
mod process;
pub mod worker;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyncError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Store(#[from] crate::store::StoreError),
    #[error("Sync schema version {0} is newer than supported; update woodpeck before syncing.")]
    NewerSchema(u32),
    #[error("Invalid sync data: {0}")]
    Invalid(String),
    #[error("{0}")]
    Git(String),
    #[error("Sync network command failed or timed out. Check connectivity and Git authentication.")]
    Offline,
}
pub type Result<T> = std::result::Result<T, SyncError>;

use crate::store::Store;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    #[default]
    Disabled,
    Synced,
    Pending,
    Syncing,
    Offline,
    Merged,
    Error,
}
impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Self::Disabled => "Disabled",
            Self::Synced => "Synced",
            Self::Pending => "Pending",
            Self::Syncing => "Syncing",
            Self::Offline => "Offline",
            Self::Merged => "Conflict resolved",
            Self::Error => "Error",
        }
    }
}
/// Local only. This value is never part of the portable snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub remote: String,
    pub enabled: bool,
    pub pending: bool,
    pub last_sync: Option<i64>,
    pub last_error: Option<String>,
    pub status: Status,
    pub synced_generation: i64,
}
pub fn config(store: &Store) -> Result<Config> {
    let mut config: Config = store
        .setting("sync_config")?
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or_default();
    if config.enabled && store.sync_generation()? != config.synced_generation {
        config.pending = true;
        if matches!(config.status, Status::Synced | Status::Merged) {
            config.status = Status::Pending;
        }
    }
    Ok(config)
}
/// Hold the write lock from the read through the update, including across connections.
fn update_config(store: &Store, update: impl FnOnce(&mut Config) -> Result<()>) -> Result<Config> {
    let transaction = store.immediate_transaction()?;
    let mut cfg = config(store)?;
    update(&mut cfg)?;
    store.set_setting("sync_config", &serde_json::to_string(&cfg)?)?;
    transaction
        .commit()
        .map_err(crate::store::StoreError::from)?;
    Ok(cfg)
}
/// Worker snapshots own runtime fields only; discard results for obsolete settings.
fn save_runtime(store: &Store, runtime: &Config) -> Result<Config> {
    update_config(store, |latest| {
        if latest.enabled && latest.remote == runtime.remote {
            latest.pending = runtime.pending;
            latest.status = runtime.status;
            latest.last_error.clone_from(&runtime.last_error);
            latest.last_sync = runtime.last_sync;
            latest.synced_generation = runtime.synced_generation;
        }
        Ok(())
    })
}
pub fn configure(store: &Store, remote: &str, enabled: bool) -> Result<Config> {
    if enabled || !remote.is_empty() {
        git::validate_remote(remote)?;
    }
    update_config(store, |cfg| {
        if cfg.remote != remote {
            cfg.synced_generation = -1;
            cfg.last_sync = None;
        }
        cfg.remote = remote.into();
        cfg.enabled = enabled;
        cfg.pending = enabled;
        cfg.last_error = None;
        cfg.status = if enabled {
            Status::Pending
        } else {
            Status::Disabled
        };
        Ok(())
    })
}
pub fn init(store: &mut Store, data: &Path, remote: Option<&str>) -> Result<Config> {
    crate::store::ensure_external_data_dir(data)?;
    let url = remote.map_or_else(|| git::create_remote(data), |s| Ok(s.to_owned()))?;
    configure(store, &url, true)?;
    synchronize(store, data, Duration::from_secs(45))
}
struct Lease(std::path::PathBuf);
impl Lease {
    fn acquire(data: &Path) -> Result<Self> {
        let path = data.join("sync.lock");
        if path.is_file()
            && fs::metadata(&path)?
                .modified()?
                .elapsed()
                .unwrap_or_default()
                > Duration::from_secs(120)
        {
            fs::remove_file(&path)?;
        }
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    SyncError::Git("Another sync is running; it will be retried.".into())
                } else {
                    SyncError::Io(e)
                }
            })?;
        Ok(Self(path))
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
/// Blocking API for CLI/tests and the background worker; never call from the UI thread.
pub fn synchronize(store: &mut Store, data: &Path, budget: Duration) -> Result<Config> {
    let deadline = Instant::now() + budget;
    let mut started = false;
    let cfg = update_config(store, |cfg| {
        if !cfg.enabled {
            return Err(SyncError::Invalid(
                "Sync is disabled. Use sync init or enable it in Settings.".into(),
            ));
        }
        // A close request can expire while waiting for an in-flight sync. There
        // is nothing to attempt in that case, and no failure to record.
        if Instant::now() < deadline {
            cfg.pending = store.sync_generation()? != cfg.synced_generation;
            cfg.status = Status::Syncing;
            started = true;
        }
        Ok(())
    })?;
    if !started {
        return Ok(cfg);
    }
    let result = synchronize_inner(store, data, &cfg.remote, deadline);
    // Preserve configuration edited while network commands were running.
    let mut latest = config(store)?;
    if latest.remote != cfg.remote || !latest.enabled {
        return Ok(latest);
    }
    match &result {
        Ok((generation, merged)) => {
            latest.synced_generation = *generation;
            latest.pending = store.sync_generation()? != *generation;
            latest.status = if latest.pending {
                Status::Pending
            } else if *merged {
                Status::Merged
            } else {
                Status::Synced
            };
            latest.last_sync = Some(chrono::Utc::now().timestamp());
            latest.last_error = None;
        }
        Err(error) => {
            latest.pending = store.sync_generation()? != latest.synced_generation;
            latest.status = if matches!(error, SyncError::Offline) {
                Status::Offline
            } else {
                Status::Error
            };
            latest.last_error = Some(error.to_string());
        }
    }
    let latest = save_runtime(store, &latest)?;
    result.map(|_| latest)
}
fn synchronize_inner(
    store: &mut Store,
    data: &Path,
    remote: &str,
    deadline: Instant,
) -> Result<(i64, bool)> {
    crate::store::ensure_external_data_dir(data)?;
    let _lease = Lease::acquire(data)?;
    let repo = git::Repository::open(data, remote, deadline)?;
    let mut changed = false;
    for _ in 0..3 {
        let (branch, head) = repo.fetch()?;
        if let Some(head) = &head {
            let remote = repo.read(head)?;
            let before = store.export_sync()?;
            let merged = store.import_sync(&remote)?;
            changed |= merged != before;
        }
        let (snapshot, generation) = store.export_sync_snapshot()?;
        let commit = repo.commit(&snapshot, head.as_deref())?;
        if repo.push(&commit, &branch)? {
            return Ok((generation, changed));
        }
        // Re-fetch and repeat the semantic union after a concurrent non-fast-forward push.
    }
    Err(SyncError::Offline)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_runtime_writes_preserve_settings_from_another_connection() {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("woodpeck.db");
        let worker = Store::open(&db).unwrap();
        let ui = Store::open(&db).unwrap();
        for (remote, enabled) in [
            ("/original/remote.git", false),
            ("/replacement/remote.git", true),
        ] {
            for status in [
                Status::Syncing,
                Status::Pending,
                Status::Synced,
                Status::Offline,
            ] {
                let mut stale = configure(&worker, "/original/remote.git", true).unwrap();
                stale.status = status;
                stale.pending = status != Status::Synced;
                stale.last_error = Some("stale runtime error".into());
                stale.last_sync = Some(123);
                stale.synced_generation = 123;
                let expected = configure(&ui, remote, enabled).unwrap();
                assert_eq!(save_runtime(&worker, &stale).unwrap(), expected);
                assert_eq!(config(&ui).unwrap(), expected);
                let persisted: Config =
                    serde_json::from_str(&ui.setting("sync_config").unwrap().unwrap()).unwrap();
                assert_eq!(persisted, expected);
            }
        }
    }
}
