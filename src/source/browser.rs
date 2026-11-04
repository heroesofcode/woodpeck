//! Lichess puzzle API over the browser's `fetch`. Lichess answers with
//! `Access-Control-Allow-Origin: *`, so no proxy is needed.
use super::{
    CancelFlag, SourceError,
    api::{API_BASE, batch_path, identifier},
    parse_batch, parse_puzzle,
};
use crate::puzzle::Puzzle;

const LIMIT: usize = 8 * 1024 * 1024;

async fn sleep(milliseconds: u32, cancel: &CancelFlag) -> Result<(), SourceError> {
    let mut left = milliseconds;
    while left > 0 {
        cancel.check()?;
        let step = left.min(100);
        gloo_timers::future::TimeoutFuture::new(step).await;
        left -= step;
    }
    cancel.check()
}
async fn json(path: &str, cancel: &CancelFlag) -> Result<String, SourceError> {
    loop {
        cancel.check()?;
        let response = ehttp::fetch_async(ehttp::Request::get(format!("{API_BASE}/{path}")))
            .await
            .map_err(SourceError::Network)?;
        cancel.check()?;
        if response.status == 429 {
            sleep(60_000, cancel).await?;
            continue;
        }
        if !response.ok {
            return Err(SourceError::Network(format!("HTTP {}", response.status)));
        }
        if response.bytes.len() > LIMIT {
            return Err(SourceError::Network("Response is too large.".into()));
        }
        return String::from_utf8(response.bytes).map_err(|e| SourceError::Network(e.to_string()));
    }
}
/// Same requests as the desktop `LichessClient`, awaited instead of blocking a thread.
pub async fn fetch_batch(
    theme: Option<&str>,
    difficulty: &str,
    count: u32,
    cancel: &CancelFlag,
) -> Result<Vec<Puzzle>, SourceError> {
    parse_batch(&json(&batch_path(theme, difficulty, count)?, cancel).await?)
}
pub async fn fetch_daily(cancel: &CancelFlag) -> Result<Puzzle, SourceError> {
    parse_puzzle(&json("daily", cancel).await?)
}
pub async fn fetch_by_id(id: &str, cancel: &CancelFlag) -> Result<Puzzle, SourceError> {
    parse_puzzle(&json(identifier(id)?, cancel).await?)
}
