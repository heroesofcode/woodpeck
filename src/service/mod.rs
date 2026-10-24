//! Application operations and a serial background worker, with no GUI dependencies.
#[cfg(test)]
mod tests;
mod worker;

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
struct Command {
    handle: JobHandle,
    job: Job,
}
type SharedStore = Arc<Mutex<Store>>;
fn lock(store: &SharedStore) -> Result<MutexGuard<'_, Store>> {
    store.lock().map_err(|_| ServiceError::WorkerUnavailable)
}

pub struct AppService {
    store: SharedStore,
    commands: mpsc::Sender<Command>,
    events: mpsc::Receiver<Event>,
    pending: BTreeMap<u64, CancelFlag>,
    next_id: u64,
    sync: Option<crate::sync::worker::Worker>,
}
impl AppService {
    pub fn open(path: &Path) -> Result<Self> {
        let mut service = Self::new(Store::open(path)?)?;
        let config = crate::sync::config(&*lock(&service.store)?)?;
        service.sync = Some(crate::sync::worker::Worker::start(path, config)?);
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
        std::thread::Builder::new()
            .name("woodpeck-source".into())
            .spawn(move || {
                worker::run(&shared, receiver, &sender, fetcher);
            })?;
        let mut service = Self {
            store,
            commands,
            events,
            pending: BTreeMap::new(),
            next_id: 0,
            sync: None,
        };
        service.submit(Job::ImportStarter { force: false })?;
        Ok(service)
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
        self.commands
            .send(Command {
                handle: handle.clone(),
                job,
            })
            .map_err(|_| ServiceError::WorkerUnavailable)?;
        self.pending.insert(handle.id, handle.cancel.clone());
        Ok(handle)
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
            if let Event::Finished { id, .. } = event {
                self.pending.remove(id);
            }
        }
        events
    }
    pub fn local_setting(&self, key: &str) -> Result<Option<String>> {
        Ok(lock(&self.store)?.setting(key)?)
    }
    pub fn set_local_setting(&self, key: &str, value: &str) -> Result<()> {
        Ok(lock(&self.store)?.set_setting(key, value)?)
    }
    pub fn rating_histogram(&self) -> Result<[u64; crate::source::RATING_BANDS]> {
        Ok(lock(&self.store)?.rating_histogram()?)
    }
    pub fn needs_onboarding(&self) -> Result<bool> {
        use crate::source::{OnboardingDecision, onboarding_decision};
        let store = lock(&self.store)?;
        let decision = onboarding_decision(
            store.setting("onboarding_done")?.is_some(),
            store.puzzle_count()?,
        );
        if decision == OnboardingDecision::MarkDone {
            store.set_setting("onboarding_done", "true")?;
        }
        Ok(decision == OnboardingDecision::Show)
    }
    pub fn puzzle_count(&self) -> Result<u64> {
        Ok(lock(&self.store)?.puzzle_count()?)
    }
    pub fn list_sets(&self) -> Result<Vec<SetSummary>> {
        let store = lock(&self.store)?;
        let reviews = store.review_cards(None, ReviewSort::MostRecent)?;
        store
            .sets()?
            .into_iter()
            .map(|set| {
                let latest_cycle = store.cycles(set.id)?.pop();
                let stats = latest_cycle
                    .as_ref()
                    .map(|c| store.cycle_stats(c.id))
                    .transpose()?
                    .unwrap_or_default();
                let completed = latest_cycle.as_ref().is_some_and(|c| {
                    c.completed_at.is_some() && c.number >= set.params.target_cycles
                });
                let review_pending =
                    u32::try_from(reviews.iter().filter(|r| r.item.set_id == set.id).count())
                        .unwrap_or(u32::MAX);
                Ok(SetSummary {
                    set,
                    latest_cycle,
                    stats,
                    review_pending,
                    completed,
                })
            })
            .collect()
    }
    /// Generates locally; a shortfall never persists a partial set. Online fallback is a job.
    pub fn create_set(&self, params: &BuilderParams, seed: u64, now: i64) -> Result<SetCreation> {
        create_set(&mut *lock(&self.store)?, params, seed, now)
    }
    pub fn start_or_resume_cycle(&self, set_id: i64, now: i64) -> Result<TrainingPosition> {
        let mut store = lock(&self.store)?;
        let cycle = store.start_cycle(set_id, now)?;
        training_position(&store, cycle)
    }
    /// Outcomes in puzzle order, used for the segmented cycle progress display.
    pub fn cycle_outcomes(&self, cycle_id: i64) -> Result<Vec<Outcome>> {
        let store = lock(&self.store)?;
        let cycle = store.cycle(cycle_id)?.ok_or(StoreError::NotFound)?;
        let mut outcomes = BTreeMap::new();
        for attempt in store
            .attempts(cycle.set_id)?
            .into_iter()
            .filter(|a| a.cycle_id == Some(cycle_id))
        {
            outcomes.entry(attempt.puzzle_id).or_insert(attempt.outcome);
        }
        Ok(cycle
            .order
            .iter()
            .filter_map(|id| outcomes.get(id).copied())
            .collect())
    }

    pub fn current_puzzle(&self, cycle_id: i64) -> Result<TrainingPosition> {
        let store = lock(&self.store)?;
        let cycle = store.cycle(cycle_id)?.ok_or(StoreError::NotFound)?;
        training_position(&store, cycle)
    }
    pub fn next_cycle(&self, set_id: i64, now: i64) -> Result<TrainingPosition> {
        let store = lock(&self.store)?;
        if store
            .cycles(set_id)?
            .last()
            .is_some_and(|c| c.completed_at.is_none())
        {
            return Err(crate::stats::CycleError::Incomplete.into());
        }
        drop(store);
        self.start_or_resume_cycle(set_id, now)
    }
    pub fn extend_cycle(&self, set_id: i64, now: i64) -> Result<TrainingPosition> {
        let mut store = lock(&self.store)?;
        store.extend_set(set_id)?;
        let cycle = store.start_cycle(set_id, now)?;
        training_position(&store, cycle)
    }
    pub fn record_attempt(&self, attempt: &Attempt) -> Result<RecordedAttempt> {
        let mut store = lock(&self.store)?;
        let sounds = store.settings()?.completion_sound;
        let recorded = store.record_attempt(attempt)?;
        drop(store);
        if sounds && attempt.outcome == Outcome::Correct {
            crate::sound::play_completion();
        }
        Ok(recorded)
    }
    pub fn record_review_attempt(&self, attempt: &Attempt) -> Result<RecordedAttempt> {
        if attempt.kind != AttemptKind::Review {
            return Err(StoreError::InvalidData("Expected a review attempt.".into()).into());
        }
        self.record_attempt(attempt)
    }
    pub fn review_queue(&self, theme: Option<&str>, sort: ReviewSort) -> Result<Vec<ReviewCard>> {
        Ok(lock(&self.store)?.review_cards(theme, sort)?)
    }
    pub fn review_count(&self) -> Result<u32> {
        Ok(lock(&self.store)?.pending_review_count()?)
    }
    pub fn progress(&self, set_id: i64) -> Result<SetProgress> {
        let store = lock(&self.store)?;
        store.set(set_id)?.ok_or(StoreError::NotFound)?;
        let cycles = store
            .cycles(set_id)?
            .into_iter()
            .map(|c| {
                Ok(CycleSummary {
                    number: c.number,
                    stats: store.cycle_stats(c.id)?,
                    complete: c.completed_at.is_some(),
                })
            })
            .collect::<std::result::Result<Vec<_>, StoreError>>()?;
        let report = progress(
            &store.set_puzzles(set_id)?,
            &store.attempts(set_id)?,
            &cycles,
        );
        Ok(SetProgress { cycles, report })
    }
    pub fn settings(&self) -> Result<Settings> {
        Ok(lock(&self.store)?.settings()?)
    }
    pub fn set_settings(&self, settings: &Settings) -> Result<()> {
        Ok(lock(&self.store)?.save_settings(settings)?)
    }
    pub fn motivation(&self) -> Result<Motivation> {
        let value = lock(&self.store)?.setting("motivation")?;
        Ok(value
            .map(|s| serde_json::from_str(&s))
            .transpose()
            .map_err(StoreError::from)?
            .unwrap_or_default())
    }
    pub fn export_pgn(&self, set_id: i64, path: &Path) -> Result<()> {
        let store = lock(&self.store)?;
        let set = store.set(set_id)?.ok_or(StoreError::NotFound)?;
        let pgn = crate::source::export_pgn(&set.params.name, &store.set_puzzles(set_id)?)?;
        drop(store);
        std::fs::write(path, pgn)?;
        Ok(())
    }
    pub fn delete_set(&self, set_id: i64) -> Result<()> {
        Ok(lock(&self.store)?.delete_set(set_id)?)
    }
    pub fn default_database_path() -> Option<PathBuf> {
        crate::store::database_path()
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
                .set_puzzles(cycle.set_id)?
                .into_iter()
                .find(|p| p.id == id)
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
