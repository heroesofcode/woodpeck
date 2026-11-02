//! Cover lookup on Open Library for books that carry no cover of their own. Only the title,
//! author and ISBN leave the machine; results are cached by the caller. Only a definite "not
//! found" is a miss; being offline, throttled or refused is an error the caller retries later.
//! Either way the library draws a generated cover meanwhile.
use std::time::Duration;

const SEARCH: &str = "https://openlibrary.org/search.json";
const COVERS: &str = "https://covers.openlibrary.org/b";
const LIMIT: u64 = 4 * 1024 * 1024;

/// Looks a cover up by ISBN first, then by title and author. Returns image bytes that
/// decode as JPEG or PNG, `Ok(None)` when Open Library has no cover for the book, and an
/// error when it could not be reached (offline), so the caller tries again later.
pub fn lookup(
    title: &str,
    author: &str,
    isbn: Option<&str>,
) -> Result<Option<Vec<u8>>, ureq::Error> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .user_agent(concat!("woodpeck/", env!("CARGO_PKG_VERSION")))
        .timeout_global(Some(Duration::from_secs(20)))
        .http_status_as_error(false)
        .build()
        .into();
    if let Some(isbn) = isbn
        && let Some(image) = image(&agent, &format!("{COVERS}/isbn/{isbn}-M.jpg?default=false"))?
    {
        return Ok(Some(image));
    }
    let words = significant(title);
    if words.is_empty() {
        return Ok(None);
    }
    let mut request = agent
        .get(SEARCH)
        .query("title", title)
        .query("fields", "title,cover_i")
        .query("limit", "10");
    if !author.trim().is_empty() {
        request = request.query("author", author);
    }
    let mut response = request.call()?;
    if !found(response.status().as_u16())? {
        return Ok(None);
    }
    let json = response
        .body_mut()
        .with_config()
        .limit(LIMIT)
        .read_to_string()?;
    match matching_cover(&json, &words) {
        Some(id) => image(&agent, &format!("{COVERS}/id/{id}-M.jpg?default=false")),
        None => Ok(None),
    }
}

/// Whether a response has a body to read: success yes, not found no, and anything else
/// (rate limits such as 403 and 429, server errors) an error so it is not cached as a miss.
fn found(status: u16) -> Result<bool, ureq::Error> {
    match status {
        200..=299 => Ok(true),
        404 | 410 => Ok(false),
        _ => Err(ureq::Error::StatusCode(status)),
    }
}

/// A cover image, `None` when there is none (404 or not a picture).
fn image(agent: &ureq::Agent, url: &str) -> Result<Option<Vec<u8>>, ureq::Error> {
    let mut response = agent.get(url).call()?;
    if !found(response.status().as_u16())? {
        return Ok(None);
    }
    let bytes = response
        .body_mut()
        .with_config()
        .limit(LIMIT)
        .read_to_vec()?;
    Ok(matches!(
        image::guess_format(&bytes),
        Ok(image::ImageFormat::Jpeg | image::ImageFormat::Png)
    )
    .then_some(bytes))
}

/// Lowercase title words of three or more letters, the first six of which a match must
/// contain.
fn significant(title: &str) -> Vec<String> {
    let mut words = title_words(title);
    words.truncate(6);
    words
}
/// Every lowercase title word of three or more letters, without filler words.
fn title_words(title: &str) -> Vec<String> {
    title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3)
        .map(str::to_lowercase)
        .filter(|w| !matches!(w.as_str(), "the" | "and" | "for" | "with"))
        .collect()
}

/// The cover id of the first search result whose title has every significant word of ours,
/// so a loose search never puts another book's cover on this one.
fn matching_cover(json: &str, words: &[String]) -> Option<u64> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    value["docs"].as_array()?.iter().find_map(|doc| {
        // All of their words: ours may sit past the sixth word of a longer title.
        let theirs = title_words(doc["title"].as_str()?);
        let id = doc["cover_i"].as_u64()?;
        words.iter().all(|w| theirs.contains(w)).then_some(id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_not_found_is_a_miss() {
        assert!(found(200).unwrap());
        assert!(!found(404).unwrap());
        for status in [403, 429, 500, 503] {
            assert!(found(status).is_err(), "{status}");
        }
    }

    #[test]
    fn only_a_result_with_our_title_lends_its_cover() {
        let words = significant("Pawn Endings: Step by Step");
        assert_eq!(words, ["pawn", "endings", "step", "step"]);
        let json = r#"{"docs":[
            {"title":"Chess for Beginners","cover_i":1},
            {"title":"Pawn endings, step by step","cover_i":2},
            {"title":"Pawn Endings Step by Step"}
        ]}"#;
        assert_eq!(matching_cover(json, &words), Some(2));
        assert_eq!(
            matching_cover(r#"{"docs":[{"title":"Other","cover_i":3}]}"#, &words),
            None
        );
        assert_eq!(matching_cover("not json", &words), None);
        assert_eq!(significant("The A to Z"), Vec::<String>::new());
    }

    #[test]
    fn a_word_past_the_sixth_of_their_title_still_matches() {
        let words = significant("Manual Chess Endgames");
        let json = r#"{"docs":[
            {"title":"Practical Lessons From Masters Every Club Player Manual: Chess Endgames","cover_i":4}
        ]}"#;
        assert_eq!(matching_cover(json, &words), Some(4));
    }
}
