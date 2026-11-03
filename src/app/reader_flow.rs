//! Presentation-only PDF reflow. Source offsets and verified chess positions never change.
use crate::books::reflow::{lost_hyphen, vocabulary};
use crate::books::{
    document::{Block, BlockKind, Page},
    model::{Book, Status},
};
use std::{collections::HashSet, ops::Range};

pub(super) struct PageRuns {
    pub page: Page,
    pub runs: Vec<(Range<usize>, Option<usize>)>,
    pub unresolved: Vec<(Range<usize>, String)>,
}

/// Short standalone labels, captions and score lines are never absorbed into prose.
fn standalone(text: &str) -> bool {
    let text = text.trim();
    text.is_empty()
        || crate::books::reflow::caption(text)
        || text.chars().all(|c| !c.is_lowercase())
        || text.starts_with(|c: char| c.is_ascii_digit())
        || (text.split_whitespace().count() >= 2
            && text.split_whitespace().count() <= 10
            && !text.ends_with(['.', ',', ';', ':', '!', '?'])
            && text.split_whitespace().all(|word| {
                word.starts_with(char::is_uppercase)
                    || ["a", "an", "and", "in", "of", "on", "the", "to"].contains(&word)
            }))
}

impl PageRuns {
    pub fn new(book: &Book, index: usize) -> Self {
        let source = &book.document.pages[index];
        let runs: Vec<_> = book
            .text_runs(index)
            .unwrap_or_default()
            .into_iter()
            .map(|r| (r.span, r.node_id))
            .collect();
        let unresolved: Vec<_> = book
            .fragments
            .iter()
            .filter(|f| f.page == index)
            .filter_map(|f| match &f.status {
                Status::Partial(reason) | Status::Unresolved(reason) => {
                    Some((f.span.clone(), reason.clone()))
                }
                Status::Resolved => None,
            })
            .collect();
        let mut page = source.clone();
        if page.blocks.is_empty() {
            page.blocks.push(Block {
                span: 0..page.text.len(),
                kind: BlockKind::Paragraph,
                level: 0,
            });
        }
        if book.document.format != "pdf" {
            return Self {
                page,
                runs,
                unresolved,
            };
        }
        page.blocks = split_paragraphs(&page);
        let protected = |span: &Range<usize>| {
            runs.iter()
                .any(|(r, n)| n.is_some() && r.start < span.end && r.end > span.start)
                || unresolved
                    .iter()
                    .any(|(r, _)| r.start < span.end && r.end > span.start)
        };
        let mut widths: Vec<_> = page
            .blocks
            .iter()
            .filter(|b| b.kind == BlockKind::Paragraph)
            .map(|b| page.text[b.span.clone()].trim().chars().count())
            .collect();
        widths.sort_unstable();
        let full = widths.get(widths.len() * 3 / 4).copied().unwrap_or(0);
        let mut blocks: Vec<Block> = Vec::new();
        for block in &page.blocks {
            let join = blocks.last().is_some_and(|prev| {
                if page.paragraphs
                    || prev.kind != BlockKind::Paragraph
                    || block.kind != BlockKind::Paragraph
                    || protected(&prev.span)
                    || protected(&block.span)
                {
                    return false;
                }
                let left = page.text[prev.span.clone()].trim_end();
                let right = &page.text[block.span.clone()];
                let tail = left.lines().last().unwrap_or(left);
                let gap = &page.text[prev.span.end..block.span.start];
                let sentence = left.ends_with(['.', '!', '?', ':', '”', '"']);
                !standalone(tail)
                    && !standalone(right)
                    && !gap.contains("\n\n")
                    && !right.starts_with(char::is_whitespace)
                    && (right.starts_with(char::is_lowercase)
                        || (!sentence && tail.chars().count() * 10 >= full * 7))
                    && (!sentence || tail.chars().count() * 10 >= full * 9)
            });
            if join {
                blocks.last_mut().unwrap().span.end = block.span.end;
            } else {
                blocks.push(block.clone());
            }
        }
        let (text, mapping) = display_text(&page, &blocks, &protected);
        let map = |span: Range<usize>| mapping[span.start]..mapping[span.end];
        for block in &mut blocks {
            block.span = map(block.span.clone());
        }
        page.text = text;
        page.blocks = blocks;
        Self {
            page,
            runs: runs.into_iter().map(|(r, n)| (map(r), n)).collect(),
            unresolved: unresolved.into_iter().map(|(r, s)| (map(r), s)).collect(),
        }
    }
}

fn split_paragraphs(page: &Page) -> Vec<Block> {
    // Explicit blank lines inside older text blocks still mark paragraphs.
    page.blocks
        .iter()
        .flat_map(|block| {
            if block.kind != BlockKind::Paragraph {
                return vec![block.clone()];
            }
            let mut start = block.span.start;
            let mut parts = Vec::new();
            for part in page.text[block.span.clone()].split("\n\n") {
                if !part.trim().is_empty() {
                    parts.push(Block {
                        span: start..start + part.len(),
                        ..block.clone()
                    });
                }
                start += part.len() + 2;
            }
            parts
        })
        .collect()
}

/// Map original UTF-8 boundaries while removing only physical line-break artifacts.
fn display_text(
    page: &Page,
    blocks: &[Block],
    protected: &impl Fn(&Range<usize>) -> bool,
) -> (String, Vec<usize>) {
    // A map from every original byte boundary to its displayed boundary keeps moves
    // clickable even after a preceding paragraph loses a hyphen or a line break.
    let mut remove = vec![false; page.text.len()];
    let mut spaces = HashSet::new();
    let words = vocabulary(&page.text);
    for block in blocks {
        if block.kind != BlockKind::Paragraph || protected(&block.span) {
            continue;
        }
        let text = &page.text[block.span.clone()];
        for (offset, _) in text.match_indices('\n') {
            let left = text[..offset].trim_end();
            let right = text[offset + 1..].trim_start();
            let start = block.span.start + left.len();
            let end = block.span.start + text.len() - right.len();
            let hyphen = left.ends_with(['-', '\u{ad}']) && right.starts_with(char::is_lowercase);
            if hyphen {
                let length = left.chars().next_back().unwrap().len_utf8();
                remove[start - length..end].fill(true);
            } else if right.starts_with(char::is_lowercase) && lost_hyphen(left, right, &words) {
                remove[start..end].fill(true);
            } else {
                remove[start..end].fill(true);
                if start < end {
                    remove[start] = false;
                    spaces.insert(start);
                }
            }
        }
    }
    let mut text = String::new();
    let mut mapping = vec![0; page.text.len() + 1];
    for (byte, c) in page.text.char_indices() {
        mapping[byte] = text.len();
        if !remove[byte] {
            text.push(if spaces.contains(&byte) { ' ' } else { c });
        }
    }
    mapping[page.text.len()] = text.len();
    (text, mapping)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::books::{document::Document, resolve};
    fn book(format: &str) -> Book {
        let mut page = Page::default();
        page.heading("An open game", 1);
        page.push(
            "A tournament teaches patient play. In another tourna-",
            BlockKind::Paragraph,
        );
        page.push(
            "ment the same lesson matters. A knight can support the",
            BlockKind::Paragraph,
        );
        page.push("centre before the attack begins.", BlockKind::Paragraph);
        page.push("Diagram 1", BlockKind::Paragraph);
        page.push("1. e4 e5 2. Nf3 Nc6", BlockKind::Paragraph);
        resolve::resolve(Document {
            title: "Study".into(),
            author: String::new(),
            format: format.into(),
            pages: vec![page],
            assets: vec![],
            cover: None,
            isbn: None,
        })
    }
    #[test]
    fn pdf_reflow_keeps_every_verified_move_and_caption() {
        let book = book("pdf");
        let display = PageRuns::new(&book, 0);
        assert_eq!(display.page.blocks.len(), 4);
        let paragraph = &display.page.blocks[1];
        assert_eq!(
            &display.page.text[paragraph.span.clone()],
            "A tournament teaches patient play. In another tournament the same lesson matters. A knight can support the centre before the attack begins."
        );
        assert_eq!(
            &display.page.text[display.page.blocks[2].span.clone()],
            "Diagram 1"
        );
        for (span, node) in &display.runs {
            if let Some(id) = node {
                let source = &book.nodes[*id];
                assert_eq!(
                    &display.page.text[span.clone()],
                    &book.document.pages[0].text[source.span.clone()]
                );
            }
        }
        assert_eq!(display.runs.iter().filter(|(_, n)| n.is_some()).count(), 4);
    }
    #[test]
    fn html_paragraphs_and_new_pdf_boundaries_are_authoritative() {
        for format in ["epub", "pdf"] {
            let mut book = book(format);
            book.document.pages[0].paragraphs = true;
            let display = PageRuns::new(&book, 0);
            assert_eq!(display.page.text, book.document.pages[0].text);
            assert_eq!(
                display.page.blocks.len(),
                book.document.pages[0].blocks.len()
            );
        }
    }
    #[test]
    fn blank_lines_indents_titles_and_diagrams_are_boundaries() {
        let mut book = book("pdf");
        let mut page = Page::default();
        page.push("The plan continues across the", BlockKind::Paragraph);
        page.text.push('\n');
        page.push("board with care.", BlockKind::Paragraph);
        page.push(
            "    An indented paragraph begins here.",
            BlockKind::Paragraph,
        );
        page.heading("The next section", 2);
        page.push(
            "",
            BlockKind::Image {
                path: "diagram.png".into(),
                alt: "Position".into(),
            },
        );
        book.document.pages = vec![page];
        book.nodes.clear();
        book.fragments.clear();
        assert_eq!(PageRuns::new(&book, 0).page.blocks.len(), 5);
    }
    #[test]
    fn missing_hyphen_requires_whole_word_evidence() {
        let words = vocabulary("tournament careful play");
        assert!(lost_hyphen("a tourna", "ment begins", &words));
        assert!(!lost_hyphen("a tourna", "ment begins", &HashSet::new()));
        assert!(!lost_hyphen("walk in", "to the room", &vocabulary("into")));
    }
    #[test]
    fn unicode_before_moves_keeps_byte_boundaries() {
        let mut book = book("pdf");
        let mut page = Page::default();
        page.push("Une compétition commence avec prépa-", BlockKind::Paragraph);
        page.push("ration et patience.", BlockKind::Paragraph);
        page.push("1. e4 e5", BlockKind::Paragraph);
        book.document.pages = vec![page];
        book = resolve::resolve(book.document);
        let display = PageRuns::new(&book, 0);
        assert!(display.page.text.contains("préparation"));
        for (span, node) in display.runs {
            if let Some(id) = node {
                assert_eq!(
                    &display.page.text[span],
                    &book.document.pages[0].text[book.nodes[id].span.clone()]
                );
            }
        }
    }
}
