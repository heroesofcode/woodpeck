use super::*;
use crate::{puzzle::fixture, source::STARTER_VERSION};
use std::{
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant},
};

#[derive(Default)]
struct MockFetcher {
    batches: Vec<Vec<Puzzle>>,
    calls: Arc<AtomicU32>,
    cancel_batch: bool,
    themes: Arc<std::sync::Mutex<Vec<Option<String>>>>,
    difficulties: Arc<std::sync::Mutex<Vec<String>>>,
}
impl PuzzleFetcher for MockFetcher {
    fn batch(
        &mut self,
        theme: Option<&str>,
        difficulty: &str,
        count: u32,
        cancel: &CancelFlag,
    ) -> std::result::Result<Vec<Puzzle>, SourceError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.themes.lock().unwrap().push(theme.map(Into::into));
        self.difficulties.lock().unwrap().push(difficulty.into());
        assert_eq!(count, 20);
        if self.cancel_batch {
            cancel.cancel();
        }
        cancel.check()?;
        Ok(if self.batches.is_empty() {
            vec![]
        } else {
            self.batches.remove(0)
        })
    }
    fn daily(&mut self, cancel: &CancelFlag) -> std::result::Result<Puzzle, SourceError> {
        cancel.check()?;
        Ok(fixture())
    }
    fn by_id(&mut self, id: &str, cancel: &CancelFlag) -> std::result::Result<Puzzle, SourceError> {
        let mut p = self.daily(cancel)?;
        p.id = id.into();
        Ok(p)
    }
}
fn pool(n: u32) -> Vec<Puzzle> {
    (0..n)
        .map(|i| {
            let mut p = fixture();
            p.id = format!("test{i}");
            p.rating = 1000;
            p.themes = vec!["fork".into()];
            p
        })
        .collect()
}
fn params() -> BuilderParams {
    BuilderParams {
        size: 20,
        include_themes: vec!["fork".into()],
        target_cycles: 1,
        ..Default::default()
    }
}
fn service(store: Store, mock: MockFetcher) -> AppService {
    // Isolate each test's source pool while still exercising normal automatic startup.
    store
        .set_setting("starter_version", STARTER_VERSION)
        .unwrap();
    let mut service = AppService::with_fetcher(store, Box::new(mock)).unwrap();
    finish(&mut service, 1).unwrap();
    let settings = Settings {
        completion_sound: false,
        ..Default::default()
    };
    service.set_settings(&settings).unwrap();
    service
}
fn finish(service: &mut AppService, job_id: u64) -> Result<JobOutput> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        for event in service.poll_events() {
            if let Event::Finished { id, result } = event {
                if id == job_id {
                    return result;
                }
                result.unwrap();
            }
        }
        assert!(Instant::now() < deadline, "worker timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn attempt(position: &TrainingPosition, outcome: Outcome, day: i64) -> Attempt {
    Attempt {
        puzzle_id: position.puzzle.as_ref().unwrap().id.clone(),
        set_id: position.cycle.set_id,
        cycle_id: Some(position.cycle.id),
        kind: AttemptKind::Cycle,
        outcome,
        time_ms: 1000,
        moves_played: vec![],
        created_at: day * 86400 + 43200,
    }
}
#[test]
fn play_review_progress_next_cycle_export_delete_and_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite");
    let mut store = Store::open(&path).unwrap();
    store.upsert_puzzles(&pool(20)).unwrap();
    let service = service(store, MockFetcher::default());
    let made = service.create_set(&params(), 42, 1).unwrap();
    assert_eq!(made.shortfall(), 0);
    let set_id = made.set_id.unwrap();
    let mut pos = service.start_or_resume_cycle(set_id, 2).unwrap();
    let order = pos.cycle.order.clone();
    assert!(service.next_cycle(set_id, 3).is_err());
    assert!(service.extend_cycle(set_id, 3).is_err());
    for i in 0..20 {
        assert_eq!(pos.position, i);
        let outcome = match i {
            0 => Outcome::Incorrect,
            1 => Outcome::Skipped,
            _ => Outcome::Correct,
        };
        let recorded = service.record_attempt(&attempt(&pos, outcome, 10)).unwrap();
        assert_eq!(recorded.cycle_completed, i == 19);
        if i < 2 {
            assert_eq!(
                service.review_count().unwrap(),
                u32::try_from(i + 1).unwrap()
            );
        }
        pos = service.current_puzzle(pos.cycle.id).unwrap();
    }
    assert!(pos.puzzle.is_none());
    assert!(service.list_sets().unwrap()[0].completed);
    let report = service.progress(set_id).unwrap();
    assert_eq!(report.report.puzzles_attempted, 20);
    assert_eq!(report.report.accuracy.percent(), 90.0);
    assert_eq!(report.report.most_missed.len(), 2);
    assert_eq!(service.motivation().unwrap().xp, 230);
    assert_eq!(
        service
            .review_queue(Some("pin"), ReviewSort::MostRecent)
            .unwrap()
            .len(),
        0
    );
    let cards = service
        .review_queue(Some("fork"), ReviewSort::LowestAccuracy)
        .unwrap();
    assert_eq!(cards.len(), 2);
    clear_reviews(&service, set_id, cards);
    assert_eq!(service.review_count().unwrap(), 0);
    assert_eq!(service.progress(set_id).unwrap(), report);
    let motivation = service.motivation().unwrap();
    assert_eq!(
        (
            motivation.xp,
            motivation.current_streak,
            motivation.best_streak
        ),
        (250, 2, 2)
    );
    let exported = dir.path().join("set.pgn");
    service.export_pgn(set_id, &exported).unwrap();
    assert_eq!(
        std::fs::read_to_string(exported)
            .unwrap()
            .matches("[PuzzleId ")
            .count(),
        20
    );
    assert!(service.next_cycle(set_id, 12).is_err());
    let second = service.extend_cycle(set_id, 12).unwrap();
    assert_eq!(second.cycle.order, order);
    assert_eq!(second.cycle.number, 2);
    service
        .record_attempt(&attempt(&second, Outcome::Skipped, 14))
        .unwrap();
    assert_eq!(service.review_count().unwrap(), 1);
    drop(service);
    let reopened = AppService::open(&path).unwrap();
    let resumed = reopened.start_or_resume_cycle(set_id, 15).unwrap();
    assert_eq!(resumed.position, 1);
    assert_eq!(resumed.cycle.id, second.cycle.id);
    assert_eq!(reopened.review_count().unwrap(), 1);
    assert_eq!(reopened.motivation().unwrap().current_streak, 1);
    assert!(!reopened.settings().unwrap().completion_sound);
    reopened.delete_set(set_id).unwrap();
    assert_eq!(reopened.list_sets().unwrap(), []);
    assert_eq!(reopened.review_count().unwrap(), 0);
    assert!(reopened.delete_set(set_id).is_err());
    assert!(reopened.progress(set_id).is_err());
    // Puzzle source data and earned XP survive set deletion.
    assert_eq!(reopened.puzzle_count().unwrap(), 20);
    assert_eq!(reopened.motivation().unwrap().xp, 250);
}
#[test]
fn local_creation_never_widens_the_range_or_saves_partial_sets() {
    let mut store = Store::memory().unwrap();
    let rated = |prefix: &str, rating: u16, n: u32| -> Vec<Puzzle> {
        pool(n)
            .into_iter()
            .map(|mut p| {
                p.id = format!("{prefix}{}", p.id);
                p.rating = rating;
                p
            })
            .collect()
    };
    // 19 puzzles on the inclusive upper edge, plenty just outside it.
    store.upsert_puzzles(&rated("edge", 1100, 19)).unwrap();
    store.upsert_puzzles(&rated("out", 1101, 50)).unwrap();
    store.upsert_puzzles(&rated("low", 899, 50)).unwrap();
    let service = service(store, MockFetcher::default());
    let short = service.create_set(&params(), 1, 1).unwrap();
    assert_eq!(short.shortfall(), 1);
    assert_eq!(short.available, 19);
    assert_eq!(short.range, (900, 1100));
    assert_eq!(short.set_id, None);
    assert_eq!(service.list_sets().unwrap(), []);
    lock(&service.store)
        .unwrap()
        .upsert_puzzles(&rated("floor", 900, 1))
        .unwrap();
    let made = service.create_set(&params(), 1, 1).unwrap();
    assert_eq!(made.shortfall(), 0);
    let puzzles = lock(&service.store)
        .unwrap()
        .set_puzzles(made.set_id.unwrap())
        .unwrap();
    assert!(puzzles.iter().all(|p| (900..=1100).contains(&p.rating)));
}
#[test]
fn mocked_fallback_filters_band_theme_and_invalid_rows_and_regenerates() {
    let mut noise = pool(3);
    noise[0].rating = 2000;
    noise[1].themes = vec!["pin".into()];
    noise[2].fen.clear();
    let good = pool(20);
    let mock = MockFetcher {
        batches: vec![noise, good.clone()],
        ..Default::default()
    };
    let calls = mock.calls.clone();
    let mock_themes = mock.themes.clone();
    let mut service = service(Store::memory().unwrap(), mock);
    let job = service
        .submit(Job::CreateSetWithFallback {
            params: params(),
            seed: 1,
            now: 1,
        })
        .unwrap();
    let JobOutput::SetCreated(created) = finish(&mut service, job.id).unwrap() else {
        panic!()
    };
    assert_eq!(created.shortfall(), 0);
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert!(
        mock_themes
            .lock()
            .unwrap()
            .iter()
            .all(|t| t.as_deref() == Some("fork"))
    );
    assert_eq!(service.puzzle_count().unwrap(), 20);
    let puzzles = lock(&service.store)
        .unwrap()
        .set_puzzles(created.set_id.unwrap())
        .unwrap();
    assert_eq!(puzzles.len(), 20);
    assert!(puzzles.iter().all(|p| good.contains(p)));
    let next = service
        .submit(Job::CreateSetWithFallback {
            params: params(),
            seed: 2,
            now: 2,
        })
        .unwrap();
    finish(&mut service, next.id).unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 2);
}
#[test]
fn fallback_maps_range_to_api_difficulties_and_filters_range_locally() {
    let batch = |prefix: &str, theme: &str, ratings: &[(u16, u32)]| -> Vec<Puzzle> {
        ratings
            .iter()
            .flat_map(|&(rating, n)| {
                pool(n).into_iter().map(move |mut p| {
                    p.id = format!("{prefix}{rating}{}", p.id);
                    p.rating = rating;
                    p.themes = vec![theme.into()];
                    p
                })
            })
            .collect()
    };
    let mock = MockFetcher {
        batches: vec![
            batch("a", "fork", &[(1300, 10), (1299, 5), (1501, 5)]),
            batch("b", "pin", &[(1500, 5), (2000, 5)]),
            batch("c", "fork", &[(1400, 5)]),
        ],
        ..Default::default()
    };
    let (themes, difficulties) = (mock.themes.clone(), mock.difficulties.clone());
    let mut service = service(Store::memory().unwrap(), mock);
    let params = BuilderParams {
        rating_min: 1300,
        rating_max: 1500,
        include_themes: vec!["fork".into(), "pin".into()],
        ..params()
    };
    let job = service
        .submit(Job::CreateSetWithFallback {
            params,
            seed: 1,
            now: 1,
        })
        .unwrap();
    let JobOutput::SetCreated(created) = finish(&mut service, job.id).unwrap() else {
        panic!()
    };
    assert_eq!(created.shortfall(), 0);
    assert_eq!(created.range, (1300, 1500));
    // Each included theme is asked at the best-overlapping difficulty, then the next one.
    assert_eq!(
        *difficulties.lock().unwrap(),
        ["normal", "normal", "easier"]
    );
    assert_eq!(
        *themes.lock().unwrap(),
        [
            Some("fork".into()),
            Some("pin".into()),
            Some("fork".to_string())
        ]
    );
    // Out-of-range rows (1299, 1501, 2000) are never stored; both edges are inclusive.
    assert_eq!(service.puzzle_count().unwrap(), 20);
    let ratings: std::collections::BTreeSet<_> = lock(&service.store)
        .unwrap()
        .set_puzzles(created.set_id.unwrap())
        .unwrap()
        .iter()
        .map(|p| p.rating)
        .collect();
    assert_eq!(ratings, [1300, 1400, 1500].into());
}
#[test]
fn fallback_rotates_included_themes_and_filters_exclusions_locally() {
    let tagged = |prefix: &str, themes: &[&str], n: u32| -> Vec<Puzzle> {
        pool(n)
            .into_iter()
            .map(|mut p| {
                p.id = format!("{prefix}{}", p.id);
                p.themes = themes.iter().map(|t| (*t).to_string()).collect();
                p
            })
            .collect()
    };
    let mut forks = tagged("f", &["fork"], 10);
    forks.extend(tagged("m", &["fork", "mate"], 5));
    let mock = MockFetcher {
        batches: vec![forks, tagged("p", &["pin"], 10)],
        ..Default::default()
    };
    let themes = mock.themes.clone();
    let mut service = service(Store::memory().unwrap(), mock);
    let params = BuilderParams {
        include_themes: vec!["fork".into(), "pin".into()],
        exclude_themes: vec!["mate".into()],
        ..params()
    };
    let job = service
        .submit(Job::CreateSetWithFallback {
            params,
            seed: 1,
            now: 1,
        })
        .unwrap();
    let JobOutput::SetCreated(created) = finish(&mut service, job.id).unwrap() else {
        panic!()
    };
    assert_eq!(created.shortfall(), 0);
    assert_eq!(
        *themes.lock().unwrap(),
        vec![Some("fork".to_string()), Some("pin".to_string())]
    );
    // Excluded rows are never stored, and the set honours both include themes.
    assert_eq!(service.puzzle_count().unwrap(), 20);
    let puzzles = lock(&service.store)
        .unwrap()
        .set_puzzles(created.set_id.unwrap())
        .unwrap();
    assert!(puzzles.iter().all(|p| !p.themes.contains(&"mate".into())));
    assert_eq!(puzzles.iter().filter(|p| p.themes == ["pin"]).count(), 10);
}
#[test]
fn api_fallback_skips_lines_without_a_player_move_before_saving_or_creating_a_set() {
    let mut short = pool(2);
    short[0].id = "empty".into();
    short[0].moves.clear();
    short[1].id = "setup".into();
    short[1].moves = "b7f7".into();
    let good = pool(20);
    let mock = MockFetcher {
        batches: vec![short, good.clone()],
        ..Default::default()
    };
    let calls = mock.calls.clone();
    let mut service = service(Store::memory().unwrap(), mock);
    let job = service
        .submit(Job::CreateSetWithFallback {
            params: params(),
            seed: 1,
            now: 1,
        })
        .unwrap();
    let JobOutput::SetCreated(created) = finish(&mut service, job.id).unwrap() else {
        panic!()
    };
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    let store = lock(&service.store).unwrap();
    assert_eq!(store.puzzle_count().unwrap(), 20);
    assert!(store.puzzle("empty").unwrap().is_none());
    assert!(store.puzzle("setup").unwrap().is_none());
    let puzzles = store.set_puzzles(created.set_id.unwrap()).unwrap();
    assert_eq!(puzzles.len(), 20);
    assert!(puzzles.iter().all(|p| good.contains(p)));
}
#[test]
fn fallback_is_bounded_cancellable_and_never_saves_a_partial_set() {
    let mock = MockFetcher::default();
    let calls = mock.calls.clone();
    let mut service = service(Store::memory().unwrap(), mock);
    let job = service
        .submit(Job::CreateSetWithFallback {
            params: params(),
            seed: 1,
            now: 1,
        })
        .unwrap();
    let JobOutput::SetCreated(short) = finish(&mut service, job.id).unwrap() else {
        panic!()
    };
    assert_eq!(short.shortfall(), 20);
    assert_eq!(calls.load(Ordering::Relaxed), 5);
    assert_eq!(service.list_sets().unwrap(), []);
    let mut service = super::tests::service(
        Store::memory().unwrap(),
        MockFetcher {
            cancel_batch: true,
            ..Default::default()
        },
    );
    let job = service
        .submit(Job::CreateSetWithFallback {
            params: params(),
            seed: 1,
            now: 1,
        })
        .unwrap();
    assert!(matches!(
        finish(&mut service, job.id),
        Err(ServiceError::Source(SourceError::Cancelled))
    ));
    assert_eq!(service.list_sets().unwrap(), []);
}
#[test]
fn worker_startup_import_daily_by_id_fetch_and_errors() {
    let mut service =
        AppService::with_fetcher(Store::memory().unwrap(), Box::<MockFetcher>::default()).unwrap();
    let JobOutput::Imported(p) = finish(&mut service, 1).unwrap() else {
        panic!()
    };
    assert_eq!(p.accepted, 8540);
    let job = service.submit(Job::ImportStarter { force: false }).unwrap();
    assert_eq!(
        finish(&mut service, job.id).unwrap(),
        JobOutput::Imported(ImportProgress::default())
    );
    let job = service.submit(Job::ImportStarter { force: true }).unwrap();
    let JobOutput::Imported(p) = finish(&mut service, job.id).unwrap() else {
        panic!()
    };
    assert_eq!(p.accepted, 8540);
    for (job, expected) in [
        (Job::FetchDaily, JobOutput::Daily(fixture())),
        (
            Job::FetchById {
                id: "specific".into(),
            },
            {
                let mut p = fixture();
                p.id = "specific".into();
                JobOutput::Puzzle(p)
            },
        ),
    ] {
        let handle = service.submit(job).unwrap();
        assert_eq!(finish(&mut service, handle.id).unwrap(), expected);
    }
    let handle = service
        .submit(Job::Fetch {
            theme: Some("fork".into()),
            target: 1000,
            count: 20,
        })
        .unwrap();
    assert_eq!(
        finish(&mut service, handle.id).unwrap(),
        JobOutput::Fetched { inserted: 0 }
    );
    let handle = service
        .submit(Job::ImportLibrary {
            size: crate::source::LibrarySize::Full,
            limit: None,
            location: "/does/not/exist".into(),
        })
        .unwrap();
    assert!(finish(&mut service, handle.id).is_err());
    let handle = service.submit(Job::FetchDaily).unwrap();
    assert!(
        finish(&mut service, handle.id).is_ok(),
        "worker survives failed jobs"
    );
}
#[test]
fn worker_import_progress_and_queued_cancel() {
    let mut service = service(Store::memory().unwrap(), MockFetcher::default());
    let handle = service
        .submit(Job::ImportLibrary {
            size: crate::source::LibrarySize::Full,
            limit: None,
            location: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lichess.csv").into(),
        })
        .unwrap();
    let JobOutput::Imported(p) = finish(&mut service, handle.id).unwrap() else {
        panic!()
    };
    assert_eq!(p.scanned, 12);
    assert!(p.bytes > 0);
    assert_eq!(service.puzzle_count().unwrap(), p.accepted);
    let handle = service.submit(Job::ImportStarter { force: true }).unwrap();
    assert!(service.is_busy());
    assert!(service.cancel_job(handle.id));
    assert!(matches!(
        finish(&mut service, handle.id),
        Err(ServiceError::Source(SourceError::Cancelled))
    ));
    assert!(!service.is_busy());
    assert!(!service.cancel_job(handle.id));
}
#[test]
fn rejected_attempt_is_atomic_for_motivation_and_invalid_review_kind_is_rejected() {
    let mut store = Store::memory().unwrap();
    store.upsert_puzzles(&pool(20)).unwrap();
    let service = service(store, MockFetcher::default());
    let set = service.create_set(&params(), 1, 1).unwrap().set_id.unwrap();
    let position = service.start_or_resume_cycle(set, 1).unwrap();
    let attempt = attempt(&position, Outcome::Correct, 10);
    assert!(service.record_review_attempt(&attempt).is_err());
    service.record_attempt(&attempt).unwrap();
    let activity = service.motivation().unwrap();
    assert!(service.record_attempt(&attempt).is_err());
    assert_eq!(service.motivation().unwrap(), activity);
    let invalid = Settings {
        user_rating: 1,
        ..Default::default()
    };
    assert!(service.set_settings(&invalid).is_err());
}

fn clear_reviews(service: &AppService, set_id: i64, cards: Vec<ReviewCard>) {
    for card in cards {
        let mut review = Attempt {
            puzzle_id: card.puzzle.id,
            set_id,
            cycle_id: None,
            kind: AttemptKind::Review,
            outcome: Outcome::Incorrect,
            time_ms: 0,
            moves_played: vec![],
            created_at: 11 * 86400 + 43200,
        };
        service.record_review_attempt(&review).unwrap();
        assert!(service.review_count().unwrap() > 0);
        review.outcome = Outcome::Correct;
        service.record_review_attempt(&review).unwrap();
    }
}

#[test]
fn blocked_network_does_not_hold_store_lock_and_jobs_report_events_in_sequence() {
    struct BlockingFetcher {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    }
    impl PuzzleFetcher for BlockingFetcher {
        fn batch(
            &mut self,
            _: Option<&str>,
            _: &str,
            _: u32,
            cancel: &CancelFlag,
        ) -> std::result::Result<Vec<Puzzle>, SourceError> {
            self.entered.send(()).unwrap();
            self.release.recv_timeout(Duration::from_secs(5)).unwrap();
            cancel.check()?;
            Ok(vec![])
        }
        fn daily(&mut self, _: &CancelFlag) -> std::result::Result<Puzzle, SourceError> {
            Ok(fixture())
        }
        fn by_id(
            &mut self,
            _: &str,
            cancel: &CancelFlag,
        ) -> std::result::Result<Puzzle, SourceError> {
            self.daily(cancel)
        }
    }
    let store = Store::memory().unwrap();
    store
        .set_setting("starter_version", STARTER_VERSION)
        .unwrap();
    let (entered, wait_entered) = mpsc::channel();
    let (release, wait_release) = mpsc::channel();
    let mut service = AppService::with_fetcher(
        store,
        Box::new(BlockingFetcher {
            entered,
            release: wait_release,
        }),
    )
    .unwrap();
    finish(&mut service, 1).unwrap();
    let job = service
        .submit(Job::Fetch {
            theme: None,
            target: 1000,
            count: 20,
        })
        .unwrap();
    wait_entered.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(service.store.try_lock().is_ok());
    assert_eq!(service.settings().unwrap().user_rating, 1200);
    let queued = service.submit(Job::FetchDaily).unwrap();
    let events = service.poll_events();
    assert!(matches!(events.as_slice(), [Event::Started { id }] if *id == job.id));
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut events = vec![];
    loop {
        events.extend(service.poll_events());
        if events
            .iter()
            .any(|e| matches!(e, Event::Finished { id, .. } if *id == queued.id))
        {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        matches!(events.as_slice(), [Event::FetchProgress { id, batches: 1, inserted: 0 }, Event::Finished { result: Ok(_), .. }, Event::Started { .. }, Event::Finished { result: Ok(_), .. }] if *id == job.id)
    );
}

#[test]
fn concurrent_synced_answers_fill_only_one_cycle_segment() {
    let mut store = Store::memory().unwrap();
    let puzzles = pool(20);
    let set = store.create_set(&params(), &puzzles, 1, 42).unwrap();
    let cycle = store.start_cycle(set, 1).unwrap();
    let mut snapshot = store.export_sync().unwrap();
    let uid = snapshot.sets.keys().next().unwrap().clone();
    let attempt = crate::sync::format::Attempt {
        uid: "a".repeat(32),
        puzzle_id: puzzles[0].id.clone(),
        cycle_uid: Some(format!("{uid}-1")),
        kind: AttemptKind::Cycle,
        outcome: Outcome::Incorrect,
        time_ms: 1000,
        moves_played: vec![],
        created_at: 2,
        local_day: None,
    };
    let mut second = attempt.clone();
    second.uid = "b".repeat(32);
    second.outcome = Outcome::Correct;
    snapshot.attempts.insert(uid, vec![attempt, second]);
    store.import_sync(&snapshot).unwrap();
    let service = service(store, MockFetcher::default());
    assert_eq!(
        service.cycle_outcomes(cycle.id).unwrap(),
        vec![Outcome::Incorrect]
    );
    assert_eq!(service.current_puzzle(cycle.id).unwrap().position, 1);
}

#[test]
fn worker_library_job_obeys_limit_and_records_local_markers() {
    use crate::source::LibrarySize;
    let mut service = service(Store::memory().unwrap(), MockFetcher::default());
    let handle = service
        .submit(Job::ImportLibrary {
            location: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lichess.csv").into(),
            size: LibrarySize::Medium,
            limit: Some(2),
        })
        .unwrap();
    let JobOutput::Imported(p) = finish(&mut service, handle.id).unwrap() else {
        panic!()
    };
    assert_eq!(p.accepted, 2);
    assert_eq!(service.puzzle_count().unwrap(), 2);
    assert_eq!(
        service.local_setting("library_size").unwrap().as_deref(),
        Some("medium")
    );
    assert!(
        service
            .local_setting("library_imported_at")
            .unwrap()
            .is_some()
    );
    service
        .set_local_setting("library_imported_at", "1")
        .unwrap();
    let handle = service
        .submit(Job::ImportLibrary {
            location: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lichess.csv").into(),
            size: LibrarySize::Full,
            limit: Some(2),
        })
        .unwrap();
    handle.cancel.cancel();
    assert!(finish(&mut service, handle.id).is_err());
    assert_eq!(
        service.local_setting("library_size").unwrap().as_deref(),
        Some("medium")
    );
    assert_eq!(
        service
            .local_setting("library_imported_at")
            .unwrap()
            .as_deref(),
        Some("1")
    );
}

#[test]
fn onboarding_is_local_and_existing_libraries_are_marked_silently() {
    let first = service(Store::memory().unwrap(), MockFetcher::default());
    assert!(first.needs_onboarding().unwrap());
    first.set_local_setting("onboarding_done", "true").unwrap();
    first.set_local_setting("library_size", "full").unwrap();
    first
        .set_local_setting("library_imported_at", "42")
        .unwrap();
    assert!(!first.needs_onboarding().unwrap());
    let state = lock(&first.store).unwrap().export_sync().unwrap();
    for key in ["onboarding_done", "library_size", "library_imported_at"] {
        assert!(!state.settings.contains_key(key));
    }
    let mut fresh = Store::memory().unwrap();
    fresh.import_sync(&state).unwrap();
    let second = service(fresh, MockFetcher::default());
    assert!(second.needs_onboarding().unwrap());
    let mut existing = Store::memory().unwrap();
    existing.upsert_puzzles(&pool(9541)).unwrap();
    let existing = service(existing, MockFetcher::default());
    assert!(!existing.needs_onboarding().unwrap());
    assert_eq!(
        existing
            .local_setting("onboarding_done")
            .unwrap()
            .as_deref(),
        Some("true")
    );
}
