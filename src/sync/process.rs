//! Bounded, noninteractive child processes; output is drained concurrently to avoid pipe stalls.
use super::{Result, SyncError};
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

pub struct Output {
    pub success: bool,
    pub stdout: String,
}
pub fn run(program: &str, args: &[&str], cwd: &Path, deadline: Instant) -> Result<Output> {
    if Instant::now() >= deadline {
        return Err(SyncError::Offline);
    }
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .env("GH_PROMPT_DISABLED", "1")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            SyncError::Git(format!(
                "{program} is missing; install it and ensure it is on PATH."
            ))
        } else {
            SyncError::Io(error)
        }
    })?;
    let (sender, receiver) = mpsc::channel();
    if let Some(mut stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = stdout
                .by_ref()
                .take(64 * 1024 * 1024)
                .read_to_end(&mut bytes)
                .map(|_| bytes);
            let _ = sender.send(result);
        });
    }
    if let Some(mut stderr) = child.stderr.take() {
        // Error output can contain credential-bearing URLs. Never persist or display it.
        std::thread::spawn(move || {
            let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        });
    }
    loop {
        if let Some(status) = child.try_wait()? {
            let bytes = receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| SyncError::Offline)??;
            return Ok(Output {
                success: status.success(),
                stdout: String::from_utf8(bytes)
                    .map_err(|_| SyncError::Invalid("Command returned non-UTF-8 data.".into()))?,
            });
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(SyncError::Offline);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
