//! Reproducible screenshot data uses an isolated SQLite database.
use crate::{
    puzzle::Outcome,
    service::AppService,
    sets::{BuilderParams, generate},
    source::{CancelFlag, import_starter},
    stats::{Attempt, AttemptKind},
    store::Store,
};
pub fn service() -> Result<AppService, Box<dyn std::error::Error>> {
    let mut store = Store::memory()?;
    import_starter(&mut store, false, &CancelFlag::default(), |_| {})?;
    let params = BuilderParams {
        name: "Pattern practice".into(),
        preset_id: None,
        size: 20,
        target_cycles: 5,
        ..Default::default()
    };
    let generated = generate(&store, &params, 42)?;
    let now = chrono::Local::now().timestamp();
    let id = store.create_set(&params, &generated.puzzles, now - 86_400 * 7, 42)?;
    for number in 1..=3 {
        let cycle = store.start_cycle(id, now - 86_400 * (4 - number))?;
        for (i, puzzle) in generated
            .puzzles
            .iter()
            .take(if number == 3 { 7 } else { 20 })
            .enumerate()
        {
            store.record_attempt(&Attempt {
                puzzle_id: puzzle.id.clone(),
                set_id: id,
                cycle_id: Some(cycle.id),
                kind: AttemptKind::Cycle,
                outcome: if (i + number as usize).is_multiple_of(6) {
                    Outcome::Incorrect
                } else if i == 4 && number == 1 {
                    Outcome::Skipped
                } else {
                    Outcome::Correct
                },
                time_ms: ((26_000 / number) as u64) + (i as u64 * 321),
                moves_played: vec![],
                created_at: now - 86_400 * (4 - number) + i64::try_from(i)?,
            })?;
        }
    }
    Ok(AppService::new(store)?)
}

/// Stable, offline states for fixed-frame welcome screenshots.
pub(super) fn welcome(app: &mut super::Woodpeck, screen: super::Screen) {
    use super::{Screen, library::WelcomeStep};
    use crate::source::{ImportProgress, LibrarySize};
    let accepted = 116_480;
    let profile = LibrarySize::Medium.histogram();
    let total = profile.iter().sum::<u64>();
    let mut bands = profile.map(|count| count * accepted / total);
    bands[5] += accepted - bands.iter().sum::<u64>();
    let progress = ImportProgress {
        bytes: 15_340_000,
        scanned: 307_000,
        accepted,
        rejected: 190_520,
        bands,
    };
    if matches!(
        screen,
        Screen::Welcome | Screen::WelcomeProgress | Screen::WelcomeDone
    ) {
        app.screen = Screen::Welcome;
        app.library.selected = LibrarySize::Medium;
        app.library.step = match screen {
            Screen::WelcomeProgress => WelcomeStep::Downloading,
            Screen::WelcomeDone => WelcomeStep::Done,
            _ => WelcomeStep::Choose,
        };
        app.library.progress = progress;
        if screen == Screen::WelcomeDone {
            app.library.count = 200_000;
            app.library.bands = *LibrarySize::Medium.histogram();
            app.library.progress.accepted = 200_000;
            app.library.progress.scanned = 527_077;
            app.library.progress.bytes = LibrarySize::Medium.download_bytes();
        }
    }
}
