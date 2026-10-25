//! Opt-in CPU time for application logic and UI construction (excluding GPU presentation).
use std::time::{Duration, Instant};

pub(super) struct FrameStats {
    enabled: bool,
    since: Instant,
    samples: Vec<Duration>,
}
impl Default for FrameStats {
    fn default() -> Self {
        Self {
            enabled: std::env::var_os("WOODPECK_FRAME_STATS").is_some_and(|v| v == "1"),
            since: Instant::now(),
            samples: Vec::new(),
        }
    }
}
impl FrameStats {
    pub fn record(&mut self, elapsed: Duration) {
        if !self.enabled {
            return;
        }
        self.samples.push(elapsed);
        if self.since.elapsed() >= Duration::from_secs(5) {
            self.samples.sort_unstable();
            let n = self.samples.len();
            eprintln!(
                "frame CPU ({n} frames): p50={:.3}ms p95={:.3}ms max={:.3}ms",
                self.samples[n / 2].as_secs_f64() * 1000.0,
                self.samples[(n * 95 / 100).min(n - 1)].as_secs_f64() * 1000.0,
                self.samples[n - 1].as_secs_f64() * 1000.0,
            );
            self.samples.clear();
            self.since = Instant::now();
        }
    }
}
