use super::*;
use crate::{
    puzzle::{Outcome, Puzzle},
    review::{ReviewSort, ReviewStatus},
    sets::BuilderParams,
    settings::{BoardTheme, ColorTheme, Settings},
    stats::{Accuracy, Attempt, AttemptKind, CycleError},
};
fn pool() -> Vec<Puzzle> {
    (0..20)
        .map(|i| {
            let mut p = crate::puzzle::fixture();
            p.id = format!("p{i:02}");
            if i % 2 == 0 {
                p.themes = vec!["fork".into()];
            } else {
                p.themes = vec!["pin".into()];
            }
            p
        })
        .collect()
}
fn setup(store: &mut Store) -> i64 {
    store
        .create_set(
            &BuilderParams {
                size: 20,
                target_cycles: 2,
                ..Default::default()
            },
            &pool(),
            1,
            42,
            1200,
        )
        .unwrap()
}
fn cycle_attempt(set_id: i64, cycle_id: i64, id: &str, outcome: Outcome, at: i64) -> Attempt {
    Attempt {
        puzzle_id: id.into(),
        set_id,
        cycle_id: Some(cycle_id),
        kind: AttemptKind::Cycle,
        outcome,
        time_ms: 1234,
        moves_played: vec!["h5f6".into()],
        created_at: at,
    }
}
fn review_attempt(set_id: i64, id: &str, outcome: Outcome, at: i64) -> Attempt {
    let mut a = cycle_attempt(set_id, 0, id, outcome, at);
    a.kind = AttemptKind::Review;
    a.cycle_id = None;
    a
}
fn finish(store: &mut Store, cycle_id: i64, at: i64) {
    let cycle = store.cycle(cycle_id).unwrap().unwrap();
    while let Some(id) = store
        .cycle_progress(cycle_id)
        .unwrap()
        .next_puzzle()
        .map(str::to_owned)
    {
        store
            .record_attempt(&cycle_attempt(
                cycle.set_id,
                cycle_id,
                &id,
                Outcome::Correct,
                at,
            ))
            .unwrap();
    }
}
#[test]
fn migrations_idempotent_and_reject_future_schema() {
    let store = Store::memory().unwrap();
    assert_eq!(store.schema_version().unwrap(), 2);
    let mut conn = store.connection;
    schema::migrate(&mut conn).unwrap();
    conn.pragma_update(None, "user_version", 99).unwrap();
    assert!(matches!(
        Store::from_connection(conn),
        Err(StoreError::NewerSchema(99))
    ));
}
#[test]
fn migrates_existing_version_one_and_retains_data() {
    let mut s = Store::memory().unwrap();
    let set = setup(&mut s);
    let c = s.start_cycle(set, 1).unwrap();
    s.record_attempt(&cycle_attempt(set, c.id, "p00", Outcome::Incorrect, 2))
        .unwrap();
    s.connection.execute_batch("DROP INDEX puzzles_rating; DROP INDEX puzzle_themes_theme_rating; DROP INDEX cycle_puzzle_once; DROP INDEX one_active_cycle; DROP INDEX attempts_lifetime; DROP INDEX review_pending_recent; PRAGMA user_version=1;").unwrap();
    let s = Store::from_connection(s.connection).unwrap();
    assert_eq!(s.schema_version().unwrap(), 2);
    assert_eq!(s.pending_review_count().unwrap(), 1);
    assert_eq!(s.cycle_progress(c.id).unwrap().next_puzzle(), Some("p01"));
}
#[test]
fn puzzles_roundtrip_query_and_upsert_theme_index() {
    let mut s = Store::memory().unwrap();
    let mut rows = pool();
    s.upsert_puzzles(&rows).unwrap();
    assert_eq!(s.puzzle_count().unwrap(), 20);
    assert_eq!(s.puzzle("p00").unwrap(), Some(rows[0].clone()));
    assert_eq!(s.puzzle("missing").unwrap(), None);
    assert_eq!(s.query_puzzles(1000, 1100, Some("fork")).unwrap().len(), 10);
    assert_eq!(s.query_puzzles(500, 900, None).unwrap(), Vec::new());
    rows[0].themes = vec!["mate".into()];
    rows[0].rating = 800;
    s.upsert_puzzles(&rows[..1]).unwrap();
    assert_eq!(s.query_puzzles(1000, 1100, Some("fork")).unwrap().len(), 9);
    assert_eq!(s.query_puzzles(800, 800, Some("mate")).unwrap().len(), 1);
}
#[test]
fn puzzle_import_is_atomic_on_overflow() {
    let mut s = Store::memory().unwrap();
    let mut rows = pool();
    rows[1].nb_plays = u64::MAX;
    assert!(s.upsert_puzzles(&rows).is_err());
    assert_eq!(s.puzzle_count().unwrap(), 0);
}
#[test]
fn set_snapshots_survive_source_updates() {
    let mut s = Store::memory().unwrap();
    let id = setup(&mut s);
    let original = s.set_puzzles(id).unwrap();
    let mut rows = pool();
    for p in &mut rows {
        p.rating = 2500;
        p.fen = "changed".into();
    }
    s.upsert_puzzles(&rows).unwrap();
    assert_eq!(s.set_puzzles(id).unwrap(), original);
    let c = s.start_cycle(id, 1).unwrap();
    s.record_attempt(&cycle_attempt(id, c.id, "p00", Outcome::Incorrect, 2))
        .unwrap();
    assert_eq!(
        s.review_cards(None, ReviewSort::MostRecent).unwrap()[0].puzzle,
        original[0]
    );
}
#[test]
fn invalid_set_input_creates_no_partial_rows() {
    let mut s = Store::memory().unwrap();
    let mut p = pool();
    p[1].id = p[0].id.clone();
    assert!(
        s.create_set(
            &BuilderParams {
                size: 20,
                ..Default::default()
            },
            &p,
            1,
            1,
            1200
        )
        .is_err()
    );
    assert_eq!(s.sets().unwrap(), Vec::new());
    assert!(
        s.create_set(
            &BuilderParams {
                size: 20,
                ..Default::default()
            },
            &pool()[..19],
            1,
            1,
            1200
        )
        .is_err()
    );
}
#[test]
fn sets_roundtrip_rename_and_u64_seed() {
    let mut s = Store::memory().unwrap();
    let params = BuilderParams {
        size: 20,
        shuffle_each_cycle: true,
        ..Default::default()
    };
    let id = s.create_set(&params, &pool(), 123, u64::MAX, 1200).unwrap();
    let set = s.set(id).unwrap().unwrap();
    assert_eq!(set.params, params);
    assert_eq!(set.created_at, 123);
    assert_eq!(set.seed, u64::MAX);
    assert_eq!(s.sets().unwrap(), vec![set]);
    s.rename_set(id, "Renamed").unwrap();
    assert_eq!(s.set(id).unwrap().unwrap().params.name, "Renamed");
    assert!(s.rename_set(id, "").is_err());
    assert!(s.rename_set(99, "x").is_err());
}
#[test]
fn wrong_immediately_enqueued_and_attempt_roundtrips() {
    let mut s = Store::memory().unwrap();
    let set = setup(&mut s);
    let c = s.start_cycle(set, 1).unwrap();
    let a = cycle_attempt(set, c.id, "p00", Outcome::Incorrect, 2);
    let recorded = s.record_attempt(&a).unwrap();
    assert!(recorded.id > 0);
    assert!(!recorded.cycle_completed);
    assert_eq!(s.pending_review_count().unwrap(), 1);
    assert_eq!(s.attempts(set).unwrap(), vec![a]);
    let card = &s.review_cards(None, ReviewSort::MostRecent).unwrap()[0];
    assert_eq!(card.position, 1);
    assert_eq!(card.set_name, "Starter Set");
    assert_eq!(
        card.accuracy,
        Accuracy {
            correct: 0,
            attempts: 1
        }
    );
}
#[test]
fn skip_immediately_enqueued() {
    let mut s = Store::memory().unwrap();
    let set = setup(&mut s);
    let c = s.start_cycle(set, 1).unwrap();
    s.record_attempt(&cycle_attempt(set, c.id, "p00", Outcome::Skipped, 2))
        .unwrap();
    assert_eq!(s.pending_review_count().unwrap(), 1);
    assert_eq!(s.cycle_stats(c.id).unwrap().skipped, 1);
}
#[test]
fn repeated_miss_one_entry_bumped_time() {
    let mut s = Store::memory().unwrap();
    let set = setup(&mut s);
    let c = s.start_cycle(set, 1).unwrap();
    s.record_attempt(&cycle_attempt(set, c.id, "p00", Outcome::Incorrect, 2))
        .unwrap();
    finish(&mut s, c.id, 3);
    let c2 = s.start_cycle(set, 4).unwrap();
    s.record_attempt(&cycle_attempt(set, c2.id, "p00", Outcome::Skipped, 5))
        .unwrap();
    assert_eq!(s.pending_review_count().unwrap(), 1);
    let item = s.review_item(set, "p00").unwrap().unwrap();
    assert_eq!((item.added_at, item.last_missed_at), (2, 5));
}
#[test]
fn incorrect_review_keeps_pending_and_does_not_change_cycle_stats() {
    let mut s = Store::memory().unwrap();
    let set = setup(&mut s);
    let c = s.start_cycle(set, 1).unwrap();
    s.record_attempt(&cycle_attempt(set, c.id, "p00", Outcome::Incorrect, 2))
        .unwrap();
    let stats = s.cycle_stats(c.id).unwrap();
    for outcome in [Outcome::Incorrect, Outcome::Skipped, Outcome::Invalid] {
        s.record_attempt(&review_attempt(set, "p00", outcome, 3))
            .unwrap();
        assert_eq!(s.pending_review_count().unwrap(), 1);
        assert_eq!(s.cycle_stats(c.id).unwrap(), stats);
    }
    assert_eq!(
        s.lifetime_accuracy("p00").unwrap(),
        Accuracy {
            correct: 0,
            attempts: 3
        }
    );
    assert_eq!(
        s.review_item(set, "p00").unwrap().unwrap().last_missed_at,
        2
    );
}
#[test]
fn correct_review_clears_and_updates_lifetime_only() {
    let mut s = Store::memory().unwrap();
    let set = setup(&mut s);
    let c = s.start_cycle(set, 1).unwrap();
    s.record_attempt(&cycle_attempt(set, c.id, "p00", Outcome::Incorrect, 2))
        .unwrap();
    let before = s.cycle_stats(c.id).unwrap();
    s.record_attempt(&review_attempt(set, "p00", Outcome::Correct, 3))
        .unwrap();
    assert_eq!(s.pending_review_count().unwrap(), 0);
    assert_eq!(s.cycle_stats(c.id).unwrap(), before);
    assert_eq!(
        s.lifetime_accuracy("p00").unwrap(),
        Accuracy {
            correct: 1,
            attempts: 2
        }
    );
    let item = s.review_item(set, "p00").unwrap().unwrap();
    assert_eq!(item.status, ReviewStatus::Cleared);
    assert_eq!(item.cleared_at, Some(3));
    assert_eq!(s.set_puzzles(set).unwrap().len(), 20);
}
#[test]
fn cleared_then_later_cycle_miss_reenqueues() {
    let mut s = Store::memory().unwrap();
    let set = setup(&mut s);
    let c = s.start_cycle(set, 1).unwrap();
    s.record_attempt(&cycle_attempt(set, c.id, "p00", Outcome::Skipped, 2))
        .unwrap();
    s.record_attempt(&review_attempt(set, "p00", Outcome::Correct, 3))
        .unwrap();
    finish(&mut s, c.id, 4);
    let c2 = s.start_cycle(set, 5).unwrap();
    s.record_attempt(&cycle_attempt(set, c2.id, "p00", Outcome::Incorrect, 6))
        .unwrap();
    assert_eq!(s.pending_review_count().unwrap(), 1);
    let item = s.review_item(set, "p00").unwrap().unwrap();
    assert_eq!(item.status, ReviewStatus::Pending);
    assert_eq!(item.cleared_at, None);
    assert_eq!((item.added_at, item.last_missed_at), (6, 6));
}
#[test]
fn later_cycle_correct_does_not_clear_review() {
    let mut s = Store::memory().unwrap();
    let set = setup(&mut s);
    let c = s.start_cycle(set, 1).unwrap();
    s.record_attempt(&cycle_attempt(set, c.id, "p00", Outcome::Incorrect, 2))
        .unwrap();
    finish(&mut s, c.id, 3);
    let c2 = s.start_cycle(set, 4).unwrap();
    s.record_attempt(&cycle_attempt(set, c2.id, "p00", Outcome::Correct, 5))
        .unwrap();
    assert_eq!(s.pending_review_count().unwrap(), 1);
}
#[test]
fn filter_by_theme_and_sort_both_ways() {
    let mut s = Store::memory().unwrap();
    let set = setup(&mut s);
    let c = s.start_cycle(set, 1).unwrap();
    s.record_attempt(&cycle_attempt(set, c.id, "p00", Outcome::Incorrect, 2))
        .unwrap();
    s.record_attempt(&cycle_attempt(set, c.id, "p01", Outcome::Skipped, 3))
        .unwrap();
    s.record_attempt(&review_attempt(set, "p01", Outcome::Incorrect, 4))
        .unwrap();
    let forks = s
        .review_cards(Some("fork"), ReviewSort::MostRecent)
        .unwrap();
    assert_eq!(forks.len(), 1);
    assert_eq!(forks[0].item.puzzle_id, "p00");
    assert_eq!(
        s.review_cards(Some("mate"), ReviewSort::MostRecent)
            .unwrap(),
        Vec::new()
    );
    assert_eq!(
        s.review_cards(None, ReviewSort::MostRecent).unwrap()[0]
            .item
            .puzzle_id,
        "p01"
    );
    // Add a correct cycle answer for p00 while keeping it pending, raising its lifetime accuracy.
    finish(&mut s, c.id, 5);
    let c2 = s.start_cycle(set, 6).unwrap();
    s.record_attempt(&cycle_attempt(set, c2.id, "p00", Outcome::Correct, 7))
        .unwrap();
    let cards = s.review_cards(None, ReviewSort::LowestAccuracy).unwrap();
    assert_eq!(cards[0].item.puzzle_id, "p01");
    assert_eq!(cards[1].accuracy.percent(), 50.0);
}
#[test]
fn queue_and_cycle_resume_across_database_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested/woodpeck.db");
    let (set, cycle_id) = {
        let mut s = Store::open(&path).unwrap();
        let set = setup(&mut s);
        let c = s.start_cycle(set, 1).unwrap();
        s.record_attempt(&cycle_attempt(set, c.id, "p00", Outcome::Incorrect, 2))
            .unwrap();
        s.record_attempt(&cycle_attempt(set, c.id, "p01", Outcome::Skipped, 3))
            .unwrap();
        (set, c.id)
    };
    let mut s = Store::open(&path).unwrap();
    assert_eq!(s.pending_review_count().unwrap(), 2);
    assert_eq!(
        s.review_cards(Some("fork"), ReviewSort::MostRecent)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        s.cycle_progress(cycle_id).unwrap().next_puzzle(),
        Some("p02")
    );
    assert_eq!(s.start_cycle(set, 999).unwrap().id, cycle_id);
    s.record_attempt(&review_attempt(set, "p00", Outcome::Correct, 4))
        .unwrap();
    drop(s);
    let s = Store::open(&path).unwrap();
    assert_eq!(s.pending_review_count().unwrap(), 1);
    assert_eq!(
        s.review_item(set, "p00").unwrap().unwrap().status,
        ReviewStatus::Cleared
    );
}
#[test]
fn same_puzzle_in_two_sets_has_two_review_entries() {
    let mut s = Store::memory().unwrap();
    for _ in 0..2 {
        let set = setup(&mut s);
        let c = s.start_cycle(set, 1).unwrap();
        s.record_attempt(&cycle_attempt(set, c.id, "p00", Outcome::Skipped, 2))
            .unwrap();
    }
    assert_eq!(s.pending_review_count().unwrap(), 2);
    assert_eq!(s.lifetime_accuracy("p00").unwrap().attempts, 2);
}
#[test]
fn failed_review_write_rolls_back_attempt_and_cycle() {
    let mut s = Store::memory().unwrap();
    let set = setup(&mut s);
    let c = s.start_cycle(set, 1).unwrap();
    s.connection.execute_batch("CREATE TRIGGER reject_review BEFORE INSERT ON review_items BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
    assert!(
        s.record_attempt(&cycle_attempt(set, c.id, "p00", Outcome::Incorrect, 2))
            .is_err()
    );
    assert_eq!(s.attempts(set).unwrap(), Vec::new());
    assert_eq!(s.pending_review_count().unwrap(), 0);
    assert_eq!(s.cycle_progress(c.id).unwrap().next_puzzle(), Some("p00"));
}
#[test]
fn demo_and_invalid_do_not_enqueue_and_zero_time_clamped() {
    let mut s = Store::memory().unwrap();
    let set = setup(&mut s);
    let c = s.start_cycle(set, 1).unwrap();
    let mut a = cycle_attempt(set, c.id, "p00", Outcome::Invalid, 2);
    a.time_ms = 0;
    s.record_attempt(&a).unwrap();
    a.kind = AttemptKind::Demo;
    a.cycle_id = None;
    a.outcome = Outcome::Skipped;
    s.record_attempt(&a).unwrap();
    assert_eq!(s.pending_review_count().unwrap(), 0);
    assert_eq!(s.lifetime_accuracy("p00").unwrap(), Accuracy::default());
    assert!(s.attempts(set).unwrap().iter().all(|a| a.time_ms == 1));
}
#[test]
fn rejects_duplicate_out_of_order_cross_set_and_nonpending_review() {
    let mut s = Store::memory().unwrap();
    let set = setup(&mut s);
    let other = setup(&mut s);
    let c = s.start_cycle(set, 1).unwrap();
    assert!(
        s.record_attempt(&cycle_attempt(set, c.id, "p01", Outcome::Correct, 2))
            .is_err()
    );
    assert!(
        s.record_attempt(&cycle_attempt(other, c.id, "p00", Outcome::Correct, 2))
            .is_err()
    );
    assert!(
        s.record_attempt(&review_attempt(set, "p00", Outcome::Correct, 2))
            .is_err()
    );
    s.record_attempt(&cycle_attempt(set, c.id, "p00", Outcome::Correct, 3))
        .unwrap();
    assert!(
        s.record_attempt(&cycle_attempt(set, c.id, "p00", Outcome::Incorrect, 4))
            .is_err()
    );
    assert_eq!(s.attempts(set).unwrap().len(), 1);
}
#[test]
fn cycles_complete_same_puzzles_extend_to_ten() {
    let mut s = Store::memory().unwrap();
    let set = setup(&mut s);
    let first = s.start_cycle(set, 1).unwrap();
    assert!(matches!(
        s.extend_set(set),
        Err(StoreError::Cycle(CycleError::Incomplete))
    ));
    finish(&mut s, first.id, 2);
    assert!(s.cycle(first.id).unwrap().unwrap().completed_at.is_some());
    for n in 2..=10 {
        if n > 2 {
            assert_eq!(s.extend_set(set).unwrap(), n);
        }
        let c = s.start_cycle(set, i64::from(n)).unwrap();
        assert_eq!(c.number, n);
        assert_eq!(c.order, first.order);
        finish(&mut s, c.id, 20);
        assert!(s.cycle_progress(c.id).unwrap().complete());
    }
    assert!(matches!(
        s.extend_set(set),
        Err(StoreError::Cycle(CycleError::MaximumReached))
    ));
    assert!(s.start_cycle(set, 99).is_err());
    assert_eq!(s.cycles(set).unwrap().len(), 10);
}
#[test]
fn settings_roundtrip_and_paths() {
    let s = Store::memory().unwrap();
    assert_eq!(s.settings().unwrap(), Settings::default());
    let prefs = Settings {
        board_theme: BoardTheme::Plum,
        color_theme: ColorTheme::Omarchy,
        completion_sound: false,
        show_theme_labels: false,
        auto_advance: true,
        user_rating: 1800,
        shuffle_each_cycle: true,
        ..Default::default()
    };
    s.save_settings(&prefs).unwrap();
    assert_eq!(s.settings().unwrap(), prefs);
    s.set_setting("custom", "value").unwrap();
    assert_eq!(s.setting("custom").unwrap().as_deref(), Some("value"));
    assert_eq!(s.setting("absent").unwrap(), None);
    let home = Path::new("/test/home");
    assert_eq!(
        database_path_for(home, None),
        PathBuf::from("/test/home/.local/share/woodpeck/woodpeck.db")
    );
    assert_eq!(
        database_path_for(home, Some(Path::new("/data"))),
        PathBuf::from("/data/woodpeck/woodpeck.db")
    );
    assert_eq!(
        database_path_for(home, Some(Path::new("relative"))),
        database_path_for(home, None)
    );
}

#[test]
fn shuffled_order_and_settings_are_stable_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite");
    let prefs = Settings {
        board_theme: BoardTheme::Lagoon,
        color_theme: ColorTheme::Light,
        shuffle_each_cycle: true,
        ..Default::default()
    };
    let (set, first) = {
        let mut s = Store::open(&path).unwrap();
        s.save_settings(&prefs).unwrap();
        let params = BuilderParams {
            size: 20,
            shuffle_each_cycle: true,
            ..Default::default()
        };
        let set = s.create_set(&params, &pool(), 1, 123, 1200).unwrap();
        let c = s.start_cycle(set, 1).unwrap();
        (set, c)
    };
    let mut s = Store::open(&path).unwrap();
    assert_eq!(s.settings().unwrap(), prefs);
    assert_eq!(s.start_cycle(set, 5).unwrap(), first);
    finish(&mut s, first.id, 6);
    let second = s.start_cycle(set, 7).unwrap();
    assert_ne!(second.order, first.order);
    let mut order = second.order;
    order.sort();
    assert_eq!(order, pool().into_iter().map(|p| p.id).collect::<Vec<_>>());
}
#[test]
fn target_cycles_enforced_until_explicit_extension() {
    let mut s = Store::memory().unwrap();
    let set = setup(&mut s);
    for n in 1..=2 {
        let c = s.start_cycle(set, n).unwrap();
        finish(&mut s, c.id, 5);
    }
    assert!(matches!(
        s.start_cycle(set, 6),
        Err(StoreError::Cycle(CycleError::TargetReached))
    ));
    assert_eq!(s.extend_set(set).unwrap(), 3);
    assert_eq!(s.start_cycle(set, 7).unwrap().number, 3);
}
#[test]
fn migration_failure_is_rolled_back_without_bumping_version() {
    let mut conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute("CREATE TABLE settings(dummy INTEGER)", [])
        .unwrap();
    assert!(schema::migrate(&mut conn).is_err());
    let version: u32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 0);
    let count: u32 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='puzzles'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}
#[test]
fn review_and_cycle_id_invariants_reject_without_writes() {
    let mut s = Store::memory().unwrap();
    let set = setup(&mut s);
    let c = s.start_cycle(set, 1).unwrap();
    let mut a = review_attempt(set, "p00", Outcome::Incorrect, 2);
    a.cycle_id = Some(c.id);
    assert!(s.record_attempt(&a).is_err());
    a.kind = AttemptKind::Cycle;
    a.cycle_id = None;
    assert!(s.record_attempt(&a).is_err());
    a.cycle_id = Some(c.id);
    a.time_ms = u64::MAX;
    assert!(s.record_attempt(&a).is_err());
    assert_eq!(s.attempts(set).unwrap(), Vec::new());
}
#[test]
fn invalid_settings_do_not_replace_saved_preferences() {
    let s = Store::memory().unwrap();
    let good = Settings::default();
    s.save_settings(&good).unwrap();
    let bad = Settings {
        user_rating: 0,
        ..good.clone()
    };
    assert!(s.save_settings(&bad).is_err());
    assert_eq!(s.settings().unwrap(), good);
}
