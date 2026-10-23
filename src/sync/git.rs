//! Git transports a semantic snapshot. It never checks out or text-merges remote content.
use super::{Result, SyncError, format::State, process};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub struct Repository {
    path: PathBuf,
    deadline: Instant,
}
impl Repository {
    pub fn open(data: &Path, remote: &str, deadline: Instant) -> Result<Self> {
        validate_remote(remote)?;
        let path = data.join("sync");
        if path.is_symlink() {
            return Err(SyncError::Invalid(
                "Sync directory must not be a symlink.".into(),
            ));
        }
        fs::create_dir_all(&path)?;
        let repo = Self { path, deadline };
        if !repo.path.join(".git").exists() {
            repo.git(&["init", "--initial-branch=main"])?;
        }
        if !repo.path.join(".git").is_dir() || repo.path.join(".git").is_symlink() {
            return Err(SyncError::Invalid(
                "Sync needs its own Git directory.".into(),
            ));
        }
        if repo.try_git(&["remote", "get-url", "origin"])?.success {
            repo.git(&["remote", "set-url", "origin", remote])?;
        } else {
            repo.git(&["remote", "add", "origin", remote])?;
        }
        Ok(repo)
    }
    fn try_git(&self, args: &[&str]) -> Result<process::Output> {
        let mut configured = vec![
            "-c",
            "user.name=woodpeck",
            "-c",
            "user.email=woodpeck@localhost",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "push.gpgSign=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "protocol.ext.allow=never",
        ];
        configured.extend_from_slice(args);
        process::run("git", &configured, &self.path, self.deadline)
    }
    fn git(&self, args: &[&str]) -> Result<String> {
        let result = self.try_git(args)?;
        if result.success {
            Ok(result.stdout.trim_end().into())
        } else {
            Err(SyncError::Git(format!(
                "Git {} failed in the sync repository.",
                args.first().copied().unwrap_or("command")
            )))
        }
    }
    pub fn fetch(&self) -> Result<(String, Option<String>)> {
        if !self
            .try_git(&[
                "fetch",
                "--prune",
                "--no-tags",
                "origin",
                "+refs/heads/*:refs/remotes/origin/*",
            ])?
            .success
        {
            return Err(SyncError::Offline);
        }
        let refs = self.git(&[
            "for-each-ref",
            "--format=%(refname:strip=3)",
            "refs/remotes/origin/",
        ])?;
        let branches: Vec<_> = refs.lines().filter(|s| *s != "HEAD").collect();
        let branch = if branches.contains(&"main") || branches.is_empty() {
            "main"
        } else if branches.contains(&"master") {
            "master"
        } else if branches.len() == 1 {
            branches[0]
        } else {
            return Err(SyncError::Invalid(
                "Remote has multiple branches and no main branch.".into(),
            ));
        };
        let result = self.try_git(&[
            "rev-parse",
            "--verify",
            &format!("refs/remotes/origin/{branch}"),
        ])?;
        Ok((
            branch.into(),
            result.success.then(|| result.stdout.trim().into()),
        ))
    }
    pub fn read(&self, revision: &str) -> Result<State> {
        let paths = self.git(&["ls-tree", "-r", "--name-only", revision])?;
        let mut files = BTreeMap::new();
        // Check the manifest first, before interpreting any other files.
        if paths.lines().any(|p| p == "manifest.json") {
            let json = self.git(&["show", &format!("{revision}:manifest.json")])?;
            let manifest: super::format::Manifest = serde_json::from_str(&json)?;
            if manifest.schema_version > super::format::SCHEMA_VERSION {
                return Err(SyncError::NewerSchema(manifest.schema_version));
            }
            files.insert("manifest.json".into(), json);
        }
        for path in paths.lines().filter(|p| {
            *p == "settings.json"
                || *p == "review.json"
                || p.starts_with("sets/")
                || p.starts_with("attempts/")
        }) {
            files.insert(
                path.into(),
                self.git(&["show", &format!("{revision}:{path}")])?,
            );
        }
        if files.is_empty() {
            Ok(State::default())
        } else {
            State::from_files(&files)
        }
    }
    pub fn commit(&self, state: &State, remote: Option<&str>) -> Result<String> {
        let head = self.try_git(&["rev-parse", "--verify", "HEAD"])?;
        let local = head.success.then(|| head.stdout.trim().to_owned());
        if let Some(base) = remote.or(local.as_deref()) {
            self.git(&["read-tree", base])?;
        } else {
            self.git(&["read-tree", "--empty"])?;
        }
        for (path, contents) in state.files()? {
            let destination = self.path.join(&path);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            // Do not follow pre-existing local symlinks when writing snapshots.
            if destination.is_symlink() || destination.parent().is_some_and(Path::is_symlink) {
                return Err(SyncError::Invalid(
                    "Snapshot paths must not be symlinks.".into(),
                ));
            }
            fs::write(&destination, contents)?;
            let hash = self.git(&["hash-object", "-w", "--no-filters", "--", &path])?;
            self.git(&[
                "update-index",
                "--add",
                "--cacheinfo",
                "100644",
                &hash,
                &path,
            ])?;
        }
        let tree = self.git(&["write-tree"])?;
        if let Some(local) = &local {
            let old_tree = self.git(&["rev-parse", &format!("{local}^{{tree}}")])?;
            if old_tree == tree && remote.is_none_or(|r| r == local) {
                return Ok(local.clone());
            }
        }
        let mut args = vec!["commit-tree", &tree, "-m", "Sync woodpeck progress"];
        if let Some(parent) = remote {
            args.extend(["-p", parent]);
        }
        if let Some(parent) = &local
            && remote != Some(parent.as_str())
        {
            args.extend(["-p", parent]);
        }
        let commit = self.git(&args)?;
        self.git(&["update-ref", "refs/heads/main", &commit])?;
        self.git(&["symbolic-ref", "HEAD", "refs/heads/main"])?;
        Ok(commit)
    }
    pub fn push(&self, commit: &str, branch: &str) -> Result<bool> {
        Ok(self
            .try_git(&["push", "origin", &format!("{commit}:refs/heads/{branch}")])?
            .success)
    }
}
pub fn validate_remote(remote: &str) -> Result<()> {
    if remote.is_empty()
        || remote.starts_with('-')
        || remote.chars().any(char::is_control)
        || remote.contains(['?', '#'])
        || remote.contains("://")
            && remote
                .split("://")
                .nth(1)
                .is_some_and(|s| s.split('/').next().is_some_and(|s| s.contains('@')))
            && !remote.starts_with("ssh://")
    {
        return Err(SyncError::Invalid(
            "Use a Git URL without embedded credentials, query parameters or control characters."
                .into(),
        ));
    }
    if remote.starts_with("ssh://")
        && remote
            .split("://")
            .nth(1)
            .and_then(|s| s.split('@').next())
            .is_some_and(|s| s.contains(':'))
    {
        return Err(SyncError::Invalid(
            "Do not embed passwords in a sync URL.".into(),
        ));
    }
    Ok(())
}
/// Authenticated user's private repository; existing repositories are reused only if private.
pub fn create_remote(data: &Path) -> Result<String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let gh = |args: &[&str]| process::run("gh", args, data, deadline);
    if !gh(&["auth", "status"])?.success {
        return Err(SyncError::Git(
            "gh is not authenticated; run gh auth login first.".into(),
        ));
    }
    let user = gh(&["api", "user", "--jq", ".login"])?;
    if !user.success || user.stdout.trim().is_empty() {
        return Err(SyncError::Git(
            "Could not identify the authenticated gh user.".into(),
        ));
    }
    let name = format!("{}/woodpeck-data", user.stdout.trim());
    let inspect = || -> Result<Option<String>> {
        let output = gh(&["repo", "view", &name, "--json", "url,isPrivate"])?;
        if !output.success {
            return Ok(None);
        }
        let value: serde_json::Value = serde_json::from_str(&output.stdout)?;
        if value["isPrivate"] != true {
            return Err(SyncError::Git(
                "woodpeck-data already exists and is public. Make it private before syncing."
                    .into(),
            ));
        }
        Ok(value["url"].as_str().map(str::to_owned))
    };
    if let Some(url) = inspect()? {
        return Ok(url);
    }
    if !gh(&["repo", "create", &name, "--private"])?.success {
        return Err(SyncError::Git(
            "Could not create the private woodpeck-data repository with gh.".into(),
        ));
    }
    inspect()?.ok_or_else(|| SyncError::Git("Could not read the created repository URL.".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn never_accept_embedded_tokens_or_command_options() {
        for remote in [
            "",
            "-u",
            "https://fake:secret@example.invalid/data.git",
            "https://example.invalid/data.git?token=fake",
            "ssh://fake:secret@example.invalid/data.git",
            "a\nb",
        ] {
            assert!(validate_remote(remote).is_err());
        }
        for remote in [
            "https://github.com/<user>/woodpeck-data.git",
            "git@github.com:<user>/woodpeck-data.git",
            "ssh://git@example.invalid/data.git",
            "/tmp/example.git",
        ] {
            assert!(validate_remote(remote).is_ok());
        }
    }
}
