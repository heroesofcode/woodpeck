//! The browser's GitHub remote against an in-memory GitHub, and against a desktop Git remote.
//! No network is used.
#![allow(
    clippy::too_many_lines,
    clippy::match_same_arms,
    clippy::cast_possible_truncation,
    clippy::unused_async_trait_impl
)]
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    path::Path,
    process::Command,
    time::Duration,
};
use woodpeck::{
    puzzle::{Outcome, Puzzle},
    sets::BuilderParams,
    stats::{Attempt, AttemptKind},
    store::Store,
    sync::{
        self,
        github::{GitHub, Http, Request, Response},
        remote::exchange_async,
    },
};

enum Object {
    Blob(String),
    Tree(BTreeMap<String, String>),
    Commit { tree: String, parents: Vec<String> },
}
/// Just enough of the GitHub Git Data API, with fast-forward checks on ref updates.
#[derive(Default)]
struct Hub {
    objects: RefCell<BTreeMap<String, Object>>,
    refs: RefCell<BTreeMap<String, String>>,
    next: Cell<u32>,
    requests: RefCell<Vec<String>>,
    /// Pushes to reject as if another device pushed first.
    races: Cell<u32>,
}
impl Hub {
    fn add(&self, object: Object) -> String {
        self.next.set(self.next.get() + 1);
        let sha = format!("{:040x}", self.next.get());
        self.objects.borrow_mut().insert(sha.clone(), object);
        sha
    }
    fn blob(&self, text: &str) -> String {
        self.add(Object::Blob(text.into()))
    }
    fn commit_files(&self, files: &BTreeMap<String, String>, parents: Vec<String>) -> String {
        let tree = files
            .iter()
            .map(|(path, text)| (path.clone(), self.blob(text)))
            .collect();
        let tree = self.add(Object::Tree(tree));
        self.add(Object::Commit { tree, parents })
    }
    fn tree_of(&self, commit: &str) -> BTreeMap<String, String> {
        let objects = self.objects.borrow();
        let Some(Object::Commit { tree, .. }) = objects.get(commit) else {
            panic!("no commit {commit}")
        };
        let Some(Object::Tree(tree)) = objects.get(tree) else {
            panic!("no tree")
        };
        tree.clone()
    }
    fn files(&self, branch: &str) -> BTreeMap<String, String> {
        let head = self.refs.borrow()[&format!("refs/heads/{branch}")].clone();
        let objects = self.objects.borrow();
        self.tree_of(&head)
            .into_iter()
            .map(|(path, blob)| match &objects[&blob] {
                Object::Blob(text) => (path, text.clone()),
                _ => panic!("not a blob"),
            })
            .collect()
    }
    fn descends(&self, commit: &str, ancestor: &str) -> bool {
        if commit == ancestor {
            return true;
        }
        let parents = match self.objects.borrow().get(commit) {
            Some(Object::Commit { parents, .. }) => parents.clone(),
            _ => return false,
        };
        parents.iter().any(|p| self.descends(p, ancestor))
    }
    fn route(&self, request: &Request) -> (u16, serde_json::Value, Vec<u8>) {
        use serde_json::json;
        let path = request
            .path
            .strip_prefix("/repos/me/data")
            .expect("only the configured repository is used");
        let path = path.split('?').next().unwrap();
        let body = request.body.clone().unwrap_or_default();
        let empty = self.refs.borrow().is_empty();
        match (request.method, path) {
            ("GET", "/git/matching-refs/heads/") if empty => (409, json!({}), vec![]),
            ("GET", "/git/matching-refs/heads/") => {
                let refs: Vec<_> = self
                    .refs
                    .borrow()
                    .iter()
                    .map(|(name, sha)| json!({"ref": name, "object": {"sha": sha}}))
                    .collect();
                (200, json!(refs), vec![])
            }
            ("GET", p) if p.starts_with("/git/commits/") => {
                match self.objects.borrow().get(&p[13..]) {
                    Some(Object::Commit { tree, .. }) => {
                        (200, json!({"tree": {"sha": tree}}), vec![])
                    }
                    _ => (404, json!({}), vec![]),
                }
            }
            ("GET", p) if p.starts_with("/git/trees/") => match self.objects.borrow().get(&p[11..])
            {
                Some(Object::Tree(entries)) => {
                    let entries: Vec<_> = entries
                        .iter()
                        .map(|(path, sha)| json!({"path": path, "type": "blob", "sha": sha}))
                        .collect();
                    (200, json!({"tree": entries, "truncated": false}), vec![])
                }
                _ => (404, json!({}), vec![]),
            },
            ("GET", p) if p.starts_with("/git/blobs/") && request.raw => {
                match self.objects.borrow().get(&p[11..]) {
                    Some(Object::Blob(text)) => (200, serde_json::Value::Null, text.clone().into()),
                    _ => (404, json!({}), vec![]),
                }
            }
            ("POST", "/git/trees") if empty => (409, json!({}), vec![]),
            ("POST", "/git/trees") => {
                let mut tree = match body["base_tree"].as_str() {
                    Some(base) => match self.objects.borrow().get(base) {
                        Some(Object::Tree(tree)) => tree.clone(),
                        _ => return (422, json!({}), vec![]),
                    },
                    None => BTreeMap::new(),
                };
                for entry in body["tree"].as_array().unwrap() {
                    assert_eq!(entry["mode"], "100644");
                    let blob = self.blob(entry["content"].as_str().unwrap());
                    tree.insert(entry["path"].as_str().unwrap().into(), blob);
                }
                (201, json!({"sha": self.add(Object::Tree(tree))}), vec![])
            }
            ("POST", "/git/commits") => {
                let parents = body["parents"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p.as_str().unwrap().to_owned())
                    .collect();
                let tree = body["tree"].as_str().unwrap().to_owned();
                (
                    201,
                    json!({"sha": self.add(Object::Commit { tree, parents })}),
                    vec![],
                )
            }
            ("POST", "/git/refs") => {
                let name = body["ref"].as_str().unwrap().to_owned();
                if self.refs.borrow().contains_key(&name) {
                    return (422, json!({}), vec![]);
                }
                self.refs
                    .borrow_mut()
                    .insert(name, body["sha"].as_str().unwrap().into());
                (201, json!({}), vec![])
            }
            ("PATCH", p) if p.starts_with("/git/refs/heads/") => {
                let name = format!("refs/heads/{}", &p[16..]);
                let new = body["sha"].as_str().unwrap().to_owned();
                if self.races.get() > 0 {
                    self.races.set(self.races.get() - 1);
                    return (422, json!({}), vec![]);
                }
                let current = self.refs.borrow()[&name].clone();
                if !self.descends(&new, &current) {
                    return (
                        422,
                        json!({"message": "Update is not a fast forward"}),
                        vec![],
                    );
                }
                self.refs.borrow_mut().insert(name, new);
                (200, json!({}), vec![])
            }
            ("PUT", "/contents/manifest.json") if empty => {
                let text = decode(body["content"].as_str().unwrap());
                let files = BTreeMap::from([("manifest.json".to_owned(), text)]);
                let commit = self.commit_files(&files, vec![]);
                self.refs
                    .borrow_mut()
                    .insert("refs/heads/main".into(), commit.clone());
                (201, json!({"commit": {"sha": commit}}), vec![])
            }
            _ => panic!("unexpected request {} {path}", request.method),
        }
    }
}
fn decode(text: &str) -> String {
    let value = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        _ => 63,
    };
    let mut bytes = vec![];
    for chunk in text.as_bytes().chunks(4) {
        let n = chunk
            .iter()
            .enumerate()
            .filter(|(_, c)| **c != b'=')
            .fold(0u32, |n, (i, c)| n | u32::from(value(*c)) << (18 - 6 * i));
        let len = chunk.iter().filter(|c| **c != b'=').count() - 1;
        for i in 0..len {
            bytes.push((n >> (16 - 8 * i)) as u8);
        }
    }
    String::from_utf8(bytes).unwrap()
}
impl Http for &Hub {
    async fn send(&self, request: Request) -> sync::Result<Response> {
        self.requests
            .borrow_mut()
            .push(format!("{} {}", request.method, request.path));
        let (status, json, raw) = self.route(&request);
        let body = if request.raw {
            raw
        } else {
            serde_json::to_vec(&json).unwrap()
        };
        Ok(Response { status, body })
    }
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
fn trained() -> (Store, i64, i64) {
    let mut store = Store::memory().unwrap();
    let set = store
        .create_set(
            &BuilderParams {
                size: 20,
                target_cycles: 2,
                ..Default::default()
            },
            &puzzles(),
            1,
            42,
        )
        .unwrap();
    let cycle = store.start_cycle(set, 1).unwrap().id;
    (store, set, cycle)
}
fn train(store: &mut Store, set_id: i64, cycle_id: i64, id: &str, outcome: Outcome, at: i64) {
    store
        .record_attempt(&Attempt {
            puzzle_id: id.into(),
            set_id,
            cycle_id: Some(cycle_id),
            kind: AttemptKind::Cycle,
            outcome,
            time_ms: 1234,
            moves_played: vec![],
            created_at: at,
        })
        .unwrap();
}
fn exchange(store: &mut Store, hub: &Hub) -> (i64, bool) {
    let remote = GitHub::new(hub, "me/data").unwrap();
    pollster::block_on(exchange_async(store, &remote)).unwrap()
}

#[test]
fn empty_repository_receives_the_desktop_file_layout() {
    let hub = Hub::default();
    let (mut a, set, cycle) = trained();
    train(&mut a, set, cycle, "p00", Outcome::Incorrect, 2);
    exchange(&mut a, &hub);
    assert_eq!(hub.files("main"), a.export_sync().unwrap().files().unwrap());
}

#[test]
fn two_browsers_merge_through_github_and_skip_unchanged_files() {
    let hub = Hub::default();
    let (mut a, aset, acycle) = trained();
    exchange(&mut a, &hub);
    let mut b = Store::memory().unwrap();
    exchange(&mut b, &hub);
    let bset = b.sets().unwrap()[0].id;
    let bcycle = b.cycles(bset).unwrap()[0].id;
    train(&mut a, aset, acycle, "p00", Outcome::Incorrect, 2);
    train(&mut b, bset, bcycle, "p00", Outcome::Correct, 3);
    exchange(&mut a, &hub);
    let (_, merged) = exchange(&mut b, &hub);
    assert!(merged);
    exchange(&mut a, &hub);
    assert_eq!(a.attempts(aset).unwrap().len(), 2);
    assert_eq!(a.export_sync().unwrap(), b.export_sync().unwrap());
    // Nothing changed since: no new tree, commit or ref update.
    hub.requests.borrow_mut().clear();
    exchange(&mut a, &hub);
    assert!(
        hub.requests.borrow().iter().all(|r| r.starts_with("GET ")),
        "{:?}",
        hub.requests.borrow()
    );
}

#[test]
fn a_concurrent_push_is_merged_and_retried() {
    let hub = Hub::default();
    let (mut a, set, cycle) = trained();
    exchange(&mut a, &hub);
    train(&mut a, set, cycle, "p00", Outcome::Incorrect, 2);
    hub.races.set(2);
    exchange(&mut a, &hub);
    assert_eq!(hub.files("main"), a.export_sync().unwrap().files().unwrap());
}

#[test]
fn a_newer_schema_on_github_is_never_overwritten() {
    let hub = Hub::default();
    let manifest = r#"{"app":"woodpeck","schema_version":999}"#;
    let files = BTreeMap::from([("manifest.json".to_owned(), manifest.to_owned())]);
    let commit = hub.commit_files(&files, vec![]);
    hub.refs
        .borrow_mut()
        .insert("refs/heads/main".into(), commit);
    let (mut a, _, _) = trained();
    let remote = GitHub::new(&hub, "me/data").unwrap();
    let result = pollster::block_on(exchange_async(&mut a, &remote));
    assert!(
        matches!(result, Err(sync::SyncError::NewerSchema(999))),
        "{result:?}"
    );
    assert_eq!(hub.files("main"), files);
}

fn git(path: &Path, args: &[&str]) -> Option<String> {
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
        .ok()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Some(String::from_utf8(output.stdout).unwrap().trim().into())
}

/// Desktop Git sync and the browser's GitHub remote read each other's snapshots.
#[test]
fn desktop_git_and_browser_github_interoperate() {
    let temp = tempfile::tempdir().unwrap();
    if git(temp.path(), &["--version"]).is_none() {
        eprintln!("Skipping: git is not installed.");
        return;
    }
    let bare = temp.path().join("remote.git");
    std::fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "--bare", "--initial-branch=main"]);
    let desktop_dir = temp.path().join("desktop");
    let mut desktop = Store::open(&desktop_dir.join("woodpeck.db")).unwrap();
    let set = desktop
        .create_set(
            &BuilderParams {
                size: 20,
                target_cycles: 2,
                ..Default::default()
            },
            &puzzles(),
            1,
            42,
        )
        .unwrap();
    let cycle = desktop.start_cycle(set, 1).unwrap().id;
    train(&mut desktop, set, cycle, "p00", Outcome::Incorrect, 2);
    sync::init(&mut desktop, &desktop_dir, Some(bare.to_str().unwrap())).unwrap();

    // Mirror the desktop's commit into the in-memory GitHub.
    let paths = git(&bare, &["ls-tree", "-r", "--name-only", "main"]).unwrap();
    let files: BTreeMap<String, String> = paths
        .lines()
        .map(|p| {
            (
                p.to_owned(),
                git(&bare, &["show", &format!("main:{p}")]).unwrap() + "\n",
            )
        })
        .collect();
    let hub = Hub::default();
    let commit = hub.commit_files(&files, vec![]);
    hub.refs
        .borrow_mut()
        .insert("refs/heads/main".into(), commit);

    let mut browser = Store::memory().unwrap();
    exchange(&mut browser, &hub);
    assert_eq!(
        browser.export_sync().unwrap(),
        desktop.export_sync().unwrap()
    );
    let bset = browser.sets().unwrap()[0].id;
    let bcycle = browser.cycles(bset).unwrap()[0].id;
    train(&mut browser, bset, bcycle, "p01", Outcome::Correct, 3);
    exchange(&mut browser, &hub);

    // Mirror the browser's commit back as a desktop-pushed commit, then sync the desktop.
    let work = temp.path().join("work");
    git(
        temp.path(),
        &["clone", "-q", bare.to_str().unwrap(), "work"],
    );
    for (path, text) in hub.files("main") {
        let file = work.join(&path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    git(&work, &["add", "-A"]);
    git(&work, &["commit", "-q", "-m", "browser"]);
    git(&work, &["push", "-q", "origin", "HEAD:main"]);
    sync::synchronize(&mut desktop, &desktop_dir, Duration::from_secs(20)).unwrap();
    assert_eq!(desktop.attempts(set).unwrap().len(), 2);
    assert_eq!(
        desktop.export_sync().unwrap(),
        browser.export_sync().unwrap()
    );
}
