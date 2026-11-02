//! Resolve only unique legal moves. A failed candidate ends its line; later numbered fragments
//! may recover from an explicitly known ply, but never from an invented position.
use super::{
    diagram::DiagramRecognizer,
    document::{Block, BlockKind, Document, Page},
    model::{Book, Fragment, Node, Section, Status},
    notation::LabelKind,
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
    /// Every position the current game reached at each ply, with the node that reached it.
    /// Analysis can reach a ply in several ways; a fragment resuming there must fit one only.
    known: BTreeMap<u32, Vec<(Chess, Option<usize>)>>,
    invalid_start: bool,
    explicit_start: bool,
    unrecognized_image: bool,
    /// After a board that resembles the current position: only its next move may follow.
    resume: bool,
    /// A confirmed image may be followed by a caption without starting a new game.
    confirmed_image: bool,
    /// A position read from an image has no move number until the text gives one.
    floating: bool,
    /// The last block ended in PGN tag pairs, so tags that follow belong to the same header.
    header: bool,
}
/// Why a move number could not be placed.
enum Miss {
    Unknown,
    Ambiguous(usize),
}
/// Plies of lookahead used to tell apart lines that reach the same move number.
const LOOKAHEAD: usize = 8;
impl Context {
    fn known(pos: Chess) -> BTreeMap<u32, Vec<(Chess, Option<usize>)>> {
        BTreeMap::from([(ply(&pos), vec![(pos, None)])])
    }
    /// Records the current position; a transposition keeps the latest node that reached it.
    fn remember(&mut self) {
        let entries = self.known.entry(ply(&self.pos)).or_default();
        let key = fen(&self.pos);
        entries.retain(|(pos, _)| fen(pos) != key);
        entries.push((self.pos.clone(), self.parent));
    }
    /// Moves to the position before `target`. Inside a line the current position wins; a
    /// fragment that starts afresh must fit exactly one known position: when several do,
    /// the one whose following moves stay legal longest is taken, and a tie abstains.
    fn seek(
        &mut self,
        target: u32,
        fresh: bool,
        fit: &dyn Fn(&Chess) -> usize,
    ) -> Result<(), Miss> {
        if self.invalid_start {
            return Err(Miss::Unknown);
        }
        if self.resume {
            if target != ply(&self.pos) {
                return Err(Miss::Unknown);
            }
            self.resume = false;
            self.unrecognized_image = false;
        }
        if self.floating {
            let pos = (target % 2 == u32::from(self.pos.turn().is_black()))
                .then(|| renumber(&self.pos, target / 2 + 1))
                .flatten();
            // "Reached after (1) P-K4, P-Q4": a game from move one leading up to the diagram
            // starts from the opening position when only that one keeps more moves legal, even
            // when the diagram has Black to move.
            if target == 0 && fit(&Chess::default()) > pos.as_ref().map_or(0, fit) {
                *self = Self::default();
                return Ok(());
            }
            let Some(pos) = pos else {
                return Err(Miss::Unknown);
            };
            self.pos = pos;
            self.floating = false;
            self.known = Self::known(self.pos.clone());
            return Ok(());
        }
        if target == ply(&self.pos) && !fresh {
            return Ok(());
        }
        let mut candidates = self.known.get(&target).cloned().unwrap_or_default();
        if target == ply(&self.pos) {
            let key = fen(&self.pos);
            candidates.retain(|(pos, _)| fen(pos) != key);
            candidates.push((self.pos.clone(), self.parent));
        }
        let chosen = match candidates.len() {
            0 if target == 0 && !self.explicit_start => {
                *self = Self::default();
                return Ok(());
            }
            // A diagram with Black to make move one has no ply-0 position of its own; a game
            // from move one leading up to it plays from the opening if that fits better.
            0 if target == 0
                && fresh
                && ply(&self.pos) == 1
                && fit(&Chess::default()) > fit(&self.pos) =>
            {
                *self = Self::default();
                return Ok(());
            }
            0 => return Err(Miss::Unknown),
            1 => candidates.pop(),
            n => {
                let scores: Vec<usize> = candidates.iter().map(|(pos, _)| fit(pos)).collect();
                let best = scores.iter().copied().max().unwrap_or(0);
                if best == 0 || scores.iter().filter(|s| **s == best).count() > 1 {
                    return Err(Miss::Ambiguous(n));
                }
                scores
                    .iter()
                    .position(|s| *s == best)
                    .map(|i| candidates.swap_remove(i))
            }
        };
        let (pos, parent) = chosen.expect("a candidate");
        // A game from move one after a diagram ("after 1. e2-e4 e7-e5 2. d2-d4") is played from
        // the opening position when only that one keeps more of the moves legal.
        if target == 0 && fresh && fit(&Chess::default()) > fit(&pos) {
            *self = Self::default();
            return Ok(());
        }
        self.pos = pos;
        self.parent = parent;
        Ok(())
    }
}
/// What a labelled number starts: "Game 3", "Example 8", or a Roman numeral heading a line
/// that begins at move one ("II. 1. e4").
fn label(ts: &[Token], i: usize, text: &str) -> Option<LabelKind> {
    match ts[i].kind {
        Kind::Label => notation::label_kind(&text[ts[i.checked_sub(1)?].span.clone()]),
        Kind::Word
            if ts.get(i + 1).is_some_and(|t| t.kind == Kind::Number(1))
                && text[ts[i].span.clone()]
                    .strip_suffix('.')
                    .is_some_and(|r| !r.is_empty() && r.chars().all(|c| "IVXL".contains(c))) =>
        {
            Some(LabelKind::Game)
        }
        _ => None,
    }
}
/// How many moves after token `i` stay legal from `pos` (move numbers and remarks skipped).
fn legal_run(pos: &Chess, ts: &[Token], i: usize, text: &str, alphabet: &str) -> usize {
    walk(pos, ts, i, text, alphabet, None).0
}
/// The moves `legal_run` would play from `i`: each move's token index and the position before
/// it, the token index where the run stopped and the position there.
fn trace(
    pos: &Chess,
    ts: &[Token],
    i: usize,
    text: &str,
    alphabet: &str,
) -> (Vec<(usize, Chess)>, usize, Chess) {
    let mut steps = Vec::new();
    let (_, stop, at) = walk(pos, ts, i, text, alphabet, Some(&mut steps));
    (steps, stop, at)
}
fn walk(
    pos: &Chess,
    ts: &[Token],
    mut i: usize,
    text: &str,
    alphabet: &str,
    mut steps: Option<&mut Vec<(usize, Chess)>>,
) -> (usize, usize, Chess) {
    let mut pos = pos.clone();
    let mut run = 0;
    while i < ts.len() && run < LOOKAHEAD {
        match ts[i].kind {
            Kind::Number(_) | Kind::Nag | Kind::Comment => i += 1,
            Kind::Word if noise(&text[ts[i].span.clone()]) => i += 1,
            Kind::Word => {
                let Some((m, _, count)) = parse_move(&pos, ts, i, text, alphabet) else {
                    break;
                };
                if let Some(steps) = steps.as_deref_mut() {
                    steps.push((i, pos.clone()));
                }
                pos.play_unchecked(m);
                run += 1;
                i += count;
            }
            _ => break,
        }
    }
    (run, i, pos)
}
/// Check marks and evaluation signs that sit between moves.
fn noise(raw: &str) -> bool {
    matches!(
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
    )
}
fn renumber(pos: &Chess, fullmoves: u32) -> Option<Chess> {
    let mut setup = pos.to_setup(EnPassantMode::Legal);
    setup.fullmoves = NonZeroU32::new(fullmoves)?;
    setup.position(CastlingMode::Standard).ok()
}
pub fn resolve(document: Document) -> Book {
    let network = super::cnn::builtin().map(|n| n as &dyn DiagramRecognizer);
    let prototypes = super::diagram::builtin().map(|p| p as &dyn DiagramRecognizer);
    match (network, prototypes) {
        (Some(first), Some(second)) => {
            resolve_using(document, Some(&super::diagram::Agreeing { first, second }))
        }
        (network, prototypes) => resolve_using(document, network.or(prototypes)),
    }
}
/// Expand a gated placement for comparison when its following moves cannot seed a position.
fn reading_of_placement(placement: &str) -> Option<String> {
    position(&format!("{placement} w - - 0 1"))
        .or_else(|| position(&format!("{placement} b - - 0 1")))?;
    Some(
        placement
            .chars()
            .filter(|c| *c != '/')
            .flat_map(|c| {
                c.to_digit(10)
                    .map_or_else(|| vec![c], |n| vec!['.'; n as usize])
            })
            .collect(),
    )
}
/// Resolves with an optional image recognizer; images it cannot read stay unrecognized.
pub fn resolve_using(mut document: Document, recognizer: Option<&dyn DiagramRecognizer>) -> Book {
    let assets = std::mem::take(&mut document.assets);
    let mut placements = HashMap::new();
    let mut readings = HashMap::new();
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
        // An image the gates reject still gets an ungated reading for confirmation.
        let read = |path: &String| {
            assets
                .iter()
                .find(|a| &a.path == path)
                .map_or((None, None), |a| match recognizer.placement(&a.bytes) {
                    Some(p) => {
                        let reading = reading_of_placement(&p);
                        (Some(p), reading)
                    }
                    None => (None, recognizer.reading(&a.bytes)),
                })
        };
        let results: Vec<(Option<String>, Option<String>)> = std::thread::scope(|scope| {
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
                .flat_map(|(n, h)| h.join().unwrap_or_else(|_| vec![(None, None); n]))
                .collect()
        });
        for (path, (placement, reading)) in paths.into_iter().zip(results) {
            if let Some(reading) = reading {
                readings.insert(path.clone(), reading);
            }
            placements.insert(path.clone(), placement);
        }
    }
    // Score whole books, rather than interpreting the same ambiguous letter differently per move.
    let mut best = None;
    for (name, alphabet) in notation::ALPHABETS {
        let candidate = resolve_with(document.clone(), name, alphabet, &placements, &readings);
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
    readings: &HashMap<String, String>,
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
                // "DIAGRAM 1" under a board names it; the position stays in force.
                if !((context.floating || context.resume || context.confirmed_image)
                    && caption(&page.text[block.span.clone()]))
                {
                    context = Context::default();
                }
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
            if block.kind == BlockKind::Paragraph
                && let Some(pos) = recognized.get(&(page_index, block_index))
            {
                block.kind = BlockKind::Diagram { fen: fen(pos) };
                context = Context {
                    pos: pos.clone(),
                    explicit_start: true,
                    floating: true,
                    ..Context::default()
                };
                continue;
            }
            // A text board that cannot be placed still starts a new position; its rank digits
            // are not move numbers.
            if block.kind == BlockKind::Paragraph
                && grid(&page.text[block.span.clone()], alphabet).is_some()
            {
                context = Context {
                    invalid_start: true,
                    ..Context::default()
                };
                continue;
            }
            if let BlockKind::Image { path, alt } = &block.kind
                && !context.invalid_start
                && readings
                    .get(path)
                    .and_then(|r| differences(r, &context.pos))
                    .is_some_and(|d| d <= CONFIRM_MAX_DIFFERENCES)
            {
                block.kind = BlockKind::RecognizedDiagram {
                    path: path.clone(),
                    alt: alt.clone(),
                    fen: fen(&context.pos),
                };
                context.confirmed_image = true;
                continue;
            }
            // A board close to the replayed position but not read exactly: the game may go on
            // from its next move only, never from an earlier branch point.
            if let BlockKind::Image { path, .. } = &block.kind
                && !context.invalid_start
                && !context.floating
                && readings
                    .get(path)
                    .and_then(|r| differences(r, &context.pos))
                    .is_some_and(|d| d <= RESUME_MAX_DIFFERENCES)
            {
                context.known = BTreeMap::from([(
                    ply(&context.pos),
                    vec![(context.pos.clone(), context.parent)],
                )]);
                context.resume = true;
                context.unrecognized_image = true;
                continue;
            }
            if matches!(block.kind, BlockKind::Image { .. }) {
                context.invalid_start = true;
                context.unrecognized_image = true;
                context.known.clear();
                continue;
            }
            let text = &page.text[block.span.clone()];
            // A FEN inside a PGN header seeds the game that follows it instead.
            if let Some(pos) = find_fen(text)
                .filter(|_| !text.contains("[FEN "))
                .or_else(|| diagram(text))
            {
                block.kind = BlockKind::Diagram { fen: fen(&pos) };
                context = Context {
                    known: Context::known(pos.clone()),
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
                && !text.contains("[FEN ")
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
            // Tag pairs on their own, one header line per paragraph, still set up the game.
            if !ts
                .iter()
                .any(|t| matches!(t.kind, Kind::Number(_) | Kind::Label | Kind::Tag))
            {
                context.header = false;
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
            context.header = ts.last().is_some_and(|t| t.kind == Kind::Tag);
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
        if let Some(kind) = label(ts, i, text)
            && stack.is_empty()
            // A caption under a diagram names the position just shown: keep it.
            && !(context.floating || (context.explicit_start && context.parent.is_none()))
        {
            *context = match kind {
                // A new game starts from the initial position; nothing earlier carries over.
                LabelKind::Game => Context::default(),
                // A new example has its own position, which the text has not given yet.
                LabelKind::Example => Context {
                    invalid_start: true,
                    ..Context::default()
                },
                LabelKind::Reference => context.clone(),
            };
            if kind != LabelKind::Reference {
                before = context.clone();
                active = false;
                blocked = false;
            }
        }
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
                let fresh = fragment.nodes.is_empty() && !active;
                let fit = |pos: &Chess| legal_run(pos, ts, i + 1, text, alphabet);
                let sought = if n == 0 {
                    Err(Miss::Unknown)
                } else if floating && blocked {
                    Ok(())
                } else {
                    context.seek(target, fresh, &fit)
                };
                if floating && blocked {
                    // An earlier number already failed: a later one must not pin the image's ply.
                } else if let Err(miss) = sought {
                    let reason = match miss {
                        Miss::Ambiguous(lines) => format!(
                            "Ambiguous continuation: {lines} earlier lines reach move {raw}"
                        ),
                        Miss::Unknown if context.unrecognized_image => {
                            format!(
                                "Unrecognized image: no verified starting position at move {raw}"
                            )
                        }
                        Miss::Unknown => format!("No known position at move {raw}"),
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
            // A PGN header starts a new game; its FEN tag, if any, gives the position.
            Kind::Tag if stack.is_empty() => {
                let continued = if i == 0 {
                    context.header
                } else {
                    ts[i - 1].kind == Kind::Tag
                };
                if !continued {
                    *context = Context::default();
                }
                if let Some(("FEN", value)) = notation::tag(raw) {
                    *context = match position(value) {
                        Some(pos) => Context {
                            known: Context::known(pos.clone()),
                            pos,
                            explicit_start: true,
                            ..Context::default()
                        },
                        None => Context {
                            invalid_start: true,
                            ..Context::default()
                        },
                    };
                }
                context.header = true;
                before = context.clone();
                active = false;
                blocked = false;
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
                if noise(raw) {
                    i += 1;
                    continue;
                }
                let read = read_move(&context.pos, ts, i, text, alphabet).or_else(|ambiguous| {
                    ambiguous
                        .and_then(|group| choose_move(&context.pos, ts, i, text, alphabet, group))
                        .ok_or(())
                });
                if let Ok((m, span, count)) = read {
                    before = context.clone();
                    context.remember();
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
                    context.remember();
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
            // Labels were handled before the match; tag pairs inside a variation are skipped.
            Kind::Word | Kind::Label | Kind::Tag => {}
        }
        i += 1;
    }
}
/// Longest legal descriptive phrase allows spaced typography without consuming prose.
/// Tokens `i..i + count` can form one move: words, plus bare rank digits after the first.
fn move_group(ts: &[Token], i: usize, count: usize, text: &str) -> bool {
    ts[i..i + count].iter().enumerate().all(|(j, t)| {
        t.kind == Kind::Word
            || j > 0 && matches!(t.kind, Kind::Number(_)) && !text[t.span.clone()].contains('.')
    })
}
type Read = (shakmaty::Move, std::ops::Range<usize>, usize);
/// One reading of an ambiguous move and how far the moves after it stay legal.
struct Reading {
    steps: Vec<(usize, Chess)>,
    stop: usize,
    at: Chess,
    m: shakmaty::Move,
}
/// The longest group of tokens from `i` that reads as one legal move; on failure, the first
/// group that read as several moves (its span and token count), if any.
fn read_move(
    pos: &Chess,
    ts: &[Token],
    i: usize,
    text: &str,
    alphabet: &str,
) -> Result<Read, Option<(std::ops::Range<usize>, usize)>> {
    let mut ambiguous = None;
    for count in (1..=8.min(ts.len() - i)).rev() {
        if !move_group(ts, i, count, text) {
            continue;
        }
        let span = ts[i].span.start..ts[i + count - 1].span.end;
        let raw = &text[span.clone()];
        // A move may close a sentence: "(2) P-g4."
        let trimmed = raw
            .strip_suffix('.')
            .filter(|r| !r.ends_with('.'))
            .map(|r| (r, span.start..span.end - 1));
        for (raw, span) in [(raw, span.clone())].into_iter().chain(trimmed) {
            match notation::parse(pos, raw, alphabet) {
                Ok(m) => return Ok((m, span, count)),
                Err(notation::MatchError::Ambiguous) if ambiguous.is_none() => {
                    ambiguous = Some((span, count));
                }
                Err(_) => {}
            }
        }
    }
    Err(ambiguous)
}
fn parse_move(pos: &Chess, ts: &[Token], i: usize, text: &str, alphabet: &str) -> Option<Read> {
    read_move(pos, ts, i, text, alphabet).ok()
}
/// A descriptive move that more than one piece could make ("B-Kt2" when both bishops reach a
/// Knight's second) is taken only when a single reading keeps the following moves legal for
/// at least two more plies than any other, and every other reading breaks on a move the best
/// one plays because that move is illegal there, not merely ambiguous.
fn choose_move(
    pos: &Chess,
    ts: &[Token],
    i: usize,
    text: &str,
    alphabet: &str,
    (span, count): (std::ops::Range<usize>, usize),
) -> Option<Read> {
    let mut runs: Vec<Reading> = notation::candidates(pos, &text[span.clone()])
        .into_iter()
        .map(|m| {
            let mut after = pos.clone();
            after.play_unchecked(m);
            let (steps, stop, at) = trace(&after, ts, i + count, text, alphabet);
            Reading { steps, stop, at, m }
        })
        .collect();
    if runs.len() < 2 {
        return None;
    }
    runs.sort_by_key(|run| std::cmp::Reverse(run.steps.len()));
    let (best, others) = runs.split_first().expect("two readings");
    let refuted = others.iter().all(|other| {
        best.steps.len() >= other.steps.len() + 2
            && best
                .steps
                .iter()
                .find(|(k, _)| *k == other.stop)
                .is_some_and(|(k, before)| {
                    parse_move(before, ts, *k, text, alphabet).is_some_and(|(_, span, _)| {
                        notation::parse(&other.at, &text[span], alphabet)
                            == Err(notation::MatchError::Illegal)
                    })
                })
    });
    refuted.then_some((best.m, span, count))
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
/// Every move changes at least two squares, so an ungated reading within this many squares of
/// the replayed position shows that position and not one a move before or after it.
const CONFIRM_MAX_DIFFERENCES: usize = 1;
/// An ungated reading this close to the replayed position is a board of the same game, which
/// the text may resume at exactly the next move.
const RESUME_MAX_DIFFERENCES: usize = 12;

/// Squares where an ungated image reading differs from a position, if it is a full reading.
fn differences(reading: &str, pos: &Chess) -> Option<usize> {
    let board = pos.board();
    let squares: Vec<char> = (0..64)
        .map(|i| {
            let square = shakmaty::Square::from_coords(
                shakmaty::File::new(i % 8),
                shakmaty::Rank::new(7 - i / 8),
            );
            board.piece_at(square).map_or('.', shakmaty::Piece::char)
        })
        .collect();
    (reading.chars().count() == 64)
        .then(|| reading.chars().zip(squares).filter(|(a, b)| a != b).count())
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
            let (placement, alt) = match &block.kind {
                BlockKind::Image { path, alt } => match placements.get(path) {
                    Some(Some(placement)) => (placement.clone(), alt.as_str()),
                    _ => continue,
                },
                BlockKind::Paragraph => match grid(&page.text[block.span.clone()], alphabet) {
                    Some(placement) => (placement, ""),
                    None => continue,
                },
                _ => continue,
            };
            if let Some(pos) = image_position(
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
                &placement,
            ) {
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
        let text = &page.text[block.span.clone()];
        if block.kind == BlockKind::Heading && caption(text) {
            continue;
        }
        if !matches!(block.kind, BlockKind::Paragraph | BlockKind::Table) {
            break;
        }
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
/// A heading that only names a diagram or figure ("DIAGRAM 1", "Fig. 12"). Chapter, section,
/// part and page headings start something new instead.
fn caption(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    words.len() <= 3
        && words.first().is_some_and(|w| {
            matches!(
                w.trim_end_matches(['.', ':']).to_lowercase().as_str(),
                "diagram"
                    | "diag"
                    | "dia"
                    | "fig"
                    | "figure"
                    | "diagramm"
                    | "diagramme"
                    | "diagrama"
                    | "diagramma"
                    | "diagramie"
            )
        })
        && words[1..].iter().all(|w| {
            w.trim_end_matches(['.', ':'])
                .chars()
                .all(|c| c.is_ascii_digit())
        })
}
/// A board printed as text rows, "8 | #R | #Kt| ... |" down to rank 1: '#' marks black men,
/// '^' white ones, letters follow the book's alphabet ("Kt" is a knight) and a bare mark is a
/// pawn. Returns the piece placement; the side to move comes from the text around it.
fn grid(text: &str, alphabet: &str) -> Option<String> {
    if !text.contains('|') {
        return None;
    }
    let parts: Vec<&str> = text.split('|').collect();
    let mut rows: [Option<String>; 8] = Default::default();
    let mut k = 0;
    while k + 8 < parts.len() {
        let rank = parts[k]
            .split_whitespace()
            .last()
            .and_then(|w| w.parse::<usize>().ok())
            .filter(|r| (1..=8).contains(r));
        let Some(rank) = rank else {
            k += 1;
            continue;
        };
        let mut row = String::new();
        let mut empty = 0u8;
        for cell in &parts[k + 1..k + 9] {
            match grid_cell(cell.trim(), alphabet)? {
                Cell::Man(piece) => {
                    if empty > 0 {
                        row.push(char::from(b'0' + empty));
                        empty = 0;
                    }
                    row.push(piece);
                }
                Cell::Empty => empty += 1,
            }
        }
        if empty > 0 {
            row.push(char::from(b'0' + empty));
        }
        let slot = &mut rows[8 - rank];
        if slot.is_some() {
            return None;
        }
        *slot = Some(row);
        k += 9;
    }
    let rows: Option<Vec<String>> = rows.into_iter().collect();
    Some(rows?.join("/"))
}
/// One square of a text board.
#[derive(Debug, PartialEq)]
enum Cell {
    Empty,
    Man(char),
}
/// Reads one grid cell; `None` when it is neither empty nor a chess man.
fn grid_cell(cell: &str, alphabet: &str) -> Option<Cell> {
    if cell.is_empty() {
        return Some(Cell::Empty);
    }
    let (white, name) = if let Some(name) = cell.strip_prefix('^') {
        (true, name)
    } else {
        (false, cell.strip_prefix('#')?)
    };
    let role = match name {
        "" => 'P',
        "Kt" => 'N',
        _ => {
            let mut chars = name.chars();
            let letter = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            match alphabet.chars().position(|c| c == letter) {
                Some(i) => "KQRBN".chars().nth(i)?,
                None if letter == 'P' => 'P',
                None => return None,
            }
        }
    };
    Some(Cell::Man(if white {
        role
    } else {
        role.to_ascii_lowercase()
    }))
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
    fn blocks(blocks: &[(&str, BlockKind)]) -> Book {
        let mut page = Page::default();
        for (text, kind) in blocks {
            page.push(text, kind.clone());
        }
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
    use std::fmt::Write as _;
    const SEP: &str = "|------------------------|";
    fn board(rows: [&str; 8]) -> String {
        let mut out = String::from("+------------------------+ ");
        for (i, row) in rows.iter().enumerate() {
            write!(out, "{} {row} {SEP} ", 8 - i).unwrap();
        }
        out
    }
    const EMPTY: &str = "|    |    |    |    |    |    |    |    |";
    fn grid_board() -> String {
        board([
            "|    |    |    |    | #K |    |    |    |",
            "|    |    |    |    | #  |    |    |    |",
            EMPTY,
            EMPTY,
            EMPTY,
            EMPTY,
            "|    |    |    |    | ^P |    |    |    |",
            "| ^R |    |    |    | ^K |    |    |    |",
        ])
    }
    #[test]
    fn a_text_grid_seeds_the_moves_after_its_caption() {
        let b = blocks(&[
            (&grid_board(), BlockKind::Paragraph),
            ("DIAGRAM 1", BlockKind::Heading),
            ("White to play wins. 1. Ra8+", BlockKind::Paragraph),
        ]);
        assert!(b.document.pages[0].blocks.iter().any(|block| matches!(
            &block.kind,
            BlockKind::Diagram { fen } if fen.starts_with("4k3/4p3/8/8/8/8/4P3/R3K3 w")
        )));
        assert_eq!(
            b.nodes.iter().map(|n| n.uci.as_str()).collect::<Vec<_>>(),
            ["a1a8"]
        );
    }
    #[test]
    fn a_grid_that_is_not_a_legal_board_is_not_shown() {
        let text = grid_board().replace("^K", "^Q");
        let b = blocks(&[
            (&text, BlockKind::Paragraph),
            ("White to play. 1. Ra8+", BlockKind::Paragraph),
        ]);
        assert!(
            !b.document.pages[0]
                .blocks
                .iter()
                .any(|block| matches!(block.kind, BlockKind::Diagram { .. }))
        );
        assert!(b.nodes.is_empty());
    }
    #[test]
    fn moves_leading_up_to_a_grid_start_from_the_opening() {
        let b = blocks(&[
            (&grid_board(), BlockKind::Paragraph),
            ("DIAGRAM 2", BlockKind::Heading),
            (
                "White to move. This is reached by (1) P-e4, P-e5; (2) B-c4.",
                BlockKind::Paragraph,
            ),
        ]);
        assert_eq!(
            b.nodes.iter().map(|n| n.uci.as_str()).collect::<Vec<_>>(),
            ["e2e4", "e7e5", "f1c4"]
        );
    }
    #[test]
    fn pgn_headers_start_a_new_game_and_are_not_moves() {
        let b = book(
            "[Event \"?\"] [PlyCount \"39\"] 1. e4 e5 2. Nf3 1-0 \
             [White \"B\"] [Black \"C\"] [SetUp \"1\"] \
             [FEN \"4k3/8/8/8/8/8/4P3/4K3 w - - 0 1\"] 1. e4 Kd7",
        );
        assert_eq!(
            b.nodes.iter().map(|n| n.uci.as_str()).collect::<Vec<_>>(),
            ["e2e4", "e7e5", "g1f3", "e2e4", "e8d7"]
        );
        assert_eq!(b.nodes[3].parent, None);
        assert!(b.fragments.iter().all(|f| f.status == Status::Resolved));
    }
    #[test]
    fn a_header_with_one_tag_per_paragraph_still_sets_up_the_game() {
        let p = BlockKind::Paragraph;
        let b = blocks(&[
            ("1. e4 e5 2. Nf3", p.clone()),
            ("[Event \"?\"]", p.clone()),
            ("[FEN \"4k3/8/8/8/8/8/4P3/4K3 w - - 0 1\"]", p.clone()),
            ("[PlyCount \"2\"]", p.clone()),
            ("1. e4 Kd7", p.clone()),
            ("[Event \"?\"]", p.clone()),
            ("A new game from the opening.", p.clone()),
            ("1. d4", p),
        ]);
        assert_eq!(
            b.nodes.iter().map(|n| n.uci.as_str()).collect::<Vec<_>>(),
            ["e2e4", "e7e5", "g1f3", "e2e4", "e8d7", "d2d4"]
        );
        assert_eq!(b.nodes[3].parent, None);
        assert_eq!(b.nodes[5].parent, None);
        assert!(b.fragments.iter().all(|f| f.status == Status::Resolved));
    }
    #[test]
    fn an_ambiguous_descriptive_move_is_read_from_the_moves_after_it() {
        let b = book("1. P-QKt3 P-K3 2. P-KKt3 P-Q4 3. B-Kt2 Kt-KB3 4. B-Q4 Kt-B3");
        assert_eq!(b.nodes[4].uci, "c1b2");
        assert_eq!(b.nodes.len(), 8);
        let b = book("1. P-QKt3 P-K3 2. P-KKt3 P-Q4 3. B-Kt2 Kt-KB3");
        assert_eq!(b.nodes.len(), 4);
        assert!(matches!(b.fragments[0].status, Status::Partial(_)));
    }
    #[test]
    fn a_move_may_end_its_sentence() {
        let b = book("(1) P-e4, P-e5; (2) B-c4. Then 1. e4 e5 2. Bc4.");
        assert_eq!(b.nodes.len(), 6);
        assert!(b.fragments.iter().all(|f| f.status == Status::Resolved));
    }
    #[test]
    fn grid_cells_follow_the_book_alphabet() {
        assert_eq!(grid_cell("#Kt", "KQRBN"), Some(Cell::Man('n')));
        assert_eq!(grid_cell("^", "KQRBN"), Some(Cell::Man('P')));
        assert_eq!(grid_cell("#D", "KDTLS"), Some(Cell::Man('q')));
        assert_eq!(grid_cell("x", "KQRBN"), None);
    }
    #[test]
    fn only_short_numbered_labels_are_captions() {
        assert!(caption("DIAGRAM 12"));
        assert!(caption("Fig. 3."));
        assert!(!caption("Diagrams of the opening"));
        assert!(!caption("No. 4"));
        assert!(caption("Diag. 7"));
        for heading in ["Chapter 3", "Section 2", "Part 4", "Page 12", "Move 9"] {
            assert!(!caption(heading), "{heading}");
        }
    }
    #[test]
    fn a_chapter_heading_after_a_diagram_starts_afresh() {
        let b = blocks(&[
            (&grid_board(), BlockKind::Paragraph),
            ("Chapter 2", BlockKind::Heading),
            ("1. ... Kd7", BlockKind::Paragraph),
        ]);
        assert!(b.nodes.is_empty());
        assert!(
            !b.document.pages[0]
                .blocks
                .iter()
                .any(|block| matches!(block.kind, BlockKind::Diagram { .. }))
        );
    }
    #[test]
    fn a_roman_numeral_starts_a_new_line_from_move_one() {
        let b = book("I. 1. e4 e5 2. Nf3 Nc6 II. 1. d4 d5 2. c4");
        let d4 = b.nodes.iter().position(|n| n.uci == "d2d4").unwrap();
        assert_eq!(b.nodes[d4].parent, None);
        assert_eq!(b.nodes.last().unwrap().uci, "c2c4");
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
    /// Rejects every image but still reports a fixed ungated reading.
    struct Reads(String);
    impl DiagramRecognizer for Reads {
        fn placement(&self, _: &[u8]) -> Option<String> {
            None
        }
        fn reading(&self, _: &[u8]) -> Option<String> {
            Some(self.0.clone())
        }
    }
    /// Squares of `fen`'s placement with `changes` squares altered, as a reading.
    fn reading_of(placement: &str, changes: &[usize]) -> String {
        let mut squares: Vec<char> = placement
            .chars()
            .filter(|c| *c != '/')
            .flat_map(|c| match c.to_digit(10) {
                Some(n) => vec!['.'; n as usize],
                None => vec![c],
            })
            .collect();
        for &i in changes {
            squares[i] = if squares[i] == '.' { 'q' } else { '.' };
        }
        squares.into_iter().collect()
    }
    /// Moves, an image read as `reading`, then more moves.
    fn around_image(before: &str, reading: String, after: &str) -> Book {
        let mut page = Page::default();
        page.push(before, BlockKind::Paragraph);
        page.push(
            "",
            BlockKind::Image {
                path: "images/d.png".into(),
                alt: String::new(),
            },
        );
        page.push(after, BlockKind::Paragraph);
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
        resolve_using(document, Some(&Reads(reading)))
    }
    const AFTER_NF3: &str = "rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R";
    #[test]
    fn a_near_reading_lets_the_game_resume_at_its_next_move() {
        let b = around_image(
            "1. e4 e5 2. Nf3",
            reading_of(AFTER_NF3, &[0, 9, 20, 33, 63]),
            "2... Nc6 3. Bb5",
        );
        assert_eq!(
            b.nodes.iter().map(|n| n.uci.as_str()).collect::<Vec<_>>(),
            ["e2e4", "e7e5", "g1f3", "b8c6", "f1b5"]
        );
        assert_eq!(b.nodes[3].parent, Some(2));
        // Not read exactly, so the image is not shown as a position.
        assert_eq!(recognized(&b), None);
    }
    #[test]
    fn a_near_reading_cannot_resume_from_an_earlier_move() {
        let b = around_image(
            "1. e4 e5 2. Nf3",
            reading_of(AFTER_NF3, &[0, 9]),
            "2. d4 exd4",
        );
        assert_eq!(b.nodes.len(), 3);
        assert!(matches!(b.fragments[1].status, Status::Unresolved(_)));
    }
    #[test]
    fn a_distant_reading_is_a_new_position() {
        let far: Vec<usize> = (0..16).collect();
        let b = around_image("1. e4 e5 2. Nf3", reading_of(AFTER_NF3, &far), "2... Nc6");
        assert_eq!(b.nodes.len(), 3);
        assert!(matches!(b.fragments[1].status, Status::Unresolved(_)));
    }
    #[test]
    fn a_reading_one_square_off_confirms_the_replayed_position() {
        let b = around_image("1. e4 e5 2. Nf3", reading_of(AFTER_NF3, &[4]), "2... Nc6");
        assert_eq!(
            recognized(&b),
            Some("rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2")
        );
        assert_eq!(b.nodes.len(), 4);
    }
    #[test]
    fn captions_keep_confirmed_and_resume_positions_without_renumbering() {
        for changes in [vec![4], vec![0, 9, 20, 33, 63]] {
            let reading = reading_of(AFTER_NF3, &changes);
            let b = around_image("1. e4 e5 2. Nf3", reading.clone(), "");
            let mut document = b.document;
            document.pages[0].blocks[1].kind = BlockKind::Image {
                path: "images/d.png".into(),
                alt: String::new(),
            };
            let page = &mut document.pages[0];
            page.blocks.pop();
            page.push("The position above.", BlockKind::Paragraph);
            page.push("Fig. 3", BlockKind::Heading);
            page.push("2... Nc6 3. Bb5", BlockKind::Paragraph);
            let b = resolve_using(document.clone(), Some(&Reads(reading.clone())));
            assert_eq!(b.nodes.len(), 5);
            assert_eq!(b.nodes[3].parent, Some(2));
            let last = document.pages[0].blocks.last_mut().unwrap();
            last.span = 0..0;
            document.pages[0].push("12... Nc6", BlockKind::Paragraph);
            let b = resolve_using(document, Some(&Reads(reading)));
            assert_eq!(b.nodes.len(), 3, "caption must not float the move number");
        }
    }
    #[test]
    fn gated_placement_can_confirm_when_following_moves_reject_it() {
        // No black king: an ungated reading is still meaningful but is not a legal placement.
        assert!(reading_of_placement("8/8/8/8/8/8/8/4K3").is_none());
        assert_eq!(
            reading_of_placement(AFTER_NF3),
            Some(reading_of(AFTER_NF3, &[]))
        );
        let b = around_image("1. e4 e5 2. Nf3", String::new(), "White to play. 2... Nc6");
        let b = resolve_using(b.document, Some(&Fixed(AFTER_NF3)));
        assert_eq!(b.nodes.len(), 4);
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
    fn printed_ellipsis_after_the_move_number_names_black() {
        for text in ["30\u{2026} e5 31. Ra8+", "30\u{2026}e5 31. Ra8+"] {
            let b = book("", &[text]);
            assert_eq!(recognized(&b), Some("4k3/4p3/8/8/8/8/4P3/R3K3 b - - 0 1"));
            assert_eq!(
                b.nodes.iter().map(|n| n.uci.as_str()).collect::<Vec<_>>(),
                ["e7e5", "a1a8"]
            );
        }
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
    fn a_game_from_move_one_after_a_diagram_starts_from_the_opening() {
        let resolve_after_diagram = |moves: &str| {
            let mut page = Page::default();
            page.push("4k3/8/8/8/8/8/3P4/4K3 w - - 0 1", BlockKind::Paragraph);
            page.push(moves, BlockKind::Paragraph);
            resolve(Document {
                cover: None,
                isbn: None,
                title: String::new(),
                author: String::new(),
                format: "test".into(),
                pages: vec![page],
                assets: vec![],
            })
        };
        let book = resolve_after_diagram("Suppose that after 1. e2-e4 e7-e5 White plays 2. d2-d4.");
        assert_eq!(book.nodes.len(), 3);
        let game = book.fragments.last().expect("the game");
        assert_eq!(game.start_fen, fen(&Chess::default()), "{:?}", game.status);
        // Moves that fit the diagram as well stay on the diagram.
        let book = resolve_after_diagram("1. d2-d4 Ke8-d7");
        assert_eq!(book.nodes.len(), 2);
        assert_eq!(
            book.fragments.last().expect("the moves").start_fen,
            "4k3/8/8/8/8/8/3P4/4K3 w - - 0 1"
        );
    }
    #[test]
    fn a_game_from_move_one_leads_up_to_a_black_to_move_diagram() {
        let mut page = Page::default();
        page.push("4k3/8/8/8/8/8/3P4/4K3 b - - 0 1", BlockKind::Paragraph);
        page.push(
            "Suppose that after 1. e2-e4 e7-e5 White plays 2. d2-d4.",
            BlockKind::Paragraph,
        );
        let book = resolve(Document {
            cover: None,
            isbn: None,
            title: String::new(),
            author: String::new(),
            format: "test".into(),
            pages: vec![page],
            assets: vec![],
        });
        assert_eq!(book.nodes.len(), 3);
        let game = book.fragments.last().expect("the game");
        assert_eq!(game.start_fen, fen(&Chess::default()), "{:?}", game.status);
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
