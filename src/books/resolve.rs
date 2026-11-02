//! Resolve only unique legal moves. A failed candidate ends its line; later numbered fragments
//! may recover from an explicitly known ply, but never from an invented position.
use super::{
    diagram::DiagramRecognizer,
    document::{Block, BlockKind, Document, Page},
    model::{Book, Fragment, Node, Section, Status},
    notation::{self, Kind, Token},
};
use shakmaty::{CastlingMode, Chess, Color, EnPassantMode, Position, fen::Fen, san::SanPlus};
use std::{
    collections::{BTreeMap, HashMap},
    num::NonZeroU32,
};

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
    /// A position read from an image has no move number until the text gives one.
    floating: bool,
}
impl Context {
    fn seek(&mut self, target: u32) -> bool {
        if self.invalid_start {
            return false;
        }
        if self.floating {
            if target % 2 != u32::from(self.pos.turn().is_black()) {
                return false;
            }
            let Some(pos) = renumber(&self.pos, target / 2 + 1) else {
                return false;
            };
            self.pos = pos;
            self.floating = false;
            self.known = BTreeMap::from([(target, (self.pos.clone(), None))]);
            return true;
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
fn renumber(pos: &Chess, fullmoves: u32) -> Option<Chess> {
    let mut setup = pos.to_setup(EnPassantMode::Legal);
    setup.fullmoves = NonZeroU32::new(fullmoves)?;
    setup.position(CastlingMode::Standard).ok()
}
pub fn resolve(document: Document) -> Book {
    resolve_using(
        document,
        super::diagram::builtin().map(|p| p as &dyn DiagramRecognizer),
    )
}
/// Resolves with an optional image recognizer; images it cannot read stay unrecognized.
pub fn resolve_using(mut document: Document, recognizer: Option<&dyn DiagramRecognizer>) -> Book {
    let assets = std::mem::take(&mut document.assets);
    let mut placements = HashMap::new();
    if let Some(recognizer) = recognizer {
        let mut paths: Vec<&String> = Vec::new();
        for block in document.pages.iter().flat_map(|p| &p.blocks) {
            if let BlockKind::Image { path, .. } = &block.kind
                && !paths.contains(&path)
            {
                paths.push(path);
            }
        }
        // Images are independent; spread them over a few threads (one decoded image each).
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(8));
        let chunk = paths.len().div_ceil(threads).max(1);
        let read = |path: &String| {
            assets
                .iter()
                .find(|a| &a.path == path)
                .and_then(|a| recognizer.placement(&a.bytes))
        };
        let results: Vec<Option<String>> = std::thread::scope(|scope| {
            let handles: Vec<_> = paths
                .chunks(chunk)
                .map(|part| {
                    let handle =
                        scope.spawn(move || part.iter().map(|p| read(p)).collect::<Vec<_>>());
                    (part.len(), handle)
                })
                .collect();
            // A failed worker abstains for its own images and keeps the rest aligned.
            handles
                .into_iter()
                .flat_map(|(n, h)| h.join().unwrap_or_else(|_| vec![None; n]))
                .collect()
        });
        for (path, placement) in paths.into_iter().zip(results) {
            placements.insert(path.clone(), placement);
        }
    }
    // Score whole books, rather than interpreting the same ambiguous letter differently per move.
    let mut best = None;
    for (name, alphabet) in notation::ALPHABETS {
        let candidate = resolve_with(document.clone(), name, alphabet, &placements);
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
#[allow(clippy::too_many_lines)] // One pass keeps block order, sections and seeding together.
fn resolve_with(
    mut document: Document,
    language: &str,
    alphabet: &str,
    placements: &HashMap<String, Option<String>>,
) -> Book {
    let recognized = recognized_positions(&document, alphabet, placements);
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
        for (block_index, block) in page.blocks.iter_mut().enumerate() {
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
            if let BlockKind::Image { path, alt } = &block.kind
                && let Some(pos) = recognized.get(&(page_index, block_index))
            {
                block.kind = BlockKind::RecognizedDiagram {
                    path: path.clone(),
                    alt: alt.clone(),
                    fen: fen(pos),
                };
                context = Context {
                    pos: pos.clone(),
                    explicit_start: true,
                    floating: true,
                    ..Context::default()
                };
                continue;
            }
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
                let floating = context.floating;
                if floating && blocked {
                    // An earlier number already failed: a later one must not pin the image's ply.
                } else if n == 0 || !context.seek(target) {
                    let reason = if context.unrecognized_image {
                        format!("Unrecognized image: no verified starting position at move {raw}")
                    } else {
                        format!("No known position at move {raw}")
                    };
                    fail(fragment, reason);
                    blocked = true;
                } else if !blocked {
                    if floating {
                        // Variations return to the numbered position, not the unnumbered image.
                        before = context.clone();
                    }
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
            // An image position gets its move number from the text before any move is read.
            Kind::Word if active && !blocked && !context.floating => {
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
                if let Some((m, span, count)) = parse_move(&context.pos, ts, i, text, alphabet) {
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
/// Longest legal descriptive phrase allows spaced typography without consuming prose.
fn parse_move(
    pos: &Chess,
    ts: &[Token],
    i: usize,
    text: &str,
    alphabet: &str,
) -> Option<(shakmaty::Move, std::ops::Range<usize>, usize)> {
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
        let span = ts[i].span.start..group.last().expect("nonempty group").span.end;
        if let Ok(m) = notation::parse(pos, &text[span.clone()], alphabet) {
            return Some((m, span, count));
        }
    }
    None
}
fn stated_turn(text: &str) -> Option<Color> {
    let lower = text.to_lowercase();
    let says = |side: &str| {
        [" to play", " to move", "'s move", "\u{2019}s move"]
            .iter()
            .any(|verb| lower.contains(&format!("{side}{verb}")))
    };
    match (says("white"), says("black")) {
        (true, false) => Some(Color::White),
        (false, true) => Some(Color::Black),
        _ => None,
    }
}
/// Diagram images the recognizer read, keyed by page and block. The side to move comes from the
/// text ("Black to play", "1. ..."); when moves follow, the first one must be legal for that side,
/// otherwise the image stays unrecognized rather than seeding a doubtful position.
fn recognized_positions(
    document: &Document,
    alphabet: &str,
    placements: &HashMap<String, Option<String>>,
) -> HashMap<(usize, usize), Chess> {
    let mut out = HashMap::new();
    for (p, page) in document.pages.iter().enumerate() {
        for (b, block) in page.blocks.iter().enumerate() {
            if let BlockKind::Image { path, alt } = &block.kind
                && let Some(Some(placement)) = placements.get(path)
                && let Some(pos) = image_position(
                    page.blocks[b + 1..]
                        .iter()
                        .map(|block| (page, block))
                        // Pagination can put the commentary on the next page.
                        .chain(document.pages.get(p + 1).into_iter().flat_map(|next| {
                            next.blocks.iter().map(move |block| (next, block))
                        }))
                        .take(4),
                    alt,
                    alphabet,
                    placement,
                )
            {
                out.insert((p, b), pos);
            }
        }
    }
    out
}
fn image_position<'a>(
    following: impl Iterator<Item = (&'a Page, &'a Block)>,
    alt: &str,
    alphabet: &str,
    placement: &str,
) -> Option<Chess> {
    let mut stated = stated_turn(alt);
    for (page, block) in following {
        if !matches!(block.kind, BlockKind::Paragraph | BlockKind::Table) {
            break;
        }
        let text = &page.text[block.span.clone()];
        if find_fen(text).is_some() || diagram(text).is_some() {
            break;
        }
        let ts = notation::tokens(text);
        // The first move number followed by a move that is legal for the side it names. Other
        // numbers (square ranks, example numbers, page references) are skipped.
        let mut found = None;
        let mut depth = 0usize;
        for (n, t) in ts.iter().enumerate() {
            match t.kind {
                Kind::Open => depth += 1,
                Kind::Close => depth = depth.saturating_sub(1),
                _ => {}
            }
            // Numbers inside parentheses belong to variations or asides, not the diagram.
            if depth > 0 || !matches!(t.kind, Kind::Number(_)) {
                continue;
            }
            let mut i = n;
            let mut side = Color::White;
            if text[t.span.clone()].ends_with("...") {
                side = Color::Black;
            }
            while ts
                .get(i + 1)
                .is_some_and(|t| matches!(&text[t.span.clone()], "..." | "…"))
            {
                side = Color::Black;
                i += 1;
            }
            if ts.get(i + 1).is_none_or(|t| t.kind != Kind::Word) {
                continue;
            }
            let legal = |side| {
                placed(placement, side)
                    .filter(|pos| parse_move(pos, &ts, i + 1, text, alphabet).is_some())
            };
            if let Some(pos) = legal(side) {
                found = Some((n, side, pos));
                break;
            }
            if legal(side.other()).is_some() {
                // The numbering names one side but only the other can play the move.
                return None;
            }
        }
        let prose = found
            .as_ref()
            .map_or(text, |(n, ..)| &text[..ts[*n].span.start]);
        if let Some(side) = stated_turn(prose) {
            if stated.is_some_and(|s| s != side) {
                return None;
            }
            stated = Some(side);
        }
        let Some((_, side, pos)) = found else {
            continue;
        };
        if stated.is_some_and(|s| s != side) {
            return None;
        }
        return Some(pos);
    }
    // Without a stated side or a move number the side to move is unknown: abstain.
    placed(placement, stated?)
}
fn placed(placement: &str, side: Color) -> Option<Chess> {
    position(&format!("{placement} {} - - 0 1", side.char()))
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
            cover: None,
            isbn: None,
            title: String::new(),
            author: String::new(),
            format: "test".into(),
            pages: vec![page],
            assets: vec![],
        })
    }
    #[test]
    fn a_diagram_without_moves_counts_as_a_position() {
        let summary = book("4k3/8/8/8/8/8/3P4/4K3 w - - 0 1").summary();
        assert_eq!((summary.positions, summary.unresolved), (1, 0));
        let summary = book("1. e4 e5 2. Nf3").summary();
        assert_eq!((summary.positions, summary.unresolved), (3, 0));
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
mod image_tests {
    use super::*;
    use crate::books::document::{Asset, Page};
    /// Reads every image as the same placement, standing in for the trained recognizer.
    struct Fixed(&'static str);
    impl DiagramRecognizer for Fixed {
        fn placement(&self, _: &[u8]) -> Option<String> {
            Some(self.0.into())
        }
    }
    // White: Ke1, Ra1, Pe2; Black: Ke8, Pe7. Pawns are still on their first squares.
    const PLACEMENT: &str = "4k3/4p3/8/8/8/8/4P3/R3K3";
    fn book(alt: &str, texts: &[&str]) -> Book {
        let mut page = Page::default();
        page.push(
            "",
            BlockKind::Image {
                path: "images/d.png".into(),
                alt: alt.into(),
            },
        );
        for text in texts {
            page.push(text, BlockKind::Paragraph);
        }
        let document = Document {
            cover: None,
            isbn: None,
            title: String::new(),
            author: String::new(),
            format: "test".into(),
            pages: vec![page],
            assets: vec![Asset {
                path: "images/d.png".into(),
                bytes: vec![0],
            }],
        };
        resolve_using(document, Some(&Fixed(PLACEMENT)))
    }
    fn recognized(b: &Book) -> Option<&str> {
        b.document.pages[0]
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                BlockKind::RecognizedDiagram { fen, .. } => Some(fen.as_str()),
                _ => None,
            })
    }
    #[test]
    fn recognized_image_seeds_following_moves() {
        let b = book("Diagram", &["1. e4 e5 2. Ra8+"]);
        assert_eq!(recognized(&b), Some("4k3/4p3/8/8/8/8/4P3/R3K3 w - - 0 1"));
        assert_eq!(
            b.nodes.iter().map(|n| n.uci.as_str()).collect::<Vec<_>>(),
            ["e2e4", "e7e5", "a1a8"]
        );
        assert!(matches!(b.fragments[0].status, Status::Resolved));
    }
    #[test]
    fn black_move_number_renumbers_the_position() {
        let b = book("", &["Black to play.", "23... e5 24. Ra8+"]);
        assert_eq!(recognized(&b), Some("4k3/4p3/8/8/8/8/4P3/R3K3 b - - 0 1"));
        assert_eq!(
            b.fragments[0].start_fen,
            "4k3/4p3/8/8/8/8/4P3/R3K3 b - - 0 23"
        );
        assert_eq!(b.nodes.len(), 2);
        assert!(b.nodes[1].fen.ends_with(" 24"));
    }
    #[test]
    fn variations_keep_the_image_move_number() {
        let b = book("", &["7. e4 (7. Ra8+) e5 (Ra8+)"]);
        let fens: Vec<_> = b.nodes.iter().map(|n| n.fen.as_str()).collect();
        assert_eq!(
            fens,
            [
                "4k3/4p3/8/8/4P3/8/8/R3K3 b - - 0 7",
                "R3k3/4p3/8/8/8/8/4P3/4K3 b - - 1 7",
                "4k3/8/8/4p3/4P3/8/8/R3K3 w - - 0 8",
            ]
        );
        assert_eq!(
            b.fragments[0].start_fen,
            "4k3/4p3/8/8/8/8/4P3/R3K3 w - - 0 7"
        );
    }
    #[test]
    fn unnumbered_moves_do_not_start_an_image_position() {
        let b = book("White to play", &["(Ra8+) then 1. e4"]);
        assert_eq!(b.nodes.len(), 1);
        assert!(b.nodes[0].fen.ends_with(" 1"));
    }
    #[test]
    fn unknown_side_to_move_abstains() {
        let b = book("Diagram", &["A quiet position."]);
        assert_eq!(recognized(&b), None);
    }
    #[test]
    fn commentary_on_the_next_page_gives_the_side() {
        let mut first = Page::default();
        first.push(
            "",
            BlockKind::Image {
                path: "images/d.png".into(),
                alt: String::new(),
            },
        );
        let mut second = Page::default();
        second.push("23... e5 24. Ra8+", BlockKind::Paragraph);
        let b = resolve_using(
            Document {
                cover: None,
                isbn: None,
                title: String::new(),
                author: String::new(),
                format: "test".into(),
                pages: vec![first, second],
                assets: vec![Asset {
                    path: "images/d.png".into(),
                    bytes: vec![0],
                }],
            },
            Some(&Fixed(PLACEMENT)),
        );
        assert_eq!(recognized(&b), Some("4k3/4p3/8/8/8/8/4P3/R3K3 b - - 0 1"));
        assert_eq!(b.nodes.len(), 2);
    }
    #[test]
    fn conflicting_side_to_move_abstains() {
        let b = book("White to play", &["Black to play.", "1... e5"]);
        assert_eq!(recognized(&b), None);
        assert!(b.nodes.is_empty());
    }
    #[test]
    fn illegal_first_move_abstains() {
        // No piece can reach d4: the image is kept rather than trusted.
        let b = book("", &["1. Nd4 e5"]);
        assert_eq!(recognized(&b), None);
        assert!(b.nodes.is_empty());
    }
    #[test]
    fn numbers_that_are_not_move_numbers_are_skipped() {
        // "Example 8" and the rank in "Ra 5" are not move numbers; "1 ... e5" names Black.
        let b = book(
            "",
            &["Example 8. It is Black's move after Ra 5, and 1 ... e5 follows."],
        );
        assert_eq!(recognized(&b), Some("4k3/4p3/8/8/8/8/4P3/R3K3 b - - 0 1"));
    }
    #[test]
    fn move_legal_only_for_the_other_side_abstains() {
        // "1." names White, but only Black can play e5.
        let b = book("", &["1. e5"]);
        assert_eq!(recognized(&b), None);
    }
    #[test]
    fn numbers_in_parentheses_do_not_set_the_side() {
        let b = book("", &["(Compare 11... e5 in another game.) 1. e4 e5"]);
        assert_eq!(recognized(&b), Some("4k3/4p3/8/8/8/8/4P3/R3K3 w - - 0 1"));
    }
    #[test]
    fn a_failed_number_keeps_the_image_unnumbered() {
        // "2. ... Nd4" fails on the White-to-move image; "3. e4" in the same failed fragment
        // must not pin the image to move 3, so the next paragraph can still number it.
        let b = book("White to move", &["2. ... Nd4 3. e4", "4. Ra8+"]);
        assert_eq!(
            b.nodes.iter().map(|n| n.uci.as_str()).collect::<Vec<_>>(),
            ["a1a8"]
        );
    }
    #[test]
    fn image_without_moves_still_shows_the_position() {
        let b = book("White to move", &["A quiet position."]);
        assert_eq!(recognized(&b), Some("4k3/4p3/8/8/8/8/4P3/R3K3 w - - 0 1"));
        assert!(b.nodes.is_empty());
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
                cover: None,
                isbn: None,
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
            cover: None,
            isbn: None,
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
            cover: None,
            isbn: None,
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
            cover: None,
            isbn: None,
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
            cover: None,
            isbn: None,
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
            cover: None,
            isbn: None,
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
