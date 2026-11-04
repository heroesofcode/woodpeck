//! Browser sync: the GitHub remote over `fetch`, the access token in `localStorage`, and a
//! scheduler on the page's event loop with the desktop worker's timing rules.
use super::{
    Config, Result, Status, SyncError, config,
    format::State,
    github::{GitHub, Http, Request, Response, validate_repository},
    remote::{SyncStorage, exchange_async},
    save_runtime, update_config,
};
use crate::store::Store;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
};

/// Kept in the browser only; it is never part of the database image or the sync data.
const TOKEN_KEY: &str = "woodpeck.github_token";
/// Wait after the last local change, as on desktop.
const DEBOUNCE_MS: f64 = 10_000.0;
/// Pull changes from other devices while the page is open.
const REFRESH_MS: f64 = 120_000.0;
const BACKOFF_MS: f64 = 5_000.0;
const MAX_BACKOFF_MS: f64 = 300_000.0;
const TICK_MS: u32 = 1_000;

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}
fn token() -> Option<String> {
    local_storage()?
        .get_item(TOKEN_KEY)
        .ok()?
        .filter(|t| !t.is_empty())
}
pub fn has_token() -> bool {
    token().is_some()
}
/// Saves the token in this browser; an empty token removes it.
pub fn set_token(token: &str) -> Result<()> {
    let storage = local_storage()
        .ok_or_else(|| SyncError::Invalid("This browser blocks local storage.".into()))?;
    let token = token.trim();
    let saved = if token.is_empty() {
        storage.remove_item(TOKEN_KEY)
    } else {
        storage.set_item(TOKEN_KEY, token)
    };
    saved.map_err(|_| SyncError::Invalid("Could not save the token in this browser.".into()))
}

pub struct Fetch {
    token: String,
}
impl Http for Fetch {
    async fn send(&self, request: Request) -> Result<Response> {
        let method = match request.method {
            "GET" => ehttp::Method::GET,
            "POST" => ehttp::Method::POST,
            "PATCH" => ehttp::Method::PATCH,
            "PUT" => ehttp::Method::PUT,
            other => {
                return Err(SyncError::Invalid(format!(
                    "Unsupported request method {other}."
                )));
            }
        };
        let accept = if request.raw {
            "application/vnd.github.raw+json"
        } else {
            "application/vnd.github+json"
        };
        let mut headers =
            ehttp::Headers::new(&[("Accept", accept), ("X-GitHub-Api-Version", "2022-11-28")]);
        headers.insert("Authorization", format!("Bearer {}", self.token));
        let mut body = vec![];
        if let Some(json) = &request.body {
            headers.insert("Content-Type", "application/json");
            body = serde_json::to_vec(json)?;
        }
        let mut http = ehttp::Request::new(
            method,
            format!("https://api.github.com{}", request.path),
            headers,
        );
        http.body = body;
        let response = ehttp::fetch_async(http)
            .await
            .map_err(|_| SyncError::Offline)?;
        Ok(Response {
            status: response.status,
            body: response.bytes,
        })
    }
}

/// Same rules as the desktop `configure`, with `owner/name` instead of a Git URL.
pub fn configure(store: &Store, repository: &str, enabled: bool) -> Result<Config> {
    if enabled || !repository.is_empty() {
        validate_repository(repository)?;
    }
    update_config(store, |cfg| {
        if cfg.remote != repository {
            cfg.synced_generation = -1;
            cfg.last_sync = None;
        }
        cfg.remote = repository.into();
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

/// Locks the shared store per call, so the lock is never held across a network await.
struct Shared<'a>(&'a Mutex<Store>);
impl Shared<'_> {
    fn store(&self) -> Result<std::sync::MutexGuard<'_, Store>> {
        self.0
            .lock()
            .map_err(|_| SyncError::Invalid("The local database is unavailable.".into()))
    }
}
impl SyncStorage for Shared<'_> {
    fn export_sync(&self) -> Result<State> {
        Ok(self.store()?.export_sync()?)
    }
    fn export_sync_snapshot(&self) -> Result<(State, i64)> {
        Ok(self.store()?.export_sync_snapshot()?)
    }
    fn import_sync(&mut self, remote: &State) -> Result<State> {
        self.store()?.import_sync(remote)
    }
}

/// One sync, recorded in the configuration exactly as the desktop `synchronize` does.
async fn synchronize(store: &Mutex<Store>, remote: &GitHub<Fetch>) -> Result<Config> {
    let shared = Shared(store);
    let cfg = {
        let store = shared.store()?;
        update_config(&store, |cfg| {
            if !cfg.enabled {
                return Err(SyncError::Invalid("Sync is disabled.".into()));
            }
            cfg.pending = store.sync_generation()? != cfg.synced_generation;
            cfg.status = Status::Syncing;
            Ok(())
        })?
    };
    let result = exchange_async(&mut Shared(store), remote).await;
    let store = shared.store()?;
    let mut latest = config(&store)?;
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
    let latest = save_runtime(&store, &latest)?;
    result.map(|_| latest)
}

#[derive(Default)]
struct Board {
    status: Config,
    /// Set after a sync finished; the UI reloads its views once.
    finished: bool,
    requested: bool,
}
/// Handle to the browser scheduler, owned by the service.
#[derive(Clone)]
pub struct Scheduler {
    state: Rc<RefCell<Board>>,
}
impl Scheduler {
    /// Syncs on open, 10 s after local changes, every 2 minutes, and retries with backoff.
    /// A newer schema or a rejected token halts it until the settings change or Sync now.
    pub fn start(store: Arc<Mutex<Store>>, wake: impl Fn() + 'static) -> Self {
        let state = Rc::new(RefCell::new(Board::default()));
        let scheduler = Self {
            state: state.clone(),
        };
        wasm_bindgen_futures::spawn_local(async move {
            let mut key: Option<(String, String)> = None;
            let mut remote: Option<Rc<GitHub<Fetch>>> = None;
            let mut next_at = 0.0;
            let mut seen_generation = None;
            let mut failures = 0u32;
            let mut halted = false;
            loop {
                let now = js_sys::Date::now();
                let current = store
                    .lock()
                    .ok()
                    .and_then(|s| Some((config(&s).ok()?, s.sync_generation().ok()?)));
                if let Some((cfg, generation)) = current {
                    let requested = std::mem::take(&mut state.borrow_mut().requested);
                    let token = token();
                    let missing = token.is_none();
                    let wanted = (cfg.enabled && token.is_some())
                        .then(|| (cfg.remote.clone(), token.unwrap_or_default()));
                    if wanted != key {
                        key.clone_from(&wanted);
                        remote = wanted.as_ref().and_then(|(repository, token)| {
                            GitHub::new(
                                Fetch {
                                    token: token.clone(),
                                },
                                repository,
                            )
                            .ok()
                            .map(Rc::new)
                        });
                        (next_at, failures, halted) = (now, 0, false);
                    }
                    if requested {
                        (next_at, halted) = (now, false);
                    }
                    if generation != cfg.synced_generation
                        && seen_generation.replace(generation) != Some(generation)
                        && failures == 0
                    {
                        next_at = now + DEBOUNCE_MS;
                    }
                    let mut status = cfg;
                    if status.enabled && missing {
                        status.last_error =
                            Some("Enter a GitHub access token to start syncing.".into());
                    }
                    state.borrow_mut().status = status;
                    if let Some(remote) = remote.clone().filter(|_| !halted && now >= next_at) {
                        if let Ok(s) = store.lock()
                            && let Ok(syncing) = config(&s)
                        {
                            let mut syncing = syncing;
                            syncing.status = Status::Syncing;
                            state.borrow_mut().status = syncing;
                        }
                        wake();
                        let result = synchronize(&store, &remote).await;
                        let done = js_sys::Date::now();
                        match &result {
                            Ok(_) => {
                                failures = 0;
                                next_at = done + REFRESH_MS;
                            }
                            Err(SyncError::Offline) => {
                                failures += 1;
                                next_at = done
                                    + (BACKOFF_MS * 2f64.powi(failures.min(16).cast_signed() - 1))
                                        .min(MAX_BACKOFF_MS);
                            }
                            Err(_) => halted = true,
                        }
                        if let Ok(s) = store.lock()
                            && let Ok(latest) = config(&s)
                        {
                            seen_generation = s.sync_generation().ok();
                            state.borrow_mut().status = latest;
                        }
                        state.borrow_mut().finished = true;
                        wake();
                    }
                }
                gloo_timers::future::TimeoutFuture::new(TICK_MS).await;
            }
        });
        scheduler
    }
    pub fn status(&self) -> Config {
        self.state.borrow().status.clone()
    }
    /// Sync as soon as possible, also after a halt.
    pub fn request(&self) {
        self.state.borrow_mut().requested = true;
    }
    /// True once after each finished sync.
    pub fn poll(&self) -> bool {
        std::mem::take(&mut self.state.borrow_mut().finished)
    }
}
