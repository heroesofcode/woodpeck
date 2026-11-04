//! Transport-independent sync exchange. The desktop transport is the system-Git
//! repository in `git.rs`; other targets can provide their own [`Remote`].
use super::{Result, SyncError, format::State};

/// A place that stores portable snapshots as a linear history of commits.
pub trait Remote {
    /// Returns the branch to push to and the current remote head, if any.
    fn fetch(&self) -> Result<(String, Option<String>)>;
    /// Reads the snapshot stored at `revision`.
    fn read(&self, revision: &str) -> Result<State>;
    /// Records `state` on top of `parent` and returns the new revision.
    fn commit(&self, state: &State, parent: Option<&str>) -> Result<String>;
    /// Publishes `commit` to `branch`; `false` means the remote moved meanwhile.
    fn push(&self, commit: &str, branch: &str) -> Result<bool>;
}

/// The local progress database as seen by the sync exchange.
pub trait SyncStorage {
    fn export_sync(&self) -> Result<State>;
    fn export_sync_snapshot(&self) -> Result<(State, i64)>;
    fn import_sync(&mut self, remote: &State) -> Result<State>;
}

/// Merges the remote snapshot into local storage and publishes the union,
/// retrying after concurrent pushes. Returns the exported generation and
/// whether remote data changed local state.
pub fn exchange(storage: &mut impl SyncStorage, remote: &impl Remote) -> Result<(i64, bool)> {
    let mut changed = false;
    for _ in 0..3 {
        let (branch, head) = remote.fetch()?;
        if let Some(head) = &head {
            let theirs = remote.read(head)?;
            let before = storage.export_sync()?;
            let merged = storage.import_sync(&theirs)?;
            changed |= merged != before;
        }
        let (snapshot, generation) = storage.export_sync_snapshot()?;
        let commit = remote.commit(&snapshot, head.as_deref())?;
        if remote.push(&commit, &branch)? {
            return Ok((generation, changed));
        }
        // Re-fetch and repeat the semantic union after a concurrent non-fast-forward push.
    }
    Err(SyncError::Offline)
}

/// [`Remote`] for transports that can only wait asynchronously, such as `fetch` in a browser.
#[allow(async_fn_in_trait)]
pub trait AsyncRemote {
    async fn fetch(&self) -> Result<(String, Option<String>)>;
    async fn read(&self, revision: &str) -> Result<State>;
    async fn commit(&self, state: &State, parent: Option<&str>) -> Result<String>;
    async fn push(&self, commit: &str, branch: &str) -> Result<bool>;
}

/// [`exchange`] over an [`AsyncRemote`], with the same merge, retry and result.
pub async fn exchange_async(
    storage: &mut impl SyncStorage,
    remote: &impl AsyncRemote,
) -> Result<(i64, bool)> {
    let mut changed = false;
    for _ in 0..3 {
        let (branch, head) = remote.fetch().await?;
        if let Some(head) = &head {
            let theirs = remote.read(head).await?;
            let before = storage.export_sync()?;
            let merged = storage.import_sync(&theirs)?;
            changed |= merged != before;
        }
        let (snapshot, generation) = storage.export_sync_snapshot()?;
        let commit = remote.commit(&snapshot, head.as_deref()).await?;
        if remote.push(&commit, &branch).await? {
            return Ok((generation, changed));
        }
    }
    Err(SyncError::Offline)
}

/// The branch sync uses among a remote's branches, chosen as the desktop Git transport does.
pub fn choose_branch<'a>(branches: &[&'a str]) -> Result<&'a str> {
    if branches.contains(&"main") || branches.is_empty() {
        Ok("main")
    } else if branches.contains(&"master") {
        Ok("master")
    } else if branches.len() == 1 {
        Ok(branches[0])
    } else {
        Err(SyncError::Invalid(
            "Remote has multiple branches and no main branch.".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct Memory {
        commits: RefCell<Vec<State>>,
        head: RefCell<Option<usize>>,
        rejects: Cell<u32>,
    }
    impl Remote for Memory {
        fn fetch(&self) -> Result<(String, Option<String>)> {
            Ok(("main".into(), self.head.borrow().map(|h| h.to_string())))
        }
        fn read(&self, revision: &str) -> Result<State> {
            let index: usize = revision.parse().unwrap();
            Ok(self.commits.borrow()[index].clone())
        }
        fn commit(&self, state: &State, _: Option<&str>) -> Result<String> {
            let mut commits = self.commits.borrow_mut();
            commits.push(state.clone());
            Ok((commits.len() - 1).to_string())
        }
        fn push(&self, commit: &str, _: &str) -> Result<bool> {
            if self.rejects.get() > 0 {
                self.rejects.set(self.rejects.get() - 1);
                return Ok(false);
            }
            *self.head.borrow_mut() = Some(commit.parse().unwrap());
            Ok(true)
        }
    }
    struct Local {
        state: State,
        imports: u32,
    }
    impl SyncStorage for Local {
        fn export_sync(&self) -> Result<State> {
            Ok(self.state.clone())
        }
        fn export_sync_snapshot(&self) -> Result<(State, i64)> {
            Ok((self.state.clone(), 7))
        }
        fn import_sync(&mut self, remote: &State) -> Result<State> {
            self.imports += 1;
            Ok(remote.clone())
        }
    }

    #[test]
    fn first_exchange_publishes_without_reading() {
        let remote = Memory::default();
        let mut local = Local {
            state: State::default(),
            imports: 0,
        };
        assert_eq!(exchange(&mut local, &remote).unwrap(), (7, false));
        assert_eq!(local.imports, 0);
        assert_eq!(*remote.head.borrow(), Some(0));
    }

    #[test]
    fn rejected_pushes_refetch_and_give_up_after_three_attempts() {
        let remote = Memory::default();
        remote.rejects.set(2);
        let mut local = Local {
            state: State::default(),
            imports: 0,
        };
        assert_eq!(exchange(&mut local, &remote).unwrap(), (7, false));
        remote.rejects.set(3);
        assert!(matches!(
            exchange(&mut local, &remote),
            Err(SyncError::Offline)
        ));
        assert_eq!(local.imports, 3);
    }

    #[allow(clippy::unused_async_trait_impl)]
    impl AsyncRemote for Memory {
        async fn fetch(&self) -> Result<(String, Option<String>)> {
            Remote::fetch(self)
        }
        async fn read(&self, revision: &str) -> Result<State> {
            Remote::read(self, revision)
        }
        async fn commit(&self, state: &State, parent: Option<&str>) -> Result<String> {
            Remote::commit(self, state, parent)
        }
        async fn push(&self, commit: &str, branch: &str) -> Result<bool> {
            Remote::push(self, commit, branch)
        }
    }

    #[test]
    fn async_exchange_matches_the_blocking_one() {
        let remote = Memory::default();
        remote.rejects.set(2);
        let mut local = Local {
            state: State::default(),
            imports: 0,
        };
        let result = pollster::block_on(exchange_async(&mut local, &remote)).unwrap();
        assert_eq!(result, (7, false));
        assert_eq!(*remote.head.borrow(), Some(2));
        remote.rejects.set(3);
        assert!(matches!(
            pollster::block_on(exchange_async(&mut local, &remote)),
            Err(SyncError::Offline)
        ));
    }

    #[test]
    fn branch_choice_prefers_main_then_master_then_the_only_branch() {
        assert_eq!(choose_branch(&[]).unwrap(), "main");
        assert_eq!(choose_branch(&["master", "main"]).unwrap(), "main");
        assert_eq!(choose_branch(&["dev", "master"]).unwrap(), "master");
        assert_eq!(choose_branch(&["data"]).unwrap(), "data");
        assert!(choose_branch(&["a", "b"]).is_err());
    }
}
