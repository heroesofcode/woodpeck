use std::process::{Command, Output};
use woodpeck::store::Store;
fn run(args: &[&str], data: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_woodpeck"))
        .args(args)
        .env("XDG_DATA_HOME", data)
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
    assert!(run(&[], dir.path()).status.success());
    let store = Store::open(&db).unwrap();
    assert!(store.puzzle_count().unwrap() >= 8540);
    assert_eq!(
        store.setting("starter_version").unwrap().as_deref(),
        Some(woodpeck::source::STARTER_VERSION)
    );
    let before = store.puzzle_count().unwrap();
    assert!(run(&[], dir.path()).status.success());
    assert_eq!(store.puzzle_count().unwrap(), before);
    assert_eq!(
        run(&["import", "/missing/puzzles.csv"], dir.path())
            .status
            .code(),
        Some(1)
    );
}
