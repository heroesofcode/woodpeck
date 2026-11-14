//! Last step of the side-to-move cascade: an explicit turn caption that exists only in the
//! page pixels.
//!
//! The binder runs only after extracted text, printed marks, triangles and numbered keys have
//! stayed silent. It abstains unless the render certificate, the full pre-placement detector
//! census and one complete high-confidence turn line uniquely own the board. Above-board
//! captions and split lines are not reconstructed.

use crate::books::diagram::{self, Frame, Gray};
use sha2::{Digest, Sha256};

/// Caption top, as a fraction of the board width, below the frame's bottom edge.
const CAPTION_GAP_MIN: f64 = 0.045;
/// Caption top, as a fraction of the board width, below the frame's bottom edge.
const CAPTION_GAP_MAX: f64 = 0.46;
/// Caption centre may sit this many board-widths off the board's centre.
const CAPTION_CENTER_SLACK: f64 = 0.15;
/// Caption may extend this many board-widths past either side of the frame.
const CAPTION_MARGIN: f64 = 0.22;
/// Caption line height may be at most this many board-widths.
const CAPTION_HEIGHT_MAX: f64 = 0.16;
/// A neighbour this many times the target-to-caption distance, plus [`NEIGHBOUR_SLACK_PX`], vetoes.
const NEIGHBOUR_DISTANCE_FACTOR: f64 = 1.5;
/// Reference-pixel slack on the neighbour veto, in raster pixels.
const NEIGHBOUR_SLACK_PX: f64 = 2.0;
/// Minimum mean line confidence. Below this the line is not evidence.
const OCR_CONFIDENCE_MIN: f32 = 0.85;
/// Bold figurine move lines OCR with lower confidence. Their skeleton is read down to this
/// floor, because it must also agree with the move number printed under the board.
const SKELETON_CONFIDENCE_MIN: f32 = 0.80;
/// Raster size implied by the PDF dimensions and scale may disagree by at most this many pixels.
const TRANSFORM_TOLERANCE_PX: f64 = 1.0;

/// Complete explicit turn phrases. The whole OCR line must equal one of these after case and
/// whitespace folding. Result verbs, material-gain prose and longer captions are absent on purpose.
const TURN_PHRASES: &[(&str, bool)] = &[
    ("white to move", true),
    ("black to move", false),
    ("white to play", true),
    ("black to play", false),
    ("white's move", true),
    ("black's move", false),
    ("white moves", true),
    ("white plays", true),
    ("black moves", false),
    ("black plays", false),
    ("weiß am zug", true),
    ("weiss am zug", true),
    ("schwarz am zug", false),
    ("trait aux blancs", true),
    ("trait aux noirs", false),
    ("vez das brancas", true),
    ("vez das pretas", false),
    ("vez das negras", false),
    ("tocca al bianco", true),
    ("tocca al nero", false),
    ("wit aan zet", true),
    ("zwart aan zet", false),
    ("ход белых", true),
    ("ход черных", false),
    ("ход чёрных", false),
    ("brancas jogam", true),
    ("jogam as brancas", true),
    ("jogam brancas", true),
    ("pretas jogam", false),
    ("jogam as pretas", false),
    ("jogam pretas", false),
    ("negras jogam", false),
    ("jogam as negras", false),
    ("juegan las blancas", true),
    ("juegan blancas", true),
    ("blancas juegan", true),
    ("mueven las blancas", true),
    ("mueven blancas", true),
    ("juegan las negras", false),
    ("juegan negras", false),
    ("negras juegan", false),
    ("mueven las negras", false),
    ("mueven negras", false),
    ("weiß zieht", true),
    ("weiss zieht", true),
    ("schwarz zieht", false),
    ("les blancs jouent", true),
    ("blancs jouent", true),
    ("les noirs jouent", false),
    ("noirs jouent", false),
    ("muove il bianco", true),
    ("il bianco muove", true),
    ("muove il nero", false),
    ("il nero muove", false),
];

/// What the renderer measured while producing the page raster. Callers do not invent these.
#[derive(Clone, Copy, Debug)]
pub(super) struct RenderFacts {
    pub page_index: u32,
    pub pdf_width: f64,
    pub pdf_height: f64,
    pub scale: f64,
    pub crop_x0: f64,
    pub crop_y0: f64,
}

/// One OCR line in raster pixels, origin at the top left.
#[derive(Clone, Debug)]
pub(super) struct OcrLine {
    pub text: String,
    pub confidence: f32,
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

/// A detector frame that may receive a side, or only take part in ownership.
#[derive(Clone, Copy, Debug)]
pub(super) struct ScanTarget {
    /// A stronger cascade source already named the side.
    pub confirmed: bool,
    /// Placement was accepted, so a side may be stored. Unrecognized boards stay ineligible.
    pub eligible: bool,
}

#[derive(Clone, Debug)]
struct PageCertificate {
    facts: RenderFacts,
    width: u32,
    height: u32,
    identity: [u8; 32],
    pixels: [u8; 32],
    ocr_digest: [u8; 32],
    frames: Vec<Frame>,
}

#[derive(Clone, Debug)]
struct OcrPayload {
    lines: Vec<OcrLine>,
    digest: [u8; 32],
}

/// `true` when a printed marker, caption, triangle or numbered key already names the side.
pub(super) fn stronger_side_present(bytes: &[u8]) -> bool {
    diagram::stated_side(bytes).is_some()
        || diagram::stated_caption_side(bytes).is_some()
        || diagram::stated_mark_side(bytes).is_some()
        || diagram::stated_triangle_side(bytes).is_some()
        || diagram::stated_key_side(bytes).is_some()
}

/// Write a pending scan side onto pictures that every stronger pass left untouched.
pub(super) fn apply_pending_scan_sides(
    pictures: &mut [Vec<super::picture::Placed>],
    cover: Option<&[u8]>,
) {
    for picture in pictures.iter_mut().flatten() {
        if picture.scan_side.is_none()
            && picture.scan_move.is_none()
            && picture.scan_replay.is_empty()
        {
            continue;
        }
        if cover.is_some_and(|bytes| bytes == picture.bytes.as_slice()) {
            continue;
        }
        if stronger_side_present(&picture.bytes) {
            continue;
        }
        let Some(png) = super::pdf::diagram_png(&picture.bytes) else {
            continue;
        };
        let png = match (picture.scan_side, &picture.scan_move) {
            (Some(white), _) => diagram::with_scan_side(&png, white),
            (None, Some((white, destination))) => {
                diagram::with_scan_move(&png, *white, destination)
            }
            (None, None) => png,
        };
        picture.bytes = if picture.scan_replay.is_empty() {
            png
        } else {
            diagram::with_scan_replay(&png, &picture.scan_replay)
        };
    }
}

/// One board's printed move: the mover and its destination.
pub(super) type PrintedMove = Option<(bool, String)>;

/// Sides certified for a scanned page, all from one certified OCR pass.
pub(super) struct PendingSides {
    /// Caption side per board. `Some(true)` is White; `None` abstains for that board.
    pub sides: Vec<Option<bool>>,
    /// Printed move per board.
    pub moves: Vec<PrintedMove>,
    /// Placements reached by game text replayed from move 1 on this page, with the side to move.
    pub replayed: Vec<(String, bool)>,
}

/// Caption sides, printed moves and replayed placements for `claimed`, or `None` when the page
/// certificate does not hold.
pub(super) fn pending_sides(
    gray: &Gray,
    facts: RenderFacts,
    claimed: &[Frame],
    targets: &[ScanTarget],
) -> Option<PendingSides> {
    let lines = recognize_page(gray)?;
    let (cert, payload) = certify(gray, facts, &lines)?;
    let (sides, moves) = bind(&cert, gray, &payload, claimed, targets)?;
    let replayed =
        super::scan_replay::replayed_placements(&payload.lines, &cert.frames, gray.width);
    Some(PendingSides {
        sides,
        moves,
        replayed,
    })
}

fn recognize_page(gray: &Gray) -> Option<Vec<OcrLine>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        super::scan_ocr::recognize(gray)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = gray;
        None
    }
}

fn certify(
    gray: &Gray,
    facts: RenderFacts,
    lines: &[OcrLine],
) -> Option<(PageCertificate, OcrPayload)> {
    if !transform_ok(facts, gray.width, gray.height) {
        return None;
    }
    let width = u32::try_from(gray.width).ok()?;
    let height = u32::try_from(gray.height).ok()?;
    let frames = diagram::page_boards(gray);
    if !census_ok(&frames) {
        return None;
    }
    let pixels = pixel_digest(gray);
    let identity = page_identity(facts, width, height, &pixels);
    let ocr_digest = evidence_digest(&pixels, lines);
    let cert = PageCertificate {
        facts,
        width,
        height,
        identity,
        pixels,
        ocr_digest,
        frames,
    };
    let payload = OcrPayload {
        lines: lines.to_vec(),
        digest: ocr_digest,
    };
    Some((cert, payload))
}

fn bind(
    cert: &PageCertificate,
    gray: &Gray,
    payload: &OcrPayload,
    claimed: &[Frame],
    targets: &[ScanTarget],
) -> Option<(Vec<Option<bool>>, Vec<PrintedMove>)> {
    let width = usize::try_from(cert.width).ok()?;
    let height = usize::try_from(cert.height).ok()?;
    if gray.width != width || gray.height != height {
        return None;
    }
    let pixels = pixel_digest(gray);
    if pixels != cert.pixels {
        return None;
    }
    if page_identity(cert.facts, cert.width, cert.height, &pixels) != cert.identity {
        return None;
    }
    if !transform_ok(cert.facts, gray.width, gray.height) {
        return None;
    }
    let frames = diagram::page_boards(gray);
    if frames != cert.frames || !census_ok(&frames) {
        return None;
    }
    if claimed != frames.as_slice() {
        return None;
    }
    if evidence_digest(&pixels, &payload.lines) != payload.digest
        || payload.digest != cert.ocr_digest
    {
        return None;
    }
    if targets.len() != frames.len() {
        return None;
    }
    // Census identity stays on the padded detector crops. The caption window uses the
    // board `locate` finds inside each crop, so that padding is not part of the gap.
    let boards = located_boards(gray, &frames)?;
    // A move printed under the board wins; the move printed right above it (the one that
    // led to the board) is read only for a board with none.
    let moves = assign_moves(&frames, &boards, &payload.lines, targets)
        .into_iter()
        .zip(assign_played(&frames, &boards, &payload.lines, targets))
        .map(|(under, above)| under.or(above))
        .collect();
    Some((
        assign_edges(&frames, &boards, &payload.lines, targets),
        moves,
    ))
}

/// Board edges inside each detector crop, in page pixels. `None` when a crop has no board.
fn located_boards(gray: &Gray, crops: &[Frame]) -> Option<Vec<Frame>> {
    crops
        .iter()
        .copied()
        .map(|crop| {
            let inner = diagram::locate(&gray.crop(crop)?)?;
            let board = [
                crop[0].max(0) + inner[0],
                crop[1].max(0) + inner[1],
                crop[0].max(0) + inner[2],
                crop[1].max(0) + inner[3],
            ];
            (board[2] > board[0] && board[3] > board[1] && board[3] <= crop[3]).then_some(board)
        })
        .collect()
}

fn transform_ok(facts: RenderFacts, width: usize, height: usize) -> bool {
    let values = [
        facts.pdf_width,
        facts.pdf_height,
        facts.scale,
        facts.crop_x0,
        facts.crop_y0,
    ];
    if !values.iter().copied().all(f64::is_finite) {
        return false;
    }
    if facts.scale <= 0.0 || facts.pdf_width <= 1.0 || facts.pdf_height <= 1.0 {
        return false;
    }
    let (Ok(width), Ok(height)) = (u32::try_from(width), u32::try_from(height)) else {
        return false;
    };
    (facts.pdf_width * facts.scale - f64::from(width)).abs() <= TRANSFORM_TOLERANCE_PX
        && (facts.pdf_height * facts.scale - f64::from(height)).abs() <= TRANSFORM_TOLERANCE_PX
}

fn census_ok(frames: &[Frame]) -> bool {
    let mut ordered = frames.to_vec();
    ordered.sort_unstable();
    let unique = ordered.windows(2).all(|pair| pair[0] != pair[1]);
    unique
        && frames
            .iter()
            .all(|frame| frame[2] > frame[0] && frame[3] > frame[1])
}

fn pixel_digest(gray: &Gray) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"woodpeck-scan-pixels-v1");
    hasher.update(u32::try_from(gray.width).unwrap_or(0).to_le_bytes());
    hasher.update(u32::try_from(gray.height).unwrap_or(0).to_le_bytes());
    hasher.update(gray.luma());
    hasher.finalize().into()
}

fn page_identity(facts: RenderFacts, width: u32, height: u32, pixels: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"woodpeck-scan-page-v1");
    hasher.update(facts.page_index.to_le_bytes());
    hasher.update(width.to_le_bytes());
    hasher.update(height.to_le_bytes());
    hasher.update(facts.pdf_width.to_le_bytes());
    hasher.update(facts.pdf_height.to_le_bytes());
    hasher.update(facts.scale.to_le_bytes());
    hasher.update(facts.crop_x0.to_le_bytes());
    hasher.update(facts.crop_y0.to_le_bytes());
    hasher.update(pixels);
    hasher.finalize().into()
}

fn evidence_digest(pixels: &[u8; 32], lines: &[OcrLine]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"woodpeck-scan-ocr-v1");
    hasher.update(pixels);
    for line in lines {
        hasher.update(line.confidence.to_le_bytes());
        hasher.update(line.left.to_le_bytes());
        hasher.update(line.top.to_le_bytes());
        hasher.update(line.right.to_le_bytes());
        hasher.update(line.bottom.to_le_bytes());
        let bytes = line.text.as_bytes();
        let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        hasher.update(len.to_le_bytes());
        hasher.update(bytes);
    }
    hasher.finalize().into()
}

/// Words a result suffix may use after a turn phrase: "White to move wins", "Black to move,
/// draw", "Black to move, White wins", "White to move and win".
const RESULT_WORDS: &[&str] = &[
    "and", "win", "wins", "draw", "draws", "drawn", "still", "a", "mate", "mates", "white", "black",
];
/// At least one of these must be in a suffix, so "White to move and" stays incomplete.
const RESULT_VERBS: &[&str] = &["win", "wins", "draw", "draws", "drawn", "mate", "mates"];

fn explicit_turn_line(text: &str) -> Option<bool> {
    let text = normalize_line(text);
    if text.is_empty() || text.contains('?') {
        return None;
    }
    let mut found = None;
    for (phrase, white) in TURN_PHRASES {
        let phrase = normalize_line(phrase);
        let Some(rest) = text.strip_prefix(phrase.as_str()) else {
            continue;
        };
        if !(rest.is_empty() || result_suffix(rest)) {
            continue;
        }
        match found {
            None => found = Some(*white),
            Some(previous) if previous == *white => {}
            Some(_) => return None,
        }
    }
    found
}

/// What follows a turn phrase: a separator, then only result words with at least one verb.
fn result_suffix(rest: &str) -> bool {
    let Some(first) = rest.chars().next() else {
        return false;
    };
    if first.is_alphanumeric() {
        return false;
    }
    let words: Vec<&str> = rest
        .split(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    ',' | '.' | ';' | ':' | '!' | '-' | '\u{2013}' | '\u{2014}'
                )
        })
        .filter(|word| !word.is_empty())
        .collect();
    !words.is_empty()
        && words.iter().all(|word| RESULT_WORDS.contains(word))
        && words.iter().any(|word| RESULT_VERBS.contains(word))
}

fn normalize_line(text: &str) -> String {
    let mut out = String::new();
    let mut spaced = false;
    for ch in text.chars() {
        let ch = match ch {
            '\u{2018}' | '\u{2019}' => '\'',
            other => other,
        };
        if ch.is_whitespace() {
            spaced = !out.is_empty();
            continue;
        }
        if spaced {
            out.push(' ');
            spaced = false;
        }
        for lower in ch.to_lowercase() {
            out.push(lower);
        }
    }
    out
}

/// The one confident, uncrowded line right under `lines[index]` that continues it: it starts
/// within one line height under it and shares its column. `None` when there is no such line
/// or more than one.
fn continuation(lines: &[OcrLine], index: usize) -> Option<usize> {
    let line = &lines[index];
    if !line_box_ok(line) {
        return None;
    }
    let height = line.bottom - line.top;
    let mut found = None;
    for (other, next) in lines.iter().enumerate() {
        if other == index || !line_box_ok(next) {
            continue;
        }
        let gap = next.top - line.bottom;
        let overlap = line.right.min(next.right) - line.left.max(next.left);
        if !(-0.3 * height..=height).contains(&gap) || overlap <= 0.0 {
            continue;
        }
        if !confident(next) || crowded(lines, other) || found.is_some() {
            return None;
        }
        found = Some(other);
    }
    found
}

fn confident(line: &OcrLine) -> bool {
    line.confidence.is_finite() && (OCR_CONFIDENCE_MIN..=1.0).contains(&line.confidence)
}

#[cfg(test)]
fn assign(frames: &[Frame], lines: &[OcrLine], targets: &[ScanTarget]) -> Vec<Option<bool>> {
    assign_edges(frames, frames, lines, targets)
}

fn assign_edges(
    frames: &[Frame],
    boards: &[Frame],
    lines: &[OcrLine],
    targets: &[ScanTarget],
) -> Vec<Option<bool>> {
    if frames.len() != boards.len() || frames.len() != targets.len() {
        return vec![None; frames.len()];
    }
    let mut owned = vec![Vec::new(); frames.len()];
    for (index, line) in lines.iter().enumerate() {
        if !confident(line) || crowded(lines, index) {
            continue;
        }
        let (white, last) = if let Some(white) = explicit_turn_line(&line.text) {
            (white, line)
        } else {
            // A caption the column width broke over two lines: "Black to" / "move, draw".
            let Some(next) = continuation(lines, index) else {
                continue;
            };
            let joined = format!("{} {}", line.text, lines[next].text);
            let Some(white) = explicit_turn_line(&joined) else {
                continue;
            };
            (white, &lines[next])
        };
        let Some(holder) = sole_owner(frames, boards, line) else {
            continue;
        };
        // The second line must not sit closer to another board than to this one.
        let own = frame_distance(frames[holder], last);
        let shared = frames.iter().enumerate().any(|(other, neighbour)| {
            other != holder
                && frame_distance(*neighbour, last)
                    <= own.mul_add(NEIGHBOUR_DISTANCE_FACTOR, NEIGHBOUR_SLACK_PX)
        });
        if !shared {
            owned[holder].push(white);
        }
    }
    owned
        .into_iter()
        .zip(targets)
        .map(|(sides, target)| {
            if target.confirmed || !target.eligible {
                return None;
            }
            match sides.as_slice() {
                [side] => Some(*side),
                _ => None,
            }
        })
        .collect()
}

/// The first text line under each board, when it opens with a numbered move (`12.` for White,
/// `12...` for Black) and names a destination. Only the nearest board may own it, and a board
/// another signal already named is skipped. The resolver later checks the destination against
/// the recognized placement.
fn assign_moves(
    frames: &[Frame],
    boards: &[Frame],
    lines: &[OcrLine],
    targets: &[ScanTarget],
) -> Vec<PrintedMove> {
    if frames.len() != boards.len() || frames.len() != targets.len() {
        return vec![None; frames.len()];
    }
    boards
        .iter()
        .enumerate()
        .map(|(index, board)| {
            let target = targets[index];
            if target.confirmed || !target.eligible {
                return None;
            }
            let line = first_line_under(*board, lines)?;
            if !confident_move(line) {
                return None;
            }
            let own = frame_distance(frames[index], line);
            let vetoed = frames.iter().enumerate().any(|(other, neighbour)| {
                other != index
                    && frame_distance(*neighbour, line)
                        <= own.mul_add(NEIGHBOUR_DISTANCE_FACTOR, NEIGHBOUR_SLACK_PX)
            });
            if !vetoed && let Some(read) = printed_move_strict(&line.text) {
                return Some(read);
            }
            // A weaker reading: OCR slips repaired, and a line under a board beside another
            // one is kept unless it sits as close over another board's top (that board's
            // lead-in). It is stored apart and never outweighs another signal.
            #[allow(clippy::cast_precision_loss)]
            let over = line.top - frames[index][3] as f64;
            #[allow(clippy::cast_precision_loss)]
            let lead_in = frames.iter().enumerate().any(|(other, neighbour)| {
                other != index
                    && (neighbour[1] as f64 - line.bottom).abs()
                        <= over
                            .max(0.0)
                            .mul_add(NEIGHBOUR_DISTANCE_FACTOR, NEIGHBOUR_SLACK_PX)
            });
            // Nor may it start right of the board, in the next text column.
            #[allow(clippy::cast_precision_loss)]
            let beside = line.left >= board[2] as f64;
            if lead_in || beside {
                return None;
            }
            printed_move(&line.text).map(|(white, to)| (white, format!("{WEAK_MOVE}{to}")))
        })
        .collect()
}

/// The line right above each board, when it holds nothing but numbered moves (`22.Nc2 Bf6`):
/// its last ply led to the board. Stored as the mover with `^`, the piece letter (`P` for a
/// pawn) and the destination (`^Bf6`); the resolver checks that the mover's piece stands there.
fn assign_played(
    frames: &[Frame],
    boards: &[Frame],
    lines: &[OcrLine],
    targets: &[ScanTarget],
) -> Vec<PrintedMove> {
    if frames.len() != boards.len() || frames.len() != targets.len() {
        return vec![None; frames.len()];
    }
    boards
        .iter()
        .enumerate()
        .map(|(index, board)| {
            let target = targets[index];
            if target.confirmed || !target.eligible {
                return None;
            }
            let line = last_line_above(*board, lines)?;
            if !(SKELETON_CONFIDENCE_MIN..=1.0).contains(&line.confidence) {
                return None;
            }
            // The line may be the end of another board's text: a nearer board vetoes it.
            let own = frame_distance(frames[index], line);
            let vetoed = frames.iter().enumerate().any(|(other, neighbour)| {
                other != index
                    && frame_distance(*neighbour, line)
                        <= own.mul_add(NEIGHBOUR_DISTANCE_FACTOR, NEIGHBOUR_SLACK_PX)
            });
            if vetoed {
                return None;
            }
            let strict = (OCR_CONFIDENCE_MIN..=1.0)
                .contains(&line.confidence)
                .then(|| played_move(&line.text))
                .flatten();
            let (white, piece, to) = strict.or_else(|| {
                // The move under the board must be this board's own: not a number that may
                // only label the diagram, not in the next column, and no other board as near
                // to it.
                let under = first_line_under(*board, lines)?;
                let own = frame_distance(frames[index], under);
                let shared = frames.iter().enumerate().any(|(other, neighbour)| {
                    other != index
                        && frame_distance(*neighbour, under)
                            <= own.mul_add(NEIGHBOUR_DISTANCE_FACTOR, NEIGHBOUR_SLACK_PX)
                });
                #[allow(clippy::cast_precision_loss)]
                let beside = under.left >= board[2] as f64;
                if shared
                    || beside
                    || label_under(&under.text)
                    || !(SKELETON_CONFIDENCE_MIN..=1.0).contains(&under.confidence)
                {
                    return None;
                }
                skeleton_played(&line.text, &under.text)
            })?;
            Some((white, format!("{PLAYED_MOVE}{piece}{to}")))
        })
        .collect()
}

/// Marks the last ply printed right above a board, the move that led to it.
pub(super) const PLAYED_MOVE: char = '^';

/// Bottom of the line above the board, at most this many board-widths over its top edge.
const PLAYED_GAP_MAX: f64 = 0.3;
/// Raster pixels a line above the board may reach into the board's top edge.
const PLAYED_OVERLAP_PX: f64 = 5.0;

/// The nearest text line over the board that overlaps it horizontally, skipping a bare
/// diagram label ("Diagram 12").
fn last_line_above(board: Frame, lines: &[OcrLine]) -> Option<&OcrLine> {
    let width = (board[2] - board[0]) as f64;
    if width < 1.0 {
        return None;
    }
    let top = board[1] as f64;
    let line = lines
        .iter()
        .filter(|line| {
            line_box_ok(line)
                && line.right > board[0] as f64
                && line.left < board[2] as f64
                && line.bottom <= top + PLAYED_OVERLAP_PX
                && !diagram_label(&line.text)
        })
        .max_by(|a, b| a.bottom.total_cmp(&b.bottom))?;
    (top - line.bottom < PLAYED_GAP_MAX * width).then_some(line)
}

/// A line that only numbers a diagram: "Diagram 12", "12".
fn diagram_label(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    matches!(words.as_slice(), [number] | [_, number]
        if number.trim_end_matches(['.', ':']).chars().all(|c| c.is_ascii_digit())
            && !number.is_empty())
}

/// A line under a board that may only number the diagram ("12", "12.", "Diagram 12"). A
/// number followed by Black's dots ("28...", "28…") continues a move, since diagram numbers
/// never carry them; a bare "28" or "17." stays ambiguous and is not read as a move.
fn label_under(text: &str) -> bool {
    let continues = text.split_whitespace().count() == 1 && {
        let digits = text.trim().trim_end_matches(['.', '\u{2026}']);
        let dots = &text.trim()[digits.len()..];
        !digits.is_empty()
            && digits.chars().all(|c| c.is_ascii_digit())
            && (dots.contains('\u{2026}') || dots.len() >= 2)
    };
    diagram_label(text) && !continues
}

/// The last ply of a line made only of numbered moves: mover, piece letter (`P` for pawns,
/// `K` for castling) and destination (`g1`/`c1` for castling). OCR slips common in bold move
/// text are repaired: `l` for the rank 1, `O`/`0` for a queen before a square, `g8-Q` for a
/// promotion. Any other word makes it prose, and the line is not read.
pub(super) fn played_move(text: &str) -> Option<(bool, char, String)> {
    let text = text.replace('\u{2026}', "...");
    let mut side: Option<bool> = None;
    let mut last = None;
    for word in text.split_whitespace() {
        let mut word = word;
        if let Some(digits) = word.find(|c: char| !c.is_ascii_digit())
            && digits > 0
            && digits <= 3
            && word[digits..].starts_with('.')
        {
            let dots = word[digits..].chars().take_while(|&c| c == '.').count();
            side = Some(dots == 1);
            word = &word[digits + dots..];
        } else if side.is_none() && word.starts_with("...") {
            side = Some(false);
            word = word.trim_start_matches('.');
        }
        if word.is_empty() {
            continue;
        }
        let white = side?;
        let word = word.trim_end_matches(['.', ',', ';']);
        if matches!(word, "1-0" | "0-1" | "etc" | "mate" | "zugzwang") {
            continue;
        }
        last = Some((white, played_ply(word, white)?));
        side = Some(!white);
    }
    let (white, (piece, to)) = last?;
    Some((white, piece, to))
}

/// The last ply of a bold move line whose figurines OCR garbled (`16.9xh2 ?e4`): the move
/// numbers and most destinations survive, the pieces do not. It is read only when the move
/// number printed right under the board continues it (`28.fxe5` above and `28...` under, or
/// `16...e4` above and `17.` under). The piece is unknown (`?`, or `K` for castling), so the
/// resolver checks only that a piece of the mover's colour stands on the destination.
/// Abstains on prose, variations in brackets and analysis-diagram captions.
fn skeleton_played(above: &str, under: &str) -> Option<(bool, char, String)> {
    let mut above = above.replace('\u{2026}', "...");
    if above.contains('(') || above.to_lowercase().contains("diagram") {
        return None;
    }
    for glued in ["0-00-0", "O-OO-O", "0-0O-O", "O-O0-0"] {
        above = above.replace(glued, &format!("{} {}", &glued[..3], &glued[3..]));
    }
    // (move number, starts with Black's ply, ply words)
    let mut groups: Vec<(u32, bool, Vec<String>)> = Vec::new();
    for word in above.split_whitespace() {
        if let Some((number, separator, rest)) = numbered(word) {
            let dots = separator.chars().filter(|c| matches!(c, '.' | ',')).count();
            let mut plies = Vec::new();
            if !rest.is_empty() {
                plies.push(rest.to_string());
            }
            groups.push((number, dots >= 2, plies));
            continue;
        }
        let Some((_, _, plies)) = groups.last_mut() else {
            continue;
        };
        match plies.last_mut() {
            Some(last)
                if (last.chars().count() == 1
                    && !last.starts_with(|c: char| c.is_ascii_digit()))
                    || word.starts_with('x') =>
            {
                last.push_str(word);
            }
            _ => plies.push(word.to_string()),
        }
    }
    let (number, black_first, plies) = groups.pop()?;
    let mut merged: Vec<String> = Vec::new();
    for ply in plies {
        if matches!(ply.as_str(), "!" | "?" | "!?" | "?!") {
            continue;
        }
        match merged.last_mut() {
            Some(last) if last.chars().count() == 1 => last.push_str(&ply),
            _ => merged.push(ply),
        }
    }
    let (white, ply) = match (black_first, merged.as_slice()) {
        (true, [ply]) | (false, [_, ply]) => (false, ply),
        (false, [ply]) => (true, ply),
        _ => return None,
    };
    let under = under.trim_start();
    let digits = under.chars().take_while(char::is_ascii_digit).count();
    if !(1..=2).contains(&digits) {
        return None;
    }
    let next: u32 = under[..digits].parse().ok()?;
    let separator: String = under[digits..]
        .trim_start()
        .chars()
        .take_while(|c| matches!(c, '.' | ',' | ':' | ';' | '-'))
        .collect();
    let continues = if white {
        next == number
    } else {
        next == number + 1 && separator.matches('.').count() < 2
    };
    if !continues {
        return None;
    }
    let ply = ply.trim_end_matches(['+', '#', '!', '?', '=', '-', '.', ',', ';', ':', ')', '±']);
    let castle: String = ply
        .chars()
        .map(|c| if c == 'O' { '0' } else { c })
        .collect();
    if castle == "0-0" || castle == "0-0-0" {
        let file = if castle == "0-0" { 'g' } else { 'c' };
        return Some((white, 'K', format!("{file}{}", if white { 1 } else { 8 })));
    }
    let chars: Vec<char> = ply.chars().collect();
    let [.., file, rank] = chars.as_slice() else {
        return None;
    };
    let rank = match rank {
        's' => '5',
        'z' => '2',
        'l' | 'i' => '1',
        other => *other,
    };
    (('a'..='h').contains(file) && ('1'..='8').contains(&rank))
        .then(|| (white, '?', format!("{file}{rank}")))
}

/// A move number opening a word: `16.`, `13..`, `25?..`, `11-f4` (OCR's dash for a dot).
fn numbered(word: &str) -> Option<(u32, &str, &str)> {
    let digits = word.chars().take_while(char::is_ascii_digit).count();
    if !(1..=2).contains(&digits) {
        return None;
    }
    let after = &word[digits..];
    let dotted = after
        .find(|c: char| !matches!(c, '.' | ',' | ':' | ';' | '?'))
        .unwrap_or(after.len());
    let separator_len = if dotted > 0 {
        dotted
    } else if after.starts_with('-')
        && after[1..]
            .chars()
            .next()
            .is_some_and(|c| !c.is_ascii_digit() && c != 'O')
    {
        1
    } else {
        return None;
    };
    Some((
        word[..digits].parse().ok()?,
        &after[..separator_len],
        &after[separator_len..],
    ))
}

/// Piece letter and destination of one SAN-like ply, after the OCR repairs of
/// [`played_move`].
fn played_ply(word: &str, white: bool) -> Option<(char, String)> {
    let word = word.trim_end_matches(['!', '?', '+', '#']);
    if matches!(word, "O-O" | "0-0") {
        return Some(('K', if white { "g1" } else { "g8" }.into()));
    }
    if matches!(word, "O-O-O" | "0-0-0") {
        return Some(('K', if white { "c1" } else { "c8" }.into()));
    }
    let mut chars: Vec<char> = word.chars().collect();
    // A promotion ("g8=Q", "g8-Q", "g8Q") puts the new piece on the square.
    let mut promoted = None;
    if let [.., rank, sep, piece] = chars.as_slice()
        && matches!(rank, '1' | '8' | 'l')
        && matches!(sep, '=' | '-')
        && matches!(piece, 'Q' | 'R' | 'B' | 'N')
    {
        promoted = Some(*piece);
        chars.truncate(chars.len() - 2);
    } else if let [.., rank, piece] = chars.as_slice()
        && matches!(rank, '1' | '8')
        && matches!(piece, 'Q' | 'R' | 'B' | 'N')
    {
        promoted = Some(*piece);
        chars.truncate(chars.len() - 1);
    }
    if let Some(first) = chars.first_mut()
        && matches!(first, 'O' | '0')
    {
        *first = 'Q';
    }
    if let Some(rank) = chars.last_mut()
        && *rank == 'l'
    {
        *rank = '1';
    }
    let (piece, rest) = match chars.split_first()? {
        (p @ ('K' | 'Q' | 'R' | 'B' | 'N'), rest) => (*p, rest),
        _ => ('P', chars.as_slice()),
    };
    let [ref from @ .., file, rank] = *rest else {
        return None;
    };
    if !('a'..='h').contains(&file) || !('1'..='8').contains(&rank) {
        return None;
    }
    // At most a disambiguating file or rank and a capture before the square.
    let from: String = from.iter().collect();
    let disambiguation = from.strip_suffix('x').unwrap_or(&from);
    let ok = match disambiguation.chars().collect::<Vec<_>>().as_slice() {
        [] => true,
        [c] => ('a'..='h').contains(c) || (piece != 'P' && ('1'..='8').contains(c)),
        _ => false,
    };
    if !ok || (piece == 'P' && from.len() == 1) {
        return None;
    }
    let piece = promoted.unwrap_or(piece);
    Some((piece, format!("{file}{rank}")))
}

/// Marks a printed move read only through OCR repairs or the looser neighbour rule.
pub(super) const WEAK_MOVE: char = '~';

/// Top of the first line under the board, at most this many board-widths below its bottom edge.
const MOVE_GAP_MAX: f64 = 0.3;
/// The line may start or end this many board-widths beyond the board's sides (text columns are
/// wider than centred diagrams).
const MOVE_MARGIN: f64 = 1.1;
/// Minimum mean confidence of a printed-move line. Figurines lower it, so it is below the
/// caption threshold, but the destination check in the resolver still has to pass.
const MOVE_CONFIDENCE_MIN: f32 = 0.5;

fn confident_move(line: &OcrLine) -> bool {
    line.confidence.is_finite() && (MOVE_CONFIDENCE_MIN..=1.0).contains(&line.confidence)
}

/// The leftmost fragment of the highest line starting under `board`, or `None` when that
/// line is missing or not in the board's column.
#[allow(clippy::cast_precision_loss)]
fn first_line_under(board: Frame, lines: &[OcrLine]) -> Option<&OcrLine> {
    let width = (board[2] - board[0]) as f64;
    if width < 1.0 {
        return None;
    }
    let bottom = board[3] as f64;
    let column = |line: &OcrLine| {
        line.right > board[0] as f64 - MOVE_MARGIN * width
            && line.left < board[2] as f64 + MOVE_MARGIN * width
    };
    let under: Vec<&OcrLine> = lines
        .iter()
        .filter(|line| line_box_ok(line) && line.top >= bottom && column(line))
        .collect();
    let first = under.iter().min_by(|a, b| a.top.total_cmp(&b.top))?;
    let (top, height) = (first.top, first.bottom - first.top);
    if top - bottom > MOVE_GAP_MAX * width {
        return None;
    }
    under
        .into_iter()
        .filter(|line| (line.top - top).abs() < height * 0.5)
        .min_by(|a, b| a.left.total_cmp(&b.left))
}

/// Mover and destination, read strictly, of a line such as `12.Nf3`, `12...Qc7`, `8…dxe4` or `15.0-0`.
/// The figurine may be misread; only the move number, the dots and the target are used.
fn printed_move_strict(text: &str) -> Option<(bool, String)> {
    let text = text.trim_start();
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 3 || text.starts_with('0') {
        return None;
    }
    let mut dots = 0;
    let mut rest = &text[digits..];
    loop {
        let trimmed = rest.trim_start_matches(' ');
        if let Some(after) = trimmed.strip_prefix('.') {
            dots += 1;
            rest = after;
        } else if let Some(after) = trimmed.strip_prefix('\u{2026}') {
            dots += 3;
            rest = after;
        } else {
            rest = trimmed;
            break;
        }
    }
    let white = match dots {
        1 => true,
        2..=4 => false,
        _ => return None,
    };
    let compact: String = rest
        .chars()
        .filter(|c| !c.is_whitespace())
        .take(8)
        .collect();
    for castle in ["O-O-O", "0-0-0", "O-O", "0-0"] {
        if compact.starts_with(castle) {
            let long = castle.len() == 5;
            return Some((white, if long { "O-O-O" } else { "O-O" }.to_owned()));
        }
    }
    let bytes: Vec<char> = compact.chars().collect();
    // Skip at most one misread figurine and a piece letter before the move proper.
    let start = bytes
        .iter()
        .position(|c| c.is_ascii_lowercase() || c.is_ascii_uppercase())?;
    if start > 1 {
        return None;
    }
    for pair in bytes[start..].windows(2).take(4) {
        if ('a'..='h').contains(&pair[0]) && ('1'..='8').contains(&pair[1]) {
            return Some((white, pair.iter().collect()));
        }
    }
    None
}

/// Mover and destination of a line such as `12.Nf3`, `12...Qc7`, `8…dxe4` or `15.0-0`.
/// The figurine may be misread; only the move number, the dots and the target are used. OCR
/// slips common in bold move lines are repaired: `1o.` is move 10, `11-f4` or `11,f4` is `11.f4`,
/// and a square's rank or file printed as a look-alike (`z` for 2, `s` for 5, `l` for 1, `C` for
/// c) still names it. The resolver checks the destination against the placement.
fn printed_move(text: &str) -> Option<(bool, String)> {
    let repaired = repaired_number(text.trim_start());
    let text = repaired.as_str();
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 3 || text.starts_with('0') {
        return None;
    }
    let mut dots = 0;
    let mut rest = &text[digits..];
    loop {
        let trimmed = rest.trim_start_matches(' ');
        if let Some(after) = trimmed.strip_prefix('.') {
            dots += 1;
            rest = after;
        } else if let Some(after) = trimmed.strip_prefix('\u{2026}') {
            dots += 3;
            rest = after;
        } else {
            rest = trimmed;
            break;
        }
    }
    let white = match dots {
        1 => true,
        2..=4 => false,
        _ => return None,
    };
    let compact: String = rest
        .chars()
        .filter(|c| !c.is_whitespace())
        .take(8)
        .collect();
    for castle in ["O-O-O", "0-0-0", "O-O", "0-0"] {
        if compact.starts_with(castle) {
            let long = castle.len() == 5;
            return Some((white, if long { "O-O-O" } else { "O-O" }.to_owned()));
        }
    }
    let chars: Vec<char> = compact.chars().collect();
    // The square comes within the first few characters, after a figurine or piece letter
    // (possibly misread) and a capture or disambiguation. A square printed as such wins over a
    // look-alike one, so the file of `Rfa1` is not read as `f4`.
    let square =
        |file: char, rank: char| ('a'..='h').contains(&file) && ('1'..='8').contains(&rank);
    if let Some(pair) = chars
        .windows(2)
        .take(5)
        .find(|pair| square(pair[0], pair[1]))
    {
        return Some((white, pair.iter().collect()));
    }
    chars.windows(2).take(5).find_map(|pair| {
        let file = match pair[0] {
            'C' => 'c',
            'E' => 'e',
            c => c,
        };
        let rank = match pair[1] {
            'z' | 'Z' => '2',
            's' | 'S' => '5',
            'a' => '4',
            'l' | 'I' | 'i' | '|' | 't' => '1',
            'B' => '8',
            c => c,
        };
        square(file, rank).then(|| (white, format!("{file}{rank}")))
    })
}

/// `text` with the move number's OCR slips repaired: `1o` reads `10`, and a dash, underscore or
/// comma right after the number reads as its dot.
fn repaired_number(text: &str) -> String {
    let mut out: String = text.to_owned();
    let bytes = out.as_bytes();
    if bytes.len() > 1 && bytes[0].is_ascii_digit() && bytes[1] == b'o' {
        out.replace_range(1..2, "0");
    }
    let digits = out.chars().take_while(char::is_ascii_digit).count();
    if (1..=3).contains(&digits) {
        let mut tail = out[digits..].chars();
        if let (Some('-' | '_' | ','), Some(next)) = (tail.next(), tail.next())
            && (next.is_whitespace()
                || next.is_ascii_alphabetic()
                || matches!(next, '.' | '?' | '#'))
        {
            out.replace_range(digits..=digits, ".");
        }
    }
    out
}

fn sole_owner(frames: &[Frame], boards: &[Frame], line: &OcrLine) -> Option<usize> {
    let mut owner = None;
    for (index, frame) in frames.iter().enumerate() {
        if !below_board(boards[index], line) {
            continue;
        }
        let own = frame_distance(*frame, line);
        let vetoed = frames.iter().enumerate().any(|(other, neighbour)| {
            other != index
                && frame_distance(*neighbour, line)
                    <= own.mul_add(NEIGHBOUR_DISTANCE_FACTOR, NEIGHBOUR_SLACK_PX)
        });
        if vetoed {
            continue;
        }
        if owner.is_some() {
            return None;
        }
        owner = Some(index);
    }
    owner
}

/// `board` is the located diagram, not the padded detector crop.
#[allow(clippy::cast_precision_loss)]
fn below_board(board: Frame, line: &OcrLine) -> bool {
    let width = (board[2] - board[0]) as f64;
    if width < 1.0 || !line_box_ok(line) {
        return false;
    }
    let gap = line.top - board[3] as f64;
    if !(CAPTION_GAP_MIN * width..=CAPTION_GAP_MAX * width).contains(&gap) {
        return false;
    }
    let height = line.bottom - line.top;
    if height > CAPTION_HEIGHT_MAX * width {
        return false;
    }
    let center = f64::midpoint(line.left, line.right);
    let middle = f64::midpoint(board[0] as f64, board[2] as f64);
    if (center - middle).abs() > CAPTION_CENTER_SLACK * width {
        return false;
    }
    let margin = CAPTION_MARGIN * width;
    line.left >= board[0] as f64 - margin && line.right <= board[2] as f64 + margin
}

fn line_box_ok(line: &OcrLine) -> bool {
    [line.left, line.top, line.right, line.bottom]
        .into_iter()
        .all(f64::is_finite)
        && line.right > line.left
        && line.bottom > line.top
}

#[allow(clippy::cast_precision_loss)]
fn frame_distance(frame: Frame, line: &OcrLine) -> f64 {
    let (left, top, right, bottom) = (
        frame[0] as f64,
        frame[1] as f64,
        frame[2] as f64,
        frame[3] as f64,
    );
    let dx = if line.right < left {
        left - line.right
    } else if line.left > right {
        line.left - right
    } else {
        0.0
    };
    let dy = if line.bottom < top {
        top - line.bottom
    } else if line.top > bottom {
        line.top - bottom
    } else {
        0.0
    };
    dx.hypot(dy)
}

/// Another fragment on the same baseline and within one joined line of this one.
fn crowded(lines: &[OcrLine], index: usize) -> bool {
    lines
        .iter()
        .enumerate()
        .any(|(other, line)| other != index && crowds(&lines[index], line))
}

fn crowds(a: &OcrLine, b: &OcrLine) -> bool {
    let height_a = a.bottom - a.top;
    let height_b = b.bottom - b.top;
    if height_a <= 0.0 || height_b <= 0.0 {
        return false;
    }
    let shorter = height_a.min(height_b);
    let middle = |line: &OcrLine| f64::midpoint(line.top, line.bottom);
    if (middle(a) - middle(b)).abs() >= shorter * 0.5 {
        return false;
    }
    let gap = if a.right < b.left {
        b.left - a.right
    } else if b.right < a.left {
        a.left - b.right
    } else {
        0.0
    };
    gap < shorter * 1.6
}

#[cfg(test)]
mod tests {
    use super::{
        CAPTION_GAP_MIN, OCR_CONFIDENCE_MIN, OcrLine, RenderFacts, ScanTarget, assign,
        assign_moves, assign_played, bind, certify, explicit_turn_line, label_under, page_identity,
        played_move, printed_move, skeleton_played,
    };
    use crate::books::diagram::{Frame, Gray};

    fn board(left: i64, top: i64, side: i64) -> Frame {
        [left, top, left + side, top + side]
    }

    fn open(count: usize) -> Vec<ScanTarget> {
        vec![
            ScanTarget {
                confirmed: false,
                eligible: true,
            };
            count
        ]
    }

    fn caption(
        text: &str,
        frame: Frame,
        gap: f64,
        shift: f64,
        height: f64,
        confidence: f32,
    ) -> OcrLine {
        let width = (frame[2] - frame[0]) as f64;
        let line_width = width * 0.55;
        let center = f64::midpoint(frame[0] as f64, frame[2] as f64) + shift;
        let top = frame[3] as f64 + gap;
        OcrLine {
            text: text.into(),
            confidence,
            left: center - line_width / 2.0,
            right: center + line_width / 2.0,
            top,
            bottom: top + height,
        }
    }

    fn under(text: &str, frame: Frame) -> OcrLine {
        caption(text, frame, 20.0, 0.0, 16.0, 0.95)
    }

    #[test]
    fn a_garbled_move_line_above_is_read_when_the_move_under_continues_it() {
        // Bold figurines garbled by OCR; the numbers and squares survive.
        assert_eq!(
            skeleton_played("13.. Ei d7 14.Wxd7 ?xd7 15.C5 ? xh2+ 16.9xh2 ?e4", "17.9f"),
            Some((false, '?', "e4".into()))
        );
        // "28...Bxd5" under the board, its dots lost.
        assert_eq!(
            skeleton_played("25?..d5 26.es He8 27.cxd5 ?xe5 28.txes", "28 8xd5"),
            Some((true, '?', "e5".into()))
        );
        assert_eq!(
            skeleton_played("8... 8xf3 9.xf3 ? bd7 10.0-00-0", "11-f4?!"),
            Some((false, 'K', "g8".into()))
        );
    }
    #[test]
    fn a_garbled_move_line_abstains_without_a_continuing_move_under_it() {
        // The line above is an earlier analysis line: the move under does not continue it.
        assert_eq!(skeleton_played("23-5f1 ?hs 24.Bidz ?f8", "27 ?xfz+"), None);
        // Black moved last, so White's move must follow, not another Black one.
        assert_eq!(skeleton_played("14.?d5 8b6", "14...Bd3"), None);
        assert_eq!(skeleton_played("14.?d5 8b6", "15...Bd3"), None);
        assert_eq!(
            skeleton_played("9...e5 10.C4 0-0", "analysis diagram"),
            None
        );
        assert_eq!(skeleton_played("24.Bidz ?f8 (+0.24)-", "25.Qe2"), None);
        assert_eq!(
            skeleton_played("inadequate compensation for the pawn.", "11. ab6"),
            None
        );
        // Three words after a White move number: not one move pair.
        assert_eq!(skeleton_played("15.Bc1 d5 h6", "16.Bd3"), None);
    }

    #[test]
    fn a_move_number_with_black_dots_under_the_board_is_not_a_diagram_label() {
        assert!(!label_under("28..."));
        assert!(!label_under("28\u{2026}"));
        assert!(!label_under("17..."));
        // A bare number may be the diagram's own label, so it never continues a move line.
        assert!(label_under("28"));
        assert!(label_under("17."));
        assert!(label_under("Diagram 12"));
        assert!(!label_under("17.Nf3"));
    }

    #[test]
    fn a_unique_owner_accepts_white_and_black() {
        let frames = [board(40, 30, 200)];
        assert_eq!(
            assign(&frames, &[under("White to move", frames[0])], &open(1)),
            vec![Some(true)]
        );
        assert_eq!(
            assign(&frames, &[under("BLACK TO PLAY", frames[0])], &open(1)),
            vec![Some(false)]
        );
        assert_eq!(explicit_turn_line("Schwarz am Zug"), Some(false));
        assert_eq!(explicit_turn_line("Vez das brancas"), Some(true));
        assert_eq!(explicit_turn_line("White  to   move"), Some(true));
        assert_eq!(explicit_turn_line("White\u{2019}s move"), Some(true));
    }

    #[test]
    fn competing_phrases_abstain_even_when_they_name_the_same_side() {
        let frames = [board(40, 30, 200)];
        let repeated = [
            under("White to move", frames[0]),
            caption("White to move", frames[0], 48.0, 0.0, 16.0, 0.95),
        ];
        assert_eq!(assign(&frames, &repeated, &open(1)), vec![None]);
        let opposed = [
            under("White to move", frames[0]),
            caption("Black to play", frames[0], 48.0, 0.0, 16.0, 0.95),
        ];
        assert_eq!(assign(&frames, &opposed, &open(1)), vec![None]);
    }

    #[test]
    fn material_gain_and_result_prose_are_rejected_in_a_fitting_caption_box() {
        let frames = [board(40, 30, 200)];
        let prose = [
            "White gains a pawn by force",
            "Black wins the exchange",
            "winning a piece",
            "White wins",
            "1-0",
            "Black mates in two",
            "White",
            "White to move?",
            "It is White to move here",
            "White to move and",
            "White to move, can he win?",
            "White to move the knight and win",
            "Black to move, White to move",
            "12) White to move",
            "after 12...Nf6",
            "Is it Black to move",
        ];
        for text in prose {
            assert_eq!(
                assign(&frames, &[under(text, frames[0])], &open(1)),
                vec![None],
                "{text}"
            );
        }
        assert_eq!(
            assign(&frames, &[under("White to move", frames[0])], &open(1)),
            vec![Some(true)]
        );
    }

    #[test]
    fn a_turn_phrase_with_a_result_suffix_names_the_mover() {
        let frames = [board(40, 30, 200)];
        for (text, white) in [
            ("White to move wins", true),
            ("Black to move, draw", false),
            ("Black to move, White wins", false),
            ("White to move and win", true),
            ("Black to move, still a draw", false),
            ("White to play and draw.", true),
            ("Black moves and wins", false),
        ] {
            assert_eq!(
                assign(&frames, &[under(text, frames[0])], &open(1)),
                vec![Some(white)],
                "{text}"
            );
        }
    }

    #[test]
    fn a_caption_split_over_two_lines_is_joined() {
        let frames = [board(40, 30, 200)];
        let split = [
            caption("White to", frames[0], 20.0, 0.0, 16.0, 0.95),
            caption("move", frames[0], 38.0, 0.0, 16.0, 0.95),
        ];
        assert_eq!(assign(&frames, &split, &open(1)), vec![Some(true)]);
        let split = [
            caption("Black to", frames[0], 20.0, 0.0, 16.0, 0.95),
            caption("move, draw", frames[0], 38.0, 0.0, 16.0, 0.95),
        ];
        assert_eq!(assign(&frames, &split, &open(1)), vec![Some(false)]);
        // Too far apart to be one caption, an unsure second line, or a second line that is
        // prose: all abstain.
        let apart = [
            caption("White to", frames[0], 20.0, 0.0, 16.0, 0.95),
            caption("move", frames[0], 60.0, 0.0, 16.0, 0.95),
        ];
        assert_eq!(assign(&frames, &apart, &open(1)), vec![None]);
        let unsure = [
            caption("White to", frames[0], 20.0, 0.0, 16.0, 0.95),
            caption("move", frames[0], 38.0, 0.0, 16.0, 0.6),
        ];
        assert_eq!(assign(&frames, &unsure, &open(1)), vec![None]);
        let prose = [
            caption("White to", frames[0], 20.0, 0.0, 16.0, 0.95),
            caption("move the rook", frames[0], 38.0, 0.0, 16.0, 0.95),
        ];
        assert_eq!(assign(&frames, &prose, &open(1)), vec![None]);
    }

    #[test]
    fn an_above_board_line_abstains() {
        let frames = [board(40, 30, 200)];
        let above = OcrLine {
            text: "White to move".into(),
            confidence: 0.95,
            left: 70.0,
            right: 180.0,
            top: 4.0,
            bottom: 20.0,
        };
        assert_eq!(assign(&frames, &[above], &open(1)), vec![None]);
    }

    #[test]
    fn a_confirmed_or_ineligible_board_is_not_overwritten() {
        let frames = [board(40, 30, 200)];
        let lines = [under("Black to move", frames[0])];
        let mut confirmed = open(1);
        confirmed[0].confirmed = true;
        assert_eq!(assign(&frames, &lines, &confirmed), vec![None]);
        let mut ineligible = open(1);
        ineligible[0].eligible = false;
        assert_eq!(assign(&frames, &lines, &ineligible), vec![None]);
    }

    #[test]
    fn confidence_below_the_floor_abstains() {
        let frames = [board(40, 30, 200)];
        let mut line = under("White to move", frames[0]);
        line.confidence = OCR_CONFIDENCE_MIN;
        assert_eq!(assign(&frames, &[line.clone()], &open(1)), vec![Some(true)]);
        line.confidence = f32::from_bits(OCR_CONFIDENCE_MIN.to_bits() - 1);
        assert_eq!(assign(&frames, &[line], &open(1)), vec![None]);
    }

    #[test]
    fn the_below_board_window_is_closed_at_its_stated_bounds() {
        let frame = board(40, 30, 200);
        let frames = [frame];
        let accept = |gap, shift, height| {
            assign(
                &frames,
                &[caption("White to move", frame, gap, shift, height, 0.95)],
                &open(1),
            )[0]
        };
        assert_eq!(accept(9.0, 0.0, 16.0), Some(true));
        assert_eq!(accept(8.9, 0.0, 16.0), None);
        assert_eq!(accept(92.0, 0.0, 16.0), Some(true));
        assert_eq!(accept(92.1, 0.0, 16.0), None);
        assert_eq!(accept(20.0, 30.0, 16.0), Some(true));
        assert_eq!(accept(20.0, 30.1, 16.0), None);
        assert_eq!(accept(20.0, 0.0, 32.0), Some(true));
        assert_eq!(accept(20.0, 0.0, 32.1), None);
        // Wide line shifted within the centre slack, past the 0.22-width margin.
        let mut wide = caption("White to move", frame, 20.0, 20.0, 16.0, 0.95);
        let center = f64::midpoint(wide.left, wide.right);
        wide.left = center - 125.0;
        wide.right = center + 125.0;
        assert!(wide.right > frame[2] as f64 + 0.22 * 200.0);
        assert_eq!(assign(&frames, &[wide], &open(1)), vec![None]);
    }

    #[test]
    fn a_caption_inside_the_detector_padding_can_still_clear_the_board() {
        let gray = paint(&[(80, 70, 240)]);
        let (preview, _) = sealed(&gray, &[]);
        let crop = preview.frames[0];
        let board = super::located_boards(&gray, &[crop]).expect("board")[0];
        assert!(
            board[3] < crop[3],
            "the detector crop is padded below the board"
        );
        let width = (board[2] - board[0]) as f64;
        let gap = CAPTION_GAP_MIN * width + 1.0;
        assert!(
            board[3] as f64 + gap < crop[3] as f64,
            "this line starts inside the crop padding"
        );
        let line = caption("White to move", board, gap, 0.0, 0.08 * width, 0.96);
        let (cert, payload) = sealed(&gray, &[line]);
        assert_eq!(
            bind(&cert, &gray, &payload, &cert.frames, &open(1))
                .expect("bound")
                .0,
            vec![Some(true)]
        );
        let tucked = caption("White to move", board, 1.0, 0.0, 0.08 * width, 0.96);
        let (cert, payload) = sealed(&gray, &[tucked]);
        assert_eq!(
            bind(&cert, &gray, &payload, &cert.frames, &open(1))
                .expect("bound")
                .0,
            vec![None]
        );
    }

    #[test]
    fn a_close_neighbour_vetoes_and_a_distant_one_does_not() {
        let target = board(40, 40, 200);
        let line = under("White to move", target);
        let distant = board(360, 40, 200);
        assert_eq!(
            assign(&[target, distant], std::slice::from_ref(&line), &open(2)),
            vec![Some(true), None]
        );
        let close = board(210, 40, 200);
        assert_eq!(
            assign(&[target, close], std::slice::from_ref(&line), &open(2)),
            vec![None, None]
        );
        // The close neighbour still vetoes when it cannot receive a side of its own.
        let mut targets = open(2);
        targets[1].eligible = false;
        assert_eq!(
            assign(&[target, close], &[line], &targets),
            vec![None, None]
        );
    }

    #[test]
    fn same_baseline_crowding_rejects_an_otherwise_explicit_line() {
        let frames = [board(40, 30, 200)];
        let phrase = under("White to move", frames[0]);
        let mut aside = phrase.clone();
        aside.text = "gains a pawn".into();
        aside.left = phrase.right + 4.0;
        aside.right = aside.left + 90.0;
        assert_eq!(assign(&frames, &[phrase, aside], &open(1)), vec![None]);
    }

    /// Checkerboard with a dark rim. `page_boards` confirms a crop only when long border
    /// lines bound an a8-light board, so a bare checker is not a census frame.
    fn paint(specs: &[(i64, i64, i64)]) -> Gray {
        let (width, height) = (860, 640);
        let mut luma = vec![255_u8; width * height];
        for &(left, top, side) in specs {
            let border = 3;
            fill(
                &mut luma,
                width,
                left - border,
                top - border,
                left + side + border,
                top + side + border,
                0,
            );
            let cell = side / 8;
            for row in 0_i64..8 {
                for col in 0_i64..8 {
                    let shade = if (row + col) % 2 == 1 { 25 } else { 235 };
                    fill(
                        &mut luma,
                        width,
                        left + col * cell,
                        top + row * cell,
                        left + (col + 1) * cell,
                        top + (row + 1) * cell,
                        shade,
                    );
                }
            }
        }
        Gray::from_luma(width, height, &luma).expect("page")
    }

    fn fill(luma: &mut [u8], width: usize, x0: i64, y0: i64, x1: i64, y1: i64, shade: u8) {
        let image_h = luma.len() / width;
        let x0 = usize::try_from(x0.max(0)).unwrap_or(0).min(width);
        let x1 = usize::try_from(x1.max(0)).unwrap_or(0).min(width);
        let y0 = usize::try_from(y0.max(0)).unwrap_or(0).min(image_h);
        let y1 = usize::try_from(y1.max(0)).unwrap_or(0).min(image_h);
        for y in y0..y1 {
            for x in x0..x1 {
                luma[y * width + x] = shade;
            }
        }
    }

    fn facts_for(gray: &Gray) -> RenderFacts {
        let pdf_width = 400.0;
        let scale = gray.width as f64 / pdf_width;
        RenderFacts {
            page_index: 4,
            pdf_width,
            pdf_height: gray.height as f64 / scale,
            scale,
            crop_x0: 12.0,
            crop_y0: 8.0,
        }
    }

    fn sealed(gray: &Gray, lines: &[OcrLine]) -> (super::PageCertificate, super::OcrPayload) {
        certify(gray, facts_for(gray), lines).expect("render certificate")
    }

    #[test]
    fn a_certified_page_accepts_its_unique_caption() {
        let gray = paint(&[(80, 70, 240)]);
        let (cert, _) = sealed(&gray, &[]);
        assert_eq!(cert.frames.len(), 1, "the detector census is the owner");
        let width = (cert.frames[0][2] - cert.frames[0][0]) as f64;
        let line = caption(
            "White to move",
            cert.frames[0],
            0.12 * width,
            0.0,
            0.08 * width,
            0.96,
        );
        let (cert, payload) = sealed(&gray, &[line]);
        let sides = bind(&cert, &gray, &payload, &cert.frames, &open(1))
            .expect("bound")
            .0;
        assert_eq!(sides, vec![Some(true)]);
    }

    #[test]
    fn an_incomplete_inventory_abstains_for_the_page() {
        let gray = paint(&[(50, 40, 200), (430, 40, 200)]);
        let (cert, payload) = sealed(&gray, &[]);
        assert!(
            cert.frames.len() >= 2,
            "both boards stay in the census, found {}",
            cert.frames.len()
        );
        let mut claimed = cert.frames.clone();
        claimed.pop();
        assert!(bind(&cert, &gray, &payload, &claimed, &open(claimed.len())).is_none());
        let mut duplicated = cert.frames.clone();
        duplicated.push(duplicated[0]);
        assert!(bind(&cert, &gray, &payload, &duplicated, &open(duplicated.len())).is_none());
    }

    #[test]
    fn a_wrong_or_inconsistent_transform_abstains() {
        let gray = paint(&[(80, 70, 240)]);
        let (cert, payload) = sealed(&gray, &[]);
        let mut scaled = cert.clone();
        scaled.facts.pdf_width *= 1.25;
        scaled.identity = page_identity(scaled.facts, scaled.width, scaled.height, &scaled.pixels);
        assert!(
            bind(
                &scaled,
                &gray,
                &payload,
                &scaled.frames,
                &open(cert.frames.len())
            )
            .is_none()
        );

        let mut identity = cert.clone();
        identity.identity[0] ^= 0x5a;
        assert!(
            bind(
                &identity,
                &gray,
                &payload,
                &identity.frames,
                &open(cert.frames.len())
            )
            .is_none()
        );

        let mut page = cert.clone();
        page.facts.page_index += 1;
        assert!(
            bind(
                &page,
                &gray,
                &payload,
                &page.frames,
                &open(cert.frames.len())
            )
            .is_none()
        );

        let mut facts = facts_for(&gray);
        facts.scale = 0.0;
        assert!(certify(&gray, facts, &[]).is_none());
        facts.scale = f64::NAN;
        assert!(certify(&gray, facts, &[]).is_none());
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn a_replaced_polygon_or_rescaled_geometry_cannot_borrow_the_certificate() {
        let gray = paint(&[(80, 70, 240)]);
        let (preview, _) = sealed(&gray, &[]);
        let width = (preview.frames[0][2] - preview.frames[0][0]) as f64;
        let line = caption(
            "Black to move",
            preview.frames[0],
            0.12 * width,
            0.0,
            0.08 * width,
            0.96,
        );
        let (cert, payload) = sealed(&gray, &[line]);
        assert_eq!(
            bind(&cert, &gray, &payload, &cert.frames, &open(1))
                .expect("bound")
                .0,
            vec![Some(false)]
        );

        let mut moved = payload.clone();
        moved.lines[0].left += 6.0;
        moved.lines[0].right += 6.0;
        assert!(bind(&cert, &gray, &moved, &cert.frames, &open(1)).is_none());

        let mut coherent = cert.clone();
        let factor = 1.1;
        coherent.frames = coherent
            .frames
            .iter()
            .map(|frame| frame.map(|value| (value as f64 * factor).round() as i64))
            .collect();
        let mut shifted = payload.clone();
        for line in &mut shifted.lines {
            line.left *= factor;
            line.right *= factor;
            line.top *= factor;
            line.bottom *= factor;
        }
        shifted.digest = super::evidence_digest(&coherent.pixels, &shifted.lines);
        coherent.ocr_digest = shifted.digest;
        assert!(bind(&coherent, &gray, &shifted, &coherent.frames, &open(1)).is_none());

        let mut tampered = Gray {
            width: gray.width,
            height: gray.height,
            px: gray.px.clone(),
        };
        tampered.px[20] = 0.0;
        assert!(bind(&cert, &tampered, &payload, &cert.frames, &open(1)).is_none());
    }

    #[test]
    fn a_printed_move_names_its_mover_and_destination() {
        let mv = |white: bool, to: &str| Some((white, to.to_owned()));
        assert_eq!(printed_move("12.Nf3 Qc7 13.Rd1"), mv(true, "f3"));
        assert_eq!(printed_move("12...Qc7"), mv(false, "c7"));
        assert_eq!(printed_move("8\u{2026}dxe4"), mv(false, "e4"));
        assert_eq!(printed_move("21 . . . Rae1"), mv(false, "e1"));
        assert_eq!(printed_move("15.0-0-0"), mv(true, "O-O-O"));
        assert_eq!(printed_move("15...O-O"), mv(false, "O-O"));
        assert_eq!(printed_move("14.\u{2658}xe5"), mv(true, "e5"));
        assert_eq!(printed_move("White to move"), None);
        assert_eq!(printed_move("1958"), None);
        assert_eq!(printed_move("12 Nf3"), None);
        assert_eq!(printed_move("12.....Nf3"), None);
        assert_eq!(printed_move("05.Nf3"), None);
        assert_eq!(printed_move("12.(see page 40)"), None);
        // OCR slips in bold move lines.
        assert_eq!(printed_move("1o.Nf3"), mv(true, "f3"));
        assert_eq!(printed_move("11-f4 e5"), mv(true, "f4"));
        assert_eq!(printed_move("23...Rcz"), mv(false, "c2"));
        assert_eq!(printed_move("17.\u{a3})xe5"), mv(true, "e5"));
        assert_eq!(printed_move("9...QeB+"), mv(false, "e8"));
        assert_eq!(printed_move("11-12 April"), None);
        // A printed square wins over a look-alike one.
        assert_eq!(printed_move("21.Rfa1"), mv(true, "a1"));
        assert_eq!(printed_move("9...Nca5"), mv(false, "a5"));
    }

    #[test]
    fn a_printed_move_binds_only_the_board_it_sits_under() {
        let frames = [board(40, 30, 200)];
        assert_eq!(
            assign_moves(&frames, &frames, &[under("17...Rf7", frames[0])], &open(1)),
            vec![Some((false, "f7".to_owned()))]
        );
        let far = caption("17...Rf7", frames[0], 120.0, 0.0, 16.0, 0.95);
        assert_eq!(assign_moves(&frames, &frames, &[far], &open(1)), vec![None]);
        let confirmed = [ScanTarget {
            confirmed: true,
            eligible: true,
        }];
        assert_eq!(
            assign_moves(
                &frames,
                &frames,
                &[under("17...Rf7", frames[0])],
                &confirmed
            ),
            vec![None]
        );
        let pair = [board(40, 30, 200), board(40, 262, 200)];
        let between = under("17...Rf7", pair[0]);
        assert_eq!(
            assign_moves(&pair, &pair, &[between], &open(2)),
            vec![None, None]
        );
    }

    #[test]
    fn a_repaired_move_is_weak_and_never_read_from_the_next_column() {
        let frames = [board(40, 30, 200)];
        assert_eq!(
            assign_moves(&frames, &frames, &[under("1o.Nf3", frames[0])], &open(1)),
            vec![Some((true, format!("{}f3", super::WEAK_MOVE)))]
        );
        let beside = caption("1o.Nf3", frames[0], 20.0, 160.0, 16.0, 0.95);
        assert_eq!(
            assign_moves(&frames, &frames, &[beside], &open(1)),
            vec![None]
        );
    }

    #[test]
    fn a_pure_move_line_names_the_move_that_led_to_the_board() {
        let ply = |white: bool, piece: char, to: &str| Some((white, piece, to.to_owned()));
        assert_eq!(played_move("22.Nc2 Bf6"), ply(false, 'B', "f6"));
        assert_eq!(
            played_move("26.b5 Bd8 27.Nb4 Bb6 28.Nc2 Ba5"),
            ply(false, 'B', "a5")
        );
        assert_eq!(
            played_move("7...Rc1+ 8.Kf2 Nc6 9.Nd1"),
            ply(true, 'N', "d1")
        );
        assert_eq!(played_move("...Kg7 6.Ke2 Kg6"), ply(false, 'K', "g6"));
        assert_eq!(played_move("1... Nc4+ 2.Bxc4"), ply(true, 'B', "c4"));
        // OCR slips: 'l' for 1, 'O' for a queen, a dashed promotion.
        assert_eq!(played_move("4...Kh4 5.Kfl Kh3 6.Kgl"), ply(true, 'K', "g1"));
        assert_eq!(played_move("6...Oxf2+ 7.Kh1 Qxg3"), ply(false, 'Q', "g3"));
        assert_eq!(played_move("14.g7! Ke6 15.g8-Q+"), ply(true, 'Q', "g8"));
        assert_eq!(played_move("9.e6 g5 10.Kxg2 h5"), ply(false, 'P', "h5"));
        assert_eq!(played_move("12.O-O"), ply(true, 'K', "g1"));
        assert_eq!(
            played_move("2...Bc3 3.Bg1 Be1 4.Bh2 a5, zugzwang."),
            ply(false, 'P', "a5")
        );
        // Prose, unnumbered moves and malformed plies are not read.
        assert_eq!(
            played_move("to cut the king off by 2.Re6! Kd5 3.Rh6."),
            None
        );
        assert_eq!(played_move("only draws: 21.Ra3 b3"), None);
        assert_eq!(played_move("Nc2 Bf6"), None);
        assert_eq!(played_move("22.Nc2 ed6"), None);
        assert_eq!(played_move("Diagram 547"), None);
    }

    #[test]
    fn the_line_right_above_a_board_is_read_unless_another_board_is_nearer() {
        let frames = [board(40, 300, 200)];
        let above = |text: &str, gap: f64| OcrLine {
            text: text.into(),
            confidence: 0.95,
            left: 60.0,
            top: 300.0 - gap - 16.0,
            right: 220.0,
            bottom: 300.0 - gap,
        };
        assert_eq!(
            assign_played(&frames, &frames, &[above("22.Nc2 Bf6", 10.0)], &open(1)),
            vec![Some((false, "^Bf6".to_owned()))]
        );
        // A diagram label between them is skipped; prose is not read; a far line is not read.
        assert_eq!(
            assign_played(
                &frames,
                &frames,
                &[above("22.Nc2 Bf6", 30.0), above("Diagram 12", 8.0)],
                &open(1)
            ),
            vec![Some((false, "^Bf6".to_owned()))]
        );
        assert_eq!(
            assign_played(
                &frames,
                &frames,
                &[above("as in 22.Nc2 Bf6", 10.0)],
                &open(1)
            ),
            vec![None]
        );
        assert_eq!(
            assign_played(&frames, &frames, &[above("22.Nc2 Bf6", 90.0)], &open(1)),
            vec![None]
        );
        // Under another board that sits closer to the line, it belongs to that board.
        let pair = [board(40, 60, 200), board(40, 300, 200)];
        assert_eq!(
            assign_played(&pair, &pair, &[above("22.Nc2 Bf6", 10.0)], &open(2))[1],
            None
        );
    }

    #[test]
    fn a_garbled_line_above_needs_the_boards_own_move_under_it() {
        let frames = [board(40, 300, 200)];
        let above = OcrLine {
            text: "15.C5 ? xh2+ 16.9xh2 ?e4".into(),
            confidence: 0.9,
            left: 60.0,
            top: 274.0,
            right: 220.0,
            bottom: 290.0,
        };
        let played = |frames: &[Frame], under: OcrLine| {
            assign_played(frames, frames, &[above.clone(), under], &open(frames.len()))[0].clone()
        };
        assert_eq!(
            played(&frames, under("17.9f", frames[0])),
            Some((false, "^?e4".to_owned()))
        );
        // A bare diagram number under the board is not the next move.
        assert_eq!(played(&frames, under("17", frames[0])), None);
        // Nor is a move in the next column, right of the board.
        assert_eq!(
            played(
                &frames,
                caption("17.9f", frames[0], 20.0, 260.0, 16.0, 0.95)
            ),
            None
        );
        // Nor a line as close to the top of another board below it.
        let pair = [board(40, 300, 200), board(40, 540, 200)];
        assert_eq!(played(&pair, under("17.9f", pair[0])), None);
    }
}
