//! Application operations and a serial background worker, with no GUI dependencies.
mod local;
#[cfg(test)]
mod tests;
mod worker;
pub(crate) use local::Wake;
#[doc(hidden)]
pub use local::{LocalService, Shutdown as LocalShutdown};

use crate::{
    motivation::Motivation,
    puzzle::{Outcome, Puzzle},
    review::{ReviewCard, ReviewSort},
    sets::{BuilderParams, GenerationError, generate},
    settings::Settings,
    source::{CancelFlag, ImportProgress, LichessClient, PuzzleFetcher, SourceError},
    stats::{Attempt, AttemptKind, CycleStats, CycleSummary, ProgressReport, progress},
    store::{CycleRecord, RecordedAttempt, Store, StoreError, StoredSet},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, mpsc},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error(transparent)]
    Generation(#[from] GenerationError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sync(#[from] crate::sync::SyncError),
    #[error("Background worker is unavailable.")]
    WorkerUnavailable,
}
pub type Result<T> = std::result::Result<T, ServiceError>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetCreation {
    pub set_id: Option<i64>,
    pub available: usize,
    pub requested: u32,
    /// The exact inclusive rating range searched; it is never widened.
    pub range: (u16, u16),
}
impl SetCreation {
    pub fn shortfall(&self) -> usize {
        (self.requested as usize).saturating_sub(self.available)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetSummary {
    pub set: StoredSet,
    pub latest_cycle: Option<CycleRecord>,
    pub stats: CycleStats,
    pub review_pending: u32,
    pub completed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrainingPosition {
    pub cycle: CycleRecord,
    /// Zero-based index of the next position; equals set size after completion.
    pub position: usize,
    /// None indicates a completed cycle.
    pub puzzle: Option<Puzzle>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SetProgress {
    pub cycles: Vec<CycleSummary>,
    pub report: ProgressReport,
}
#[derive(Clone, Debug)]
pub enum Job {
    ImportStarter {
        force: bool,
    },
    ImportLibrary {
        location: String,
        size: crate::source::LibrarySize,
        limit: Option<u64>,
    },
    Fetch {
        theme: Option<String>,
        target: u16,
        count: u32,
    },
    FetchDaily,
    FetchById {
        id: String,
    },
    /// Explicitly enqueue this after showing the local shortfall to the user.
    CreateSetWithFallback {
        params: BuilderParams,
        seed: u64,
        now: i64,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobOutput {
    Imported(ImportProgress),
    Fetched { inserted: usize },
    Daily(Puzzle),
    Puzzle(Puzzle),
    SetCreated(SetCreation),
}
#[derive(Debug)]
pub enum Event {
    Started {
        id: u64,
    },
    ImportProgress {
        id: u64,
        progress: ImportProgress,
    },
    FetchProgress {
        id: u64,
        batches: u32,
        inserted: usize,
    },
    Finished {
        id: u64,
        result: Result<JobOutput>,
    },
}
#[derive(Clone, Debug)]
pub struct JobHandle {
    pub id: u64,
    pub cancel: CancelFlag,
}
#[derive(Clone, Copy, Debug)]
pub struct ImportStatus {
    pub id: u64,
    pub size: crate::source::LibrarySize,
    pub started: bool,
    pub progress: ImportProgress,
}
struct Command {
    handle: JobHandle,
    job: Job,
}
type SharedStore = Arc<Mutex<Store>>;
fn lock(store: &SharedStore) -> Result<MutexGuard<'_, Store>> {
    store.lock().map_err(|_| ServiceError::WorkerUnavailable)
}

pub struct AppService {
    local: LocalService,
    local_commands: mpsc::Sender<local::Task>,
    local_shutdown: LocalShutdown,
    wake: Wake,
    commands: mpsc::Sender<Command>,
    events: mpsc::Receiver<Event>,
    pending: BTreeMap<u64, CancelFlag>,
    imports: BTreeMap<u64, ImportStatus>,
    next_id: u64,
    sync: Option<crate::sync::worker::Worker>,
}
impl AppService {
    pub fn open(path: &Path) -> Result<Self> {
        let mut service = Self::new(Store::open(path)?)?;
        let config = crate::sync::config(&*lock(&service.store)?)?;
        service.sync = Some(crate::sync::worker::Worker::start_with_wake(
            path,
            config,
            service.wake.clone(),
        )?);
        Ok(service)
    }
    /// Starts the worker and queues the versioned, idempotent first-run starter import.
    pub fn new(store: Store) -> Result<Self> {
        Self::with_fetcher(store, Box::<LichessClient>::default())
    }
    pub fn with_fetcher(store: Store, fetcher: Box<dyn PuzzleFetcher>) -> Result<Self> {
        let store = Arc::new(Mutex::new(store));
        let (commands, receiver) = mpsc::channel();
        let (sender, events) = mpsc::channel();
        let shared = store.clone();
        let wake = Wake::default();
        let event_sender = local::EventSender {
            sender,
            wake: wake.clone(),
        };
        std::thread::Builder::new()
            .name("woodpeck-source".into())
            .spawn(move || {
                worker::run(&shared, receiver, &event_sender, fetcher);
            })?;
        let local = LocalService { store };
        let (local_commands, local_shutdown) = local::start(local.clone())?;
        let mut service = Self {
            local,
            local_commands,
            local_shutdown,
            wake,
            commands,
            events,
            pending: BTreeMap::new(),
            imports: BTreeMap::new(),
            next_id: 0,
            sync: None,
        };
        service.submit(Job::ImportStarter { force: false })?;
        Ok(service)
    }
    pub fn set_waker(&self, wake: impl Fn() + Send + Sync + 'static) {
        self.wake.set(wake);
    }
    pub(crate) fn background<T: Send + 'static>(
        &self,
        work: impl FnOnce(&LocalService) -> Result<T> + Send + 'static,
    ) -> Result<mpsc::Receiver<Result<T>>> {
        let (sender, receiver) = mpsc::channel();
        let wake = self.wake.clone();
        self.local_commands
            .send(Box::new(move |local| {
                let _ = sender.send(work(local));
                wake.notify();
            }))
            .map_err(|_| ServiceError::WorkerUnavailable)?;
        Ok(receiver)
    }
    pub(crate) fn sync_configured(&mut self, config: crate::sync::Config) {
        if let Some(worker) = &mut self.sync {
            worker.update(config);
        }
    }
    pub fn sync_status(&self) -> crate::sync::Config {
        self.sync
            .as_ref()
            .map_or_else(crate::sync::Config::default, |worker| {
                worker.status().clone()
            })
    }
    pub fn configure_sync(&mut self, remote: &str, enabled: bool) -> Result<()> {
        let config = crate::sync::configure(&*lock(&self.store)?, remote, enabled)?;
        if let Some(worker) = &mut self.sync {
            worker.update(config);
        }
        Ok(())
    }
    pub fn sync_now(&self) {
        if let Some(worker) = &self.sync {
            worker.request();
        }
    }
    pub fn poll_sync(&mut self) -> bool {
        self.sync
            .as_mut()
            .is_some_and(crate::sync::worker::Worker::poll)
    }
    pub fn local_shutdown(&self) -> LocalShutdown {
        self.local_shutdown.clone()
    }
    pub fn sync_shutdown(&self) -> Option<crate::sync::worker::Shutdown> {
        self.sync
            .as_ref()
            .map(crate::sync::worker::Worker::shutdown_handle)
    }
    pub fn submit(&mut self, job: Job) -> Result<JobHandle> {
        self.next_id += 1;
        let handle = JobHandle {
            id: self.next_id,
            cancel: CancelFlag::default(),
        };
        let size = match &job {
            Job::ImportStarter { .. } => Some(crate::source::LibrarySize::Starter),
            Job::ImportLibrary { size, .. } => Some(*size),
            _ => None,
        };
        self.commands
            .send(Command {
                handle: handle.clone(),
                job,
            })
            .map_err(|_| ServiceError::WorkerUnavailable)?;
        self.pending.insert(handle.id, handle.cancel.clone());
        if let Some(size) = size {
            self.imports.insert(
                handle.id,
                ImportStatus {
                    id: handle.id,
                    size,
                    started: false,
                    progress: ImportProgress::default(),
                },
            );
        }
        Ok(handle)
    }
    /// The first pending import, including the automatic starter import before it starts.
    pub fn background_import(&self) -> Option<ImportStatus> {
        self.imports.values().next().copied()
    }
    pub fn import_status(&self, id: u64) -> Option<ImportStatus> {
        self.imports.get(&id).copied()
    }
    /// Also allows cancelling the automatic startup import by its event ID.
    pub fn cancel_job(&self, id: u64) -> bool {
        if let Some(flag) = self.pending.get(&id) {
            flag.cancel();
            true
        } else {
            false
        }
    }
    /// Cancels queued and active background jobs without blocking the UI.
    pub fn cancel_all(&self) {
        for flag in self.pending.values() {
            flag.cancel();
        }
    }
    pub fn is_busy(&self) -> bool {
        !self.pending.is_empty()
    }
    /// Nonblocking channel poll for the UI's frame loop. Completed jobs already committed.
    pub fn poll_events(&mut self) -> Vec<Event> {
        let events: Vec<_> = self.events.try_iter().collect();
        for event in &events {
            match event {
                Event::Started { id } => {
                    if let Some(import) = self.imports.get_mut(id) {
                        import.started = true;
                    }
                }
                Event::ImportProgress { id, progress } => {
                    if let Some(import) = self.imports.get_mut(id) {
                        import.progress = *progress;
                    }
                }
                Event::Finished { id, .. } => {
                    self.pending.remove(id);
                    self.imports.remove(id);
                }
                Event::FetchProgress { .. } => {}
            }
        }
        events
    }
    pub fn default_database_path() -> Option<PathBuf> {
        crate::store::database_path()
    }
}
impl std::ops::Deref for AppService {
    type Target = LocalService;
    fn deref(&self) -> &Self::Target {
        &self.local
    }
}
impl Drop for AppService {
    fn drop(&mut self) {
        for flag in self.pending.values() {
            flag.cancel();
        }
    }
}
fn training_position(store: &Store, cycle: CycleRecord) -> Result<TrainingPosition> {
    let progress = store.cycle_progress(cycle.id)?;
    let position = store.cycle_stats(cycle.id)?.attempted() as usize;
    let puzzle = progress
        .next_puzzle()
        .map(|id| {
            store
                .set_puzzle(cycle.set_id, id)?
                .ok_or(StoreError::NotFound)
        })
        .transpose()?;
    Ok(TrainingPosition {
        cycle,
        position,
        puzzle,
    })
}
fn create_set(
    store: &mut Store,
    params: &BuilderParams,
    seed: u64,
    now: i64,
) -> Result<SetCreation> {
    match generate(store, params, seed) {
        Ok(generated) => {
            let id = store.create_set(params, &generated.puzzles, now, seed)?;
            Ok(SetCreation {
                set_id: Some(id),
                available: generated.puzzles.len(),
                requested: params.size,
                range: params.range(),
            })
        }
        Err(GenerationError::Insufficient {
            available,
            requested,
        }) => Ok(SetCreation {
            set_id: None,
            available,
            requested,
            range: params.range(),
        }),
        Err(error) => Err(error.into()),
    }
}
impl From<crate::stats::CycleError> for ServiceError {
    fn from(error: crate::stats::CycleError) -> Self {
        Self::Store(error.into())
    }
}
