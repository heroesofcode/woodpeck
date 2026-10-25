//! Optional generated completion chime. Device discovery and playback stay off the caller thread.
#[cfg(feature = "sound")]
use std::time::Duration;

pub fn play_completion() {
    #[cfg(feature = "sound")]
    {
        use std::sync::{OnceLock, mpsc};
        static PLAYER: OnceLock<mpsc::SyncSender<()>> = OnceLock::new();
        let sender = PLAYER.get_or_init(|| {
            let (sender, receiver) = mpsc::sync_channel(2);
            let _ = std::thread::Builder::new()
                .name("woodpeck-sound".into())
                .spawn(move || {
                    use rodio::{DeviceSinkBuilder, Player};
                    let mut output = None;
                    for () in receiver {
                        if output.is_none() {
                            let Ok(builder) = DeviceSinkBuilder::from_default_device() else {
                                continue;
                            };
                            let Ok(mut stream) = builder.with_error_callback(|_| {}).open_stream()
                            else {
                                continue;
                            };
                            stream.log_on_drop(false);
                            let player = Player::connect_new(stream.mixer());
                            output = Some((stream, player));
                        }
                        let (_, player) = output.as_ref().expect("audio output");
                        player.append(tone(660.0, 100, 0.12));
                        player.append(tone(880.0, 160, 0.10));
                        player.sleep_until_end();
                    }
                });
            sender
        });
        let _ = sender.try_send(());
    }
}

#[cfg(feature = "sound")]
fn tone(hz: f32, ms: u64, volume: f32) -> impl rodio::Source + Send {
    use rodio::{Source, source::SineWave};
    SineWave::new(hz)
        .take_duration(Duration::from_millis(ms))
        .amplify(volume)
        .fade_in(Duration::from_millis(5))
        .fade_out(Duration::from_millis(ms))
}
#[cfg(all(test, feature = "sound"))]
mod tests {
    use super::*;
    use rodio::Source;
    #[test]
    fn generated_tones_are_short_finite_and_quiet_without_opening_a_device() {
        for (hz, ms, volume) in [(660.0, 100, 0.12), (880.0, 160, 0.10)] {
            let tone = tone(hz, ms, volume);
            let expected =
                u64::from(tone.sample_rate().get()) * u64::from(tone.channels().get()) * ms / 1000;
            let samples: Vec<_> = tone.collect();
            assert!(u64::try_from(samples.len()).unwrap().abs_diff(expected) <= 1);
            assert_ne!(samples, Vec::<f32>::new());
            assert!(samples.iter().all(|x| x.is_finite() && x.abs() <= volume));
            assert!(samples.iter().any(|x| x.abs() > 0.01));
        }
    }
}
