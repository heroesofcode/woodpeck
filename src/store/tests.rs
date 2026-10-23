use super::*;
use crate::{
    puzzle::{Outcome, Puzzle},
    review::{ReviewSort, ReviewStatus},
    sets::{BuilderParams, ThemeFilter, ThemeMatch},
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
    assert_eq!(store.schema_version().unwrap(), schema::VERSION);
    let mut conn = store.connection;
    schema::migrate(&mut conn).unwrap();
    conn.pragma_update(None, "user_version", 99).unwrap();
    assert!(matches!(
        Store::from_connection(conn),
        Err(StoreError::NewerSchema(99))
    ));
}
fn version_one_connection() -> Connection {
    let schema = include_str!("schema.rs");
    let sql = schema
        .split("tx.execute_batch(\"CREATE TABLE puzzles (")
        .nth(1)
        .unwrap()
        .split("\")?;")
        .next()
        .unwrap();
    let old = Connection::open_in_memory().unwrap();
    old.execute_batch(&format!("CREATE TABLE puzzles ({sql}"))
        .unwrap();
    old.pragma_update(None, "user_version", 1).unwrap();
    old
}
#[test]
fn migrates_focus_theme_to_include_list_with_its_sync_clock() {
    let mut old = version_one_connection();
    old.execute_batch(
        "INSERT INTO sets VALUES(1,'Forks','fork',1000,100,20,2,1,'theme-forks','fixed',1,'42');
         INSERT INTO sets VALUES(2,'Mixed',NULL,1000,100,20,2,1,'general','fixed',1,'43');",
    )
    .unwrap();
    super::schema::migrate_until(&mut old, 3).unwrap();
    old.execute(
        "INSERT INTO sync_fields(key,value,updated_at) SELECT 'set/'||uid||'/focus_theme','\"fork\"',777 FROM sets WHERE id=1",
        [],
    )
    .unwrap();
    let s = Store::from_connection(old).unwrap();
    assert_eq!(s.schema_version().unwrap(), schema::VERSION);
    let sets = s.sets().unwrap();
    assert_eq!(sets[0].params.include_themes, vec!["fork".to_string()]);
    assert_eq!(sets[0].params.exclude_themes, Vec::<String>::new());
    assert_eq!(sets[0].params.theme_match, ThemeMatch::Any);
    assert_eq!(sets[1].params.include_themes, Vec::<String>::new());
    let mut clocks: Vec<(String, String, i64)> = s
        .connection
        .prepare("SELECT substr(key,38),value,updated_at FROM sync_fields ORDER BY key")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    clocks.sort();
    assert_eq!(
        clocks,
        vec![
            ("exclude_themes".into(), "[]".into(), 777),
            ("include_themes".into(), "[\"fork\"]".into(), 777),
            ("theme_match".into(), "\"any\"".into(), 777),
        ]
    );
    // Theme lists round-trip through the new columns.
    let mut s = s;
    let mut rows = pool();
    for p in &mut rows {
        p.themes = vec!["fork".into(), "pin".into()];
    }
    let params = BuilderParams {
        size: 20,
        include_themes: vec!["fork".into(), "pin".into()],
        exclude_themes: vec!["mate".into()],
        theme_match: ThemeMatch::All,
        ..BuilderParams::default()
    };
    let id = s.create_set(&params, &rows, 5, 1).unwrap();
    assert_eq!(s.set(id).unwrap().unwrap().params, params);
}
#[test]
fn migrates_target_and_variance_to_an_equivalent_range_with_sync_clocks() {
    let mut old = version_one_connection();
    let mut expected = Vec::new();
    {
        let tx = old.transaction().unwrap();
        let mut id = 0;
        for target in (400..=3250).step_by(13) {
            for variance in [50, 75, 100, 125, 150, 175, 200] {
                id += 1;
                tx.execute(
                    "INSERT INTO sets VALUES(?1,'Old',NULL,?2,?3,20,2,1,NULL,'fixed',1,'42')",
                    rusqlite::params![id, target, variance],
                )
                .unwrap();
                expected.push(crate::sets::legacy_range(target, variance));
            }
        }
        tx.commit().unwrap();
    }
    super::schema::migrate_until(&mut old, 4).unwrap();
    old.execute_batch(
        "INSERT INTO sync_fields(key,value,updated_at) SELECT 'set/'||uid||'/target_rating','400',500 FROM sets WHERE id=1;
         INSERT INTO sync_fields(key,value,updated_at) SELECT 'set/'||uid||'/variance','50',800 FROM sets WHERE id=1;
         INSERT INTO sync_fields(key,value,updated_at) SELECT 'set/'||uid||'/restrict_recommended','true',900 FROM sets WHERE id=1;
         INSERT INTO sync_fields(key,value,updated_at) SELECT 'set/'||uid||'/size','20',300 FROM sets WHERE id=1;",
    )
    .unwrap();
    let s = Store::from_connection(old).unwrap();
    assert_eq!(s.schema_version().unwrap(), schema::VERSION);
    let ranges: Vec<_> = s.sets().unwrap().iter().map(|x| x.params.range()).collect();
    // SQL and the Rust rule used by the sync upgrade agree on every legacy band.
    assert_eq!(ranges, expected);
    assert_eq!(
        ranges[0],
        (400, 500),
        "a clamped 400±50 band grows to 100 points"
    );
    assert!(
        s.sets()
            .unwrap()
            .iter()
            .all(|x| x.params.validate().is_ok())
    );
    let mut clocks: Vec<(String, String, i64)> = s
        .connection
        .prepare("SELECT substr(key,38),value,updated_at FROM sync_fields ORDER BY key")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    clocks.sort();
    assert_eq!(
        clocks,
        vec![
            ("rating_max".into(), "500".into(), 800),
            ("rating_min".into(), "400".into(), 800),
            ("size".into(), "20".into(), 300),
        ]
    );
    // The new columns reject invalid ranges at the database level too.
    assert!(
        s.connection
            .execute("UPDATE sets SET rating_max=rating_min WHERE id=1", [])
            .is_err()
    );
}
#[test]
fn migrates_existing_version_one_and_retains_data() {
    let old = version_one_connection();
    old.execute_batch(
        "INSERT INTO sets VALUES(1,'Old',NULL,1000,100,20,2,1,NULL,'fixed',1,'42');
        INSERT INTO set_puzzles VALUES(1,1,'p00','{}');
        INSERT INTO cycles VALUES(1,1,1,1,NULL,'[\"p00\"]');
        INSERT INTO attempts VALUES(1,'p00',1,1,'cycle','incorrect',1,'[]',2);
        INSERT INTO review_items VALUES('p00',1,2,2,'pending',NULL);",
    )
    .unwrap();
    let s = Store::from_connection(old).unwrap();
    assert_eq!(s.schema_version().unwrap(), schema::VERSION);
    for table in ["sets", "cycles", "attempts", "review_items"] {
        let uid: String = s
            .connection
            .query_row(&format!("SELECT uid FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert!(uid.len() >= 32);
    }
    assert_eq!(s.pending_review_count().unwrap(), 1);
    assert_eq!(s.cycle_progress(1).unwrap().next_puzzle(), None);
    let before: String = s
        .connection
        .query_row("SELECT uid FROM attempts", [], |r| r.get(0))
        .unwrap();
    let s = Store::from_connection(s.connection).unwrap();
    let after: String = s
        .connection
        .query_row("SELECT uid FROM attempts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, after);
}

#[test]
fn puzzles_roundtrip_query_and_upsert_theme_index() {
    let mut s = Store::memory().unwrap();
    let mut rows = pool();
    let (fork, mate) = (vec!["fork".to_string()], vec!["mate".to_string()]);
    s.upsert_puzzles(&rows).unwrap();
    assert_eq!(s.puzzle_count().unwrap(), 20);
    assert_eq!(s.puzzle("p00").unwrap(), Some(rows[0].clone()));
    assert_eq!(s.puzzle("missing").unwrap(), None);
    assert_eq!(s.query_puzzles(1000, 1100, only(&fork)).unwrap().len(), 10);
    assert_eq!(
        s.query_puzzles(500, 900, ThemeFilter::default()).unwrap(),
        Vec::new()
    );
    rows[0].themes = vec!["mate".into()];
    rows[0].rating = 800;
    s.upsert_puzzles(&rows[..1]).unwrap();
    assert_eq!(s.query_puzzles(1000, 1100, only(&fork)).unwrap().len(), 9);
    assert_eq!(s.query_puzzles(800, 800, only(&mate)).unwrap().len(), 1);
}
fn only(include: &[String]) -> ThemeFilter<'_> {
    ThemeFilter {
        include,
        ..ThemeFilter::default()
    }
}
#[test]
fn sqlite_theme_query_matches_local_filter() {
    let mut s = Store::memory().unwrap();
    let mut rows = pool();
    // p00..p04 fork+pin, then alternating fork / pin, p19 also mate.
    for p in &mut rows[..5] {
        p.themes = vec!["fork".into(), "pin".into()];
    }
    rows[19].themes.push("mate".into());
    s.upsert_puzzles(&rows).unwrap();
    let strings = |l: &[&str]| l.iter().map(|t| (*t).to_string()).collect::<Vec<_>>();
    let (fork_pin, pin, mate) = (
        strings(&["fork", "pin"]),
        strings(&["pin"]),
        strings(&["mate"]),
    );
    let none: &[String] = &[];
    for (include, exclude, mode, expected) in [
        (&fork_pin[..], none, ThemeMatch::Any, 20),
        (&fork_pin[..], none, ThemeMatch::All, 5),
        (none, &pin[..], ThemeMatch::Any, 7),
        (&pin[..], &mate[..], ThemeMatch::Any, 12),
        (&fork_pin[..], &fork_pin[..], ThemeMatch::Any, 0),
        (none, none, ThemeMatch::All, 20),
    ] {
        let filter = ThemeFilter {
            include,
            exclude,
            mode,
        };
        let found = s.query_puzzles(400, 3000, filter).unwrap();
        assert_eq!(found.len(), expected, "{filter:?}");
        assert!(found.iter().all(|p| filter.matches(&p.themes)));
        assert_eq!(
            rows.iter().filter(|p| filter.matches(&p.themes)).count(),
            expected
        );
    }
    // Theme keys are bound parameters, so quoting cannot alter the query.
    let hostile = strings(&["fork') OR 1=1 --"]);
    assert_eq!(
        s.query_puzzles(400, 3000, only(&hostile)).unwrap(),
        Vec::new()
    );
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
            1
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
            1
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
    let id = s.create_set(&params, &pool(), 123, u64::MAX).unwrap();
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
        let set = s.create_set(&params, &pool(), 1, 123).unwrap();
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

#[test]
fn portable_sync_roundtrip_merges_divergence_and_retains_local_ids() {
    use crate::sync::{format::State, merge::merge};
    let mut a = Store::memory().unwrap();
    let id = setup(&mut a);
    let cycle = a.start_cycle(id, 1).unwrap();
    let initial = a.export_sync().unwrap();
    let mut b = Store::memory().unwrap();
    // Force unrelated local row IDs before importing the shared set.
    setup(&mut b);
    b.import_sync(&initial).unwrap();
    let bid = b.sets().unwrap().last().unwrap().id;
    let bcycle = b.cycles(bid).unwrap()[0].id;
    a.record_attempt(&cycle_attempt(id, cycle.id, "p00", Outcome::Incorrect, 2))
        .unwrap();
    b.record_attempt(&cycle_attempt(bid, bcycle, "p00", Outcome::Skipped, 3))
        .unwrap();
    a.record_attempt(&review_attempt(id, "p00", Outcome::Correct, 4))
        .unwrap();
    let mut settings = a.settings().unwrap();
    settings.auto_advance = true;
    a.save_settings(&settings).unwrap();
    let sa = a.export_sync().unwrap();
    let sb = b.export_sync().unwrap();
    let joined = merge(&sa, &sb).unwrap();
    assert_eq!(joined, merge(&sb, &sa).unwrap());
    assert_eq!(joined, merge(&joined, &joined).unwrap());
    assert_eq!(joined, merge(&joined, &sa).unwrap());
    a.import_sync(&joined).unwrap();
    b.import_sync(&joined).unwrap();
    assert_eq!(a.attempts(id).unwrap().len(), 3);
    assert_eq!(b.attempts(bid).unwrap().len(), 3);
    assert_eq!(b.pending_review_count().unwrap(), 0);
    assert!(b.settings().unwrap().auto_advance);
    assert_eq!(a.export_sync().unwrap(), joined);
    let wire = State::from_files(&joined.files().unwrap()).unwrap();
    assert_eq!(joined, wire);
    assert_eq!(
        a.cycle_progress(cycle.id).unwrap().next_puzzle(),
        Some("p01")
    );
}
#[test]
fn multi_theme_sets_roundtrip_through_sync_files_to_another_store() {
    use crate::sync::format::State;
    let mut a = Store::memory().unwrap();
    let mut rows = pool();
    for p in &mut rows {
        p.themes = vec!["fork".into(), "pin".into()];
    }
    let params = BuilderParams {
        name: "Forks and pins".into(),
        size: 20,
        include_themes: vec!["fork".into(), "pin".into()],
        exclude_themes: vec!["mate".into(), "sacrifice".into()],
        theme_match: ThemeMatch::All,
        preset_id: None,
        ..BuilderParams::default()
    };
    a.create_set(&params, &rows, 5, 1).unwrap();
    let files = a.export_sync().unwrap().files().unwrap();
    let set_file = files
        .iter()
        .find(|(path, _)| path.starts_with("sets/"))
        .unwrap()
        .1;
    for key in ["include_themes", "exclude_themes", "theme_match"] {
        assert!(set_file.contains(key), "{key}");
    }
    assert!(!set_file.contains("focus_theme"));
    assert!(files["manifest.json"].contains(&format!(
        "\"schema_version\": {}",
        crate::sync::format::SCHEMA_VERSION
    )));
    let mut b = Store::memory().unwrap();
    b.import_sync(&State::from_files(&files).unwrap()).unwrap();
    assert_eq!(b.sets().unwrap()[0].params, params);
    assert_eq!(b.export_sync().unwrap().files().unwrap(), files);
}
#[test]
fn sync_import_rejects_bad_state_atomically_and_preserves_new_training() {
    let mut s = Store::memory().unwrap();
    let id = setup(&mut s);
    let c = s.start_cycle(id, 1).unwrap();
    let old = s.export_sync().unwrap();
    s.record_attempt(&cycle_attempt(id, c.id, "p00", Outcome::Incorrect, 2))
        .unwrap();
    s.import_sync(&old).unwrap();
    assert_eq!(s.attempts(id).unwrap().len(), 1);
    let mut corrupt = old;
    corrupt.sets.values_mut().next().unwrap().puzzles.clear();
    assert!(s.import_sync(&corrupt).is_err());
    assert_eq!(s.attempts(id).unwrap().len(), 1);
}

#[test]
fn imported_set_field_clocks_survive_reexport() {
    let mut store = Store::memory().unwrap();
    let id = setup(&mut store);
    let mut remote = store.export_sync().unwrap();
    let set = remote.sets.values_mut().next().unwrap();
    let field = set.fields.get_mut("rating_max").unwrap();
    field.value = serde_json::json!(1150);
    field.updated_at = 9000;
    store.import_sync(&remote).unwrap();
    assert_eq!(store.export_sync().unwrap(), remote);
    assert_eq!(store.set(id).unwrap().unwrap().params.range(), (900, 1150));
}
#[test]
fn source_directory_is_never_a_data_directory() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(ensure_external_data_dir(&source.join("unused/woodpeck.db")).is_err());
    let dir = tempfile::tempdir().unwrap();
    assert!(ensure_external_data_dir(&dir.path().join("new/woodpeck.db")).is_ok());
    #[cfg(unix)]
    {
        let link = dir.path().join("source-link");
        std::os::unix::fs::symlink(source, &link).unwrap();
        assert!(ensure_external_data_dir(&link.join("unused/woodpeck.db")).is_err());
    }
}
