use super::{CancelFlag, SourceError, parse_batch, parse_puzzle};
use crate::puzzle::Puzzle;
use std::{sync::Mutex, time::Duration};

const API_BASE: &str = "https://lichess.org/api/puzzle";
static API_SERIAL: Mutex<()> = Mutex::new(());

pub fn difficulty(target: u16) -> &'static str {
    match i32::from(target) - 1500 {
        ..=-400 => "easiest",
        -399..=-150 => "easier",
        -149..=149 => "normal",
        150..=399 => "harder",
        _ => "hardest",
    }
}
/// Injectable boundary for offline service/fallback tests. Implementations must honor cancellation.
pub trait PuzzleFetcher: Send {
    fn batch(
        &mut self,
        theme: Option<&str>,
        target: u16,
        count: u32,
        cancel: &CancelFlag,
    ) -> Result<Vec<Puzzle>, SourceError>;
    fn daily(&mut self, cancel: &CancelFlag) -> Result<Puzzle, SourceError>;
    fn by_id(&mut self, id: &str, cancel: &CancelFlag) -> Result<Puzzle, SourceError>;
}
pub struct LichessClient {
    agent: ureq::Agent,
}
impl Default for LichessClient {
    fn default() -> Self {
        Self { agent: agent() }
    }
}
pub(super) fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(concat!("woodpeck/", env!("CARGO_PKG_VERSION")))
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_per_read(Some(Duration::from_secs(15)))
        .http_status_as_error(false)
        .build()
        .into()
}
fn wait(duration: Duration, cancel: &CancelFlag) -> Result<(), SourceError> {
    let deadline = std::time::Instant::now() + duration;
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        cancel.check()?;
        std::thread::sleep(left.min(Duration::from_millis(100)));
    }
    cancel.check()
}
pub(super) fn request(
    agent: &ureq::Agent,
    url: &str,
    cancel: &CancelFlag,
) -> Result<ureq::http::Response<ureq::Body>, SourceError> {
    request_with_wait(agent, url, cancel, wait)
}
fn request_with_wait(
    agent: &ureq::Agent,
    url: &str,
    cancel: &CancelFlag,
    mut sleep: impl FnMut(Duration, &CancelFlag) -> Result<(), SourceError>,
) -> Result<ureq::http::Response<ureq::Body>, SourceError> {
    loop {
        cancel.check()?;
        let response = agent
            .get(url)
            .call()
            .map_err(|e| SourceError::Network(e.to_string()))?;
        cancel.check()?;
        if response.status().as_u16() == 429 {
            drop(response);
            sleep(Duration::from_secs(60), cancel)?;
            continue;
        }
        if !response.status().is_success() {
            return Err(SourceError::Network(format!("HTTP {}", response.status())));
        }
        return Ok(response);
    }
}
impl LichessClient {
    fn json(&self, path: &str, cancel: &CancelFlag) -> Result<String, SourceError> {
        // Serialize through body consumption, including across multiple client instances.
        let _guard = loop {
            cancel.check()?;
            if let Ok(guard) = API_SERIAL.try_lock() {
                break guard;
            }
            wait(Duration::from_millis(50), cancel)?;
        };
        let mut response = request(&self.agent, &format!("{API_BASE}/{path}"), cancel)?;
        let json = response
            .body_mut()
            .with_config()
            .limit(8 * 1024 * 1024)
            .read_to_string()
            .map_err(|e| SourceError::Network(e.to_string()))?;
        cancel.check()?;
        Ok(json)
    }
}
fn identifier(value: &str) -> Result<&str, SourceError> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        Err(SourceError::Invalid("Invalid Lichess identifier.".into()))
    } else {
        Ok(value)
    }
}
impl PuzzleFetcher for LichessClient {
    fn batch(
        &mut self,
        theme: Option<&str>,
        target: u16,
        count: u32,
        cancel: &CancelFlag,
    ) -> Result<Vec<Puzzle>, SourceError> {
        let angle = identifier(theme.unwrap_or("mix"))?;
        parse_batch(&self.json(
            &format!(
                "batch/{angle}?nb={}&difficulty={}",
                count.clamp(1, 100),
                difficulty(target)
            ),
            cancel,
        )?)
    }
    fn daily(&mut self, cancel: &CancelFlag) -> Result<Puzzle, SourceError> {
        parse_puzzle(&self.json("daily", cancel)?)
    }
    fn by_id(&mut self, id: &str, cancel: &CancelFlag) -> Result<Puzzle, SourceError> {
        parse_puzzle(&self.json(identifier(id)?, cancel)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    #[test]
    fn rating_maps_relative_to_anonymous_1500() {
        for (r, expected) in [
            (800, "easiest"),
            (1200, "easier"),
            (1500, "normal"),
            (1750, "harder"),
            (2100, "hardest"),
        ] {
            assert_eq!(difficulty(r), expected);
        }
    }
    #[test]
    fn rate_limit_retries_after_sixty_seconds_and_sets_user_agent() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for status in ["429 Too Many Requests", "200 OK"] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 4096];
                let n = stream.read(&mut request).unwrap();
                assert!(
                    String::from_utf8_lossy(&request[..n])
                        .to_lowercase()
                        .contains(concat!("user-agent: woodpeck/", env!("CARGO_PKG_VERSION")))
                );
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
            }
        });
        let mut waits = vec![];
        assert!(
            request_with_wait(&agent(), &url, &CancelFlag::default(), |d, _| {
                waits.push(d);
                Ok(())
            })
            .is_ok()
        );
        server.join().unwrap();
        assert_eq!(waits, vec![Duration::from_secs(60)]);
    }
    #[test]
    fn cancellation_interrupts_rate_limit_wait() {
        let cancel = CancelFlag::default();
        cancel.cancel();
        assert!(matches!(
            wait(Duration::from_secs(60), &cancel),
            Err(SourceError::Cancelled)
        ));
        assert!(matches!(
            request(&agent(), "http://127.0.0.1:1", &cancel),
            Err(SourceError::Cancelled)
        ));
    }
    #[test]
    fn single_daily_or_id_shape_matches_batch_reconstruction() {
        let json: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/batch.json")).unwrap();
        let single = parse_puzzle(&json["puzzles"][0].to_string()).unwrap();
        assert_eq!(single, parse_batch(&json.to_string()).unwrap()[0]);
    }
}
