//! Resolve only unique legal moves. A failed candidate ends its line; later numbered fragments
//! may recover from an explicitly known ply, but never from an invented position.
use super::{
    document::{BlockKind, Document},
    model::{Book, Fragment, Node, Section, Status},
    notation::{self, Kind, Token},
};
use shakmaty::{CastlingMode, Chess, EnPassantMode, Position, fen::Fen, san::SanPlus};
use std::collections::BTreeMap;

pub fn fen(pos: &Chess) -> String {
    Fen::from_position(pos, EnPassantMode::Legal).to_string()
}
pub fn position(text: &str) -> Option<Chess> {
    text.parse::<Fen>()
        .ok()?
        .into_position(CastlingMode::Standard)
        .ok()
}
fn ply(pos: &Chess) -> u32 {
    (pos.fullmoves().get() - 1) * 2 + u32::from(pos.turn().is_black())
}
#[derive(Clone, Default)]
struct Context {
    pos: Chess,
    parent: Option<usize>,
    known: BTreeMap<u32, (Chess, Option<usize>)>,
    invalid_start: bool,
    explicit_start: bool,
    unrecognized_image: bool,
}
impl Context {
    fn seek(&mut self, target: u32) -> bool {
        if self.invalid_start {
            return false;
        }
        if target == ply(&self.pos) {
            return true;
        }
        if let Some((pos, parent)) = self.known.get(&target) {
            self.pos = pos.clone();
            self.parent = *parent;
            true
        } else if target == 0 && !self.explicit_start {
            *self = Self::default();
            true
        } else {
            false
        }
    }
}
pub fn resolve(mut document: Document) -> Book {
    let assets = std::mem::take(&mut document.assets);
    // Score whole books, rather than interpreting the same ambiguous letter differently per move.
    let mut best = None;
    for (name, alphabet) in notation::ALPHABETS {
        let candidate = resolve_with(document.clone(), name, alphabet);
        if best
            .as_ref()
            .is_none_or(|b: &Book| candidate.nodes.len() > b.nodes.len())
        {
            best = Some(candidate);
        }
    }
    let mut best = best.expect("nonempty alphabet table");
    best.document.assets = assets;
    best
}
fn resolve_with(mut document: Document, language: &str, alphabet: &str) -> Book {
    let mut book = Book {
        version: 1,
        document: document.clone(),
        language: language.into(),
        nodes: vec![],
        fragments: vec![],
        sections: vec![Section {
            title: document.title.clone(),
            page: 0,
            fragments: vec![],
            status: Status::Resolved,
        }],
    };
    let mut context = Context::default();
    for (page_index, page) in document.pages.iter_mut().enumerate() {
        for block in &mut page.blocks {
            if block.kind == BlockKind::Heading {
                context = Context::default();
                book.sections.push(Section {
                    title: page.text[block.span.clone()].to_owned(),
                    page: page_index,
                    fragments: vec![],
                    status: Status::Resolved,
                });
                continue;
            }
            // An unrecognized image may introduce a new diagram position. Carrying an
            // opening position through it can produce legal but entirely wrong endgames.
            if matches!(block.kind, BlockKind::Image { .. }) {
                context.invalid_start = true;
                context.unrecognized_image = true;
                context.known.clear();
                continue;
            }
            let text = &page.text[block.span.clone()];
            if let Some(pos) = find_fen(text).or_else(|| diagram(text)) {
                block.kind = BlockKind::Diagram { fen: fen(&pos) };
                context = Context {
                    known: BTreeMap::from([(ply(&pos), (pos.clone(), None))]),
                    pos,
                    explicit_start: true,
                    ..Context::default()
                };
                continue;
            }
            let words: Vec<_> = text.split_whitespace().collect();
            if words
                .windows(6)
                .any(|w| w[0].matches('/').count() == 7 && matches!(w[1], "w" | "b"))
            {
                context.invalid_start = true;
                book.fragments.push(Fragment {
                    page: page_index,
                    span: block.span.clone(),
                    start_fen: String::new(),
                    nodes: vec![],
                    status: Status::Unresolved("Invalid FEN in source text".into()),
                });
                let section = book.sections.last_mut().expect("initial section");
                section.fragments.push(book.fragments.len() - 1);
                continue;
            }
            if matches!(block.kind, BlockKind::Image { .. }) {
                continue;
            }
            let ts = notation::tokens(text);
            if !ts.iter().any(|t| matches!(t.kind, Kind::Number(_))) {
                continue;
            }
            let start = fen(&context.pos);
            let mut fragment = Fragment {
                page: page_index,
                span: block.span.clone(),
                start_fen: start,
                nodes: vec![],
                status: Status::Resolved,
            };
            resolve_tokens(
                &mut book,
                &mut context,
                &ts,
                text,
                block.span.start,
                alphabet,
                &mut fragment,
            );
            if !fragment.nodes.is_empty() || fragment.status != Status::Resolved {
                let section = book.sections.last_mut().expect("initial section");
                section.fragments.push(book.fragments.len());
                book.fragments.push(fragment);
            }
        }
    }
    summarize_sections(&mut book);
    book.document = document;
    book
}
fn summarize_sections(book: &mut Book) {
    for section in &mut book.sections {
        let reason = section
            .fragments
            .iter()
            .find_map(|i| match &book.fragments[*i].status {
                Status::Resolved => None,
                Status::Partial(reason) | Status::Unresolved(reason) => Some(reason.clone()),
            });
        section.status = reason.map_or(Status::Resolved, |reason| {
            if section
                .fragments
                .iter()
                .any(|i| !book.fragments[*i].nodes.is_empty())
            {
                Status::Partial(reason)
            } else {
                Status::Unresolved(reason)
            }
        });
    }
}
fn fail(fragment: &mut Fragment, reason: String) {
    fragment.status = if fragment.nodes.is_empty() {
        Status::Unresolved(reason)
    } else {
        Status::Partial(reason)
    };
}
#[allow(clippy::too_many_lines)] // Token transitions share the same branch stack and safety gate.
fn resolve_tokens(
    book: &mut Book,
    context: &mut Context,
    ts: &[Token],
    text: &str,
    offset: usize,
    alphabet: &str,
    fragment: &mut Fragment,
) {
    let mut stack = Vec::new();
    let mut before = context.clone();
    let mut active = false;
    let mut blocked = false;
    let mut i = 0;
    while i < ts.len() {
        let t = &ts[i];
        let raw = &text[t.span.clone()];
        match t.kind {
            Kind::Number(n) => {
                let mut black = raw.ends_with("...");
                if ts
                    .get(i + 1)
                    .is_some_and(|t| matches!(&text[t.span.clone()], "..." | "…"))
                {
                    black = true;
                    i += 1;
                }
                let mut target = n.saturating_sub(1).saturating_mul(2) + u32::from(black);
                if active && !fragment.nodes.is_empty() && !black && ply(&context.pos) == target + 1
                {
                    target += 1;
                }
                if n == 0 || !context.seek(target) {
                    let reason = if context.unrecognized_image {
                        format!("Unrecognized image: no verified starting position at move {raw}")
                    } else {
                        format!("No known position at move {raw}")
                    };
                    fail(fragment, reason);
                    blocked = true;
                } else if !blocked {
                    active = true;
                    if fragment.nodes.is_empty() {
                        fragment.start_fen = fen(&context.pos);
                    }
                }
            }
            Kind::Open => {
                stack.push((context.clone(), before.clone(), active, blocked));
                *context = before.clone();
                active = true;
                blocked = false;
            }
            Kind::Close => {
                if let Some((saved, previous, a, b)) = stack.pop() {
                    *context = saved;
                    before = previous;
                    active = a;
                    blocked = b;
                }
            }
            Kind::Result => {
                active = false;
            }
            Kind::Comment => {
                if let Some(id) = context.parent {
                    book.nodes[id]
                        .comment
                        .push_str(raw.trim_matches(['{', '}']));
                }
            }
            Kind::Nag => {
                if let Some(id) = context.parent {
                    book.nodes[id].nags.push(raw.into());
                }
            }
            Kind::Word if active && !blocked => {
                if matches!(
                    raw.to_lowercase().as_str(),
                    "ch" | "dbl"
                        | "dis"
                        | "mate"
                        | "e.p."
                        | "+-"
                        | "-+"
                        | "="
                        | "+/-"
                        | "±"
                        | "∓"
                        | "⩲"
                        | "⩱"
                        | "..."
                        | "…"
                ) {
                    i += 1;
                    continue;
                }
                let mut found = None;
                // Longest legal descriptive phrase allows spaced typography without consuming prose.
                for count in (1..=8.min(ts.len() - i)).rev() {
                    let group = &ts[i..i + count];
                    if group.iter().enumerate().any(|(j, t)| {
                        t.kind != Kind::Word
                            && !(j > 0
                                && matches!(t.kind, Kind::Number(_))
                                && !text[t.span.clone()].contains('.'))
                    }) {
                        continue;
                    }
                    let span = t.span.start..group.last().expect("nonempty group").span.end;
                    if let Ok(m) = notation::parse(&context.pos, &text[span.clone()], alphabet) {
                        found = Some((m, span, count));
                        break;
                    }
                }
                if let Some((m, span, count)) = found {
                    before = context.clone();
                    context
                        .known
                        .insert(ply(&context.pos), (context.pos.clone(), context.parent));
                    let san = SanPlus::from_move(context.pos.clone(), m).to_string();
                    context.pos.play_unchecked(m);
                    let id = book.nodes.len();
                    book.nodes.push(Node {
                        id,
                        parent: context.parent,
                        san,
                        uci: m.to_uci(CastlingMode::Standard).to_string(),
                        fen: fen(&context.pos),
                        page: fragment.page,
                        span: offset + span.start..offset + span.end,
                        nags: vec![],
                        comment: String::new(),
                    });
                    context.parent = Some(id);
                    context
                        .known
                        .insert(ply(&context.pos), (context.pos.clone(), context.parent));
                    fragment.nodes.push(id);
                    i += count - 1;
                } else if looks_like_move(raw)
                    || [
                        "P", "N", "Kt", "B", "R", "Q", "K", "KKt", "QKt", "KN", "QN", "KR", "QR",
                    ]
                    .contains(&raw.trim_end_matches('.'))
                        && ts.get(i + 1).is_some_and(|next| {
                            matches!(
                                text[next.span.clone()].to_lowercase().as_str(),
                                "-" | "x" | "×" | "to" | "takes"
                            ) || raw.ends_with('.')
                        })
                {
                    fail(fragment, format!("Illegal or ambiguous move: {raw}"));
                    blocked = true;
                } else if let Some(id) = context.parent {
                    let comment = &mut book.nodes[id].comment;
                    if !comment.is_empty() {
                        comment.push(' ');
                    }
                    comment.push_str(raw);
                }
            }
            Kind::Word => {}
        }
        i += 1;
    }
}
fn looks_like_move(s: &str) -> bool {
    let piece = s.starts_with([
        'P', 'N', 'B', 'R', 'Q', 'K', 'D', 'T', 'C', 'A', 'L', 'S', 'F',
    ]);
    s.contains(|c: char| c.is_ascii_digit())
        || s.contains(['♔', '♕', '♖', '♗', '♘', '♙', '♚', '♛', '♜', '♝', '♞', '♟'])
        || (piece && s.contains(['-', 'x', '×']))
        || s.starts_with("Castles")
        || s.starts_with("O-O")
}

fn find_fen(text: &str) -> Option<Chess> {
    let words: Vec<_> = text.split_whitespace().collect();
    words
        .windows(6)
        .filter(|w| w[0].matches('/').count() == 7)
        .find_map(|w| position(&w.join(" ")))
}
/// Unicode text grids are accepted only with explicit side-to-move and a valid king setup.
/// Unidentified legacy font encodings are deliberately retained as text, not guessed.
pub fn diagram(text: &str) -> Option<Chess> {
    let lines: Vec<_> = text
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let chars: Vec<_> = line.chars().collect();
            if chars.len() == 10 && matches!(chars[0], '4' | '$') && matches!(chars[9], '5' | '%') {
                Some(chars[1..9].iter().collect::<String>())
            } else if chars.iter().filter(|c| !c.is_whitespace()).count() == 8 {
                Some(line.to_owned())
            } else {
                None
            }
        })
        .collect();
    if lines.len() != 8 {
        return None;
    }
    let mut rows = Vec::new();
    let legacy = lines
        .iter()
        .any(|l| l.contains(['o', 'O', 't', 'T', 'v', 'V', 'm', 'M', 'l', 'L', 'w', 'W']));
    for line in lines {
        let mut row = String::new();
        let mut empty = 0;
        for c in line.chars().filter(|c| !c.is_whitespace()) {
            let mut piece = "♔♕♖♗♘♙♚♛♜♝♞♟"
                .chars()
                .position(|p| p == c)
                .and_then(|i| "KQRBNPkqrbnp".chars().nth(i));
            if legacy {
                piece = "kqrbnplwtvmo"
                    .chars()
                    .position(|p| p == c.to_ascii_lowercase())
                    .and_then(|i| "KQRBNPkqrbnp".chars().nth(i));
            }
            if let Some(piece) = piece {
                if empty > 0 {
                    row.push(char::from_digit(empty, 10)?);
                    empty = 0;
                }
                row.push(piece);
            } else if matches!(c, '.' | '·' | '-' | '□' | '■' | '*' | '+' | ':') {
                empty += 1;
            } else {
                return None;
            }
        }
        if empty > 0 {
            row.push(char::from_digit(empty, 10)?);
        }
        rows.push(row);
    }
    let side = if text.contains("Black to move") {
        "b"
    } else if text.contains("White to move") {
        "w"
    } else {
        return None;
    };
    position(&format!("{} {side} - - 0 1", rows.join("/")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::books::document::Page;
    fn book(text: &str) -> Book {
        let mut page = Page::default();
        page.push(text, BlockKind::Paragraph);
        resolve(Document {
            title: String::new(),
            author: String::new(),
            format: "test".into(),
            pages: vec![page],
            assets: vec![],
        })
    }
    #[test]
    fn variation_and_illegal_safety() {
        let b = book("1. e4 (1. d4 d5 [1... Nf6]) e5 2. Nf3 Nc6 3. Bh9 a6");
        assert_eq!(
            b.nodes.iter().map(|n| n.uci.as_str()).collect::<Vec<_>>(),
            ["e2e4", "d2d4", "d7d5", "g8f6", "e7e5", "g1f3", "b8c6"]
        );
        assert_eq!(b.nodes[4].parent, Some(0));
        assert!(matches!(b.fragments[0].status, Status::Partial(_)));
        for node in &b.nodes {
            assert!(position(&node.fen).is_some());
        }
    }
    #[test]
    fn unknown_midgame_is_never_started_from_initial_board() {
        let b = book("12. e4 e5");
        assert!(b.nodes.is_empty());
        assert!(matches!(b.fragments[0].status, Status::Unresolved(_)));
    }
}

#[cfg(test)]
mod safety_tests {
    use super::*;
    use crate::books::document::Page;
    #[test]
    fn move_one_variation_returns_to_explicit_diagram() {
        for seed in [
            "4k3/8/8/8/8/8/4P3/4K3 w - - 0 1",
            "....♚...\n........\n........\n........\n........\n........\n....♙...\n....♔...\nWhite to move",
        ] {
            let mut page = Page::default();
            page.push(seed, BlockKind::Paragraph);
            page.push("1. e4", BlockKind::Paragraph);
            page.push("1. e3", BlockKind::Paragraph);
            let book = resolve(Document {
                title: String::new(),
                author: String::new(),
                format: "test".into(),
                pages: vec![page],
                assets: vec![],
            });
            assert_eq!(book.nodes.len(), 2);
            assert_eq!(book.nodes[1].parent, None);
            assert_eq!(book.fragments[1].start_fen, book.fragments[0].start_fen);
            assert_eq!(book.nodes[1].fen, "4k3/8/8/8/8/4P3/8/4K3 b - - 0 1");
        }
    }
    #[test]
    fn explicit_midgame_cannot_restart_from_standard_board() {
        let mut page = Page::default();
        page.push("4k3/8/8/8/8/8/4P3/4K3 w - - 0 12", BlockKind::Paragraph);
        page.push("1. e4", BlockKind::Paragraph);
        let book = resolve(Document {
            title: String::new(),
            author: String::new(),
            format: "test".into(),
            pages: vec![page],
            assets: vec![],
        });
        assert!(book.nodes.is_empty());
        assert!(matches!(book.fragments[0].status, Status::Unresolved(_)));
    }
    #[test]
    fn invalid_explicit_fen_cannot_fall_back_to_opening() {
        let mut page = Page::default();
        page.push("8/8/8/8/8/8/8/8 w - - 0 1", BlockKind::Paragraph);
        page.push("1. e4 e5", BlockKind::Paragraph);
        let book = resolve(Document {
            title: String::new(),
            author: String::new(),
            format: "epub".into(),
            pages: vec![page],
            assets: vec![],
        });
        assert!(book.nodes.is_empty());
        assert!(
            book.fragments
                .iter()
                .all(|f| matches!(f.status, Status::Unresolved(_)))
        );
    }
    #[test]
    fn unrecognized_diagram_cannot_seed_a_legal_but_wrong_opening() {
        let mut page = Page::default();
        page.push(
            "",
            BlockKind::Image {
                path: "diagram.jpg".into(),
                alt: String::new(),
            },
        );
        page.push("1 P - K 3, K - K 4", BlockKind::Paragraph);
        let book = resolve(Document {
            title: String::new(),
            author: String::new(),
            format: "epub".into(),
            pages: vec![page],
            assets: vec![],
        });
        assert!(book.nodes.is_empty());
        assert!(book.fragments.iter().any(|f| matches!(&f.status, Status::Unresolved(reason) if reason.contains("Unrecognized image"))));
    }
    #[test]
    fn ambiguous_spaced_move_cannot_skip_its_piece_name() {
        let mut page = Page::default();
        page.push("1. P - K4 P - K4 2. Kt - B3 Kt - QB3", BlockKind::Paragraph);
        let book = resolve(Document {
            title: String::new(),
            author: String::new(),
            format: "epub".into(),
            pages: vec![page],
            assets: vec![],
        });
        assert_eq!(book.nodes.len(), 2);
        assert!(matches!(book.fragments[0].status, Status::Partial(_)));
    }
    #[test]
    fn runs_preserve_all_utf8_text_and_link_each_move_once() {
        let mut page = Page::default();
        page.push(
            "A game: 1. ♘f3! d5 {central advance} 2. d4",
            BlockKind::Paragraph,
        );
        let book = resolve(Document {
            title: String::new(),
            author: String::new(),
            format: "epub".into(),
            pages: vec![page],
            assets: vec![],
        });
        let runs = book.text_runs(0).unwrap();
        assert_eq!(
            runs.iter().map(|r| r.text).collect::<String>(),
            book.document.pages[0].text
        );
        assert_eq!(runs.iter().filter(|r| r.node_id.is_some()).count(), 3);
        assert_eq!(book.nodes[0].nags, ["!"]);
        assert_eq!(book.nodes[1].comment, "central advance");
    }
}
