//! Sync remote over the GitHub REST Git Data API, for targets without system Git.
//!
//! It stores exactly the files the desktop Git transport stores, at the same paths and with the
//! same contents, so both read and write the same repository. Unchanged files are not uploaded.
use super::{
    Result, SyncError,
    format::{Manifest, SCHEMA_VERSION, State},
    remote::{AsyncRemote, choose_branch},
};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap};

/// The commit message the desktop transport uses too.
const MESSAGE: &str = "Sync woodpeck progress";

pub struct Request {
    pub method: &'static str,
    /// Path below `https://api.github.com`, starting with `/`.
    pub path: String,
    pub body: Option<Value>,
    /// Ask for raw file bytes instead of JSON.
    pub raw: bool,
}
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}
/// Authenticated HTTPS to the GitHub API. A transport failure is [`SyncError::Offline`].
#[allow(async_fn_in_trait)]
pub trait Http {
    async fn send(&self, request: Request) -> Result<Response>;
}

/// `owner/name` with GitHub's allowed characters.
pub fn validate_repository(repository: &str) -> Result<()> {
    let valid = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    match repository.split_once('/') {
        Some((owner, name)) if valid(owner) && valid(name) => Ok(()),
        _ => Err(SyncError::Invalid(
            "Use the repository as owner/name, for example you/woodpeck-data.".into(),
        )),
    }
}

/// Files the desktop transport reads from a snapshot, besides `manifest.json`.
fn tracked(path: &str) -> bool {
    path == "settings.json"
        || path == "review.json"
        || path.starts_with("sets/")
        || path.starts_with("attempts/")
}

#[derive(Clone)]
struct Tree {
    sha: String,
    /// Contents of the tracked files, by path.
    files: BTreeMap<String, String>,
}

pub struct GitHub<H> {
    http: H,
    repository: String,
    head: RefCell<Option<String>>,
    /// Trees already read or written, by commit.
    trees: RefCell<BTreeMap<String, Tree>>,
    blobs: RefCell<BTreeMap<String, String>>,
}
impl<H: Http> GitHub<H> {
    pub fn new(http: H, repository: &str) -> Result<Self> {
        validate_repository(repository)?;
        Ok(Self {
            http,
            repository: repository.into(),
            head: RefCell::default(),
            trees: RefCell::default(),
            blobs: RefCell::default(),
        })
    }
    async fn call(
        &self,
        method: &'static str,
        path: &str,
        body: Option<Value>,
    ) -> Result<(u16, Value)> {
        let response = self
            .http
            .send(Request {
                method,
                path: format!("/repos/{}{path}", self.repository),
                body,
                raw: false,
            })
            .await?;
        let value = if response.body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&response.body).unwrap_or(Value::Null)
        };
        Ok((response.status, value))
    }
    /// Maps statuses outside `expected` to errors a person can act on.
    fn check(status: u16, value: Value, expected: &[u16]) -> Result<Value> {
        if expected.contains(&status) {
            return Ok(value);
        }
        Err(match status {
            401 => SyncError::Invalid("GitHub rejected the access token. Enter a new one.".into()),
            403 | 429 => SyncError::Invalid(
                "GitHub refused the request. Check the token's repository access and Contents \
                 permission, or wait for the rate limit to reset."
                    .into(),
            ),
            404 => SyncError::Invalid(
                "Repository not found. Check the name and that the token can access it.".into(),
            ),
            500.. => SyncError::Offline,
            _ => SyncError::Git(format!("GitHub answered {status}.")),
        })
    }
    async fn ok(&self, method: &'static str, path: &str, body: Option<Value>) -> Result<Value> {
        let (status, value) = self.call(method, path, body).await?;
        Self::check(status, value, &[200, 201])
    }
    async fn blob(&self, sha: &str) -> Result<String> {
        if let Some(text) = self.blobs.borrow().get(sha) {
            return Ok(text.clone());
        }
        let response = self
            .http
            .send(Request {
                method: "GET",
                path: format!("/repos/{}/git/blobs/{sha}", self.repository),
                body: None,
                raw: true,
            })
            .await?;
        Self::check(response.status, Value::Null, &[200])?;
        let text = String::from_utf8(response.body)
            .map_err(|_| SyncError::Invalid("Sync file is not UTF-8.".into()))?;
        self.blobs.borrow_mut().insert(sha.into(), text.clone());
        Ok(text)
    }
    async fn tree(&self, commit: &str) -> Result<Tree> {
        if let Some(tree) = self.trees.borrow().get(commit) {
            return Ok(tree.clone());
        }
        let commit_object = self
            .ok("GET", &format!("/git/commits/{commit}"), None)
            .await?;
        let sha = text(&commit_object["tree"]["sha"])?;
        let listing = self
            .ok("GET", &format!("/git/trees/{sha}?recursive=1"), None)
            .await?;
        if listing["truncated"].as_bool() == Some(true) {
            return Err(SyncError::Invalid(
                "The sync repository is too large to read.".into(),
            ));
        }
        let blobs: BTreeMap<String, String> = listing["tree"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entry| entry["type"] == "blob")
            .filter_map(|entry| {
                Some((
                    entry["path"].as_str()?.into(),
                    entry["sha"].as_str()?.into(),
                ))
            })
            .collect();
        let mut files = BTreeMap::new();
        // Check the manifest first, before interpreting any other files.
        if let Some(manifest) = blobs.get("manifest.json") {
            let json = self.blob(manifest).await?;
            let parsed: Manifest = serde_json::from_str(&json)?;
            if parsed.schema_version > SCHEMA_VERSION {
                return Err(SyncError::NewerSchema(parsed.schema_version));
            }
            files.insert("manifest.json".to_owned(), json);
        }
        for (path, blob) in blobs.iter().filter(|(path, _)| tracked(path)) {
            files.insert(path.clone(), self.blob(blob).await?);
        }
        let tree = Tree { sha, files };
        self.trees.borrow_mut().insert(commit.into(), tree.clone());
        Ok(tree)
    }
    /// An empty GitHub repository accepts no trees, so its first commit goes through the
    /// contents API; the exchange then retries on top of it.
    async fn bootstrap(&self, state: &State) -> Result<String> {
        let files = state.files()?;
        let manifest = files.get("manifest.json").cloned().unwrap_or_default();
        let created = self
            .ok(
                "PUT",
                "/contents/manifest.json",
                Some(json!({"message": MESSAGE, "content": base64(manifest.as_bytes())})),
            )
            .await?;
        text(&created["commit"]["sha"])
    }
}
fn text(value: &Value) -> Result<String> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| SyncError::Invalid("Unexpected answer from GitHub.".into()))
}
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[(n >> (18 - 6 * i)) as usize & 63]));
            } else {
                out.push('=');
            }
        }
    }
    out
}
impl<H: Http> AsyncRemote for GitHub<H> {
    async fn fetch(&self) -> Result<(String, Option<String>)> {
        // The query defeats the browser cache, which would otherwise serve a stale head.
        let stamp = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let (status, refs) = self
            .call("GET", &format!("/git/matching-refs/heads/?t={stamp}"), None)
            .await?;
        // 409 is GitHub's answer for a repository without any commit.
        let refs = if status == 409 {
            Value::Array(vec![])
        } else {
            Self::check(status, refs, &[200])?
        };
        let heads: Vec<(String, String)> = refs
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| {
                Some((
                    r["ref"].as_str()?.strip_prefix("refs/heads/")?.to_owned(),
                    r["object"]["sha"].as_str()?.to_owned(),
                ))
            })
            .collect();
        let names: Vec<&str> = heads.iter().map(|(name, _)| name.as_str()).collect();
        let branch = choose_branch(&names)?.to_owned();
        let head = heads
            .iter()
            .find(|(name, _)| *name == branch)
            .map(|(_, sha)| sha.clone());
        self.head.replace(head.clone());
        Ok((branch, head))
    }
    async fn read(&self, revision: &str) -> Result<State> {
        let files = self.tree(revision).await?.files;
        // The bootstrap commit of an empty repository holds only the manifest.
        if !files.contains_key("settings.json") && files.len() <= 1 {
            Ok(State::default())
        } else {
            State::from_files(&files)
        }
    }
    async fn commit(&self, state: &State, parent: Option<&str>) -> Result<String> {
        let base = match parent {
            Some(parent) => Some(self.tree(parent).await?),
            None => None,
        };
        let files = state.files()?;
        let changed: Vec<_> = files
            .iter()
            .filter(|(path, contents)| {
                base.as_ref()
                    .is_none_or(|base| base.files.get(*path) != Some(*contents))
            })
            .collect();
        if let (Some(parent), true) = (parent, changed.is_empty()) {
            return Ok(parent.into());
        }
        let entries: Vec<Value> = changed
            .iter()
            .map(|(path, contents)| {
                json!({"path": path, "mode": "100644", "type": "blob", "content": contents})
            })
            .collect();
        let mut body = json!({ "tree": entries });
        if let Some(base) = &base {
            body["base_tree"] = json!(base.sha);
        }
        let (status, tree) = self.call("POST", "/git/trees", Some(body)).await?;
        if status == 409 && parent.is_none() {
            return self.bootstrap(state).await;
        }
        let tree = text(&Self::check(status, tree, &[201])?["sha"])?;
        let parents: Vec<&str> = parent.into_iter().collect();
        let commit = self
            .ok(
                "POST",
                "/git/commits",
                Some(json!({"message": MESSAGE, "tree": tree, "parents": parents})),
            )
            .await?;
        let commit = text(&commit["sha"])?;
        let mut tracked_files = base.map(|base| base.files).unwrap_or_default();
        tracked_files.extend(files);
        self.trees.borrow_mut().insert(
            commit.clone(),
            Tree {
                sha: tree,
                files: tracked_files,
            },
        );
        Ok(commit)
    }
    async fn push(&self, commit: &str, branch: &str) -> Result<bool> {
        let head = self.head.borrow().clone();
        let (status, value) = match &head {
            Some(head) if head == commit => return Ok(true),
            Some(_) => {
                self.call(
                    "PATCH",
                    &format!("/git/refs/heads/{branch}"),
                    Some(json!({"sha": commit, "force": false})),
                )
                .await?
            }
            None => {
                self.call(
                    "POST",
                    "/git/refs",
                    Some(json!({"ref": format!("refs/heads/{branch}"), "sha": commit})),
                )
                .await?
            }
        };
        // 422: not a fast-forward, or the branch was created meanwhile.
        if status == 422 {
            return Ok(false);
        }
        Self::check(status, value, &[200, 201])?;
        self.head.replace(Some(commit.into()));
        Ok(true)
    }
}
