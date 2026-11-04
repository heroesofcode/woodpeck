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
}
