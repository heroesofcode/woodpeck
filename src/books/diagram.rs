//! CPU recognition of printed diagram images. A board is located from its outer frame and
//! square alternation, each square is compared with compact prototypes, and the whole image is
//! rejected unless every square, the style distance and the material sanity checks pass.
//! Callers must still verify the side to move and the following moves before trusting it.

/// Seam for other recognizers, such as a future OCR pipeline for scanned pages.
pub trait DiagramRecognizer: Sync {
    /// FEN piece placement (White at the bottom), or `None` to abstain.
    fn placement(&self, image: &[u8]) -> Option<String>;
    /// Best guess for all 64 squares (row 0 = rank 8, '.' empty) without the confidence gates.
    /// It may only confirm a position the moves already reached, never seed one.
    fn reading(&self, _image: &[u8]) -> Option<String> {
        None
    }
}

/// Built-in nearest-prototype recognizer.
pub struct Prototypes {
    dim: usize,
    labels: Vec<u8>,
    dark: Vec<bool>,
    values: Vec<f32>,
}

pub const CELL: usize = 32;
pub const DIM: usize = 16 * 16 + 8 * 8;
const MAGIC: &[u8; 4] = b"WPDG";
const SCALE: f32 = 200.0;
/// Squares are labelled with FEN letters; '.' is empty.
pub const LABELS: &str = ".PNBRQKpnbrqk";

static BUILTIN: std::sync::OnceLock<Option<Prototypes>> = std::sync::OnceLock::new();

/// The recognizer trained from the bundled prototype file.
pub fn builtin() -> Option<&'static Prototypes> {
    BUILTIN
        .get_or_init(|| Prototypes::decode(include_bytes!("diagram_prototypes.bin")))
        .as_ref()
}

pub struct Gray {
    pub width: usize,
    pub height: usize,
    pub px: Vec<f32>,
}

impl Gray {
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let image = image::load_from_memory(bytes).ok()?.to_luma8();
        let (width, height) = (image.width() as usize, image.height() as usize);
        if width < 64 || height < 64 || width > 4096 || height > 4096 {
            return None;
        }
        let px = image
            .into_raw()
            .into_iter()
            .map(|v| f32::from(v) / 255.0)
            .collect();
        Some(Self { width, height, px })
    }
    fn at(&self, x: usize, y: usize) -> f32 {
        self.px[y * self.width + x]
    }
}

/// Left, top, right, bottom in pixels.
pub type Frame = [i64; 4];

#[derive(Clone, Copy)]
struct Rect {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn cell_rect(g: &Gray, frame: Frame, row: usize, col: usize) -> Option<Rect> {
    let cw = (frame[2] - frame[0]) as f64 / 8.0;
    let ch = (frame[3] - frame[1]) as f64 / 8.0;
    let x0 = frame[0] as f64 + col as f64 * cw;
    let y0 = frame[1] as f64 + row as f64 * ch;
    let clamp = |v: f64, max: usize| (v.round().max(0.0) as usize).min(max);
    let rect = Rect {
        x0: clamp(x0, g.width),
        y0: clamp(y0, g.height),
        x1: clamp(x0 + cw, g.width),
        y1: clamp(y0 + ch, g.height),
    };
    (rect.x1 > rect.x0 + 4 && rect.y1 > rect.y0 + 4).then_some(rect)
}

fn mean(g: &Gray, r: Rect) -> f32 {
    let mut sum = 0.0;
    for y in r.y0..r.y1 {
        for x in r.x0..r.x1 {
            sum += g.at(x, y);
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let n = ((r.x1 - r.x0) * (r.y1 - r.y0)) as f32;
    sum / n.max(1.0)
}

fn median(values: &mut [f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mid = values.len() / 2;
    let (_, m, _) = values.select_nth_unstable_by(mid, f32::total_cmp);
    let m = *m;
    if values.len().is_multiple_of(2) {
        let lower = values[..mid].iter().copied().fold(f32::MIN, f32::max);
        f32::midpoint(lower, m)
    } else {
        m
    }
}

fn dark_square(row: usize, col: usize) -> bool {
    (row + col) % 2 == 1
}

/// Median of four corner patches estimates the square colour without the piece in the middle.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn corner_shade(g: &Gray, r: Rect) -> f32 {
    let side = (r.x1 - r.x0).min(r.y1 - r.y0);
    let k = ((side as f32 * 0.18) as usize).max(2);
    let m = ((side as f32 * 0.06) as usize).max(1);
    if 2 * (m + k) > side {
        return mean(g, r);
    }
    let patch = |x: usize, y: usize| {
        mean(
            g,
            Rect {
                x0: x,
                y0: y,
                x1: x + k,
                y1: y + k,
            },
        )
    };
    let mut shades = [
        patch(r.x0 + m, r.y0 + m),
        patch(r.x1 - m - k, r.y0 + m),
        patch(r.x0 + m, r.y1 - m - k),
        patch(r.x1 - m - k, r.y1 - m - k),
    ];
    median(&mut shades)
}

/// Squares agreeing with the a8-light checker pattern, and mean dark/light shades.
fn checker(g: &Gray, frame: Frame) -> Option<(usize, f32, f32)> {
    let mut shades = [0.0f32; 64];
    for (i, shade) in shades.iter_mut().enumerate() {
        *shade = corner_shade(g, cell_rect(g, frame, i / 8, i % 8)?);
    }
    let (mut dark, mut light) = (0.0, 0.0);
    for (i, s) in shades.iter().enumerate() {
        if dark_square(i / 8, i % 8) {
            dark += s / 32.0;
        } else {
            light += s / 32.0;
        }
    }
    let threshold = f32::midpoint(dark, light);
    let agree = shades
        .iter()
        .enumerate()
        .filter(|(i, s)| (**s < threshold) == dark_square(i / 8, i % 8))
        .count();
    Some((agree, dark, light))
}

#[allow(clippy::cast_precision_loss)]
fn frame_profile(profile: &[f32], threshold: f32) -> Option<(i64, i64)> {
    let n = profile.len();
    let lo = profile
        .iter()
        .position(|p| *p > threshold)
        .filter(|i| (*i as f32) < n as f32 * 0.25)?;
    let hi = profile
        .iter()
        .rposition(|p| *p > threshold)
        .filter(|i| (*i as f32) > n as f32 * 0.75)?;
    let (mut a, mut b) = (lo, hi);
    while a + 1 < n && profile[a + 1] > threshold {
        a += 1;
    }
    while b > 0 && profile[b - 1] > threshold {
        b -= 1;
    }
    Some((i64::try_from(a + 1).ok()?, i64::try_from(b).ok()?))
}

/// Candidate boards: the whole image, and frames bounded by long dark border lines.
#[allow(clippy::cast_precision_loss)]
fn candidates(g: &Gray) -> Vec<Frame> {
    let w = i64::try_from(g.width).unwrap_or(i64::MAX);
    let h = i64::try_from(g.height).unwrap_or(i64::MAX);
    let mut rows = vec![0.0f32; g.height];
    let mut cols = vec![0.0f32; g.width];
    for (y, row) in rows.iter_mut().enumerate() {
        for (x, col) in cols.iter_mut().enumerate() {
            if g.at(x, y) < 0.5 {
                *row += 1.0 / g.width as f32;
                *col += 1.0 / g.height as f32;
            }
        }
    }
    let mut out = vec![[0, 0, w, h]];
    for threshold in [0.6, 0.45, 0.35] {
        if let (Some(r), Some(c)) = (
            frame_profile(&rows, threshold),
            frame_profile(&cols, threshold),
        ) && (r.1 - r.0) as f32 > 0.6 * h as f32
            && (c.1 - c.0) as f32 > 0.6 * w as f32
        {
            out.push([c.0, r.0, c.1, r.1]);
        }
    }
    out.extend(line_frames(g));
    out
}

/// Longest run of dark pixels along each row (`vertical` false) or column.
fn runs(g: &Gray, vertical: bool) -> Vec<usize> {
    let (outer, inner) = if vertical {
        (g.width, g.height)
    } else {
        (g.height, g.width)
    };
    (0..outer)
        .map(|o| {
            let (mut best, mut run) = (0, 0);
            for i in 0..inner {
                let v = if vertical { g.at(o, i) } else { g.at(i, o) };
                run = if v < 0.5 { run + 1 } else { 0 };
                best = best.max(run);
            }
            best
        })
        .collect()
}

/// Groups of adjacent long lines, as (first, last) index of each group.
fn line_groups(runs: &[usize], min: usize) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (i, r) in runs.iter().enumerate() {
        if *r < min {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.1 + 1 == i => last.1 = i,
            _ => out.push((i, i)),
        }
    }
    out
}

/// Square frames bounded by two long vertical and two long horizontal border lines, so a board
/// beside coordinates, captions or empty margin is found even when it fills little of the image.
#[allow(clippy::cast_precision_loss)]
fn line_frames(g: &Gray) -> Vec<Frame> {
    let min = (g.width.min(g.height) / 3).max(64);
    let cols = line_groups(&runs(g, true), min);
    let rows = line_groups(&runs(g, false), min);
    if cols.len() > 6 || rows.len() > 6 {
        return vec![];
    }
    let mut out = Vec::new();
    for (i, a) in cols.iter().enumerate() {
        for b in &cols[i + 1..] {
            for (j, c) in rows.iter().enumerate() {
                for d in &rows[j + 1..] {
                    let (x0, x1, y0, y1) = (a.1 + 1, b.0, c.1 + 1, d.0);
                    let (w, h) = ((x1 - x0) as f32, (y1 - y0) as f32);
                    if w >= 96.0 && (0.85..=1.18).contains(&(w / h)) {
                        let f = [x0, y0, x1, y1].map(|v| i64::try_from(v).unwrap_or(i64::MAX));
                        out.push(f);
                    }
                }
            }
        }
    }
    out
}

/// Light minus dark square contrast, from per-square medians (robust to pieces).
fn contrast(g: &Gray, frame: Frame) -> Option<f32> {
    let (mut dark, mut light) = (0.0, 0.0);
    let mut buffer = Vec::new();
    for i in 0..64 {
        let r = cell_rect(g, frame, i / 8, i % 8)?;
        let step = ((r.x1 - r.x0).max(r.y1 - r.y0) / 24).max(1);
        buffer.clear();
        for y in (r.y0..r.y1).step_by(step) {
            for x in (r.x0..r.x1).step_by(step) {
                buffer.push(g.at(x, y));
            }
        }
        let m = median(&mut buffer);
        if dark_square(i / 8, i % 8) {
            dark += m / 32.0;
        } else {
            light += m / 32.0;
        }
    }
    Some(light - dark)
}

/// Nudges each edge to maximise square contrast, compensating for frames and margins.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn refine(g: &Gray, mut frame: Frame) -> Frame {
    let size = frame[2] - frame[0];
    let step = usize::try_from((size / 160).max(1)).unwrap_or(1);
    let reach = (size as f64 * 0.05) as i64;
    let Some(mut best) = contrast(g, frame) else {
        return frame;
    };
    for _ in 0..3 {
        for k in 0..4 {
            let mut candidate = None;
            for d in (-reach..=reach).step_by(step) {
                let mut f = frame;
                f[k] += d;
                if f[2] - f[0] < 64 || f[3] - f[1] < 64 {
                    continue;
                }
                if let Some(s) = contrast(g, f)
                    && s > best + 1e-6
                {
                    best = s;
                    candidate = Some(f);
                }
            }
            if let Some(f) = candidate {
                frame = f;
            }
        }
    }
    frame
}

/// Locates an 8x8 checkered board, or `None` when the image is not a board.
#[allow(clippy::cast_precision_loss)]
pub fn locate(g: &Gray) -> Option<Frame> {
    let mut best: Option<((usize, f32), Frame)> = None;
    for frame in candidates(g) {
        let Some((agree, dark, light)) = checker(g, frame) else {
            continue;
        };
        let key = (agree, light - dark);
        if best
            .as_ref()
            .is_none_or(|(b, _)| key.0 > b.0 || (key.0 == b.0 && key.1 > b.1))
        {
            best = Some((key, frame));
        }
    }
    let ((agree, gap), frame) = best?;
    let (w, h) = ((frame[2] - frame[0]) as f32, (frame[3] - frame[1]) as f32);
    if agree < 56 || gap < 0.05 || w < 96.0 || !(0.85..=1.18).contains(&(w / h)) {
        return None;
    }
    Some(refine(g, frame))
}

/// Triangle-filter weights mapping `len` source pixels onto `CELL` outputs.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn weights(len: usize) -> Vec<(usize, Vec<f32>)> {
    let scale = len as f32 / CELL as f32;
    let support = scale.max(1.0);
    (0..CELL)
        .map(|o| {
            let center = (o as f32 + 0.5) * scale;
            let lo = (center - support).floor().max(0.0) as usize;
            let hi = ((center + support).ceil() as usize).min(len);
            let mut w: Vec<f32> = (lo..hi)
                .map(|x| (1.0 - ((x as f32 - center + 0.5) / support).abs()).max(0.0))
                .collect();
            let total: f32 = w.iter().sum();
            for v in &mut w {
                *v /= total.max(1e-6);
            }
            (lo, w)
        })
        .collect()
}

/// Resampled square contents (6% inner margin), `CELL` x `CELL`, row-major.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub fn cells(g: &Gray, frame: Frame) -> Option<Vec<Vec<f32>>> {
    let mut out = Vec::with_capacity(64);
    for i in 0..64 {
        let rect = cell_rect(g, frame, i / 8, i % 8)?;
        let inset = ((rect.x1 - rect.x0).min(rect.y1 - rect.y0) as f32 * 0.06) as usize;
        let (x0, y0) = (rect.x0 + inset, rect.y0 + inset);
        let (width, h) = (rect.x1 - rect.x0 - 2 * inset, rect.y1 - rect.y0 - 2 * inset);
        let (wx, wy) = (weights(width), weights(h));
        let mut rows = vec![0.0f32; h * CELL];
        for y in 0..h {
            for (ox, (lo, ws)) in wx.iter().enumerate() {
                rows[y * CELL + ox] = ws
                    .iter()
                    .enumerate()
                    .map(|(k, v)| v * g.at(x0 + lo + k, y0 + y))
                    .sum();
            }
        }
        let mut cell = vec![0.0f32; CELL * CELL];
        for (oy, (lo, ws)) in wy.iter().enumerate() {
            for ox in 0..CELL {
                cell[oy * CELL + ox] = ws
                    .iter()
                    .enumerate()
                    .map(|(k, v)| v * rows[(lo + k) * CELL + ox])
                    .sum();
            }
        }
        out.push(cell);
    }
    Some(out)
}

/// Square images of a located board, or `None` when the image is not a board.
pub fn board_cells(bytes: &[u8]) -> Option<Vec<Vec<f32>>> {
    let g = Gray::decode(bytes)?;
    let frame = locate(&g)?;
    cells(&g, frame)
}

/// 2x2 block means (shape) plus 4x4 block deviations (ink texture versus hatching).
pub fn features(cell: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(DIM);
    for by in 0..16 {
        for bx in 0..16 {
            let mut s = 0.0;
            for y in 0..2 {
                for x in 0..2 {
                    s += cell[(by * 2 + y) * CELL + bx * 2 + x];
                }
            }
            out.push(s / 4.0);
        }
    }
    for by in 0..8 {
        for bx in 0..8 {
            let (mut s, mut q) = (0.0, 0.0);
            for y in 0..4 {
                for x in 0..4 {
                    let v = cell[(by * 4 + y) * CELL + bx * 4 + x];
                    s += v;
                    q += v * v;
                }
            }
            let m = s / 16.0;
            out.push(2.5 * (q / 16.0 - m * m).max(0.0).sqrt());
        }
    }
    out
}

/// Per-square features of a located board, or `None` when the image is not a board.
pub fn board_features(bytes: &[u8]) -> Option<Vec<Vec<f32>>> {
    Some(board_cells(bytes)?.iter().map(|c| features(c)).collect())
}

#[derive(Clone, Copy, Debug)]
pub struct Square {
    pub label: u8,
    /// Nearest other label, with `margin` its squared-distance ratio to `label`.
    pub second: u8,
    /// For a king: squared distance to that side's nearest queen over the squared distance.
    pub queen_ratio: f32,
    pub distance: f32,
    pub margin: f32,
}

impl Prototypes {
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let header = bytes.get(..8)?;
        if &header[..4] != MAGIC {
            return None;
        }
        let dim = usize::from(u16::from_le_bytes([header[4], header[5]]));
        let count = usize::from(u16::from_le_bytes([header[6], header[7]]));
        let body = bytes.get(8..)?;
        if dim != DIM || body.len() != count * (dim + 2) {
            return None;
        }
        let mut p = Self {
            dim,
            labels: Vec::with_capacity(count),
            dark: Vec::with_capacity(count),
            values: Vec::with_capacity(count * dim),
        };
        for record in body.chunks_exact(dim + 2) {
            if !LABELS.as_bytes().contains(&record[0]) {
                return None;
            }
            p.labels.push(record[0]);
            p.dark.push(record[1] == 1);
            p.values
                .extend(record[2..].iter().map(|v| f32::from(*v) / SCALE));
        }
        Some(p)
    }
    /// Serialises prototypes as (label, dark square, quantised features) records.
    pub fn encode(records: &[(u8, bool, Vec<f32>)]) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend(u16::try_from(DIM).unwrap_or(0).to_le_bytes());
        out.extend(u16::try_from(records.len()).unwrap_or(0).to_le_bytes());
        for (label, dark, values) in records {
            out.push(*label);
            out.push(u8::from(*dark));
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            out.extend(
                values
                    .iter()
                    .map(|v| (v * SCALE).round().clamp(0.0, 255.0) as u8),
            );
        }
        out
    }
    pub fn len(&self) -> usize {
        self.labels.len()
    }
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }
    /// Nearest prototype of the same square colour, with the distance ratio to the nearest
    /// prototype of any other label.
    pub fn classify(&self, f: &[f32], dark: bool) -> Option<Square> {
        let mut best: Option<(f32, u8)> = None;
        let mut distances = Vec::with_capacity(self.labels.len());
        for (i, values) in self.values.chunks_exact(self.dim).enumerate() {
            if self.dark[i] != dark {
                continue;
            }
            let d: f32 = values.iter().zip(f).map(|(a, b)| (a - b) * (a - b)).sum();
            distances.push((d, self.labels[i]));
            if best.is_none_or(|(b, _)| d < b) {
                best = Some((d, self.labels[i]));
            }
        }
        let (d1, label) = best?;
        let (other, second) = distances.iter().filter(|(_, l)| *l != label).fold(
            (f32::INFINITY, b'.'),
            |a, (d, l)| if *d < a.0 { (*d, *l) } else { a },
        );
        let queen = match label {
            b'K' => Some(b'Q'),
            b'k' => Some(b'q'),
            _ => None,
        };
        let queen_ratio = queen.map_or(f32::INFINITY, |q| {
            distances
                .iter()
                .filter(|(_, l)| *l == q)
                .map(|(d, _)| *d)
                .fold(f32::INFINITY, f32::min)
                / (d1 + 1e-6)
        });
        Some(Square {
            label,
            second,
            queen_ratio,
            distance: d1.sqrt(),
            margin: other / (d1 + 1e-6),
        })
    }
    /// Classifies all 64 squares, row 0 = rank 8.
    pub fn squares(&self, bytes: &[u8]) -> Option<Vec<Square>> {
        board_features(bytes)?
            .iter()
            .enumerate()
            .map(|(i, f)| self.classify(f, dark_square(i / 8, i % 8)))
            .collect()
    }
}

/// Gates calibrated on held-out book diagrams: unseen print styles are far from every
/// prototype, so the whole board abstains instead of guessing.
pub const MAX_MEDIAN_DISTANCE: f32 = 1.0;
pub const MAX_DISTANCE: f32 = 3.0;
pub const MIN_MEDIAN_MARGIN: f32 = 3.0;
pub const MIN_MARGIN: f32 = 1.2;

/// Engraved kings and queens differ mainly in the crown. When a side shows two kings, the one
/// relatively closer to that side's queen becomes the queen, provided it is clearly the closer
/// one. That separation becomes the square's margin and must pass the same margin gates as any
/// other square, so a doubtful choice keeps the whole board abstaining.
fn one_king_each(squares: &mut [Square]) {
    for (king, queen) in [(b'K', b'Q'), (b'k', b'q')] {
        let kings: Vec<usize> = (0..squares.len())
            .filter(|i| squares[*i].label == king)
            .collect();
        let [a, b] = kings[..] else {
            continue;
        };
        let (swap, keep) = if squares[a].queen_ratio < squares[b].queen_ratio {
            (a, b)
        } else {
            (b, a)
        };
        let separation = squares[keep].queen_ratio / squares[swap].queen_ratio.max(1e-6);
        let s = &mut squares[swap];
        s.distance *= s.queen_ratio.sqrt();
        s.label = queen;
        s.second = king;
        s.margin = separation;
    }
}

/// FEN placement from classified squares, if the style and material checks pass.
pub fn placement(squares: &[Square]) -> Option<String> {
    if squares.len() != 64 {
        return None;
    }
    let mut squares = squares.to_vec();
    one_king_each(&mut squares);
    let squares = &squares[..];
    let mut distances: Vec<f32> = squares.iter().map(|s| s.distance).collect();
    let mut margins: Vec<f32> = squares.iter().map(|s| s.margin).collect();
    if median(&mut distances) > MAX_MEDIAN_DISTANCE
        || squares.iter().any(|s| s.distance > MAX_DISTANCE)
        || median(&mut margins) < MIN_MEDIAN_MARGIN
        || squares.iter().any(|s| s.margin < MIN_MARGIN)
    {
        return None;
    }
    let labels: Vec<u8> = squares.iter().map(|s| s.label).collect();
    placement_of(&labels)
}

/// FEN placement of 64 square labels (row 0 = rank 8, `b'.'` empty) when the material is
/// plausible: one king each, at most eight pawns a side, none on a back rank, 32 pieces.
pub fn placement_of(labels: &[u8]) -> Option<String> {
    if labels.len() != 64 || !labels.iter().all(|l| LABELS.as_bytes().contains(l)) {
        return None;
    }
    let mut counts = [0usize; 256];
    for l in labels {
        counts[usize::from(*l)] += 1;
    }
    let count = |c: u8| counts[usize::from(c)];
    let pieces = labels.iter().filter(|l| **l != b'.').count();
    let back_rank_pawn = labels
        .iter()
        .enumerate()
        .any(|(i, l)| !(8..56).contains(&i) && matches!(l, b'P' | b'p'));
    if count(b'K') != 1
        || count(b'k') != 1
        || count(b'P') > 8
        || count(b'p') > 8
        || pieces > 32
        || back_rank_pawn
    {
        return None;
    }
    let mut rows = Vec::with_capacity(8);
    for rank in labels.chunks(8) {
        let mut row = String::new();
        let mut empty = 0u8;
        for l in rank {
            if *l == b'.' {
                empty += 1;
            } else {
                if empty > 0 {
                    row.push(char::from(b'0' + empty));
                    empty = 0;
                }
                row.push(char::from(*l));
            }
        }
        if empty > 0 {
            row.push(char::from(b'0' + empty));
        }
        rows.push(row);
    }
    Some(rows.join("/"))
}

impl DiagramRecognizer for Prototypes {
    fn placement(&self, image: &[u8]) -> Option<String> {
        placement(&self.squares(image)?)
    }
    fn reading(&self, image: &[u8]) -> Option<String> {
        Some(
            self.squares(image)?
                .iter()
                .map(|s| char::from(s.label))
                .collect(),
        )
    }
}

/// Two recognizers that must not contradict each other: a placement is returned when one
/// abstains and the other reads the board, or when both agree; any disagreement abstains.
/// The ungated reading used for confirmation is the second recognizer's, or the first's when
/// the second has none, so adding a recognizer in front never changes existing confirmations.
pub struct Agreeing<'a> {
    pub first: &'a dyn DiagramRecognizer,
    pub second: &'a dyn DiagramRecognizer,
}

impl DiagramRecognizer for Agreeing<'_> {
    fn placement(&self, image: &[u8]) -> Option<String> {
        match (self.first.placement(image), self.second.placement(image)) {
            (Some(a), Some(b)) => (a == b).then_some(a),
            (a, b) => a.or(b),
        }
    }
    fn reading(&self, image: &[u8]) -> Option<String> {
        self.second
            .reading(image)
            .or_else(|| self.first.reading(image))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(Option<&'static str>);
    impl DiagramRecognizer for Fixed {
        fn placement(&self, _: &[u8]) -> Option<String> {
            self.0.map(str::to_owned)
        }
        fn reading(&self, _: &[u8]) -> Option<String> {
            self.0.map(|p| p.replace('/', ""))
        }
    }

    #[test]
    fn placement_of_checks_material() {
        let mut labels = [b'.'; 64];
        labels[4] = b'k';
        labels[60] = b'K';
        labels[52] = b'P';
        assert_eq!(
            placement_of(&labels).as_deref(),
            Some("4k3/8/8/8/8/8/4P3/4K3")
        );
        labels[59] = b'K';
        assert_eq!(placement_of(&labels), None, "two white kings");
        labels[59] = b'P';
        assert_eq!(placement_of(&labels), None, "back-rank pawn");
        assert_eq!(placement_of(&labels[..63]), None);
    }

    fn both(first: &dyn DiagramRecognizer, second: &dyn DiagramRecognizer) -> Option<String> {
        Agreeing { first, second }.placement(&[])
    }

    fn read(first: &dyn DiagramRecognizer, second: &dyn DiagramRecognizer) -> Option<String> {
        Agreeing { first, second }.reading(&[])
    }

    #[test]
    fn agreeing_recognizers_abstain_on_conflict() {
        let a = Fixed(Some("8/8/8/8/8/8/8/K6k"));
        let b = Fixed(Some("8/8/8/8/8/8/8/k6K"));
        let none = Fixed(None);
        assert_eq!(both(&a, &a).as_deref(), Some("8/8/8/8/8/8/8/K6k"));
        assert_eq!(both(&a, &b), None);
        assert_eq!(both(&none, &b).as_deref(), Some("8/8/8/8/8/8/8/k6K"));
        assert_eq!(both(&a, &none).as_deref(), Some("8/8/8/8/8/8/8/K6k"));
        assert_eq!(both(&none, &none), None);
        assert_eq!(
            read(&a, &b).as_deref(),
            Some("8888888k6K"),
            "second reading first"
        );
        assert_eq!(read(&a, &none).as_deref(), Some("8888888K6k"));
    }
}
