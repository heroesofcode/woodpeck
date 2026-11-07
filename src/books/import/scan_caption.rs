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
        let Some(white) = picture.scan_side else {
            continue;
        };
        if cover.is_some_and(|bytes| bytes == picture.bytes.as_slice()) {
            continue;
        }
        if stronger_side_present(&picture.bytes) {
            continue;
        }
        let Some(png) = super::pdf::diagram_png(&picture.bytes) else {
            continue;
        };
        picture.bytes = diagram::with_scan_side(&png, white);
    }
}

/// Sides for `claimed`, or `None` when the page certificate does not hold.
/// `Some(true)` is White. `None` inside the vector abstains for that board.
pub(super) fn pending_sides(
    gray: &Gray,
    facts: RenderFacts,
    claimed: &[Frame],
    targets: &[ScanTarget],
) -> Option<Vec<Option<bool>>> {
    let lines = recognize_page(gray)?;
    let (cert, payload) = certify(gray, facts, &lines)?;
    bind(&cert, gray, &payload, claimed, targets)
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
) -> Option<Vec<Option<bool>>> {
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
    Some(assign(&frames, &payload.lines, targets))
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

fn explicit_turn_line(text: &str) -> Option<bool> {
    let text = normalize_line(text);
    if text.is_empty() {
        return None;
    }
    let mut found = None;
    for (phrase, white) in TURN_PHRASES {
        if normalize_line(phrase) != text {
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

fn confident(line: &OcrLine) -> bool {
    line.confidence.is_finite() && (OCR_CONFIDENCE_MIN..=1.0).contains(&line.confidence)
}

fn assign(frames: &[Frame], lines: &[OcrLine], targets: &[ScanTarget]) -> Vec<Option<bool>> {
    let mut owned = vec![Vec::new(); frames.len()];
    for (index, line) in lines.iter().enumerate() {
        let Some(white) = explicit_turn_line(&line.text) else {
            continue;
        };
        if !confident(line) || crowded(lines, index) {
            continue;
        }
        if let Some(owner) = sole_owner(frames, line) {
            owned[owner].push(white);
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

fn sole_owner(frames: &[Frame], line: &OcrLine) -> Option<usize> {
    let mut owner = None;
    for (index, frame) in frames.iter().enumerate() {
        if !below_board(*frame, line) {
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

#[allow(clippy::cast_precision_loss)]
fn below_board(frame: Frame, line: &OcrLine) -> bool {
    let width = (frame[2] - frame[0]) as f64;
    if width < 1.0 || !line_box_ok(line) {
        return false;
    }
    let gap = line.top - frame[3] as f64;
    if !(CAPTION_GAP_MIN * width..=CAPTION_GAP_MAX * width).contains(&gap) {
        return false;
    }
    let height = line.bottom - line.top;
    if height > CAPTION_HEIGHT_MAX * width {
        return false;
    }
    let center = f64::midpoint(line.left, line.right);
    let board = f64::midpoint(frame[0] as f64, frame[2] as f64);
    if (center - board).abs() > CAPTION_CENTER_SLACK * width {
        return false;
    }
    let margin = CAPTION_MARGIN * width;
    line.left >= frame[0] as f64 - margin && line.right <= frame[2] as f64 + margin
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
        OCR_CONFIDENCE_MIN, OcrLine, RenderFacts, ScanTarget, assign, bind, certify,
        explicit_turn_line, page_identity,
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
            "White to move and win",
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
    fn a_split_caption_and_an_above_board_line_abstain() {
        let frames = [board(40, 30, 200)];
        let split = [
            caption("White to", frames[0], 20.0, 0.0, 16.0, 0.95),
            caption("move", frames[0], 48.0, 0.0, 16.0, 0.95),
        ];
        assert_eq!(assign(&frames, &split, &open(1)), vec![None]);
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
        let sides = bind(&cert, &gray, &payload, &cert.frames, &open(1)).expect("bound");
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
            bind(&cert, &gray, &payload, &cert.frames, &open(1)).expect("bound"),
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
}
