//! Browser executor: the same serial job queue as the desktop worker thread, run as one
//! `spawn_local` task at a time on the page's event loop. Network steps are awaited.
use super::local::EventSender;
use super::{
    Command, Event, Job, JobOutput, Result, SharedStore, create_set, fallback_request, in_band,
    lock, save_single, save_valid, starter,
};
use crate::{
    sets::generate,
    source::{
        CancelFlag, SourceError,
        browser::{fetch_batch, fetch_by_id, fetch_daily},
    },
};
use std::{cell::Cell, rc::Rc, sync::mpsc};

pub(super) struct Executor {
    commands: Rc<mpsc::Receiver<Command>>,
    running: Rc<Cell<bool>>,
    store: SharedStore,
    events: Rc<EventSender>,
}
impl Executor {
    pub fn new(store: SharedStore, commands: mpsc::Receiver<Command>, events: EventSender) -> Self {
        Self {
            commands: Rc::new(commands),
            running: Rc::default(),
            store,
            events: Rc::new(events),
        }
    }
    /// Starts draining the queue unless a drain is already running; jobs stay strictly serial.
    pub fn kick(&self) {
        if self.running.replace(true) {
            return;
        }
        let (commands, running, store, events) = (
            self.commands.clone(),
            self.running.clone(),
            self.store.clone(),
            self.events.clone(),
        );
        wasm_bindgen_futures::spawn_local(async move {
            while let Ok(command) = commands.try_recv() {
                let id = command.handle.id;
                let _ = events.send(Event::Started { id });
                let result =
                    execute(&store, command.job, &command.handle.cancel, id, &events).await;
                let _ = events.send(Event::Finished { id, result });
            }
            running.set(false);
        });
    }
}
async fn execute(
    store: &SharedStore,
    job: Job,
    cancel: &CancelFlag,
    id: u64,
    events: &EventSender,
) -> Result<JobOutput> {
    cancel.check()?;
    let progress = |progress| {
        let _ = events.send(Event::ImportProgress { id, progress });
    };
    match job {
        Job::ImportStarter { force } => starter(store, force, cancel, progress),
        Job::ImportLibrary { .. } => Err(SourceError::Invalid(
            "Importing the full Lichess library is only available in the desktop app.".into(),
        )
        .into()),
        Job::Fetch {
            theme,
            target,
            count,
        } => {
            let puzzles = fetch_batch(
                theme.as_deref(),
                crate::source::difficulty(target),
                count,
                cancel,
            )
            .await?;
            cancel.check()?;
            let inserted = save_valid(store, puzzles)?;
            let _ = events.send(Event::FetchProgress {
                id,
                batches: 1,
                inserted,
            });
            Ok(JobOutput::Fetched { inserted })
        }
        Job::FetchDaily => {
            let puzzle = fetch_daily(cancel).await?;
            cancel.check()?;
            save_single(store, puzzle, true)
        }
        Job::FetchById { id: puzzle } => {
            let puzzle = fetch_by_id(&puzzle, cancel).await?;
            cancel.check()?;
            save_single(store, puzzle, false)
        }
        Job::CreateSetWithFallback { params, seed, now } => {
            params
                .validate()
                .map_err(crate::sets::GenerationError::from)?;
            let mut inserted = 0;
            for batches in 1..=5 {
                cancel.check()?;
                match generate(&*lock(store)?, &params, seed) {
                    Ok(_) => break,
                    Err(crate::sets::GenerationError::Insufficient { .. }) => {}
                    Err(error) => return Err(error.into()),
                }
                let (theme, difficulty) = fallback_request(&params, batches);
                let puzzles = fetch_batch(theme, difficulty, params.size.min(100), cancel).await?;
                cancel.check()?;
                let puzzles = in_band(&params, puzzles);
                lock(store)?.upsert_puzzles(&puzzles)?;
                inserted += puzzles.len();
                let _ = events.send(Event::FetchProgress {
                    id,
                    batches,
                    inserted,
                });
            }
            cancel.check()?;
            Ok(JobOutput::SetCreated(create_set(
                &mut *lock(store)?,
                &params,
                seed,
                now,
            )?))
        }
    }
}
