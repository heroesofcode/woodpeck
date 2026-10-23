//! Independent SQLite connection and serial worker; network work never holds the service lock.
use super::{Config, Result, Status, config, save_config, synchronize};
use crate::store::Store;
use std::{
    path::Path,
    sync::{Arc, Condvar, Mutex, mpsc},
    time::{Duration, Instant},
};

enum Message {
    Now,
    Close(Instant),
}
#[derive(Clone)]
pub struct Shutdown {
    commands: mpsc::Sender<Message>,
    done: Arc<(Mutex<bool>, Condvar)>,
}
impl Shutdown {
    /// Called after the native event loop returns, never from an egui frame.
    pub fn finish(&self) {
        let deadline = Instant::now() + Duration::from_secs(3);
        let _ = self.commands.send(Message::Close(deadline));
        let (lock, wake) = &*self.done;
        if let Ok(done) = lock.lock() {
            let _ = wake.wait_timeout_while(
                done,
                deadline.saturating_duration_since(Instant::now()),
                |done| !*done,
            );
        }
    }
}
pub struct Worker {
    shutdown: Shutdown,
    events: mpsc::Receiver<Config>,
    latest: Config,
}
impl Worker {
    pub fn start(path: &Path, latest: Config) -> Result<Self> {
        let (commands, receiver) = mpsc::channel();
        let (sender, events) = mpsc::channel();
        let done = Arc::new((Mutex::new(false), Condvar::new()));
        let shutdown = Shutdown {
            commands,
            done: done.clone(),
        };
        let path = path.to_path_buf();
        std::thread::Builder::new()
            .name("woodpeck-sync".into())
            .spawn(move || {
                run(&path, &receiver, &sender);
                let (lock, wake) = &*done;
                if let Ok(mut finished) = lock.lock() {
                    *finished = true;
                    wake.notify_all();
                }
            })?;
        Ok(Self {
            shutdown,
            events,
            latest,
        })
    }
    pub fn shutdown_handle(&self) -> Shutdown {
        self.shutdown.clone()
    }
    pub fn request(&self) {
        let _ = self.shutdown.commands.send(Message::Now);
    }
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        for config in self.events.try_iter() {
            changed |= config != self.latest;
            self.latest = config;
        }
        changed
    }
    pub fn status(&self) -> &Config {
        &self.latest
    }
    pub fn update(&mut self, config: Config) {
        self.latest = config;
        self.request();
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self
            .shutdown
            .commands
            .send(Message::Close(Instant::now() + Duration::from_secs(3)));
    }
}
fn run(path: &Path, commands: &mpsc::Receiver<Message>, events: &mpsc::Sender<Config>) {
    let result = Store::open(path);
    let Ok(mut store) = result else {
        let _ = events.send(Config {
            status: Status::Error,
            last_error: result.err().map(|e| e.to_string()),
            ..Config::default()
        });
        return;
    };
    let data = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut next = Some(Instant::now());
    let mut observed = store.sync_generation().unwrap_or_default();
    let mut backoff = Duration::from_secs(5);
    let mut previous = Config::default();
    let mut halted = false;
    loop {
        let mut close = None;
        let mut force = false;
        match commands.recv_timeout(Duration::from_millis(500)) {
            Ok(Message::Now) => {
                next = Some(Instant::now());
                force = true;
                halted = false;
            }
            Ok(Message::Close(deadline)) => close = Some(deadline),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        let Ok(mut cfg) = config(&store) else {
            continue;
        };
        if cfg.enabled != previous.enabled || cfg.remote != previous.remote {
            next = Some(Instant::now());
            backoff = Duration::from_secs(5);
            halted = false;
        }
        if let Ok(generation) = store.sync_generation()
            && generation != observed
        {
            observed = generation;
            if !force && !halted {
                next = Some(Instant::now() + Duration::from_secs(10));
            }
            if cfg.enabled {
                cfg.pending = true;
                if !matches!(cfg.status, Status::Error | Status::Offline) {
                    cfg.status = Status::Pending;
                }
                let _ = save_config(&store, &cfg);
            }
        }
        if cfg != previous {
            let _ = events.send(cfg.clone());
            previous = cfg.clone();
        }
        if cfg.enabled && (close.is_some() || next.is_some_and(|at| at <= Instant::now())) {
            let _ = events.send(Config {
                status: Status::Syncing,
                ..cfg
            });
            let budget = close.map_or(Duration::from_secs(45), |deadline| {
                deadline.saturating_duration_since(Instant::now())
            });
            let result = synchronize(&mut store, &data, budget);
            observed = store.sync_generation().unwrap_or(observed);
            match &result {
                Ok(cfg) => {
                    next = cfg
                        .pending
                        .then(|| Instant::now() + Duration::from_secs(10));
                    backoff = Duration::from_secs(5);
                }
                Err(super::SyncError::NewerSchema(_)) => {
                    next = None;
                    halted = true;
                }
                Err(_) => {
                    next = Some(Instant::now() + backoff);
                    backoff = (backoff * 2).min(Duration::from_secs(300));
                }
            }
            if let Ok(cfg) = config(&store) {
                let _ = events.send(cfg.clone());
                previous = cfg;
            }
        }
        if close.is_some() {
            break;
        }
    }
}
