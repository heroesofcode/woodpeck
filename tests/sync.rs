//! No network, global Git configuration, or files in the source directory are used here.
use std::{path::Path, process::Command, time::Duration};
use woodpeck::{
    puzzle::{Outcome, Puzzle},
    sets::BuilderParams,
    stats::{Attempt, AttemptKind},
    store::Store,
    sync::{self, Status},
};
fn git_available() -> bool {
    let available = Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !available {
        eprintln!("Skipping Git sync test: git is not installed.");
    }
    available
}
fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
        ])
        .args(args)
        .current_dir(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn bare(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    git(path, &["init", "--bare", "--initial-branch=main"]);
}
fn puzzles() -> Vec<Puzzle> {
    (0..20)
        .map(|i| Puzzle {
            id: format!("p{i:02}"),
            fen: "5r1k/1R4b1/p2P2Pp/1p5n/2p5/P4B1P/1pP5/5R1K w - - 1 32".into(),
            moves: "b7f7 h5g3 h1g2 g3f1".into(),
            rating: 1020,
            rd: 78,
            popularity: 100,
            nb_plays: 100,
            themes: vec!["fork".into()],
        })
        .collect()
}
fn train(
    store: &mut Store,
    set_id: i64,
    cycle_id: Option<i64>,
    id: &str,
    outcome: Outcome,
    at: i64,
) {
    store
        .record_attempt(&Attempt {
            puzzle_id: id.into(),
            set_id,
            cycle_id,
            kind: if cycle_id.is_some() {
                AttemptKind::Cycle
            } else {
                AttemptKind::Review
            },
            outcome,
            time_ms: 1234,
            moves_played: vec![],
            created_at: at,
        })
        .unwrap();
}
fn now(store: &mut Store, data: &Path) {
    sync::synchronize(store, data, Duration::from_secs(20)).unwrap();
}
#[test]
fn two_machines_union_progress_review_and_latest_preferences() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let remote = temp.path().join("remote.git");
    bare(&remote);
    let adir = temp.path().join("a");
    let bdir = temp.path().join("b");
    let mut a = Store::open(&adir.join("woodpeck.db")).unwrap();
    let aid = a
        .create_set(
            &BuilderParams {
                size: 20,
                target_cycles: 2,
                ..Default::default()
            },
            &puzzles(),
            1,
            42,
            1200,
        )
        .unwrap();
    let ac = a.start_cycle(aid, 1).unwrap().id;
    sync::init(&mut a, &adir, Some(remote.to_str().unwrap())).unwrap();
    let mut b = Store::open(&bdir.join("woodpeck.db")).unwrap();
    sync::init(&mut b, &bdir, Some(remote.to_str().unwrap())).unwrap();
    assert_eq!(b.puzzle_count().unwrap(), 0);
    assert_eq!(b.set_puzzles(b.sets().unwrap()[0].id).unwrap(), puzzles());
    let bid = b.sets().unwrap()[0].id;
    let bc = b.cycles(bid).unwrap()[0].id;
    train(&mut a, aid, Some(ac), "p00", Outcome::Incorrect, 2);
    train(&mut b, bid, Some(bc), "p00", Outcome::Skipped, 3);
    train(&mut a, aid, None, "p00", Outcome::Correct, 4);
    train(&mut b, bid, Some(bc), "p01", Outcome::Incorrect, 5);
    let mut settings = a.settings().unwrap();
    settings.auto_advance = true;
    a.save_settings(&settings).unwrap();
    let mut settings = b.settings().unwrap();
    settings.user_rating = 1400;
    b.save_settings(&settings).unwrap();
    now(&mut a, &adir);
    now(&mut b, &bdir);
    now(&mut a, &adir);
    assert_eq!(a.attempts(aid).unwrap().len(), 4);
    assert_eq!(a.export_sync().unwrap(), b.export_sync().unwrap());
    assert_eq!(a.pending_review_count().unwrap(), 1);
    assert_eq!(
        a.review_cards(None, woodpeck::review::ReviewSort::MostRecent)
            .unwrap()[0]
            .puzzle
            .id,
        "p01"
    );
    assert!(a.settings().unwrap().auto_advance);
    assert_eq!(a.settings().unwrap().user_rating, 1400);
    assert_eq!(a.cycle_stats(ac).unwrap().attempted(), 2);
    // Both offline machines can start the same next cycle after completion.
    for i in 2..20 {
        train(
            &mut a,
            aid,
            Some(ac),
            &format!("p{i:02}"),
            Outcome::Correct,
            10 + i,
        );
    }
    now(&mut a, &adir);
    now(&mut b, &bdir);
    let ac2 = a.start_cycle(aid, 40).unwrap().id;
    let bc2 = b.start_cycle(bid, 41).unwrap().id;
    train(&mut a, aid, Some(ac2), "p00", Outcome::Incorrect, 42);
    train(&mut b, bid, Some(bc2), "p00", Outcome::Correct, 43);
    now(&mut a, &adir);
    now(&mut b, &bdir);
    now(&mut a, &adir);
    assert_eq!(a.cycles(aid).unwrap().len(), 2);
    assert_eq!(a.export_sync().unwrap(), b.export_sync().unwrap());
    assert_eq!(a.attempts(aid).unwrap().len(), 24);
    assert_eq!(
        git(&remote, &["log", "-1", "--format=%an <%ae>"]),
        "woodpeck <woodpeck@localhost>"
    );
    let files = git(&remote, &["ls-tree", "-r", "--name-only", "main"]);
    assert!(!files.contains("woodpeck.db"));
    assert!(!files.contains("config"));
}
#[test]
fn exhausted_budget_preserves_success_and_pending_tracks_local_changes() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let remote = temp.path().join("remote.git");
    bare(&remote);
    let data = temp.path().join("machine");
    let mut store = Store::open(&data.join("woodpeck.db")).unwrap();
    let synced = sync::init(&mut store, &data, Some(remote.to_str().unwrap())).unwrap();
    let persisted = store.setting("sync_config").unwrap();
    assert_eq!(
        sync::synchronize(&mut store, &data, Duration::ZERO).unwrap(),
        synced
    );
    assert_eq!(store.setting("sync_config").unwrap(), persisted);

    std::fs::remove_dir_all(&remote).unwrap();
    assert!(sync::synchronize(&mut store, &data, Duration::from_secs(5)).is_err());
    assert!(!sync::config(&store).unwrap().pending);
    let mut settings = store.settings().unwrap();
    settings.auto_advance = true;
    store.save_settings(&settings).unwrap();
    assert!(
        sync::synchronize(&mut store, &data, Duration::ZERO)
            .unwrap()
            .pending
    );
    assert!(sync::synchronize(&mut store, &data, Duration::from_secs(5)).is_err());
    assert!(sync::config(&store).unwrap().pending);
}

#[test]
fn offline_pending_survives_restart_then_recovers() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("machine");
    let remote = temp.path().join("unavailable.git");
    let db = data.join("woodpeck.db");
    let mut store = Store::open(&db).unwrap();
    sync::configure(&store, remote.to_str().unwrap(), true).unwrap();
    assert!(sync::synchronize(&mut store, &data, Duration::from_secs(5)).is_err());
    drop(store);
    let mut store = Store::open(&db).unwrap();
    let cfg = sync::config(&store).unwrap();
    assert!(cfg.pending);
    assert_eq!(cfg.status, Status::Offline);
    assert!(cfg.last_error.is_some());
    bare(&remote);
    now(&mut store, &data);
    let cfg = sync::config(&store).unwrap();
    assert!(!cfg.pending);
    assert!(cfg.last_sync.is_some());
    assert!(cfg.last_error.is_none());
    let head = git(&remote, &["rev-parse", "main"]);
    now(&mut store, &data);
    assert_eq!(git(&remote, &["rev-parse", "main"]), head);
}
#[test]
fn future_remote_schema_is_never_overwritten() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let remote = temp.path().join("remote.git");
    bare(&remote);
    let data = temp.path().join("a");
    let mut store = Store::open(&data.join("woodpeck.db")).unwrap();
    sync::init(&mut store, &data, Some(remote.to_str().unwrap())).unwrap();
    std::fs::write(
        data.join("sync/manifest.json"),
        "{\"app\":\"woodpeck\",\"schema_version\":99}\n",
    )
    .unwrap();
    git(&data.join("sync"), &["add", "manifest.json"]);
    git(
        &data.join("sync"),
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "Future schema",
        ],
    );
    git(&data.join("sync"), &["push", "origin", "main"]);
    let before = git(&remote, &["rev-parse", "main"]);
    assert!(matches!(
        sync::synchronize(&mut store, &data, Duration::from_secs(10)),
        Err(sync::SyncError::NewerSchema(99))
    ));
    assert_eq!(git(&remote, &["rev-parse", "main"]), before);
    assert_eq!(sync::config(&store).unwrap().status, Status::Error);
}

#[test]
fn worker_pulls_at_start_and_flushes_pending_progress_on_close() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let remote = temp.path().join("remote.git");
    bare(&remote);
    let a_data = temp.path().join("a");
    let mut a = Store::open(&a_data.join("woodpeck.db")).unwrap();
    let set = a
        .create_set(
            &BuilderParams {
                size: 20,
                ..Default::default()
            },
            &puzzles(),
            1,
            42,
            1200,
        )
        .unwrap();
    let cycle = a.start_cycle(set, 1).unwrap().id;
    sync::init(&mut a, &a_data, Some(remote.to_str().unwrap())).unwrap();
    let b_data = temp.path().join("b");
    let db = b_data.join("woodpeck.db");
    let mut b = Store::open(&db).unwrap();
    let cfg = sync::configure(&b, remote.to_str().unwrap(), true).unwrap();
    let mut worker = sync::worker::Worker::start(&db, cfg).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        worker.poll();
        if worker.status().last_sync.is_some() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "worker status: {:?}",
            worker.status()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let bset = b.sets().unwrap()[0].id;
    let bcycle = b.cycles(bset).unwrap()[0].id;
    train(&mut b, bset, Some(bcycle), "p00", Outcome::Skipped, 2);
    assert!(sync::config(&b).unwrap().pending);
    let start = std::time::Instant::now();
    worker.shutdown_handle().finish();
    assert!(start.elapsed() < Duration::from_secs(4));
    now(&mut a, &a_data);
    assert_eq!(a.attempts(set).unwrap().len(), 1);
    assert_eq!(a.cycle_progress(cycle).unwrap().next_puzzle(), Some("p01"));
}
#[test]
fn simultaneous_pushes_converge_without_losing_attempts() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let remote = temp.path().join("remote.git");
    bare(&remote);
    let adir = temp.path().join("a");
    let bdir = temp.path().join("b");
    let mut a = Store::open(&adir.join("woodpeck.db")).unwrap();
    let mut b = Store::open(&bdir.join("woodpeck.db")).unwrap();
    let aid = a
        .create_set(
            &BuilderParams {
                size: 20,
                ..Default::default()
            },
            &puzzles(),
            1,
            42,
            1200,
        )
        .unwrap();
    let ac = a.start_cycle(aid, 1).unwrap().id;
    sync::init(&mut a, &adir, Some(remote.to_str().unwrap())).unwrap();
    sync::init(&mut b, &bdir, Some(remote.to_str().unwrap())).unwrap();
    let bid = b.sets().unwrap()[0].id;
    let bc = b.cycles(bid).unwrap()[0].id;
    train(&mut a, aid, Some(ac), "p00", Outcome::Correct, 2);
    train(&mut b, bid, Some(bc), "p00", Outcome::Skipped, 3);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let other = barrier.clone();
    let a_path = adir.clone();
    let thread = std::thread::spawn(move || {
        other.wait();
        now(&mut a, &a_path);
        a
    });
    barrier.wait();
    now(&mut b, &bdir);
    let mut a = thread.join().unwrap();
    now(&mut a, &adir);
    now(&mut b, &bdir);
    assert_eq!(a.export_sync().unwrap(), b.export_sync().unwrap());
    assert_eq!(a.attempts(aid).unwrap().len(), 2);
}
