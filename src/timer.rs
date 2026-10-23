//! Per-puzzle elapsed time; update at state transitions, repaint every 100 ms.
use std::time::{Duration, Instant};
pub const TICK: Duration = Duration::from_millis(100);
pub trait Clock {
    fn now(&self) -> Duration;
}
#[derive(Debug)]
pub struct MonotonicClock {
    origin: Instant,
}
impl Default for MonotonicClock {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}
impl Clock for MonotonicClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct TimerConditions {
    pub player_turn: bool,
    pub interactive: bool,
    pub focused: bool,
    pub minimized: bool,
    pub promotion_open: bool,
}
impl TimerConditions {
    pub fn running(self) -> bool {
        self.player_turn
            && self.interactive
            && self.focused
            && !self.minimized
            && !self.promotion_open
    }
}
pub struct Timer<C> {
    clock: C,
    elapsed: Duration,
    since: Option<Duration>,
    visible: bool,
}
impl<C: Clock> Timer<C> {
    pub fn new(clock: C) -> Self {
        Self {
            clock,
            elapsed: Duration::ZERO,
            since: None,
            visible: true,
        }
    }
    pub fn set_conditions(&mut self, conditions: TimerConditions) {
        let now = self.clock.now();
        if let Some(since) = self.since.take() {
            self.elapsed += now.saturating_sub(since);
        }
        if conditions.running() {
            self.since = Some(now);
        }
    }
    pub fn elapsed(&self) -> Duration {
        self.elapsed
            + self.since.map_or(Duration::ZERO, |since| {
                self.clock.now().saturating_sub(since)
            })
    }
    pub fn recorded_ms(&self) -> u64 {
        u64::try_from(self.elapsed().as_millis())
            .unwrap_or(u64::MAX)
            .max(1)
    }
    /// Reset on advance; a subsequent state transition starts the next timer.
    pub fn reset(&mut self) {
        self.elapsed = Duration::ZERO;
        self.since = None;
    }
    pub fn toggle_visible(&mut self) {
        self.visible = !self.visible;
    }
    pub fn visible(&self) -> bool {
        self.visible
    }
    pub fn display(&self, tenths: bool) -> String {
        format_elapsed(self.elapsed(), tenths)
    }
}
pub fn format_elapsed(elapsed: Duration, tenths: bool) -> String {
    let seconds = elapsed.as_secs();
    let base = format!("{:02}:{:02}", seconds / 60, seconds % 60);
    if tenths {
        format!("{base}.{}", elapsed.subsec_millis() / 100)
    } else {
        base
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};
    #[derive(Clone)]
    struct Fake(Rc<Cell<Duration>>);
    impl Clock for Fake {
        fn now(&self) -> Duration {
            self.0.get()
        }
    }
    fn ready() -> TimerConditions {
        TimerConditions {
            player_turn: true,
            interactive: true,
            focused: true,
            ..Default::default()
        }
    }
    #[test]
    fn pauses_for_every_condition() {
        let clock = Fake(Rc::new(Cell::new(Duration::ZERO)));
        let mut timer = Timer::new(clock.clone());
        timer.set_conditions(ready());
        clock.0.set(Duration::from_millis(1234));
        for c in [
            TimerConditions {
                player_turn: false,
                ..ready()
            },
            TimerConditions {
                interactive: false,
                ..ready()
            },
            TimerConditions {
                focused: false,
                ..ready()
            },
            TimerConditions {
                minimized: true,
                ..ready()
            },
            TimerConditions {
                promotion_open: true,
                ..ready()
            },
        ] {
            timer.set_conditions(c);
            let before = timer.elapsed();
            clock.0.set(clock.now() + Duration::from_secs(10));
            assert_eq!(timer.elapsed(), before);
        }
        assert_eq!(timer.recorded_ms(), 1234);
        timer.set_conditions(ready());
        clock.0.set(clock.now() + Duration::from_millis(66));
        assert_eq!(timer.recorded_ms(), 1300);
        assert_eq!(timer.display(true), "00:01.3");
        timer.toggle_visible();
        assert!(!timer.visible());
        timer.reset();
        assert_eq!(timer.recorded_ms(), 1);
        clock.0.set(clock.now() + Duration::from_secs(1));
        assert_eq!(timer.recorded_ms(), 1);
    }
    #[test]
    fn display_rolls_over_minutes() {
        assert_eq!(
            format_elapsed(Duration::from_millis(65_999), true),
            "01:05.9"
        );
        assert_eq!(format_elapsed(Duration::from_secs(3601), false), "60:01");
    }
    #[test]
    fn repeated_updates_do_not_double_count() {
        let c = Fake(Rc::new(Cell::new(Duration::ZERO)));
        let mut t = Timer::new(c.clone());
        t.set_conditions(ready());
        c.0.set(Duration::from_secs(1));
        t.set_conditions(ready());
        c.0.set(Duration::from_secs(2));
        assert_eq!(t.recorded_ms(), 2000);
    }
}
