//! Importer seam: future OCR implements `PageSource` and returns the same text/block document.
use super::document::{Asset, BlockKind, Document, Page};
use scraper::{Html, Selector};
use std::{
    collections::HashMap,
    io::Read,
    path::{Component, Path, PathBuf},
};
use thiserror::Error;
mod pdf;

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("This PDF has no text layer (scanned). Scanned books are not supported yet.")]
    NoTextLayer,
    #[error("Unsupported book format; expected EPUB or PDF.")]
    Unsupported,
    #[error("Invalid book: {0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
pub trait PageSource {
    fn document(&self, path: &Path) -> Result<Document, ImportError>;
}
pub struct TextImporter;
impl PageSource for TextImporter {
    fn document(&self, path: &Path) -> Result<Document, ImportError> {
        let bytes = std::fs::read(path)?;
        if bytes.starts_with(b"%PDF-") {
            let extracted =
                pdf::extract(&bytes).map_err(|e| ImportError::Invalid(e.to_string()))?;
            let texts = &extracted.pages;
            if texts.is_empty()
                || texts
                    .iter()
                    .filter(|t| {
                        t.iter()
                            .flat_map(|l| l.text.chars())
                            .filter(|c| c.is_alphanumeric())
                            .count()
                            < 8
                    })
                    .count()
                    * 2
                    > texts.len()
            {
                return Err(ImportError::NoTextLayer);
            }
            let body = body_size(texts);
            let pages = extracted
                .pages
                .iter()
                .enumerate()
                .map(|(i, lines)| {
                    let mut page = Page {
                        source: format!("page:{}", i + 1),
                        ..Page::default()
                    };
                    let outline: Vec<_> = extracted
                        .outline
                        .iter()
                        .filter(|(_, _, p)| *p == i)
                        .collect();
                    pdf_page(&mut page, lines, body, &outline);
                    page
                })
                .collect();
            let title = extracted.title.clone().unwrap_or_else(|| {
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into()
            });
            return Ok(Document {
                title,
                author: extracted.author.clone().unwrap_or_default(),
                format: "pdf".into(),
                pages,
                assets: vec![],
            });
        }
        if !bytes.starts_with(b"PK") {
            return Err(ImportError::Unsupported);
        }
        epub(&bytes)
    }
}
/// The size most body text is set in, weighted by characters.
fn body_size(pages: &[Vec<pdf::Line>]) -> f64 {
    let mut sizes: Vec<(f64, usize)> = pages
        .iter()
        .flatten()
        .filter(|l| l.size > 0.0)
        .map(|l| (l.size, l.text.len()))
        .collect();
    sizes.sort_by(|a, b| a.0.total_cmp(&b.0));
    let total: usize = sizes.iter().map(|s| s.1).sum();
    let mut seen = 0;
    for (size, n) in &sizes {
        seen += n;
        if seen * 2 >= total {
            return *size;
        }
    }
    0.0
}
/// Lines opening with a move or list number ("12.", "3...", "12 ... Nf6", "1 e4 e6") start
/// their own record.
fn numbered(text: &str) -> bool {
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return false;
    }
    let rest = &text[digits..];
    if rest.starts_with('.') {
        return true;
    }
    let next = rest.trim_start();
    next.len() < rest.len()
        && (next.starts_with("...")
            || next.starts_with('\u{2026}')
            || next
                .split_whitespace()
                .next()
                .is_some_and(|token| san(token.trim_end_matches(['+', '#', '!', '?']))))
}
/// SAN, also accepting castling printed with zeros ("0-0", "0-0-0").
fn san(token: &str) -> bool {
    let token = if token.starts_with("0-0") {
        token.replace('0', "O")
    } else {
        token.to_string()
    };
    shakmaty::san::San::from_ascii(token.as_bytes()).is_ok()
}
/// A token with move numbers and annotations stripped.
fn san_core(token: &str) -> &str {
    let token = token.trim_end_matches(['+', '#', '!', '?', ',', ';', ')']);
    // A move number is digits followed by dots ("12.", "3..."); bare zeros are castling.
    let digits = token.chars().take_while(char::is_ascii_digit).count();
    let rest = &token[digits..];
    let unnumbered = rest.trim_start_matches(['.', '\u{2026}']);
    if digits > 0 && unnumbered.len() < rest.len() {
        unnumbered
    } else {
        token.trim_start_matches(['.', '\u{2026}'])
    }
}
/// A token that reads as a SAN move once move numbers and annotations are stripped.
fn move_token(token: &str) -> bool {
    let token = san_core(token);
    token.len() >= 2 && san(token)
}
/// Text that reads as moves: a piece move, capture or castling ("Nf3", "exd5", "O-O"), or
/// mostly bare squares ("e4 e5"). A title that names a square ("The e4 opening") does not.
fn move_line(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    let squares = words
        .iter()
        .filter(|w| move_token(w) && san_core(w).len() == 2)
        .count();
    words.iter().any(|w| move_token(w) && san_core(w).len() > 2) || squares * 2 > words.len()
}
/// Heading level from type size relative to the body: titles, sections, subsections.
fn pdf_heading(line: &pdf::Line, body: f64) -> Option<u8> {
    let words = line.text.split_whitespace().count();
    if body <= 0.0
        || line.text.len() > 90
        || words == 0
        || line.text.ends_with([',', ';', '-', '\u{ad}'])
        || line.text.contains("(D)")
    {
        return None;
    }
    // Game moves are often set bold or larger ("7 0-0", "12 ... Nf6"); a numbered heading
    // needs a capitalized word after its number ("1 The Kingside Attack").
    let mut tokens = line.text.split_whitespace();
    if tokens
        .next()
        .is_some_and(|t| t.starts_with(|c: char| c.is_ascii_digit()))
        && !tokens.next().is_some_and(|t| {
            // "A", "Queen's", "Nimzo-Indian" are words; "O-O-O", "Nf3" and "Castles" are moves.
            t.starts_with(char::is_uppercase)
                && t.chars()
                    .all(|c| c.is_alphabetic() || matches!(c, '\'' | '\u{2019}' | '-'))
                && !t.chars().all(|c| matches!(c, 'O' | '0' | '-'))
                && !t.eq_ignore_ascii_case("castles")
                && !move_token(t)
        })
    {
        return None;
    }
    // Highlighted moves without a number ("O-O", "Nxe5!") are not headings either.
    if move_line(&line.text) {
        return None;
    }
    let ratio = line.size / body;
    if ratio >= 1.6 {
        Some(1)
    } else if ratio >= 1.35 {
        Some(2)
    } else if ratio >= 1.18 {
        Some(3)
    } else {
        None
    }
}
/// Where each bookmark title is printed: its first line mapped to how many lines it wraps
/// over (up to three) and its level. Longer titles claim their lines first, so a parent title
/// cannot take the line a wrapped child starts on; among several places a title is printed,
/// the largest type wins over a running head. Titles not printed become headings at the top.
fn printed_titles(
    page: &mut Page,
    lines: &[&pdf::Line],
    outline: &[&(u8, String, usize)],
) -> HashMap<usize, (usize, u8)> {
    let squash = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let in_text: Vec<String> = lines.iter().map(|l| squash(&l.text)).collect();
    let mut order: Vec<usize> = (0..outline.len()).collect();
    order.sort_by_key(|&t| std::cmp::Reverse(squash(&outline[t].1).len()));
    let mut claimed = vec![false; lines.len()];
    let mut printed: HashMap<usize, (usize, u8)> = HashMap::new();
    let mut missing = vec![false; outline.len()];
    for t in order {
        let (level, title, _) = outline[t];
        // A bookmark that names a move line ("1 e4 e5 2 Nf3") points into a score: it must
        // neither turn the printed moves into a heading nor start a section over them.
        if move_line(title) {
            continue;
        }
        let squashed = squash(title);
        let found = (0..in_text.len())
            .filter_map(|start| {
                (1..=3).find_map(|count| {
                    let joined: String = in_text.get(start..start + count)?.concat();
                    (!squashed.is_empty()
                        && joined == squashed
                        && !claimed[start..start + count].iter().any(|c| *c))
                    .then_some((start, count))
                })
            })
            .max_by(|a, b| {
                lines[a.0]
                    .size
                    .total_cmp(&lines[b.0].size)
                    .then(b.0.cmp(&a.0))
            });
        match found {
            Some((start, count)) => {
                claimed[start..start + count].fill(true);
                printed.insert(start, (count, *level));
            }
            None => missing[t] = true,
        }
    }
    // Bookmarks whose title is not printed on the page still mark the chapter start.
    for (t, (level, title, _)) in outline.iter().enumerate() {
        if missing[t] {
            page.heading(title, *level);
        }
    }
    printed
}
/// Builds blocks for one PDF page: outline entries and larger type become headings, text
/// diagrams stay intact, and wrapped lines of the same paragraph are joined (with hyphenated
/// words rejoined) so the reader can reflow them.
fn pdf_page(page: &mut Page, lines: &[pdf::Line], body: f64, outline: &[&(u8, String, usize)]) {
    let lines: Vec<_> = lines.iter().filter(|l| !l.text.trim().is_empty()).collect();
    let printed = printed_titles(page, &lines, outline);
    let mut widths: Vec<usize> = lines.iter().map(|l| l.text.chars().count()).collect();
    widths.sort_unstable();
    let full = widths.get(widths.len() * 3 / 4).copied().unwrap_or(0);
    let mut gaps: Vec<f64> = lines
        .windows(2)
        .map(|w| w[0].y - w[1].y)
        .filter(|g| *g > 0.0)
        .collect();
    gaps.sort_by(f64::total_cmp);
    let leading = gaps.get(gaps.len() / 2).copied().unwrap_or(0.0);
    let mut paragraph = String::new();
    let mut last: Option<&pdf::Line> = None;
    // Inside a move record: since the last numbered line, every line has been moves.
    let mut scoring = false;
    let flush = |page: &mut Page, paragraph: &mut String| {
        if !paragraph.is_empty() {
            page.push(paragraph, BlockKind::Paragraph);
            paragraph.clear();
        }
    };
    let mut i = 0;
    while i < lines.len() {
        let group: Vec<&str> = lines
            .get(i..i + 9)
            .map(|g| g.iter().map(|l| l.text.as_str()).collect())
            .unwrap_or_default();
        if group.len() == 9
            && let Some(pos) = super::resolve::diagram(&group.join("\n"))
        {
            flush(page, &mut paragraph);
            page.push(
                &group.join("\n"),
                BlockKind::Diagram {
                    fen: super::resolve::fen(&pos),
                },
            );
            last = None;
            scoring = false;
            i += 9;
            continue;
        }
        if let Some(&(count, level)) = printed.get(&i) {
            flush(page, &mut paragraph);
            let title: Vec<&str> = lines[i..i + count].iter().map(|l| l.text.trim()).collect();
            page.heading(&title.join(" "), level);
            last = None;
            scoring = false;
            i += count;
            continue;
        }
        let line = lines[i];
        // Larger type between two move records is part of the score, not a section title
        // that would start a new game.
        let in_score = scoring && lines.get(i + 1).is_some_and(|next| numbered(&next.text));
        if !in_score && let Some(level) = pdf_heading(line, body) {
            flush(page, &mut paragraph);
            page.heading(&line.text, level);
            last = None;
            scoring = false;
            i += 1;
            continue;
        }
        let continues = last.is_some_and(|prev| {
            let gap = prev.y - line.y;
            let short = prev.text.chars().count() * 10 < full * 7;
            !short
                && prev.cells == 1
                && line.cells == 1
                && !numbered(&line.text)
                && gap > 0.0
                && (leading <= 0.0 || gap <= leading * 1.45)
                && (prev.size - line.size).abs() <= body * 0.1
        });
        if continues {
            if paragraph.ends_with('\u{ad}') {
                // A soft hyphen only marks where the line broke.
                paragraph.pop();
            } else if paragraph.ends_with('-')
                && paragraph[..paragraph.len() - 1].ends_with(char::is_alphabetic)
                && line.text.starts_with(char::is_lowercase)
            {
                paragraph.pop();
            } else {
                paragraph.push(' ');
            }
        } else {
            flush(page, &mut paragraph);
        }
        paragraph.push_str(&line.text);
        scoring = numbered(&line.text) || (scoring && move_line(&line.text));
        last = Some(line);
        i += 1;
    }
    flush(page, &mut paragraph);
}
pub fn load(path: &Path) -> Result<super::model::Book, ImportError> {
    Ok(super::resolve::resolve(TextImporter.document(path)?))
}
fn selector(s: &str) -> Selector {
    Selector::parse(s).expect("constant CSS selector")
}
fn joined(base: &Path, href: &str) -> String {
    let mut result = PathBuf::new();
    for component in base
        .join(href.split('#').next().unwrap_or(href))
        .components()
    {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::Normal(v) => result.push(v),
            _ => {}
        }
    }
    result.to_string_lossy().replace('\\', "/")
}
#[allow(clippy::too_many_lines)] // Keep spine traversal and pagination in source order.
fn epub(bytes: &[u8]) -> Result<Document, ImportError> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| ImportError::Invalid(e.to_string()))?;
    let mut files = HashMap::new();
    let mut total = 0;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| ImportError::Invalid(e.to_string()))?;
        // Bound decompressed input, including images, before allocating model storage.
        let mut data = Vec::new();
        entry
            .by_ref()
            .take(32 * 1024 * 1024 + 1)
            .read_to_end(&mut data)?;
        total += data.len();
        if data.len() > 32 * 1024 * 1024 || total > 128 * 1024 * 1024 {
            return Err(ImportError::Invalid("EPUB exceeds import limits".into()));
        }
        files.insert(
            entry
                .name()
                .map_err(|e| ImportError::Invalid(e.to_string()))?
                .into_owned(),
            data,
        );
    }
    let html = |name: &str| -> Result<Html, ImportError> {
        Ok(Html::parse_document(&String::from_utf8_lossy(
            files
                .get(name)
                .ok_or_else(|| ImportError::Invalid(format!("Missing {name}")))?,
        )))
    };
    let container = html("META-INF/container.xml")?;
    let opf_path = container
        .select(&selector("rootfile"))
        .next()
        .and_then(|e| e.attr("full-path"))
        .ok_or_else(|| ImportError::Invalid("Missing OPF rootfile".into()))?;
    let base = Path::new(opf_path).parent().unwrap_or(Path::new(""));
    let opf = html(opf_path)?;
    let metadata = |name: &str| {
        opf.select(&selector(name))
            .next()
            .map(|e| e.text().collect::<String>())
            .unwrap_or_default()
    };
    let manifest: HashMap<_, _> = opf
        .select(&selector("item"))
        .filter_map(|e| Some((e.attr("id")?.to_owned(), e.attr("href")?.to_owned())))
        .collect();
    let mut doc = Document {
        title: metadata("dc\\:title"),
        author: metadata("dc\\:creator"),
        format: "epub".into(),
        pages: vec![],
        assets: vec![],
    };
    for item in opf.select(&selector("itemref")) {
        let href = item
            .attr("idref")
            .and_then(|id| manifest.get(id))
            .ok_or_else(|| ImportError::Invalid("Invalid spine reference".into()))?;
        let source = joined(base, href);
        let content = html(&source)?;
        let mut page = Page {
            source: source.clone(),
            ..Page::default()
        };
        for el in content.select(&selector("h1,h2,h3,h4,h5,h6,p,table,pre,img")) {
            if el
                .ancestors()
                .filter_map(scraper::ElementRef::wrap)
                .any(|p| {
                    matches!(
                        p.value().name(),
                        "table" | "p" | "pre" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                    )
                })
            {
                continue;
            }
            // Split containers around images so their text cannot run ahead of an
            // intervening diagram. Nested images are emitted here exactly once.
            let mut pieces = Vec::new();
            let mut text = String::new();
            for node in el.descendants() {
                if let Some(image) = scraper::ElementRef::wrap(node)
                    && image.value().name() == "img"
                {
                    if !text.is_empty() {
                        pieces.push((el, std::mem::take(&mut text)));
                    }
                    pieces.push((image, String::new()));
                } else if let scraper::node::Node::Text(value) = node.value() {
                    if el.value().name() != "pre" && !text.is_empty() {
                        text.push(' ');
                    }
                    text.push_str(value);
                }
            }
            if !text.is_empty() {
                pieces.push((el, text));
            }
            for (el, text) in pieces {
                let name = el.value().name();
                let text = if name == "pre" {
                    text
                } else {
                    text.split_whitespace().collect::<Vec<_>>().join(" ")
                };
                if !page.blocks.is_empty()
                    && (page.text.len() + text.len() > 6000 || name == "h1" || name == "h2")
                {
                    doc.pages.push(page);
                    page = Page {
                        source: source.clone(),
                        ..Page::default()
                    };
                }
                let kind = match name {
                    "img" => {
                        let path = joined(
                            Path::new(&source).parent().unwrap_or(base),
                            el.attr("src").unwrap_or(""),
                        );
                        if let Some(bytes) = files.get(&path)
                            && !doc.assets.iter().any(|a| a.path == path)
                        {
                            doc.assets.push(Asset {
                                path: path.clone(),
                                bytes: bytes.clone(),
                            });
                        }
                        BlockKind::Image {
                            path,
                            alt: el.attr("alt").unwrap_or("").into(),
                        }
                    }
                    "table" => BlockKind::Table,
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        page.heading(&text, name.as_bytes()[1] - b'0');
                        continue;
                    }
                    _ => BlockKind::Paragraph,
                };
                page.push(&text, kind);
            }
        }
        if !page.blocks.is_empty() {
            doc.pages.push(page);
        }
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn line(text: &str, size: f64, y: f64) -> pdf::Line {
        pdf::Line {
            text: text.into(),
            size,
            y,
            cells: 1,
        }
    }
    fn blocks(page: &Page) -> Vec<(BlockKind, u8, &str)> {
        page.blocks
            .iter()
            .map(|b| (b.kind.clone(), b.level, &page.text[b.span.clone()]))
            .collect()
    }

    #[test]
    fn pdf_headings_follow_type_size_but_not_bold_moves() {
        assert_eq!(pdf_heading(&line("The Opening", 17.0, 0.0), 10.0), Some(1));
        assert_eq!(pdf_heading(&line("Pawn Endings", 14.0, 0.0), 10.0), Some(2));
        assert_eq!(
            pdf_heading(&line("1 The Kingside Attack", 12.0, 0.0), 10.0),
            Some(3)
        );
        assert_eq!(pdf_heading(&line("Body text", 10.0, 0.0), 10.0), None);
        assert_eq!(pdf_heading(&line("7 0-0 Nf6", 14.0, 0.0), 10.0), None);
        assert_eq!(pdf_heading(&line("12 ... Nf6", 14.0, 0.0), 10.0), None);
        assert_eq!(pdf_heading(&line("9 O-O-O Kb8", 14.0, 0.0), 10.0), None);
        for moves in ["8 Castles", "9 Kt-B3", "10 Bxe5", "11 O-O"] {
            assert_eq!(pdf_heading(&line(moves, 14.0, 0.0), 10.0), None, "{moves}");
        }
        for highlighted in ["O-O", "Nxe5!", "Qh5+ Kf8", "...Bc5"] {
            assert_eq!(pdf_heading(&line(highlighted, 14.0, 0.0), 10.0), None);
        }
        for record in [
            "12. Nf3",
            "12 ... Nf6",
            "12 \u{2026} Nf6",
            "1 e4 e6",
            "3...Bc5",
        ] {
            assert!(numbered(record), "{record}");
        }
        for prose in ["12 pawns were left", "1960 was the year", "Nf3 first"] {
            assert!(!numbered(prose), "{prose}");
        }
        for title in [
            "1 Queen's Gambit",
            "2 Nimzo-Indian Defence",
            "1 An Introduction",
            "2 In the Opening",
            "3 A Quiet Move",
        ] {
            assert_eq!(pdf_heading(&line(title, 14.0, 0.0), 10.0), Some(2));
        }
        for title in ["The e4 Opening", "Play d4", "The c3 Sicilian"] {
            assert_eq!(
                pdf_heading(&line(title, 14.0, 0.0), 10.0),
                Some(2),
                "{title}"
            );
        }
        for moves in ["0-0 Be7", "12.0-0-0 Kb8", "O-O"] {
            assert!(move_line(moves), "{moves}");
        }
        assert!(numbered("7 0-0 Nf6"));
        for moves in ["e4", "e4 e5", "d4 d5 c4"] {
            assert_eq!(pdf_heading(&line(moves, 14.0, 0.0), 10.0), None, "{moves}");
        }
        assert_eq!(
            pdf_heading(&line("A sentence that goes on,", 14.0, 0.0), 10.0),
            None
        );
    }

    #[test]
    fn pdf_lines_reflow_into_paragraphs_and_keep_records() {
        let mut page = Page::default();
        let lines = [
            line("Chapter One", 18.0, 700.0),
            line(
                "The pawn structure decides the plan in many posi-",
                10.0,
                680.0,
            ),
            line(
                "tions, and the knight finds its outpost on the",
                10.0,
                668.0,
            ),
            line("weakened square.", 10.0, 656.0),
            line(
                "1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 5. O-O Be7",
                10.0,
                644.0,
            ),
            line(
                "2. d4 exd4 and the long line continues further on",
                10.0,
                632.0,
            ),
        ];
        pdf_page(&mut page, &lines, 10.0, &[]);
        assert_eq!(
            blocks(&page),
            vec![
                (BlockKind::Heading, 1, "Chapter One"),
                (
                    BlockKind::Paragraph,
                    0,
                    "The pawn structure decides the plan in many positions, and the knight finds its outpost on the weakened square."
                ),
                (
                    BlockKind::Paragraph,
                    0,
                    "1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 5. O-O Be7"
                ),
                (
                    BlockKind::Paragraph,
                    0,
                    "2. d4 exd4 and the long line continues further on"
                ),
            ]
        );
    }

    #[test]
    fn pdf_bookmarks_mark_chapters_missing_from_the_text() {
        let mut page = Page::default();
        let outline = (1, "Part One".to_string(), 0);
        pdf_page(
            &mut page,
            &[line("Some opening words of body text here.", 10.0, 700.0)],
            10.0,
            &[&outline],
        );
        assert_eq!(blocks(&page)[0], (BlockKind::Heading, 1, "Part One"));
    }

    #[test]
    fn a_wrapped_bookmark_title_stays_where_it_is_printed() {
        let mut page = Page::default();
        let outline = (1, "The Middle Game in Practice".to_string(), 0);
        pdf_page(
            &mut page,
            &[
                line("The last words of the previous chapter.", 10.0, 700.0),
                line("The Middle Game", 18.0, 650.0),
                line("in Practice", 18.0, 630.0),
                line("Body text of the new chapter.", 10.0, 600.0),
            ],
            10.0,
            &[&outline],
        );
        assert_eq!(
            blocks(&page),
            [
                (
                    BlockKind::Paragraph,
                    0,
                    "The last words of the previous chapter."
                ),
                (BlockKind::Heading, 1, "The Middle Game in Practice"),
                (BlockKind::Paragraph, 0, "Body text of the new chapter."),
            ]
        );
    }

    #[test]
    fn nested_and_repeated_titles_each_find_their_own_lines() {
        let mut page = Page::default();
        let part = (1, "Part One".to_string(), 0);
        let chapter = (2, "Part One Openings".to_string(), 0);
        let running = (2, "Endings".to_string(), 0);
        pdf_page(
            &mut page,
            &[
                line("Endings", 8.0, 760.0),
                line("Part One", 18.0, 700.0),
                line("Openings", 18.0, 680.0),
                line("Body text of the part.", 10.0, 650.0),
                line("Endings", 16.0, 600.0),
                line("Body text of the chapter.", 10.0, 580.0),
            ],
            10.0,
            &[&part, &chapter, &running],
        );
        assert_eq!(
            blocks(&page),
            [
                (BlockKind::Heading, 1, "Part One"),
                (BlockKind::Paragraph, 0, "Endings"),
                (BlockKind::Heading, 2, "Part One Openings"),
                (BlockKind::Paragraph, 0, "Body text of the part."),
                (BlockKind::Heading, 2, "Endings"),
                (BlockKind::Paragraph, 0, "Body text of the chapter."),
            ]
        );
    }

    #[test]
    fn a_bookmark_naming_moves_leaves_the_printed_moves_alone() {
        let mut page = Page::default();
        let game = (1, "1 e4 e5 2 Nf3 Nc6".to_string(), 0);
        pdf_page(
            &mut page,
            &[
                line("1 e4 e5 2 Nf3 Nc6", 10.0, 700.0),
                line("3 Bb5 a6", 10.0, 680.0),
            ],
            10.0,
            &[&game],
        );
        assert!(blocks(&page).iter().all(|b| b.0 == BlockKind::Paragraph));
        assert!(blocks(&page)[0].2.starts_with("1 e4 e5"));
        let mut page = Page::default();
        pdf_page(
            &mut page,
            &[
                line("1 e4 e5 2 Nf3 Nc6", 10.0, 700.0),
                line("A Quiet Move", 14.0, 680.0),
                line("3 Bb5 a6", 10.0, 660.0),
            ],
            10.0,
            &[],
        );
        assert!(blocks(&page).iter().all(|b| b.0 == BlockKind::Paragraph));
        // A record that wraps onto a second line of moves is still the score.
        let mut page = Page::default();
        pdf_page(
            &mut page,
            &[
                line("1 e4 e5 2 Nf3 Nc6 3 Bb5 a6 4 Ba4 Nf6", 10.0, 700.0),
                line("O-O Be7 Re1 b5", 10.0, 688.0),
                line("A Quiet Move", 14.0, 668.0),
                line("6 Bb3 d6", 10.0, 648.0),
            ],
            10.0,
            &[],
        );
        assert!(blocks(&page).iter().all(|b| b.0 == BlockKind::Paragraph));
    }

    #[test]
    fn epub_heading_levels_are_kept() {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, data) in [
            (
                "META-INF/container.xml",
                "<container><rootfile full-path='book.opf'/></container>",
            ),
            (
                "book.opf",
                "<package><manifest><item id='chapter' href='chapter.html'/></manifest><spine><itemref idref='chapter'/></spine></package>",
            ),
            (
                "chapter.html",
                "<h1>Chapter</h1><h3>Section</h3><p>Text.</p><h6>Note</h6>",
            ),
        ] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(data.as_bytes()).unwrap();
        }
        let document = epub(&zip.finish().unwrap().into_inner()).unwrap();
        let levels: Vec<(u8, String)> = document
            .pages
            .iter()
            .flat_map(|p| {
                p.blocks
                    .iter()
                    .filter(|b| b.kind == BlockKind::Heading)
                    .map(|b| (b.heading_level(), p.text[b.span.clone()].to_string()))
            })
            .collect();
        assert_eq!(
            levels,
            vec![
                (1, "Chapter".into()),
                (3, "Section".into()),
                (6, "Note".into())
            ]
        );
    }

    #[test]
    fn nested_images_invalidate_moves_in_source_order() {
        for body in [
            "<p><img src='diagram.png'/>1. e4 e5</p>",
            "<table><tr><td><img src='diagram.png'/>1. e4 e5</td></tr></table>",
            "<pre><img src='diagram.png'/>1. e4 e5</pre>",
            "<p>1. d4 d5 <span><img src='diagram.png'/></span>2. Nf3 Nf6</p>",
        ] {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            for (name, data) in [
                (
                    "META-INF/container.xml",
                    "<container><rootfile full-path='book.opf'/></container>",
                ),
                (
                    "book.opf",
                    "<package><manifest><item id='chapter' href='chapter.html'/></manifest><spine><itemref idref='chapter'/></spine></package>",
                ),
                ("chapter.html", body),
                ("diagram.png", "image bytes"),
            ] {
                zip.start_file(name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(data.as_bytes()).unwrap();
            }
            let document = epub(&zip.finish().unwrap().into_inner()).unwrap();
            assert_eq!(document.assets.len(), 1);
            assert_eq!(
                document
                    .pages
                    .iter()
                    .flat_map(|p| &p.blocks)
                    .filter(|b| matches!(b.kind, BlockKind::Image { .. }))
                    .count(),
                1
            );
            let book = super::super::resolve::resolve(document);
            let expected = if body.contains("d4") {
                vec!["d2d4", "d7d5"]
            } else {
                vec![]
            };
            assert_eq!(
                book.nodes
                    .iter()
                    .map(|n| n.uci.as_str())
                    .collect::<Vec<_>>(),
                expected,
                "{body}"
            );
            assert!(book.fragments.iter().any(|f| matches!(&f.status, super::super::model::Status::Unresolved(reason) if reason.contains("Unrecognized image"))));
        }
    }
}
