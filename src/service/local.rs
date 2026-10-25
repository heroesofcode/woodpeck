//! Serial local operations run independently of network downloads.
use super::{
    Attempt, AttemptKind, BTreeMap, BuilderParams, CycleSummary, Event, Motivation, Mutex, Outcome,
    Path, RecordedAttempt, Result, ReviewCard, ReviewSort, SetCreation, SetProgress, SetSummary,
    Settings, SharedStore, StoreError, TrainingPosition, create_set, lock, mpsc, progress,
    training_position,
};
use std::{
    sync::{Arc, Condvar},
    time::Duration,
};

type Callback = Box<dyn Fn() + Send + Sync>;
#[derive(Clone, Default)]
pub(crate) struct Wake(Arc<Mutex<Option<Callback>>>);
impl Wake {
    pub fn set(&self, callback: impl Fn() + Send + Sync + 'static) {
        *self.0.lock().expect("wake lock") = Some(Box::new(callback));
    }
    pub fn notify(&self) {
        if let Ok(callback) = self.0.lock()
            && let Some(callback) = &*callback
        {
            callback();
        }
    }
}
pub(super) struct EventSender {
    pub sender: mpsc::Sender<Event>,
    pub wake: Wake,
}
impl EventSender {
    pub fn send(&self, event: Event) -> std::result::Result<(), ()> {
        let result = self.sender.send(event).map_err(|_| ());
        self.wake.notify();
        result
    }
}
#[doc(hidden)]
#[derive(Clone)]
pub struct LocalService {
    pub(super) store: SharedStore,
}
pub(super) type Task = Box<dyn FnOnce(&LocalService) + Send>;
#[doc(hidden)]
#[derive(Clone)]
pub struct Shutdown(Arc<(Mutex<bool>, Condvar)>);
impl Shutdown {
    pub fn finish(&self) {
        let (lock, wake) = &*self.0;
        if let Ok(done) = lock.lock() {
            let _ = wake.wait_timeout_while(done, Duration::from_secs(3), |done| !*done);
        }
    }
}
pub(super) fn start(local: LocalService) -> Result<(mpsc::Sender<Task>, Shutdown)> {
    let (sender, receiver) = mpsc::channel::<Task>();
    let shutdown = Shutdown(Arc::new((Mutex::new(false), Condvar::new())));
    let done = shutdown.clone();
    std::thread::Builder::new()
        .name("woodpeck-local".into())
        .spawn(move || {
            for task in receiver {
                task(&local);
            }
            let (lock, wake) = &*done.0;
            if let Ok(mut finished) = lock.lock() {
                *finished = true;
                wake.notify_all();
            }
        })?;
    Ok((sender, shutdown))
}
impl LocalService {
    pub fn configure_sync(&self, remote: &str, enabled: bool) -> Result<crate::sync::Config> {
        Ok(crate::sync::configure(
            &*lock(&self.store)?,
            remote,
            enabled,
        )?)
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
        Ok(lock(&self.store)?.record_attempt(attempt)?)
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
}
