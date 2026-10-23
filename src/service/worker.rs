use super::{Command, Event, Job, JobOutput, Result, SharedStore, create_set, lock};
use crate::{
    sets::generate,
    source::{
        CancelFlag, PuzzleFetcher, STARTER_BYTES, STARTER_VERSION, SourceError, import_stream,
        open_location,
    },
};
use std::{io::Cursor, sync::mpsc};

pub(super) fn run(
    store: &SharedStore,
    commands: mpsc::Receiver<Command>,
    events: &mpsc::Sender<Event>,
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
    events: &mpsc::Sender<Event>,
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
        Job::ImportStarter { force } => {
            if !force
                && lock(store)?.setting("starter_version")?.as_deref() == Some(STARTER_VERSION)
            {
                return Ok(JobOutput::Imported(crate::source::ImportProgress::default()));
            }
            let result = import_stream(Cursor::new(STARTER_BYTES), cancel, progress, save)?;
            cancel.check()?;
            lock(store)?.set_setting("starter_version", STARTER_VERSION)?;
            Ok(JobOutput::Imported(result))
        }
        Job::ImportFull { location } => {
            let reader = open_location(&location, cancel)?;
            Ok(JobOutput::Imported(import_stream(
                reader, cancel, progress, save,
            )?))
        }
        Job::Fetch {
            theme,
            target,
            count,
        } => {
            let puzzles = fetcher.batch(theme.as_deref(), target, count, cancel)?;
            cancel.check()?;
            let puzzles: Vec<_> = puzzles
                .into_iter()
                .filter(|p| p.validate().is_ok())
                .collect();
            lock(store)?.upsert_puzzles(&puzzles)?;
            let _ = events.send(Event::FetchProgress {
                id,
                batches: 1,
                inserted: puzzles.len(),
            });
            Ok(JobOutput::Fetched {
                inserted: puzzles.len(),
            })
        }
        Job::FetchDaily | Job::FetchById { .. } => {
            let daily = matches!(job, Job::FetchDaily);
            let puzzle = if let Job::FetchById { id } = job {
                fetcher.by_id(&id, cancel)?
            } else {
                fetcher.daily(cancel)?
            };
            cancel.check()?;
            puzzle
                .validate()
                .map_err(|e| SourceError::Invalid(e.to_string()))?;
            lock(store)?.upsert_puzzles(std::slice::from_ref(&puzzle))?;
            Ok(if daily {
                JobOutput::Daily(puzzle)
            } else {
                JobOutput::Puzzle(puzzle)
            })
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
    let rating = lock(store)?.settings()?.user_rating;
    params
        .validate(rating)
        .map_err(crate::sets::GenerationError::from)?;
    let mut inserted = 0;
    // Bound requests even if the anonymous API cannot satisfy the requested band.
    for batches in 1..=5 {
        cancel.check()?;
        match generate(&*lock(store)?, params, rating, seed) {
            Ok(_) => break,
            Err(crate::sets::GenerationError::Insufficient { .. }) => {}
            Err(error) => return Err(error.into()),
        }
        let puzzles = fetcher.batch(
            params.focus_theme.as_deref(),
            params.target_rating,
            params.size.min(100),
            cancel,
        )?;
        cancel.check()?;
        let band = params.band();
        let puzzles: Vec<_> = puzzles
            .into_iter()
            .filter(|p| {
                (band.0..=band.1).contains(&p.rating)
                    && params
                        .focus_theme
                        .as_ref()
                        .is_none_or(|t| p.themes.contains(t))
                    && p.validate().is_ok()
            })
            .collect();
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
