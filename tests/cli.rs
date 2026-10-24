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

#[test]
fn sync_cli_initializes_reports_and_disables_using_only_local_data() {
    if !Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        eprintln!("Skipping Git CLI test: git is not installed.");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let remote = dir.path().join("remote.git");
    assert!(
        Command::new("git")
            .args(["init", "--bare", "--initial-branch=main"])
            .arg(&remote)
            .output()
            .unwrap()
            .status
            .success()
    );
    let output = run(&["sync", "status"], dir.path());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Disabled"));
    let output = run(&["sync", "init", remote.to_str().unwrap()], dir.path());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(run(&["sync", "now"], dir.path()).status.success());
    let output = run(&["sync", "status"], dir.path());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Synced"));
    assert!(run(&["sync", "disable"], dir.path()).status.success());
    assert!(!run(&["sync", "now"], dir.path()).status.success());
    let store = Store::open(&dir.path().join("woodpeck/woodpeck.db")).unwrap();
    assert!(!woodpeck::sync::config(&store).unwrap().enabled);
    let json = woodpeck::sync::format::pretty(&store.export_sync().unwrap()).unwrap();
    assert!(!json.contains(remote.to_str().unwrap()));
}

#[cfg(unix)]
#[test]
fn create_uses_authenticated_private_repo_and_reports_gh_errors() {
    use std::os::unix::fs::PermissionsExt;
    if !Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        eprintln!("Skipping gh adapter test: git is not installed.");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let remote = dir.path().join("remote.git");
    assert!(
        Command::new("git")
            .args(["init", "--bare", "--initial-branch=main"])
            .arg(&remote)
            .output()
            .unwrap()
            .status
            .success()
    );
    let command = |mode: &str, data: &std::path::Path, path: &std::ffi::OsStr| {
        Command::new(env!("CARGO_BIN_EXE_woodpeck"))
            .args(["sync", "init", "--create"])
            .env("WOODPECK_DATA_DIR", data)
            .env("PATH", path)
            .env("FAKE_GH_MODE", mode)
            .env("FAKE_SYNC_REMOTE", &remote)
            .env("FAKE_GH_CREATED", dir.path().join("created"))
            .output()
            .unwrap()
    };
    let missing = command("missing", &dir.path().join("missing"), bin.as_os_str());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("gh is missing"));
    let gh = bin.join("gh");
    std::fs::write(
        &gh,
        r#"#!/bin/sh
case "$1 $2" in
  'auth status') [ "$FAKE_GH_MODE" != unauthenticated ]; exit $? ;;
  'api user') printf 'example-user\n'; exit 0 ;;
  'repo view')
    if [ "$FAKE_GH_MODE" = create ] && [ ! -f "$FAKE_GH_CREATED" ]; then exit 1; fi
    private=true
    if [ "$FAKE_GH_MODE" = public ]; then private=false; fi
    printf '{"url":"%s","isPrivate":%s}\n' "$FAKE_SYNC_REMOTE" "$private"
    exit 0 ;;
  'repo create')
    [ "$3" = example-user/woodpeck-data ] && [ "$4" = --private ] || exit 2
    : > "$FAKE_GH_CREATED"
    exit 0 ;;
esac
exit 2
"#,
    )
    .unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = std::env::join_paths(std::iter::once(bin.clone()).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    let unauth = command("unauthenticated", &dir.path().join("unauth"), &path);
    assert!(String::from_utf8_lossy(&unauth.stderr).contains("not authenticated"));
    let public = command("public", &dir.path().join("public"), &path);
    assert!(String::from_utf8_lossy(&public.stderr).contains("is public"));
    let reuse = command("reuse", &dir.path().join("reuse"), &path);
    assert!(
        reuse.status.success(),
        "{}",
        String::from_utf8_lossy(&reuse.stderr)
    );
    assert!(!dir.path().join("created").exists());
    let created = command("create", &dir.path().join("new"), &path);
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    assert!(dir.path().join("created").exists());
}

#[test]
fn limited_file_import_reports_summary_and_persists_selected_size() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lichess.csv");
    let output = run(
        &["import", fixture, "--size", "medium", "--limit", "2"],
        dir.path(),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary = String::from_utf8_lossy(&output.stdout);
    assert!(summary.contains("Imported 2 puzzles"));
    assert!(summary.contains("bytes"));
    assert!(summary.contains("database bytes"));
    let store = Store::open(&dir.path().join("woodpeck/woodpeck.db")).unwrap();
    assert_eq!(store.puzzle_count().unwrap(), 2);
    assert_eq!(
        store.setting("library_size").unwrap().as_deref(),
        Some("medium")
    );
    assert!(store.setting("library_imported_at").unwrap().is_some());
    assert!(store.setting("onboarding_done").unwrap().is_none());
}
