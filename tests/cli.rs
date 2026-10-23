use std::process::{Command, Output};
use woodpeck::store::Store;
fn run(args: &[&str], data: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_woodpeck"))
        .args(args)
        .env("XDG_DATA_HOME", data)
        .env_remove("WOODPECK_DATA_DIR")
        .output()
        .unwrap()
}
#[test]
fn help_version_and_argument_errors_do_not_create_a_database() {
    let dir = tempfile::tempdir().unwrap();
    for args in [&["--help"][..], &["--version"], &["import", "--help"]] {
        let output = run(args, dir.path());
        assert!(output.status.success());
        assert_ne!(output.stdout, Vec::<u8>::new());
    }
    assert_eq!(run(&["import"], dir.path()).status.code(), Some(2));
    assert!(!dir.path().join("woodpeck").exists());
}
#[test]
fn imports_plain_and_zstd_files_and_first_run_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lichess.csv");
    let output = run(&["import", fixture], dir.path());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let db = dir.path().join("woodpeck/woodpeck.db");
    let count = Store::open(&db).unwrap().puzzle_count().unwrap();
    assert!(count > 0);
    let compressed = dir.path().join("puzzles.zst");
    std::fs::write(
        &compressed,
        zstd::stream::encode_all(std::fs::File::open(fixture).unwrap(), 1).unwrap(),
    )
    .unwrap();
    assert!(
        run(&["import", compressed.to_str().unwrap()], dir.path())
            .status
            .success()
    );
    assert_eq!(Store::open(&db).unwrap().puzzle_count().unwrap(), count);
    startup_import(&db);
    let store = Store::open(&db).unwrap();
    assert!(store.puzzle_count().unwrap() >= 8540);
    assert_eq!(
        store.setting("starter_version").unwrap().as_deref(),
        Some(woodpeck::source::STARTER_VERSION)
    );
    let before = store.puzzle_count().unwrap();
    startup_import(&db);
    assert_eq!(store.puzzle_count().unwrap(), before);
    assert_eq!(
        run(&["import", "/missing/puzzles.csv"], dir.path())
            .status
            .code(),
        Some(1)
    );
}

fn startup_import(path: &std::path::Path) {
    let mut service = woodpeck::service::AppService::open(path).unwrap();
    let start = std::time::Instant::now();
    while service.is_busy() {
        for event in service.poll_events() {
            if let woodpeck::service::Event::Finished { result, .. } = event {
                result.unwrap();
            }
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
#[test]
fn data_override_is_used_by_import_without_touching_xdg_location() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lichess.csv");
    let output = Command::new(env!("CARGO_BIN_EXE_woodpeck"))
        .args(["import", fixture])
        .env("WOODPECK_DATA_DIR", dir.path().join("portable"))
        .env("XDG_DATA_HOME", dir.path().join("xdg"))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(dir.path().join("portable/woodpeck.db").is_file());
    assert!(!dir.path().join("xdg").exists());
}
