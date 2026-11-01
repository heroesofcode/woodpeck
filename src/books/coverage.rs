//! Conservative, parser-independent surface-token coverage. This is not semantic accuracy.
use super::{
    eval::Metrics,
    model::{Book, Status},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, ops::Range};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Coverage {
    pub tokens: usize,
    pub resolved: usize,
    pub partial: usize,
    pub unresolved: usize,
    pub reasons: BTreeMap<String, usize>,
    pub diagnostics: BTreeMap<String, usize>,
    pub examples: BTreeMap<String, Vec<String>>,
}
impl Coverage {
    pub fn add(&mut self, other: &Self) {
        self.tokens += other.tokens;
        self.resolved += other.resolved;
        self.partial += other.partial;
        self.unresolved += other.unresolved;
        for (reason, count) in &other.diagnostics {
            *self.diagnostics.entry(reason.clone()).or_default() += count;
        }
        for (reason, count) in &other.reasons {
            *self.reasons.entry(reason.clone()).or_default() += count;
        }
        for (reason, snippets) in &other.examples {
            let examples = self.examples.entry(reason.clone()).or_default();
            for snippet in snippets.iter().take(1) {
                if examples.len() < 3 && !examples.contains(snippet) {
                    examples.push(snippet.clone());
                }
            }
        }
    }
}
fn shape(s: &str) -> bool {
    let s = s.trim_matches(['.', ',', ';', ':', '!', '?', '+', '#']);
    if matches!(s, "O-O" | "O-O-O" | "0-0" | "0-0-0" | "Castles") {
        return true;
    }
    // Algebraic / LAN surface grammar. Require a destination coordinate and
    // bounded source qualifier; accepting any string containing e4 counts prose.
    let algebraic = s.replace("Kt", "N").replace('×', "x");
    let mut body = algebraic.as_str();
    let mut piece = false;
    if let Some(c) = body.chars().next()
        && "KQRBNDTCLSFAPHWG♔♕♖♗♘♚♛♜♝♞".contains(c)
    {
        piece = true;
        body = &body[c.len_utf8()..];
    }
    if let Some((base, promotion)) = body.split_once('=') {
        if !["Q", "R", "B", "N", "D", "T", "C", "S"].contains(&promotion) {
            return false;
        }
        body = base;
    }
    let b = body.as_bytes();
    if b.len() >= 2 && b.is_ascii() {
        let destination = &b[b.len() - 2..];
        if (b'a'..=b'h').contains(&destination[0]) && (b'1'..=b'8').contains(&destination[1]) {
            let source = &body[..body.len() - 2];
            let qualifier = source.trim_end_matches(['x', '-', ':']);
            let source_square = qualifier.len() == 2
                && (b'a'..=b'h').contains(&qualifier.as_bytes()[0])
                && (b'1'..=b'8').contains(&qualifier.as_bytes()[1]);
            if qualifier.is_empty()
                || qualifier.len() == 1
                    && qualifier
                        .bytes()
                        .all(|c| (b'a'..=b'h').contains(&c) || piece && (b'1'..=b'8').contains(&c))
                || source_square && (piece || source.ends_with(['-', 'x', ':']))
            {
                return true;
            }
        }
    }
    let s = s
        .to_uppercase()
        .replace(['.', '\'', '’'], "")
        .replace("TAKES", "X")
        .replace("TO", "-")
        .replace('×', "X");
    let Some((role, dest)) = s.split_once(['-', 'X', '—', '–']) else {
        return false;
    };
    if ![
        "P", "N", "KT", "B", "R", "Q", "K", "KKT", "QKT", "KN", "QN", "KR", "QR", "KBP", "QBP",
        "KRP", "QRP",
    ]
    .contains(&role)
    {
        return false;
    }
    if ["P", "N", "KT", "B", "R", "Q", "K"].contains(&dest) && s.contains('X') {
        return true;
    }
    let dest = dest.trim_end_matches("CH").trim_end_matches("MATE");
    let dest = dest
        .replace("SQ", "1")
        .replace("1ST", "1")
        .replace("2D", "2")
        .replace("3D", "3")
        .replace('S', "");
    let Some(rank) = dest.chars().last() else {
        return false;
    };
    ('1'..='8').contains(&rank)
        && [
            "R", "KT", "N", "B", "Q", "K", "QR", "QKT", "QN", "QB", "KB", "KKT", "KN", "KR",
        ]
        .contains(&&dest[..dest.len() - 1])
}
fn spans(text: &str) -> Vec<Range<usize>> {
    let words: Vec<_> = text
        .split_whitespace()
        .scan(0, |offset, w| {
            let start = *offset + text[*offset..].find(w)?;
            *offset = start + w.len();
            Some((start, *offset, w))
        })
        .collect();
    let mut result = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let mut joined = String::new();
        let mut found = None;
        for (j, word) in words.iter().enumerate().skip(i).take(8) {
            joined.push_str(word.2);
            if shape(&joined) {
                found = Some(j);
                break;
            }
            if joined.len() > 32 {
                break;
            }
        }
        if let Some(j) = found {
            result.push(words[i].0..words[j].1);
            i = j + 1;
        } else {
            i += 1;
        }
    }
    result
}
fn category(reason: &str, snippet: &str) -> String {
    if reason == "No detected fragment" {
        return "Move-like text outside detected fragments".into();
    }
    if reason.contains("Unrecognized image") {
        return "Unrecognized image / diagram-dependent position".into();
    }
    if reason.contains("No known position") {
        return "Unknown starting position / diagram-dependent fragment".into();
    }
    if snippet.contains("3d") || snippet.contains(" to ") && snippet.contains("Kt.") {
        return "Old-style descriptive typography".into();
    }
    if reason.contains("FEN") {
        return "Invalid FEN-shaped text".into();
    }
    if reason.contains("ambiguous") {
        return "Illegal or ambiguous move / lost line context".into();
    }
    reason.into()
}
pub(super) fn measure(book: &Book, metrics: &mut Metrics) {
    let mut coverage = Coverage::default();
    let mut example_locations: BTreeMap<String, Vec<(usize, usize)>> = BTreeMap::new();
    for (page_index, page) in book.document.pages.iter().enumerate() {
        for span in spans(&page.text) {
            coverage.tokens += 1;
            let fragment = book.fragments.iter().find(|f| {
                f.page == page_index && f.span.start < span.end && f.span.end > span.start
            });
            let reason = match fragment.map(|f| &f.status) {
                Some(Status::Resolved) => {
                    coverage.resolved += 1;
                    continue;
                }
                Some(Status::Partial(r)) => {
                    coverage.partial += 1;
                    r.as_str()
                }
                Some(Status::Unresolved(r)) => {
                    coverage.unresolved += 1;
                    r.as_str()
                }
                None => {
                    coverage.unresolved += 1;
                    "No detected fragment"
                }
            };
            *coverage.diagnostics.entry(reason.to_owned()).or_default() += 1;
            let lo = page.text[..span.start]
                .char_indices()
                .rev()
                .nth(55)
                .map_or(0, |(i, _)| i);
            let hi = page.text[span.end..]
                .char_indices()
                .nth(75)
                .map_or(page.text.len(), |(i, _)| span.end + i);
            let snippet = page.text[lo..hi].replace(['\n', '\r'], " ");
            let category = category(reason, &snippet);
            *coverage.reasons.entry(category.clone()).or_default() += 1;
            let examples = coverage.examples.entry(category.clone()).or_default();
            let locations = example_locations.entry(category).or_default();
            if examples.len() < 3
                && locations
                    .iter()
                    .all(|&(page, start)| page != page_index || start.abs_diff(span.start) > 200)
            {
                examples.push(format!("{} page {}: {snippet}", metrics.name, page_index));
                locations.push((page_index, span.start));
            }
        }
    }
    metrics.coverage = coverage;
}
#[cfg(test)]
mod tests {
    #[test]
    fn surface_detector_does_not_count_years_or_page_numbers() {
        let s =
            "1. 1899 Page 64 Black's King a5 paper e4 P - K 4 K. Kt. to B's 3d. Nbd2 R1e2 Qh4e1";
        let detected: Vec<_> = super::spans(s).into_iter().map(|r| &s[r]).collect();
        assert_eq!(super::spans("e4 e5 Nf3 Nc6").len(), 4);
        assert_eq!(
            detected,
            vec![
                "a5",
                "e4",
                "P - K 4",
                "K. Kt. to B's 3d.",
                "Nbd2",
                "R1e2",
                "Qh4e1"
            ]
        );
    }
}
