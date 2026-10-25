//! Short synthesized effects. Device discovery and playback only run on the audio worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Move,
    Capture,
    Solved,
    Wrong,
}
impl Event {
    pub fn from_move(m: shakmaty::Move) -> Self {
        if m.is_capture() {
            Self::Capture
        } else {
            Self::Move
        }
    }
}

#[cfg(feature = "sound")]
static VOLUME: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(35);

/// Update gain without creating a worker or opening a device. Also affects queued effects.
pub fn configure(settings: &crate::settings::Settings) {
    #[cfg(feature = "sound")]
    VOLUME.store(
        if settings.completion_sound {
            settings.sound_volume.min(100)
        } else {
            0
        },
        std::sync::atomic::Ordering::Relaxed,
    );
    #[cfg(not(feature = "sound"))]
    let _ = settings;
}

/// Never waits for the device or for space in the bounded queue.
pub fn play(event: Event, settings: &crate::settings::Settings) {
    configure(settings);
    if !settings.completion_sound || settings.sound_volume == 0 {
        return;
    }
    #[cfg(feature = "sound")]
    {
        use std::sync::{OnceLock, mpsc};
        static PLAYER: OnceLock<mpsc::SyncSender<Event>> = OnceLock::new();
        let sender = PLAYER.get_or_init(|| {
            let (sender, receiver) = mpsc::sync_channel::<Event>(4);
            let _ = std::thread::Builder::new()
                .name("woodpeck-sound".into())
                .spawn(move || {
                    use rodio::{DeviceSinkBuilder, Player};
                    // One lazy attempt per worker lifetime: headless systems stay silent.
                    let mut output = None;
                    let mut attempted = false;
                    for event in receiver {
                        if !attempted {
                            attempted = true;
                            if let Ok(builder) = DeviceSinkBuilder::from_default_device()
                                && let Ok(mut stream) =
                                    builder.with_error_callback(|_| {}).open_stream()
                            {
                                stream.log_on_drop(false);
                                let player = Player::connect_new(stream.mixer());
                                output = Some((stream, player));
                            }
                        }
                        if let Some((_, player)) = &output {
                            player.append(Effect(rodio::buffer::SamplesBuffer::new(
                                std::num::NonZeroU16::new(1).unwrap(),
                                std::num::NonZeroU32::new(SAMPLE_RATE).unwrap(),
                                samples(event),
                            )));
                            player.sleep_until_end();
                        }
                    }
                });
            sender
        });
        let _ = sender.try_send(event);
    }
    #[cfg(not(feature = "sound"))]
    let _ = event;
}

#[cfg(feature = "sound")]
struct Effect(rodio::buffer::SamplesBuffer);
#[cfg(feature = "sound")]
impl Iterator for Effect {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        self.0.next().map(|sample| {
            sample * f32::from(VOLUME.load(std::sync::atomic::Ordering::Relaxed)) / 100.0
        })
    }
}
#[cfg(feature = "sound")]
impl rodio::Source for Effect {
    fn current_span_len(&self) -> Option<usize> {
        self.0.current_span_len()
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.0.channels()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.0.sample_rate()
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        self.0.total_duration()
    }
}

#[cfg(any(feature = "sound", test))]
const SAMPLE_RATE: u32 = 44_100;

/// Deterministic mono PCM with attack/release envelopes and no external assets.
#[cfg(any(feature = "sound", test))]
fn samples(event: Event) -> Vec<f32> {
    use std::f32::consts::TAU;
    let ms = match event {
        Event::Move => 85,
        Event::Capture => 120,
        Event::Solved => 260,
        Event::Wrong => 180,
    };
    let count = SAMPLE_RATE * ms / 1000;
    let mut noise = 0x1234_5678_u32;
    let mut filtered = 0.0;
    (0..count)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let attack = (t / 0.003).min(1.0);
            let release = ((count - i - 1) as f32 / (SAMPLE_RATE as f32 * 0.008)).min(1.0);
            let sample = match event {
                Event::Move | Event::Capture => {
                    noise ^= noise << 13;
                    noise ^= noise >> 17;
                    noise ^= noise << 5;
                    let white = noise as f32 / u32::MAX as f32 * 2.0 - 1.0;
                    filtered += 0.28 * (white - filtered);
                    let click = |time: f32| {
                        if time < 0.0 {
                            0.0
                        } else {
                            (0.11 * (TAU * 230.0 * time).sin() + 0.10 * filtered)
                                * (-65.0 * time).exp()
                        }
                    };
                    click(t)
                        + if event == Event::Capture {
                            click(t - 0.032) * 0.8
                        } else {
                            0.0
                        }
                }
                Event::Solved => {
                    let (hz, local) = if t < 0.10 {
                        (660.0, t)
                    } else {
                        (880.0, t - 0.10)
                    };
                    0.16 * (TAU * hz * local).sin()
                        * (local / 0.004).min(1.0)
                        * (-24.0 * local).exp()
                }
                Event::Wrong => {
                    // Integral of a descending frequency: 240 Hz down to 140 Hz.
                    0.16 * (TAU * (240.0 * t - 100.0 / 0.36 * t * t)).sin() * (-22.0 * t).exp()
                }
            };
            sample * attack * release
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn effects_are_finite_short_quiet_and_distinct() {
        let mut sources = Vec::new();
        for event in [Event::Move, Event::Capture, Event::Solved, Event::Wrong] {
            let samples = samples(event);
            assert_ne!(samples.len(), 0);
            assert!(samples.len() < SAMPLE_RATE as usize * 3 / 10);
            assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 0.3));
            assert!(samples.iter().any(|s| s.abs() > 0.01));
            assert_eq!(samples[0], 0.0);
            assert_eq!(*samples.last().unwrap(), 0.0);
            assert!(!sources.contains(&samples));
            sources.push(samples);
        }
    }
}
