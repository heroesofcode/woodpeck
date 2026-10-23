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
        restrict_recommended: false,
        ..Default::default()
    };
    let generated = generate(&store, &params, 1200, 42)?;
    let now = chrono::Local::now().timestamp();
    let id = store.create_set(&params, &generated.puzzles, now - 86_400 * 7, 42, 1200)?;
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
