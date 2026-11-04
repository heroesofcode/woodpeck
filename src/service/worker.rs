use super::local::EventSender;
use super::{
    Command, Event, Job, JobOutput, Result, SharedStore, create_set, fallback_request, in_band,
    lock, save_single, save_valid, starter,
};
use crate::{
    sets::generate,
    source::{
        CancelFlag, ImportOptions, PuzzleFetcher, SourceError, import_stream_options, open_location,
    },
};
use std::sync::mpsc;

pub(super) fn run(
    store: &SharedStore,
    commands: mpsc::Receiver<Command>,
    events: &EventSender,
    mut fetcher: Box<dyn PuzzleFetcher>,
) {
    for command in commands {
        let id = command.handle.id;
        let _ = events.send(Event::Started { id });
        let result = execute(
            store,
            command.job,
            &command.handle.cancel,
            id,
            events,
            fetcher.as_mut(),
        );
        if events.send(Event::Finished { id, result }).is_err() {
            break;
        }
    }
}
fn execute(
    store: &SharedStore,
    job: Job,
    cancel: &CancelFlag,
    id: u64,
    events: &EventSender,
    fetcher: &mut dyn PuzzleFetcher,
) -> Result<JobOutput> {
    cancel.check()?;
    let progress = |progress| {
        let _ = events.send(Event::ImportProgress { id, progress });
    };
    // No lock is held while reading, downloading, decompressing or waiting on the network.
    let save = |batch: &[_]| {
        lock(store)
            .map_err(|e| SourceError::Invalid(e.to_string()))?
            .upsert_puzzles(batch)?;
        Ok(())
    };
    match job {
        Job::ImportStarter { force } => starter(store, force, cancel, progress),
        Job::ImportLibrary {
            location,
            size,
            limit,
        } => {
            let reader = open_location(&location, cancel)?;
            let result =
                import_stream_options(reader, ImportOptions { limit }, cancel, progress, save)?;
            cancel.check()?;
            lock(store)?.record_library_import(size, chrono::Utc::now().timestamp())?;
            Ok(JobOutput::Imported(result))
        }
        Job::Fetch {
            theme,
            target,
            count,
        } => {
            let puzzles = fetcher.batch(
                theme.as_deref(),
                crate::source::difficulty(target),
                count,
                cancel,
            )?;
            cancel.check()?;
            let inserted = save_valid(store, puzzles)?;
            let _ = events.send(Event::FetchProgress {
                id,
                batches: 1,
                inserted,
            });
            Ok(JobOutput::Fetched { inserted })
        }
        Job::FetchDaily | Job::FetchById { .. } => {
            let daily = matches!(job, Job::FetchDaily);
            let puzzle = if let Job::FetchById { id } = job {
                fetcher.by_id(&id, cancel)?
            } else {
                fetcher.daily(cancel)?
            };
            cancel.check()?;
            save_single(store, puzzle, daily)
        }
        Job::CreateSetWithFallback { params, seed, now } => fallback(
            store,
            &params,
            seed,
            now,
            cancel,
            |batches, inserted| {
                let _ = events.send(Event::FetchProgress {
                    id,
                    batches,
                    inserted,
                });
            },
            fetcher,
        ),
    }
}

fn fallback(
    store: &SharedStore,
    params: &crate::sets::BuilderParams,
    seed: u64,
    now: i64,
    cancel: &CancelFlag,
    mut report: impl FnMut(u32, usize),
    fetcher: &mut dyn PuzzleFetcher,
) -> Result<JobOutput> {
    params
        .validate()
        .map_err(crate::sets::GenerationError::from)?;
    let mut inserted = 0;
    // Bound requests even if the anonymous API cannot satisfy the requested band.
    for batches in 1..=5 {
        cancel.check()?;
        match generate(&*lock(store)?, params, seed) {
            Ok(_) => break,
            Err(crate::sets::GenerationError::Insufficient { .. }) => {}
            Err(error) => return Err(error.into()),
        }
        let (theme, difficulty) = fallback_request(params, batches);
        let puzzles = fetcher.batch(theme, difficulty, params.size.min(100), cancel)?;
        cancel.check()?;
        let puzzles = in_band(params, puzzles);
        lock(store)?.upsert_puzzles(&puzzles)?;
        inserted += puzzles.len();
        report(batches, inserted);
    }
    cancel.check()?;
    Ok(JobOutput::SetCreated(create_set(
        &mut *lock(store)?,
        params,
        seed,
        now,
    )?))
}
