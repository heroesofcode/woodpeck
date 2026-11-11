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
    /// An 8-bit gray raster, row-major.
    pub fn from_luma(width: usize, height: usize, luma: &[u8]) -> Option<Self> {
        (luma.len() == width * height && width > 0 && height > 0).then(|| Self {
            width,
            height,
            px: luma.iter().map(|v| f32::from(*v) / 255.0).collect(),
        })
    }
    /// The pixels inside `frame`, clamped to the image.
    pub fn crop(&self, frame: Frame) -> Option<Self> {
        let clamp = |v: i64, max: usize| usize::try_from(v.max(0)).unwrap_or(0).min(max);
        let (x0, y0) = (clamp(frame[0], self.width), clamp(frame[1], self.height));
        let (x1, y1) = (clamp(frame[2], self.width), clamp(frame[3], self.height));
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let mut px = Vec::with_capacity((x1 - x0) * (y1 - y0));
        for y in y0..y1 {
            px.extend_from_slice(&self.px[y * self.width + x0..y * self.width + x1]);
        }
        Some(Self {
            width: x1 - x0,
            height: y1 - y0,
            px,
        })
    }
    /// 8-bit gray bytes, row-major.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn luma(&self) -> Vec<u8> {
        self.px
            .iter()
            .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect()
    }
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

    /// Downscale so `max(width, height) <= max_side`. Larger `ClearScan` page crops alias
    /// hatched dark squares when resampled to [`CELL`]; the nets were trained near this
    /// side length. Unchanged when already small enough.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    pub fn fit_max_side(&self, max_side: usize) -> Self {
        let long = self.width.max(self.height);
        if long <= max_side || max_side < 64 {
            return Self {
                width: self.width,
                height: self.height,
                px: self.px.clone(),
            };
        }
        let scale = max_side as f32 / long as f32;
        let nw = ((self.width as f32) * scale).round().max(1.0) as usize;
        let nh = ((self.height as f32) * scale).round().max(1.0) as usize;
        let mut px = vec![0.0f32; nw * nh];
        // Area average: each output pixel is the mean of its source rectangle.
        for y in 0..nh {
            let y0 = (y as f32) / scale;
            let y1 = ((y + 1) as f32) / scale;
            let iy0 = y0.floor().max(0.0) as usize;
            let iy1 = y1.ceil().min(self.height as f32) as usize;
            for x in 0..nw {
                let x0 = (x as f32) / scale;
                let x1 = ((x + 1) as f32) / scale;
                let ix0 = x0.floor().max(0.0) as usize;
                let ix1 = x1.ceil().min(self.width as f32) as usize;
                let mut sum = 0.0;
                let mut wsum = 0.0;
                for sy in iy0..iy1.max(iy0 + 1).min(self.height) {
                    let top = sy as f32;
                    let overlap_y = (y1.min(top + 1.0) - y0.max(top)).max(0.0);
                    if overlap_y <= 0.0 {
                        continue;
                    }
                    for sx in ix0..ix1.max(ix0 + 1).min(self.width) {
                        let left = sx as f32;
                        let overlap_x = (x1.min(left + 1.0) - x0.max(left)).max(0.0);
                        if overlap_x <= 0.0 {
                            continue;
                        }
                        let w = overlap_x * overlap_y;
                        sum += self.px[sy * self.width + sx] * w;
                        wsum += w;
                    }
                }
                px[y * nw + x] = if wsum > 0.0 { sum / wsum } else { 0.0 };
            }
        }
        Self {
            width: nw,
            height: nh,
            px,
        }
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

/// Insets past a thick uniform border (gold/brown frames, coordinate bands inside the rim).
/// Fallback candidates for [`locate_with_light_rim`] (embedded diagrams with a bright rim).
/// Not mixed into [`candidates`]: `page_boards` must keep the dark-line-only [`locate`].
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn inset_frames(g: &Gray) -> Vec<Frame> {
    let side = g.width.min(g.height);
    if side < 96 {
        return vec![];
    }
    let mut out = Vec::new();
    // 4%..18% of the short side, in a few steps — covers ~12–56px on a 312px diagram.
    for tenth in [4, 6, 8, 10, 12, 14, 16, 18] {
        let m = (side * tenth / 100).max(4);
        if g.width <= 2 * m + 96 || g.height <= 2 * m + 96 {
            continue;
        }
        let (m, w, h) = (
            i64::try_from(m).unwrap_or(0),
            i64::try_from(g.width).unwrap_or(0),
            i64::try_from(g.height).unwrap_or(0),
        );
        out.push([m, m, w - m, h - m]);
    }
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
    locate_among(g, &candidates(g))
}

/// Like [`locate`], then light-rim insets if the dark-line path finds nothing.
/// For single embedded diagrams (epub gold frames). Not used by [`page_boards`]: an inset hit on
/// an early square crop there can mask a later good crop and drop boards (N15).
fn locate_with_light_rim(g: &Gray) -> Option<Frame> {
    locate(g).or_else(|| locate_among(g, &inset_frames(g)))
}

fn locate_among(g: &Gray, frames: &[Frame]) -> Option<Frame> {
    let mut best: Option<((usize, f32), Frame)> = None;
    let mut boards: Vec<Frame> = Vec::new();
    for &frame in frames {
        let Some((agree, dark, light)) = checker(g, frame) else {
            continue;
        };
        if agree >= 56 && light - dark >= 0.05 {
            boards.push(frame);
        }
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
    // A page of several boards is not one diagram: which board the text means is unknown.
    let apart =
        |f: &Frame| f[0] >= frame[2] || f[2] <= frame[0] || f[1] >= frame[3] || f[3] <= frame[1];
    if boards.iter().any(apart) {
        return None;
    }
    Some(refine(g, frame))
}

/// PNG text keyword carrying a placement read from a diagram font, not from pixels.
const PLACEMENT_KEY: &[u8] = b"woodpeck-placement";

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for b in bytes {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A PNG with the FEN placement stored beside the pixels (a text chunk after the header).
pub fn with_placement(png: &[u8], placement: &str) -> Vec<u8> {
    // Signature (8) + IHDR chunk (4 length + 4 type + 13 data + 4 CRC).
    const HEADER: usize = 8 + 25;
    if png.len() < HEADER || !png.starts_with(b"\x89PNG\r\n\x1a\n") || &png[12..16] != b"IHDR" {
        return png.to_vec();
    }
    let mut data = PLACEMENT_KEY.to_vec();
    data.push(0);
    data.extend_from_slice(placement.as_bytes());
    let mut chunk = u32::try_from(data.len())
        .unwrap_or(0)
        .to_be_bytes()
        .to_vec();
    let mut body = b"tEXt".to_vec();
    body.extend_from_slice(&data);
    chunk.extend_from_slice(&body);
    chunk.extend_from_slice(&crc32(&body).to_be_bytes());
    let mut out = png[..HEADER].to_vec();
    out.extend_from_slice(&chunk);
    out.extend_from_slice(&png[HEADER..]);
    out
}

/// The placement stored by [`with_placement`], if the image carries one.
pub fn stated_placement(bytes: &[u8]) -> Option<String> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    let mut at = 8;
    while at + 12 <= bytes.len() {
        let len = usize::try_from(u32::from_be_bytes(bytes[at..at + 4].try_into().ok()?)).ok()?;
        let kind = &bytes[at + 4..at + 8];
        let data = bytes.get(at + 8..at + 8 + len)?;
        if kind == b"tEXt" {
            if let Some(text) = data
                .strip_prefix(PLACEMENT_KEY)
                .and_then(|d| d.strip_prefix(&[0]))
            {
                let text = std::str::from_utf8(text).ok()?;
                return (text.split('/').count() == 8
                    && text.chars().all(|c| {
                        c == '/' || ('1'..='8').contains(&c) || "PNBRQKpnbrqk".contains(c)
                    }))
                .then(|| text.to_owned());
            }
        } else if kind == b"IDAT" {
            return None;
        }
        at += 12 + len;
    }
    None
}

/// Key of the PNG text chunk holding the side to move a page marker shows.
const SIDE_KEY: &[u8] = b"woodpeck-side";

/// A PNG with the side to move stored beside the pixels, as [`with_placement`] does.
pub fn with_side(png: &[u8], white: bool) -> Vec<u8> {
    with_text(png, SIDE_KEY, if white { "w" } else { "b" })
}

/// The side stored by [`with_side`]: `Some(true)` for White to move.
pub fn stated_side(bytes: &[u8]) -> Option<bool> {
    match text_chunk(bytes, SIDE_KEY)?.as_slice() {
        b"w" => Some(true),
        b"b" => Some(false),
        _ => None,
    }
}

/// Key of the PNG text chunk holding a side read from a printed caption beside one diagram.
const CAPTION_SIDE_KEY: &[u8] = b"woodpeck-caption-side";

/// A PNG with a caption side stored beside the pixels. This is not a printed marker: the
/// resolver may use it only while the side is still unknown.
pub fn with_caption_side(png: &[u8], white: bool) -> Vec<u8> {
    with_text(png, CAPTION_SIDE_KEY, if white { "w" } else { "b" })
}

/// The side stored by [`with_caption_side`]: `Some(true)` for White to move.
pub fn stated_caption_side(bytes: &[u8]) -> Option<bool> {
    match text_chunk(bytes, CAPTION_SIDE_KEY)?.as_slice() {
        b"w" => Some(true),
        b"b" => Some(false),
        _ => None,
    }
}

/// Key of the PNG text chunk holding a side read from a numbered solution.
const KEY_SIDE_KEY: &[u8] = b"woodpeck-key-side";

/// A PNG with a side taken from a numbered answer key. Weaker than a caption or a printed
/// mark: the resolver may use it only while every stronger signal left the side unknown.
pub fn with_key_side(png: &[u8], white: bool) -> Vec<u8> {
    with_text(png, KEY_SIDE_KEY, if white { "w" } else { "b" })
}

/// The side stored by [`with_key_side`]: `Some(true)` for White to move.
pub fn stated_key_side(bytes: &[u8]) -> Option<bool> {
    match text_chunk(bytes, KEY_SIDE_KEY)?.as_slice() {
        b"w" => Some(true),
        b"b" => Some(false),
        _ => None,
    }
}

/// Key of the PNG text chunk holding a side read from an OCR turn caption on a scan.
const SCAN_SIDE_KEY: &[u8] = b"woodpeck-scan-side";

/// A PNG with a side taken from an explicit turn caption in the page pixels.
/// Weaker than a numbered solution: the resolver may use it only while every stronger signal
/// left the side unknown.
pub fn with_scan_side(png: &[u8], white: bool) -> Vec<u8> {
    with_text(png, SCAN_SIDE_KEY, if white { "w" } else { "b" })
}

/// The side stored by [`with_scan_side`]: `Some(true)` for White to move.
pub fn stated_scan_side(bytes: &[u8]) -> Option<bool> {
    match text_chunk(bytes, SCAN_SIDE_KEY)?.as_slice() {
        b"w" => Some(true),
        b"b" => Some(false),
        _ => None,
    }
}

/// Key of the PNG text chunk holding the first move printed under a scanned board.
const SCAN_MOVE_KEY: &[u8] = b"woodpeck-scan-move";

/// A PNG with the mover and the destination (`e4`, `O-O` or `O-O-O`) of the move printed right
/// under the board. Weakest pixel signal: the resolver uses it only while everything else is
/// silent, and only when that side can play to that destination.
pub fn with_scan_move(png: &[u8], white: bool, destination: &str) -> Vec<u8> {
    let value = format!("{}{destination}", if white { 'w' } else { 'b' });
    with_text(png, SCAN_MOVE_KEY, &value)
}

/// The move stored by [`with_scan_move`]: `(true, "e4")` when White plays to e4.
pub fn stated_scan_move(bytes: &[u8]) -> Option<(bool, String)> {
    let text = String::from_utf8(text_chunk(bytes, SCAN_MOVE_KEY)?).ok()?;
    let white = match text.as_bytes().first()? {
        b'w' => true,
        b'b' => false,
        _ => return None,
    };
    let destination = &text[1..];
    let square = destination.len() == 2
        && matches!(destination.as_bytes()[0], b'a'..=b'h')
        && matches!(destination.as_bytes()[1], b'1'..=b'8');
    (square || destination == "O-O" || destination == "O-O-O")
        .then(|| (white, destination.to_owned()))
}

/// Key of the PNG text chunk holding placements reached by game text replayed on a scan.
const SCAN_REPLAY_KEY: &[u8] = b"woodpeck-scan-replay";

/// A PNG with the placements (first FEN field) that the page's game text reaches when replayed
/// from move 1, each with its side to move. Weaker than every other pixel signal: the resolver
/// uses one only when the board's whole placement equals it and nothing else named a side.
pub fn with_scan_replay(png: &[u8], replayed: &[(String, bool)]) -> Vec<u8> {
    let value: Vec<String> = replayed
        .iter()
        .map(|(placement, white)| format!("{placement} {}", if *white { 'w' } else { 'b' }))
        .collect();
    with_text(png, SCAN_REPLAY_KEY, &value.join(";"))
}

/// The placements stored by [`with_scan_replay`], each with `true` for White to move. A
/// malformed entry drops the whole list.
pub fn stated_scan_replay(bytes: &[u8]) -> Option<Vec<(String, bool)>> {
    let text = String::from_utf8(text_chunk(bytes, SCAN_REPLAY_KEY)?).ok()?;
    text.split(';')
        .map(|entry| {
            let (placement, side) = entry.split_once(' ')?;
            let white = match side {
                "w" => true,
                "b" => false,
                _ => return None,
            };
            Some((placement.to_owned(), white))
        })
        .collect()
}

fn with_text(png: &[u8], key: &[u8], value: &str) -> Vec<u8> {
    const HEADER: usize = 8 + 25;
    if png.len() < HEADER || !png.starts_with(b"\x89PNG\r\n\x1a\n") || &png[12..16] != b"IHDR" {
        return png.to_vec();
    }
    let mut body = b"tEXt".to_vec();
    body.extend_from_slice(key);
    body.push(0);
    body.extend_from_slice(value.as_bytes());
    let mut out = png[..HEADER].to_vec();
    out.extend_from_slice(&u32::try_from(body.len() - 4).unwrap_or(0).to_be_bytes());
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
    out.extend_from_slice(&png[HEADER..]);
    out
}

fn text_chunk(bytes: &[u8], key: &[u8]) -> Option<Vec<u8>> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    let mut at = 8;
    while at + 12 <= bytes.len() {
        let len = usize::try_from(u32::from_be_bytes(bytes[at..at + 4].try_into().ok()?)).ok()?;
        let kind = &bytes[at + 4..at + 8];
        let data = bytes.get(at + 8..at + 8 + len)?;
        if kind == b"tEXt" {
            if let Some(v) = data.strip_prefix(key).and_then(|d| d.strip_prefix(&[0])) {
                return Some(v.to_vec());
            }
        } else if kind == b"IDAT" {
            return None;
        }
        at += 12 + len;
    }
    None
}

/// The side-to-move marker some books print beside a diagram: a small square level with the
/// bottom rank, just right of the board, hollow for White and filled for Black. `Some(true)` is
/// White. Only one clear square counts; anything doubtful abstains.
/// Squares on the other three corners, and in the caption line, are read by [`side_marks`].
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::many_single_char_names
)]
pub fn side_marker(g: &Gray, frame: Frame) -> Option<bool> {
    let s = (frame[2] - frame[0]).min(frame[3] - frame[1]) as f64;
    if s < 80.0 {
        return None;
    }
    let at = |v: f64| v.round() as i64;
    let x0 = (frame[2] + at(0.02 * s)).max(0);
    let x1 = (frame[2] + at(0.4 * s)).min(g.width as i64);
    let y0 = (frame[3] - at(0.25 * s)).max(0);
    let y1 = (frame[3] + at(0.05 * s)).min(g.height as i64);
    if x1 - x0 < at(0.1 * s) || y1 - y0 < at(0.1 * s) {
        return None;
    }
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let dark = |x: usize, y: usize| g.px[(y0 as usize + y) * g.width + x0 as usize + x] < 0.5;
    let mut seen = vec![false; w * h];
    let mut found = None;
    for start in 0..w * h {
        if seen[start] || !dark(start % w, start / w) {
            continue;
        }
        // One connected blob of ink, its bounding box, and whether it reaches the region edge.
        let mut stack = vec![start];
        seen[start] = true;
        let (mut bx0, mut by0, mut bx1, mut by1) = (w, h, 0, 0);
        let mut edge = false;
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            (bx0, by0, bx1, by1) = (bx0.min(x), by0.min(y), bx1.max(x + 1), by1.max(y + 1));
            edge |= x == 0 || y == 0 || x + 1 == w || y + 1 == h;
            let around = [
                (x.wrapping_sub(1), y),
                (x + 1, y),
                (x, y.wrapping_sub(1)),
                (x, y + 1),
            ];
            for (nx, ny) in around {
                if nx < w && ny < h && !seen[ny * w + nx] && dark(nx, ny) {
                    seen[ny * w + nx] = true;
                    stack.push(ny * w + nx);
                }
            }
        }
        let (bw, bh) = ((bx1 - bx0) as f64, (by1 - by0) as f64);
        if bw.max(bh) < 0.04 * s || edge {
            continue;
        }
        // Anything larger than a marker (a caption line, a second board) makes it doubtful.
        if bw > 0.13 * s || bh > 0.13 * s || (bw - bh).abs() > 0.25 * bw.max(bh) {
            if bw.min(bh) >= 0.04 * s {
                return None;
            }
            continue;
        }
        let share = |xa: usize, ya: usize, xb: usize, yb: usize| {
            let n = (xb.saturating_sub(xa) * yb.saturating_sub(ya)).max(1);
            let d = (ya..yb)
                .flat_map(|y| (xa..xb).map(move |x| (x, y)))
                .filter(|&(x, y)| dark(x, y))
                .count();
            d as f64 / n as f64
        };
        let (iw, ih) = (bx1 - bx0, by1 - by0);
        let (ix, iy) = (iw * 3 / 10, ih * 3 / 10);
        let inner = share(bx0 + ix, by0 + iy, bx1 - ix, by1 - iy);
        let full = share(bx0, by0, bx1, by1);
        // Each side of a hollow square is one straight inked line.
        let (bx, by) = ((iw / 5).max(1), (ih / 5).max(1));
        let line = |rows: bool, from: usize, to: usize| {
            (from..to)
                .map(|k| {
                    if rows {
                        share(bx0, k, bx1, k + 1)
                    } else {
                        share(k, by0, k + 1, by1)
                    }
                })
                .fold(0.0, f64::max)
        };
        let sides = [
            line(true, by0, by0 + by),
            line(true, by1 - by, by1),
            line(false, bx0, bx0 + bx),
            line(false, bx1 - bx, bx1),
        ];
        let side = if full > 0.85 {
            false
        } else if inner < 0.1 && sides.iter().all(|v| *v > 0.8) {
            true
        } else {
            return None;
        };
        if found.is_some() {
            return None;
        }
        found = Some(side);
    }
    found
}

/// Quality Chess triangle beside the board: outline ▲ by the bottom rank for White, filled ▼
/// by the top rank for Black. Used only when [`side_marker`] abstains. `Some(true)` is White.
/// Anything doubtful (two markers, wrong place, not a triangle) abstains. Triangles on any rank
/// to the right of the board, or in the caption line, are read by [`triangle_marks`].
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::many_single_char_names
)]
pub fn triangle_marker(g: &Gray, frame: Frame) -> Option<bool> {
    let s = (frame[2] - frame[0]).min(frame[3] - frame[1]) as f64;
    if s < 80.0 {
        return None;
    }
    let at = |v: f64| v.round() as i64;
    // A narrow band just right of the board, padded a little past the top and bottom ranks.
    let x0 = frame[2].max(0);
    let x1 = (frame[2] + at(0.22 * s)).min(g.width as i64);
    let y0 = (frame[1] - at(0.05 * s)).max(0);
    let y1 = (frame[3] + at(0.05 * s)).min(g.height as i64);
    if x1 - x0 < at(0.08 * s) || y1 - y0 < at(0.5 * s) {
        return None;
    }
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let dark = |x: usize, y: usize| g.px[(y0 as usize + y) * g.width + x0 as usize + x] < 0.55;
    let mut seen = vec![false; w * h];
    let mut white = 0usize;
    let mut black = 0usize;
    for start in 0..w * h {
        if seen[start] || !dark(start % w, start / w) {
            continue;
        }
        let mut stack = vec![start];
        seen[start] = true;
        let (mut bx0, mut by0, mut bx1, mut by1) = (w, h, 0usize, 0usize);
        let mut area = 0usize;
        let mut touch_right = false;
        let mut touch_top = false;
        let mut touch_bot = false;
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            area += 1;
            (bx0, by0, bx1, by1) = (bx0.min(x), by0.min(y), bx1.max(x + 1), by1.max(y + 1));
            touch_right |= x + 1 == w;
            touch_top |= y == 0;
            touch_bot |= y + 1 == h;
            for (nx, ny) in [
                (x.wrapping_sub(1), y),
                (x + 1, y),
                (x, y.wrapping_sub(1)),
                (x, y + 1),
                (x.wrapping_sub(1), y.wrapping_sub(1)),
                (x + 1, y.wrapping_sub(1)),
                (x.wrapping_sub(1), y + 1),
                (x + 1, y + 1),
            ] {
                if nx < w && ny < h && !seen[ny * w + nx] && dark(nx, ny) {
                    seen[ny * w + nx] = true;
                    stack.push(ny * w + nx);
                }
            }
        }
        let (bw, bh) = ((bx1 - bx0) as f64, (by1 - by0) as f64);
        let max_side = bw.max(bh);
        // Marker size is a small fraction of the board; reject noise, captions, and
        // rank/file digits (taller than wide, often ~0.05 of the side).
        if max_side < 0.055 * s || max_side > 0.12 * s || bw.min(bh) < 0.045 * s {
            continue;
        }
        if (bw - bh).abs() > 0.28 * max_side {
            continue;
        }
        // Circled exercise numbers and page junk sit on the far edge of the band.
        if touch_right || touch_top || touch_bot {
            continue;
        }
        let fill = area as f64 / (bw * bh).max(1.0);
        let cy_page = y0 as f64 + (by0 + by1) as f64 / 2.0;
        let y_rel = (cy_page - frame[1] as f64) / s;
        let third = ((by1 - by0) / 3).max(1);
        let mut top_w = 0.0;
        let mut bot_w = 0.0;
        for y in by0..by0 + third {
            for x in bx0..bx1 {
                if dark(x, y) {
                    top_w += 1.0;
                }
            }
        }
        for y in by1 - third..by1 {
            for x in bx0..bx1 {
                if dark(x, y) {
                    bot_w += 1.0;
                }
            }
        }
        let apex_up = top_w + 0.5 < bot_w;
        let apex_down = bot_w + 0.5 < top_w;
        // Hollow ▲ by the bottom rank → White; filled ▼ by the top rank → Black.
        if fill < 0.38 && apex_up && y_rel > 0.70 {
            white += 1;
        } else if fill > 0.48 && apex_down && y_rel < 0.30 {
            black += 1;
        }
    }
    match (white, black) {
        (1, 0) => Some(true),
        (0, 1) => Some(false),
        _ => None,
    }
}

/// Printed side squares on every side of a board, for pages where [`side_marker`] abstains.
mod side_square {
    #![allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]

    use super::{Frame, Gray, locate_with_light_rim};

    /// PNG text chunk for a side square that is not the bottom-right marker.
    const KEY: &[u8] = b"woodpeck-mark-side";
    /// Ink at or below this luma joins a mark. Gray fills (about 0.5) stay connected to the outline.
    const INK: f32 = 0.60;
    /// Interior range at or above this is a glyph (a letter, a rank "8"), not a flat printed square.
    const FLAT: f32 = 0.40;
    /// Corner bands accept a square down to this fraction of the board. Caption text sits a little larger.
    const MIN_CORNER: f64 = 0.03;
    const MIN_CAPTION: f64 = 0.04;
    const MAX_MARK: f64 = 0.13;

    /// A PNG with a side read from a square off the bottom-right corner.
    /// The resolver may use it only while the side to move is still unknown.
    pub fn with_mark_side(png: &[u8], white: bool) -> Vec<u8> {
        super::with_text(png, KEY, if white { "w" } else { "b" })
    }

    /// The side stored by [`with_mark_side`]: `Some(true)` for White to move.
    pub fn stated_mark_side(bytes: &[u8]) -> Option<bool> {
        match super::text_chunk(bytes, KEY)?.as_slice() {
            b"w" => Some(true),
            b"b" => Some(false),
            _ => None,
        }
    }

    /// When `bytes` is one diagram and a side square sits off the bottom-right corner, return
    /// the image with that side stored beside the pixels. `None` leaves the bytes unchanged.
    /// A side already stored by [`super::with_side`] is not replaced.
    pub(crate) fn attach_printed_mark(bytes: &[u8]) -> Option<Vec<u8>> {
        if super::stated_side(bytes).is_some() || stated_mark_side(bytes).is_some() {
            return None;
        }
        let gray = Gray::decode(bytes)?;
        let frame = locate_with_light_rim(&gray)?;
        let white = side_marks(&gray, &[frame]).pop().flatten()?;
        // Keep the source pixels. A JPEG (or other) embed is re-encoded as PNG only so the
        // side can sit in a text chunk; the colours are not reduced to gray.
        let png = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            bytes.to_vec()
        } else {
            let image = image::load_from_memory(bytes).ok()?;
            let mut png = Vec::new();
            image
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .ok()?;
            png
        };
        Some(with_mark_side(&png, white))
    }

    #[derive(Clone, Copy)]
    struct Mark {
        white: bool,
        x0: i64,
        y0: i64,
        x1: i64,
        y1: i64,
    }

    struct Survey {
        /// Every square found, including ones that made the reading doubtful.
        marks: Vec<Mark>,
        side: Option<bool>,
    }

    /// One reading per frame. `Some(true)` is White to move. `None` abstains.
    ///
    /// Looks in five bands: the four corners just outside the board, and the caption line
    /// directly under it. A hollow square is White; a filled or gray-filled square is Black.
    /// A thin arrow attached to a square, or drawn beside it, is not another mark. The inside
    /// of an accepted square is one flat tone — a varying interior is a glyph, not a mark.
    ///
    /// Abstains when one band holds two marks, two bands name different sides, a mark is shared
    /// by two boards, or a mark is about as close to another board as to its own. Callers must
    /// apply a hit only while the side is still unknown.
    pub fn side_marks(g: &Gray, frames: &[Frame]) -> Vec<Option<bool>> {
        let surveys: Vec<Survey> = frames
            .iter()
            .copied()
            .map(|frame| survey(g, frame))
            .collect();
        let n = surveys.len();
        let mut drop = vec![false; n];
        for i in 0..n {
            for j in (i + 1)..n {
                let shared = surveys[i]
                    .marks
                    .iter()
                    .any(|a| surveys[j].marks.iter().any(|b| boxes_overlap(a, b)));
                if shared {
                    drop[i] = true;
                    drop[j] = true;
                }
            }
        }
        for (i, survey) in surveys.iter().enumerate() {
            if drop[i] {
                continue;
            }
            if survey.marks.iter().any(|mark| contested(frames, i, mark)) {
                drop[i] = true;
            }
        }
        surveys
            .into_iter()
            .enumerate()
            .map(|(i, survey)| if drop[i] { None } else { survey.side })
            .collect()
    }

    fn survey(g: &Gray, frame: Frame) -> Survey {
        let s = board_side(frame);
        if s < 80.0 {
            return Survey {
                marks: Vec::new(),
                side: None,
            };
        }
        let mut marks = Vec::new();
        let mut doubt = false;
        for (window, caption) in bands(frame) {
            let hits = scan_band(g, window, s, caption);
            if let Ok(clear) = merge_overlaps(hits.clone(), true) {
                marks.extend(clear);
            } else {
                doubt = true;
                marks.extend(hits);
            }
        }
        let side = if doubt {
            None
        } else {
            merge_overlaps(marks.clone(), false)
                .ok()
                .and_then(|clear| clear.first().map(|mark| mark.white))
        };
        // `marks` keeps every hit, including a doubtful one, so a neighbour can see a shared square.
        Survey { marks, side }
    }

    fn board_side(frame: Frame) -> f64 {
        if frame[2] <= frame[0] || frame[3] <= frame[1] {
            return 0.0;
        }
        (frame[2] - frame[0]).min(frame[3] - frame[1]) as f64
    }

    /// `(window, caption band)`.
    fn bands(frame: Frame) -> [(Frame, bool); 5] {
        let s = board_side(frame);
        let reach = snap(0.42 * s);
        let along = snap(0.28 * s);
        let past = snap(0.12 * s);
        let below = snap(0.18 * s);
        let pad = snap(0.008 * s).clamp(2, 6);
        let (left, top, right, bottom) = (frame[0], frame[1], frame[2], frame[3]);
        [
            ([left - reach, top - past, left - pad, top + along], false),
            ([right + pad, top - past, right + reach, top + along], false),
            (
                [left - reach, bottom - along, left - pad, bottom + past],
                false,
            ),
            (
                [right + pad, bottom - along, right + reach, bottom + past],
                false,
            ),
            (
                [left - past, bottom + pad, right + past, bottom + below],
                true,
            ),
        ]
    }

    fn snap(v: f64) -> i64 {
        v.round() as i64
    }

    fn scan_band(g: &Gray, window: Frame, s: f64, caption: bool) -> Vec<Mark> {
        let Some((x0, y0, x1, y1)) = clamp_window(window, g.width, g.height) else {
            return Vec::new();
        };
        let ww = x1 - x0;
        let hh = y1 - y0;
        let mut dark = vec![false; ww * hh];
        for y in 0..hh {
            for x in 0..ww {
                dark[y * ww + x] = g.px[(y0 + y) * g.width + (x0 + x)] < INK;
            }
        }
        // One pixel of closing joins a broken outline without reaching across a real gap.
        let mut closed = dark.clone();
        for y in 0..hh {
            for x in 0..ww {
                if !dark[y * ww + x] {
                    continue;
                }
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let nx = x as isize + dx;
                        let ny = y as isize + dy;
                        if nx >= 0 && ny >= 0 && (nx as usize) < ww && (ny as usize) < hh {
                            closed[ny as usize * ww + nx as usize] = true;
                        }
                    }
                }
            }
        }
        let mut seen = vec![false; ww * hh];
        let mut marks = Vec::new();
        let huge = (0.45 * s).powi(2).max(64.0) as usize;
        for start in 0..ww * hh {
            if seen[start] || !closed[start] {
                continue;
            }
            let mut stack = vec![start];
            seen[start] = true;
            let mut pix = Vec::new();
            while let Some(i) = stack.pop() {
                let (x, y) = (i % ww, i / ww);
                if dark[i] {
                    pix.push((x, y));
                }
                for (dx, dy) in [
                    (-1, 0),
                    (1, 0),
                    (0, -1),
                    (0, 1),
                    (-1, -1),
                    (-1, 1),
                    (1, -1),
                    (1, 1),
                ] {
                    let nx = x as isize + dx;
                    let ny = y as isize + dy;
                    if nx >= 0 && ny >= 0 && (nx as usize) < ww && (ny as usize) < hh {
                        let ni = ny as usize * ww + nx as usize;
                        if !seen[ni] && closed[ni] {
                            seen[ni] = true;
                            stack.push(ni);
                        }
                    }
                }
            }
            if pix.is_empty() || pix.len() > huge {
                continue;
            }
            if let Some(mark) = classify(g, &pix, ww, x0, y0, s, caption) {
                marks.push(mark);
            }
        }
        marks
    }

    fn clamp_window(
        window: Frame,
        width: usize,
        height: usize,
    ) -> Option<(usize, usize, usize, usize)> {
        let x0 = window[0].max(0);
        let y0 = window[1].max(0);
        let x1 = window[2].min(width as i64);
        let y1 = window[3].min(height as i64);
        if x1 - x0 < 4 || y1 - y0 < 4 {
            return None;
        }
        Some((x0 as usize, y0 as usize, x1 as usize, y1 as usize))
    }

    fn classify(
        g: &Gray,
        pix: &[(usize, usize)],
        ww: usize,
        origin_x: usize,
        origin_y: usize,
        s: f64,
        caption: bool,
    ) -> Option<Mark> {
        let window_h = pix.iter().map(|p| p.1).max().unwrap_or(0) + 1;
        let mut member = vec![false; ww * window_h.max(1)];
        let (mut x0, mut y0, mut x1, mut y1) = (ww, window_h, 0usize, 0usize);
        for &(x, y) in pix {
            if y < window_h && x < ww {
                member[y * ww + x] = true;
            }
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
        (x0, y0, x1, y1) = trim_arrow(&member, ww, x0, y0, x1, y1);
        let (bw, bh) = ((x1 - x0) as f64, (y1 - y0) as f64);
        let max_side = bw.max(bh);
        let min_frac = if caption { MIN_CAPTION } else { MIN_CORNER };
        if max_side < min_frac * s || max_side > MAX_MARK * s {
            return None;
        }
        if (bw - bh).abs() > 0.28 * max_side || bw.min(bh) < 0.72 * max_side {
            return None;
        }
        let (mean, range, samples) = interior(g, origin_x, origin_y, x0, y0, x1, y1);
        if samples < 4 || range >= FLAT {
            return None;
        }
        let white = if mean <= 0.72 {
            false
        } else if mean >= 0.80 && borders_inked(&member, ww, x0, y0, x1, y1) {
            true
        } else {
            return None;
        };
        Some(Mark {
            white,
            x0: (origin_x + x0) as i64,
            y0: (origin_y + y0) as i64,
            x1: (origin_x + x1) as i64,
            y1: (origin_y + y1) as i64,
        })
    }

    /// Drops outer rows and columns that are only a thin shaft, so an arrow attached to a
    /// square does not stretch the box. A hollow square keeps its full span on every row.
    fn trim_arrow(
        member: &[bool],
        ww: usize,
        mut x0: usize,
        mut y0: usize,
        mut x1: usize,
        mut y1: usize,
    ) -> (usize, usize, usize, usize) {
        for _ in 0..32 {
            if x1.saturating_sub(x0) < 4 || y1.saturating_sub(y0) < 4 {
                break;
            }
            let max_row = (y0..y1)
                .map(|y| row_span(member, ww, x0, x1, y))
                .max()
                .unwrap_or(0);
            let max_col = (x0..x1)
                .map(|x| col_span(member, ww, x, y0, y1))
                .max()
                .unwrap_or(0);
            let row_keep = ((max_row as f64) * 0.50).ceil().max(1.0) as usize;
            let col_keep = ((max_col as f64) * 0.50).ceil().max(1.0) as usize;
            let mut next = (x0, y0, x1, y1);
            if row_span(member, ww, x0, x1, y0) < row_keep {
                next.1 += 1;
            }
            if next.3 > next.1 && row_span(member, ww, x0, x1, y1 - 1) < row_keep {
                next.3 -= 1;
            }
            if col_span(member, ww, x0, y0, y1) < col_keep {
                next.0 += 1;
            }
            if next.2 > next.0 && col_span(member, ww, x1 - 1, y0, y1) < col_keep {
                next.2 -= 1;
            }
            if next == (x0, y0, x1, y1) {
                break;
            }
            (x0, y0, x1, y1) = next;
        }
        (x0, y0, x1, y1)
    }

    fn row_span(member: &[bool], ww: usize, x0: usize, x1: usize, y: usize) -> usize {
        let mut min = x1;
        let mut max = x0;
        for x in x0..x1 {
            if member.get(y * ww + x).copied().unwrap_or(false) {
                min = min.min(x);
                max = max.max(x + 1);
            }
        }
        max.saturating_sub(min)
    }

    fn col_span(member: &[bool], ww: usize, x: usize, y0: usize, y1: usize) -> usize {
        let mut min = y1;
        let mut max = y0;
        for y in y0..y1 {
            if member.get(y * ww + x).copied().unwrap_or(false) {
                min = min.min(y);
                max = max.max(y + 1);
            }
        }
        max.saturating_sub(min)
    }

    fn interior(
        g: &Gray,
        origin_x: usize,
        origin_y: usize,
        x0: usize,
        y0: usize,
        x1: usize,
        y1: usize,
    ) -> (f32, f32, usize) {
        let inset = |span: usize| ((span as f64) * 0.30).floor().max(1.0) as usize;
        let ix = inset(x1 - x0);
        let iy = inset(y1 - y0);
        if x0 + ix >= x1.saturating_sub(ix) || y0 + iy >= y1.saturating_sub(iy) {
            return (1.0, 1.0, 0);
        }
        let mut min_v = 1.0f32;
        let mut max_v = 0.0f32;
        let mut sum = 0.0f32;
        let mut n = 0usize;
        for y in (y0 + iy)..(y1 - iy) {
            for x in (x0 + ix)..(x1 - ix) {
                let v = g.px[(origin_y + y) * g.width + origin_x + x];
                min_v = min_v.min(v);
                max_v = max_v.max(v);
                sum += v;
                n += 1;
            }
        }
        if n == 0 {
            return (1.0, 1.0, 0);
        }
        (sum / n as f32, max_v - min_v, n)
    }

    fn borders_inked(
        member: &[bool],
        ww: usize,
        x0: usize,
        y0: usize,
        x1: usize,
        y1: usize,
    ) -> bool {
        let w = x1.saturating_sub(x0);
        let h = y1.saturating_sub(y0);
        if w < 4 || h < 4 {
            return false;
        }
        let bx = (w / 5).max(1);
        let by = (h / 5).max(1);
        let row = |y: usize| {
            let ink = (x0..x1)
                .filter(|&x| member.get(y * ww + x).copied().unwrap_or(false))
                .count();
            ink as f64 / w as f64
        };
        let col = |x: usize| {
            let ink = (y0..y1)
                .filter(|&y| member.get(y * ww + x).copied().unwrap_or(false))
                .count();
            ink as f64 / h as f64
        };
        let max_rows = |from: usize, to: usize| (from..to).map(row).fold(0.0_f64, f64::max);
        let max_cols = |from: usize, to: usize| (from..to).map(col).fold(0.0_f64, f64::max);
        max_rows(y0, y0 + by) > 0.72
            && max_rows(y1 - by, y1) > 0.72
            && max_cols(x0, x0 + bx) > 0.72
            && max_cols(x1 - bx, x1) > 0.72
    }

    /// `Err` when the squares disagree, or when `many_is_doubt` and more than one remains.
    fn merge_overlaps(marks: Vec<Mark>, many_is_doubt: bool) -> Result<Vec<Mark>, ()> {
        let mut kept: Vec<Mark> = Vec::new();
        for mark in marks {
            if let Some(prev) = kept.iter_mut().find(|prev| boxes_overlap(prev, &mark)) {
                if prev.white != mark.white {
                    return Err(());
                }
                prev.x0 = prev.x0.min(mark.x0);
                prev.y0 = prev.y0.min(mark.y0);
                prev.x1 = prev.x1.max(mark.x1);
                prev.y1 = prev.y1.max(mark.y1);
            } else if kept.iter().any(|prev| prev.white != mark.white) {
                return Err(());
            } else {
                kept.push(mark);
            }
        }
        if many_is_doubt && kept.len() >= 2 {
            return Err(());
        }
        Ok(kept)
    }

    fn boxes_overlap(a: &Mark, b: &Mark) -> bool {
        let ix = (a.x1.min(b.x1) - a.x0.max(b.x0)).max(0);
        let iy = (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0);
        if ix == 0 || iy == 0 {
            return false;
        }
        let inter = ix * iy;
        let area = |m: &Mark| (m.x1 - m.x0).max(1) * (m.y1 - m.y0).max(1);
        inter * 2 >= area(a).min(area(b))
    }

    /// True when some other board is about as close to the square as its own board.
    fn contested(frames: &[Frame], index: usize, mark: &Mark) -> bool {
        let own = frames[index];
        let d_own = gap(own, mark);
        frames.iter().enumerate().any(|(other_i, other)| {
            other_i != index && gap(*other, mark) <= d_own.mul_add(1.35, 2.0)
        })
    }

    fn gap(frame: Frame, mark: &Mark) -> f64 {
        let cx = f64::midpoint(mark.x0 as f64, mark.x1 as f64);
        let cy = f64::midpoint(mark.y0 as f64, mark.y1 as f64);
        let dx = if cx < frame[0] as f64 {
            frame[0] as f64 - cx
        } else if cx > frame[2] as f64 {
            cx - frame[2] as f64
        } else {
            0.0
        };
        let dy = if cy < frame[1] as f64 {
            frame[1] as f64 - cy
        } else if cy > frame[3] as f64 {
            cy - frame[3] as f64
        } else {
            0.0
        };
        dx.hypot(dy)
    }
}

pub(crate) use side_square::attach_printed_mark;
pub use side_square::{side_marks, stated_mark_side, with_mark_side};
pub(crate) use side_triangle::attach_printed_triangle;
pub use side_triangle::{stated_triangle_side, triangle_marks, with_triangle_side};

/// Printed triangles the Quality Chess reader misses: any rank in the band right of the board,
/// or the caption line under it. A filled triangle is Black and a hollow one is White, pointing
/// either way. Applied only while the side to move is still unknown.
mod side_triangle {
    #![allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]

    use super::{Frame, Gray, locate_with_light_rim};

    /// PNG text chunk for a triangle that is not the Quality Chess bottom/top pair.
    const KEY: &[u8] = b"woodpeck-triangle-side";
    /// Ink at or below this luma belongs to the mark.
    const INK: f32 = 0.55;
    const MIN_MARK: f64 = 0.045;
    const MAX_MARK: f64 = 0.14;

    /// A PNG with a side read from a triangle off the Quality Chess places.
    /// The resolver may use it only while the side to move is still unknown.
    pub fn with_triangle_side(png: &[u8], white: bool) -> Vec<u8> {
        super::with_text(png, KEY, if white { "w" } else { "b" })
    }

    /// The side stored by [`with_triangle_side`]: `Some(true)` for White to move.
    pub fn stated_triangle_side(bytes: &[u8]) -> Option<bool> {
        match super::text_chunk(bytes, KEY)?.as_slice() {
            b"w" => Some(true),
            b"b" => Some(false),
            _ => None,
        }
    }

    /// When `bytes` is one diagram and a side triangle sits in its margin, return the image
    /// with that side stored beside the pixels. `None` leaves the bytes unchanged. A side
    /// already stored by a square marker or [`super::with_side`] is not replaced.
    pub(crate) fn attach_printed_triangle(bytes: &[u8]) -> Option<Vec<u8>> {
        if super::stated_side(bytes).is_some()
            || super::stated_mark_side(bytes).is_some()
            || stated_triangle_side(bytes).is_some()
        {
            return None;
        }
        let gray = Gray::decode(bytes)?;
        let frame = locate_with_light_rim(&gray)?;
        let white = triangle_marks(&gray, &[frame]).pop().flatten()?;
        let png = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            bytes.to_vec()
        } else {
            let image = image::load_from_memory(bytes).ok()?;
            let mut png = Vec::new();
            image
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .ok()?;
            png
        };
        Some(with_triangle_side(&png, white))
    }

    #[derive(Clone, Copy)]
    struct Mark {
        white: bool,
        x0: i64,
        y0: i64,
        x1: i64,
        y1: i64,
    }

    struct Survey {
        marks: Vec<Mark>,
        side: Option<bool>,
    }

    /// One reading per frame. `Some(true)` is White to move. `None` abstains.
    ///
    /// Looks in the band just right of the board, at any rank, and in the caption line under
    /// the board. A filled triangle is Black; a hollow triangle is White; the point may face
    /// up or down. The outline must taper in a straight line, so a letter is not a triangle.
    /// In the caption band the mark must sit apart from other ink.
    ///
    /// Abstains when one band holds two marks, two marks name different sides, a mark is shared
    /// by two boards, or a mark is about as close to another board as to its own.
    pub fn triangle_marks(g: &Gray, frames: &[Frame]) -> Vec<Option<bool>> {
        let surveys: Vec<Survey> = frames
            .iter()
            .copied()
            .map(|frame| survey(g, frame))
            .collect();
        let n = surveys.len();
        let mut drop = vec![false; n];
        for i in 0..n {
            for j in (i + 1)..n {
                let shared = surveys[i]
                    .marks
                    .iter()
                    .any(|a| surveys[j].marks.iter().any(|b| boxes_overlap(a, b)));
                if shared {
                    drop[i] = true;
                    drop[j] = true;
                }
            }
        }
        for (i, survey) in surveys.iter().enumerate() {
            if drop[i] {
                continue;
            }
            if survey.marks.iter().any(|mark| contested(frames, i, mark)) {
                drop[i] = true;
            }
        }
        surveys
            .into_iter()
            .enumerate()
            .map(|(i, survey)| if drop[i] { None } else { survey.side })
            .collect()
    }

    fn survey(g: &Gray, frame: Frame) -> Survey {
        let s = board_side(frame);
        if s < 80.0 {
            return Survey {
                marks: Vec::new(),
                side: None,
            };
        }
        let mut marks = Vec::new();
        let mut doubt = false;
        for (window, caption) in bands(frame) {
            let hits = scan_band(g, frame, window, s, caption);
            if let Ok(clear) = merge_overlaps(hits.clone(), true) {
                marks.extend(clear);
            } else {
                doubt = true;
                marks.extend(hits);
            }
        }
        let side = if doubt {
            None
        } else {
            merge_overlaps(marks.clone(), false)
                .ok()
                .and_then(|clear| clear.first().map(|mark| mark.white))
        };
        Survey { marks, side }
    }

    fn board_side(frame: Frame) -> f64 {
        if frame[2] <= frame[0] || frame[3] <= frame[1] {
            return 0.0;
        }
        (frame[2] - frame[0]).min(frame[3] - frame[1]) as f64
    }

    /// `(window, caption band)`. The right-hand window covers every rank.
    fn bands(frame: Frame) -> [(Frame, bool); 2] {
        let s = board_side(frame);
        let reach = snap(0.32 * s);
        let y_pad = snap(0.05 * s);
        let below = snap(0.22 * s);
        let past = snap(0.10 * s);
        let (left, top, right, bottom) = (frame[0], frame[1], frame[2], frame[3]);
        [
            (
                [right + 1, top - y_pad, right + reach, bottom + y_pad],
                false,
            ),
            (
                [left - past, bottom + 2, right + past, bottom + below],
                true,
            ),
        ]
    }

    fn snap(v: f64) -> i64 {
        v.round() as i64
    }

    fn scan_band(g: &Gray, board: Frame, window: Frame, s: f64, caption: bool) -> Vec<Mark> {
        let Some((x0, y0, x1, y1)) = clamp_window(window, g.width, g.height) else {
            return Vec::new();
        };
        let ww = x1 - x0;
        let hh = y1 - y0;
        let mut dark = vec![false; ww * hh];
        for y in 0..hh {
            for x in 0..ww {
                dark[y * ww + x] = g.px[(y0 + y) * g.width + (x0 + x)] < INK;
            }
        }
        let mut seen = vec![false; ww * hh];
        let mut marks = Vec::new();
        let huge = (0.45 * s).powi(2).max(64.0) as usize;
        for start in 0..ww * hh {
            if seen[start] || !dark[start] {
                continue;
            }
            let mut stack = vec![start];
            seen[start] = true;
            let mut pix = Vec::new();
            while let Some(i) = stack.pop() {
                let (x, y) = (i % ww, i / ww);
                pix.push((x, y));
                for (dx, dy) in [
                    (-1, 0),
                    (1, 0),
                    (0, -1),
                    (0, 1),
                    (-1, -1),
                    (-1, 1),
                    (1, -1),
                    (1, 1),
                ] {
                    let nx = x as isize + dx;
                    let ny = y as isize + dy;
                    if nx >= 0 && ny >= 0 && (nx as usize) < ww && (ny as usize) < hh {
                        let ni = ny as usize * ww + nx as usize;
                        if !seen[ni] && dark[ni] {
                            seen[ni] = true;
                            stack.push(ni);
                        }
                    }
                }
            }
            if pix.len() < 8 || pix.len() > huge {
                continue;
            }
            if let Some(mark) = classify(g, board, &pix, x0, y0, s, caption) {
                marks.push(mark);
            }
        }
        marks
    }

    fn clamp_window(
        window: Frame,
        width: usize,
        height: usize,
    ) -> Option<(usize, usize, usize, usize)> {
        let x0 = window[0].max(0);
        let y0 = window[1].max(0);
        let x1 = window[2].min(width as i64);
        let y1 = window[3].min(height as i64);
        if x1 - x0 < 4 || y1 - y0 < 4 {
            return None;
        }
        Some((x0 as usize, y0 as usize, x1 as usize, y1 as usize))
    }

    fn classify(
        g: &Gray,
        board: Frame,
        pix: &[(usize, usize)],
        origin_x: usize,
        origin_y: usize,
        s: f64,
        caption: bool,
    ) -> Option<Mark> {
        let (x0, y0, x1, y1) = bbox(pix);
        let (bw, bh) = ((x1 - x0) as f64, (y1 - y0) as f64);
        let max_side = bw.max(bh);
        if max_side < MIN_MARK * s || max_side > MAX_MARK * s || bw.min(bh) < 0.55 * max_side {
            return None;
        }
        let white = triangle_color(pix, x0, y0, x1, y1)?;
        let mark = Mark {
            white,
            x0: (origin_x + x0) as i64,
            y0: (origin_y + y0) as i64,
            x1: (origin_x + x1) as i64,
            y1: (origin_y + y1) as i64,
        };
        if caption && !isolated(g, board, &mark) {
            return None;
        }
        Some(mark)
    }

    fn bbox(pix: &[(usize, usize)]) -> (usize, usize, usize, usize) {
        let mut x0 = usize::MAX;
        let mut y0 = usize::MAX;
        let mut x1 = 0usize;
        let mut y1 = 0usize;
        for &(x, y) in pix {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
        (x0, y0, x1, y1)
    }

    /// `Some(true)` when the blob is a hollow triangle. Filled is `Some(false)`.
    fn triangle_color(
        pix: &[(usize, usize)],
        x0: usize,
        y0: usize,
        x1: usize,
        y1: usize,
    ) -> Option<bool> {
        let mut rows = row_spans(pix, y0, y1);
        if rows.len() < 8 || rows.iter().any(|row| row.ink == 0) {
            return None;
        }
        let k = (rows.len() / 4).max(1);
        let top: usize = rows[..k].iter().map(|row| row.width).sum();
        let bot: usize = rows[rows.len() - k..].iter().map(|row| row.width).sum();
        let up = bot > top.saturating_mul(2);
        let down = top > bot.saturating_mul(2);
        if up == down {
            return None;
        }
        if down {
            rows.reverse();
        }
        let base = rows.last().map_or(0, |row| row.width);
        let apex = rows.first().map_or(base, |row| row.width);
        if base < 8 || apex * 2 > base {
            return None;
        }
        let apex_mid = rows.first().map_or(0.0, Span::mid);
        let base_mid = rows.last().map_or(0.0, Span::mid);
        if (apex_mid - base_mid).abs() > 0.22 * base as f64 {
            return None;
        }
        if edge_deviation(&rows) > (0.12 * base as f64).max(1.75) {
            return None;
        }
        let area = ((x1 - x0) * (y1 - y0)).max(1) as f64;
        let fill = pix.len() as f64 / area;
        let solid = rows
            .iter()
            .filter(|row| row.ink * 5 >= row.width * 4)
            .count() as f64
            / rows.len() as f64;
        let base_row = rows.last()?;
        let base_ink = base_row.ink as f64 / base_row.width.max(1) as f64;
        if fill >= 0.40 && solid >= 0.80 {
            return Some(false);
        }
        let middle_clear = rows.iter().enumerate().all(|(i, row)| {
            let t = i as f64 / (rows.len() - 1) as f64;
            if row.width < 8 || !(0.18..=0.82).contains(&t) || row.interior == 0 {
                return true;
            }
            row.interior_ink * 5 < row.interior
        });
        ((0.08..=0.38).contains(&fill) && base_ink >= 0.75 && middle_clear).then_some(true)
    }

    struct Span {
        width: usize,
        ink: usize,
        interior_ink: usize,
        interior: usize,
        left: f64,
        right: f64,
    }

    impl Span {
        fn mid(&self) -> f64 {
            f64::midpoint(self.left, self.right)
        }
    }

    fn row_spans(pix: &[(usize, usize)], y0: usize, y1: usize) -> Vec<Span> {
        let mut rows: Vec<Vec<usize>> = vec![Vec::new(); y1.saturating_sub(y0)];
        for &(x, y) in pix {
            if y >= y0 && y < y1 {
                rows[y - y0].push(x);
            }
        }
        rows.iter()
            .map(|xs| {
                if xs.is_empty() {
                    return Span {
                        width: 0,
                        ink: 0,
                        interior_ink: 0,
                        interior: 0,
                        left: 0.0,
                        right: 0.0,
                    };
                }
                let left = *xs.iter().min().unwrap_or(&0);
                let right = xs.iter().max().unwrap_or(&left) + 1;
                let width = right.saturating_sub(left);
                let inset = width / 4;
                let (mid_l, mid_r) = if width >= 8 && inset > 0 {
                    (left + inset, right.saturating_sub(inset))
                } else {
                    (left, left)
                };
                let interior_ink = xs.iter().filter(|x| **x >= mid_l && **x < mid_r).count();
                Span {
                    width,
                    ink: xs.len(),
                    interior_ink,
                    interior: mid_r.saturating_sub(mid_l),
                    left: left as f64,
                    right: right as f64,
                }
            })
            .collect()
    }

    /// How far the left and right edges stray from the straight sides through the apex and base.
    fn edge_deviation(rows: &[Span]) -> f64 {
        let n = rows.len();
        let (Some(apex), Some(base)) = (rows.first(), rows.last()) else {
            return f64::MAX;
        };
        let mut worst = 0.0_f64;
        for (i, row) in rows.iter().enumerate() {
            let t = i as f64 / (n - 1) as f64;
            let expect_l = apex.left + t * (base.left - apex.left);
            let expect_r = apex.right + t * (base.right - apex.right);
            worst = worst.max((row.left - expect_l).abs());
            worst = worst.max((row.right - expect_r).abs());
        }
        worst
    }

    /// Caption glyphs sit a fraction of the mark away. The board border is not a glyph.
    fn isolated(g: &Gray, board: Frame, mark: &Mark) -> bool {
        let span = (mark.x1 - mark.x0).max(mark.y1 - mark.y0).max(1) as f64;
        let margin = (span * 0.25).round().clamp(3.0, 14.0) as i64;
        let (x0, y0) = (mark.x0 - margin, mark.y0 - margin);
        let (x1, y1) = (mark.x1 + margin, mark.y1 + margin);
        for y in y0..y1 {
            for x in x0..x1 {
                if (mark.x0..mark.x1).contains(&x) && (mark.y0..mark.y1).contains(&y) {
                    continue;
                }
                if x < 0 || y < 0 || x >= g.width as i64 || y >= g.height as i64 {
                    continue;
                }
                if (board[0] - 2..board[2] + 2).contains(&x)
                    && (board[1] - 2..board[3] + 2).contains(&y)
                {
                    continue;
                }
                if g.px[y as usize * g.width + x as usize] < INK {
                    return false;
                }
            }
        }
        true
    }

    /// `Err` when the triangles disagree, or when `many_is_doubt` and more than one remains.
    fn merge_overlaps(marks: Vec<Mark>, many_is_doubt: bool) -> Result<Vec<Mark>, ()> {
        let mut kept: Vec<Mark> = Vec::new();
        for mark in marks {
            if let Some(prev) = kept.iter_mut().find(|prev| boxes_overlap(prev, &mark)) {
                if prev.white != mark.white {
                    return Err(());
                }
                prev.x0 = prev.x0.min(mark.x0);
                prev.y0 = prev.y0.min(mark.y0);
                prev.x1 = prev.x1.max(mark.x1);
                prev.y1 = prev.y1.max(mark.y1);
            } else if kept.iter().any(|prev| prev.white != mark.white) {
                return Err(());
            } else {
                kept.push(mark);
            }
        }
        if many_is_doubt && kept.len() >= 2 {
            return Err(());
        }
        Ok(kept)
    }

    fn boxes_overlap(a: &Mark, b: &Mark) -> bool {
        let ix = (a.x1.min(b.x1) - a.x0.max(b.x0)).max(0);
        let iy = (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0);
        if ix == 0 || iy == 0 {
            return false;
        }
        let inter = ix * iy;
        let area = |m: &Mark| (m.x1 - m.x0).max(1) * (m.y1 - m.y0).max(1);
        inter * 2 >= area(a).min(area(b))
    }

    /// True when some other board is about as close to the triangle as its own board.
    fn contested(frames: &[Frame], index: usize, mark: &Mark) -> bool {
        let own = frames[index];
        let d_own = gap(own, mark);
        frames.iter().enumerate().any(|(other_i, other)| {
            other_i != index && gap(*other, mark) <= d_own.mul_add(1.35, 2.0)
        })
    }

    fn gap(frame: Frame, mark: &Mark) -> f64 {
        let cx = f64::midpoint(mark.x0 as f64, mark.x1 as f64);
        let cy = f64::midpoint(mark.y0 as f64, mark.y1 as f64);
        let dx = if cx < frame[0] as f64 {
            frame[0] as f64 - cx
        } else if cx > frame[2] as f64 {
            cx - frame[2] as f64
        } else {
            0.0
        };
        let dy = if cy < frame[1] as f64 {
            frame[1] as f64 - cy
        } else if cy > frame[3] as f64 {
            cy - frame[3] as f64
        } else {
            0.0
        };
        dx.hypot(dy)
    }
}

/// Boards on a whole page (a scan or a rendered page): square clusters of connected ink, each
/// confirmed by [`locate`] on its own crop. Returned as crop boxes with a small margin, top to
/// bottom, so each crop can go through the same diagram gate as a picture.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss
)]
pub fn page_boards(g: &Gray) -> Vec<Frame> {
    let (w, h) = (g.width as i64, g.height as i64);
    let min_side = (g.width.min(g.height) / 9).max(64);
    let mut out: Vec<Frame> = Vec::new();
    for b in ink_boxes(g, min_side / 2) {
        let (bw, bh) = (b[2] - b[0], b[3] - b[1]);
        let side = bw.max(bh);
        if side < min_side as i64 || side as f32 > 0.95 * w as f32 || 2 * bw.min(bh) < side {
            continue;
        }
        // A broken border leaves part of the board outside its cluster: square it up on
        // either side of the short dimension.
        let squares = if bw >= bh {
            vec![
                [b[0], b[1], b[2], b[1] + side],
                [b[0], b[3] - side, b[2], b[3]],
            ]
        } else {
            vec![
                [b[0], b[1], b[0] + side, b[3]],
                [b[2] - side, b[1], b[2], b[3]],
            ]
        };
        for s in squares {
            let margin = side / 25 + 2;
            let crop = [
                (s[0] - margin).max(0),
                (s[1] - margin).max(0),
                (s[2] + margin).min(w),
                (s[3] + margin).min(h),
            ];
            let overlaps = |f: &Frame| {
                let ix = (f[2].min(crop[2]) - f[0].max(crop[0])).max(0);
                let iy = (f[3].min(crop[3]) - f[1].max(crop[1])).max(0);
                4 * ix * iy > (crop[2] - crop[0]) * (crop[3] - crop[1])
            };
            if out.iter().any(overlaps) {
                break;
            }
            let Some(frame) = g.crop(crop).as_ref().and_then(locate) else {
                continue;
            };
            // The board must fill most of the crop, not be a corner of a larger figure.
            if 4 * (frame[2] - frame[0]).min(frame[3] - frame[1]) < 3 * side {
                continue;
            }
            out.push(crop);
            break;
        }
    }
    out.sort_by_key(|f| (f[1], f[0]));
    out
}

/// Root of a union-find set, halving paths on the way.
fn root(parents: &mut [u32], mut node: u32) -> u32 {
    while parents[node as usize] != node {
        parents[node as usize] = parents[parents[node as usize] as usize];
        node = parents[node as usize];
    }
    node
}

/// Bounding boxes of 8-connected dark components at least `min_side` on both sides, largest
/// first.
#[allow(clippy::cast_possible_wrap)]
fn ink_boxes(image: &Gray, min_side: usize) -> Vec<Frame> {
    let (width, height) = (image.width, image.height);
    let mut parents: Vec<u32> = Vec::new();
    let mut labels = vec![u32::MAX; width * height];
    for row in 0..height {
        for col in 0..width {
            if image.at(col, row) >= 0.55 {
                continue;
            }
            let mut mine = u32::MAX;
            let neighbours = [
                (col > 0).then(|| (col - 1, row)),
                (row > 0 && col > 0).then(|| (col - 1, row - 1)),
                (row > 0).then(|| (col, row - 1)),
                (row > 0 && col + 1 < width).then(|| (col + 1, row - 1)),
            ];
            for (nc, nr) in neighbours.into_iter().flatten() {
                let label = labels[nr * width + nc];
                if label == u32::MAX {
                    continue;
                }
                let found = root(&mut parents, label);
                if mine == u32::MAX {
                    mine = found;
                } else if found != mine {
                    let (low, high) = (mine.min(found), mine.max(found));
                    parents[high as usize] = low;
                    mine = low;
                }
            }
            if mine == u32::MAX {
                mine = u32::try_from(parents.len()).unwrap_or(u32::MAX - 1);
                parents.push(mine);
            }
            labels[row * width + col] = mine;
        }
    }
    let mut boxes: std::collections::HashMap<u32, Frame> = std::collections::HashMap::new();
    for row in 0..height {
        for col in 0..width {
            let label = labels[row * width + col];
            if label == u32::MAX {
                continue;
            }
            let id = root(&mut parents, label);
            let (x, y) = (col as i64, row as i64);
            let frame = boxes.entry(id).or_insert([x, y, x + 1, y + 1]);
            frame[0] = frame[0].min(x);
            frame[1] = frame[1].min(y);
            frame[2] = frame[2].max(x + 1);
            frame[3] = frame[3].max(y + 1);
        }
    }
    let min = min_side as i64;
    let mut out: Vec<Frame> = boxes
        .into_values()
        .filter(|f| f[2] - f[0] >= min && f[3] - f[1] >= min)
        .collect();
    // Largest first, so an outer frame claims its board before inner fragments do.
    out.sort_by_key(|f| std::cmp::Reverse((f[2] - f[0]) * (f[3] - f[1])));
    out
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

/// Longest board-crop side fed to locate and the square nets. `ClearScan` page crops near
/// the import raster width (~1100 px) alias hatched dark squares when reduced to [`CELL`].
/// Capping restores the size band used in training (~300–600 px).
pub const MAX_BOARD_SIDE: usize = 560;

/// Square images of a located board, or `None` when the image is not a board.
/// See [`board_cells_scales`]: the native grid is preferred when both exist.
pub fn board_cells(bytes: &[u8]) -> Option<Vec<Vec<f32>>> {
    board_cells_scales(bytes).into_iter().next()
}

/// Native-resolution cells first, then a [`MAX_BOARD_SIDE`] downscale when the crop is
/// larger. Recognizers try each grid and keep the first confident placement so mid-size
/// boards (~680 px) keep their pre-cap reads while ~1200 px `ClearScan` boards can still
/// recover after downscale.
pub fn board_cells_scales(bytes: &[u8]) -> Vec<Vec<Vec<f32>>> {
    let Some(g0) = Gray::decode(bytes) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(2);
    if let Some(frame) = locate_with_light_rim(&g0)
        && let Some(c) = cells(&g0, frame)
    {
        out.push(c);
    }
    if g0.width.max(g0.height) > MAX_BOARD_SIDE {
        let g = g0.fit_max_side(MAX_BOARD_SIDE);
        if let Some(frame) = locate_with_light_rim(&g)
            && let Some(c) = cells(&g, frame)
        {
            out.push(c);
        }
    }
    out
}

/// Per-square feature rows for each grid from [`board_cells_scales`].
pub fn board_features_scales(bytes: &[u8]) -> Vec<Vec<Vec<f32>>> {
    board_cells_scales(bytes)
        .into_iter()
        .map(|cells| cells.iter().map(|c| features(c)).collect())
        .collect()
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
        for feats in board_features_scales(image) {
            let squares: Option<Vec<Square>> = feats
                .iter()
                .enumerate()
                .map(|(i, f)| self.classify(f, dark_square(i / 8, i % 8)))
                .collect();
            if let Some(squares) = squares
                && let Some(fen) = placement(&squares)
            {
                return Some(fen);
            }
        }
        None
    }
    fn reading(&self, image: &[u8]) -> Option<String> {
        let mut fallback = None;
        for feats in board_features_scales(image) {
            let squares: Option<Vec<Square>> = feats
                .iter()
                .enumerate()
                .map(|(i, f)| self.classify(f, dark_square(i / 8, i % 8)))
                .collect();
            let Some(squares) = squares else {
                continue;
            };
            let reading: String = squares.iter().map(|s| char::from(s.label)).collect();
            // Prefer the scale that also passes placement gates (same choice as `placement`).
            if placement(&squares).is_some() {
                return Some(reading);
            }
            fallback.get_or_insert(reading);
        }
        fallback
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

/// Prefer the primary recognizer; only ask the fallback when the primary abstains.
/// Disagreement never arises because the fallback is not consulted after a primary hit, so a
/// newer network can gain boards without giving up ones only the older network still reads.
pub struct Fallback<'a> {
    pub primary: &'a dyn DiagramRecognizer,
    pub fallback: &'a dyn DiagramRecognizer,
}

impl DiagramRecognizer for Fallback<'_> {
    fn placement(&self, image: &[u8]) -> Option<String> {
        self.primary
            .placement(image)
            .or_else(|| self.fallback.placement(image))
    }
    fn reading(&self, image: &[u8]) -> Option<String> {
        self.primary
            .reading(image)
            .or_else(|| self.fallback.reading(image))
    }
}

#[cfg(test)]
mod tests {

    /// A blank page with a 300 px board outline and optional squares (left, top, side, filled).
    fn marked(squares: &[(usize, usize, usize, bool)]) -> (Gray, Frame) {
        let (w, h) = (500, 400);
        let mut luma = vec![255u8; w * h];
        for k in 50..350 {
            for (x, y) in [(k, 50), (k, 349), (50, k), (349, k)] {
                luma[y * w + x] = 0;
            }
        }
        for &(left, top, side, filled) in squares {
            for y in top..top + side {
                for x in left..left + side {
                    let border =
                        x < left + 2 || x >= left + side - 2 || y < top + 2 || y >= top + side - 2;
                    if filled || border {
                        luma[y * w + x] = 0;
                    }
                }
            }
        }
        (Gray::from_luma(w, h, &luma).unwrap(), [50, 50, 350, 350])
    }

    #[test]
    fn fit_max_side_shrinks_only_when_larger() {
        let (g, _) = marked(&[]);
        assert_eq!(g.width, 500);
        let same = g.fit_max_side(560);
        assert_eq!((same.width, same.height), (g.width, g.height));
        let small = g.fit_max_side(200);
        assert!(small.width <= 200 && small.height <= 200);
        assert_eq!(small.width.max(small.height), 200);
    }

    #[test]
    fn side_marker_reads_hollow_and_filled_squares() {
        let (g, f) = marked(&[(400, 322, 25, false)]);
        assert_eq!(side_marker(&g, f), Some(true));
        let (g, f) = marked(&[(400, 322, 25, true)]);
        assert_eq!(side_marker(&g, f), Some(false));
    }

    #[test]
    fn side_marker_abstains_without_one_clear_square() {
        let (g, f) = marked(&[]);
        assert_eq!(side_marker(&g, f), None);
        let (g, f) = marked(&[(380, 322, 25, false), (420, 322, 25, true)]);
        assert_eq!(side_marker(&g, f), None);
        // Far above the bottom rank is a caption's place, not the marker's.
        let (g, f) = marked(&[(400, 100, 25, true)]);
        assert_eq!(side_marker(&g, f), None);
    }

    /// Blank page with a 300 px board and optional triangles: (left, top, side, filled, up).
    /// `up` true draws an outline/filled ▲; false a ▼.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::many_single_char_names
    )]
    fn marked_triangles(tris: &[(usize, usize, usize, bool, bool)]) -> (Gray, Frame) {
        let (w, h) = (500, 400);
        let mut luma = vec![255u8; w * h];
        for k in 50..350 {
            for (x, y) in [(k, 50), (k, 349), (50, k), (349, k)] {
                luma[y * w + x] = 0;
            }
        }
        for &(left, top, side, filled, up) in tris {
            for dy in 0..side {
                let t = if up {
                    dy as f64 / (side as f64 - 1.0).max(1.0)
                } else {
                    1.0 - dy as f64 / (side as f64 - 1.0).max(1.0)
                };
                let half = (t * (side as f64) / 2.0).round() as isize;
                let mid = left + side / 2;
                let y = top + dy;
                for dx in -half..=half {
                    let x = mid as isize + dx;
                    if x < 0 || y >= h {
                        continue;
                    }
                    let x = x as usize;
                    if x >= w {
                        continue;
                    }
                    let on_edge = dx.abs() >= half.saturating_sub(1) || dy == 0 || dy + 1 == side;
                    if filled || on_edge {
                        luma[y * w + x] = 0;
                    }
                }
            }
        }
        (Gray::from_luma(w, h, &luma).unwrap(), [50, 50, 350, 350])
    }

    #[test]
    fn triangle_marker_reads_outline_up_and_filled_down() {
        // Outline ▲ by the bottom rank → White.
        let (g, f) = marked_triangles(&[(360, 310, 28, false, true)]);
        assert_eq!(triangle_marker(&g, f), Some(true));
        // Filled ▼ by the top rank → Black.
        let (g, f) = marked_triangles(&[(360, 60, 28, true, false)]);
        assert_eq!(triangle_marker(&g, f), Some(false));
    }

    #[test]
    fn triangle_marker_abstains_on_conflict_or_wrong_place() {
        let (g, f) = marked_triangles(&[]);
        assert_eq!(triangle_marker(&g, f), None);
        // Both a White and a Black triangle → abstain.
        let (g, f) = marked_triangles(&[(360, 310, 28, false, true), (360, 60, 28, true, false)]);
        assert_eq!(triangle_marker(&g, f), None);
        // Filled ▼ parked by the bottom rank is not the convention.
        let (g, f) = marked_triangles(&[(360, 310, 28, true, false)]);
        assert_eq!(triangle_marker(&g, f), None);
    }

    /// `filled` paints a solid triangle; otherwise only the three sides. `up` points the apex up.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::too_many_arguments
    )]
    fn paint_triangle(
        buf: &mut [u8],
        width: usize,
        height: usize,
        left: usize,
        top: usize,
        side: usize,
        filled: bool,
        up: bool,
    ) {
        if side < 2 {
            return;
        }
        for dy in 0..side {
            let t = if up {
                dy as f64 / (side as f64 - 1.0)
            } else {
                1.0 - dy as f64 / (side as f64 - 1.0)
            };
            let half = (t * (side as f64) / 2.0).round() as isize;
            let mid = left + side / 2;
            let y = top + dy;
            if y >= height {
                continue;
            }
            for dx in -half..=half {
                let x = mid as isize + dx;
                if x < 0 {
                    continue;
                }
                let x = x as usize;
                if x >= width {
                    continue;
                }
                let on_edge = dx.abs() >= half.saturating_sub(1) || dy == 0 || dy + 1 == side;
                if filled || on_edge {
                    buf[y * width + x] = 0;
                }
            }
        }
    }

    /// Outline ▲ with a crossbar: a letter, not a mark.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]
    fn paint_letter_a(
        buf: &mut [u8],
        width: usize,
        height: usize,
        left: usize,
        top: usize,
        side: usize,
    ) {
        paint_triangle(buf, width, height, left, top, side, false, true);
        let bar = side / 2;
        for dy in bar.saturating_sub(1)..(bar + 2).min(side) {
            let y = top + dy;
            if y >= height {
                continue;
            }
            let t = dy as f64 / (side as f64 - 1.0);
            let half = (t * (side as f64) / 2.0).round() as isize;
            let mid = left + side / 2;
            for dx in -half..=half {
                let x = mid as isize + dx;
                if x >= 0 && (x as usize) < width {
                    buf[y * width + x as usize] = 0;
                }
            }
        }
    }

    /// Two stems and a diagonal.
    fn paint_letter_n(
        buf: &mut [u8],
        width: usize,
        height: usize,
        left: usize,
        top: usize,
        side: usize,
    ) {
        for dy in 0..side {
            let y = top + dy;
            if y >= height {
                continue;
            }
            for dx in 0..2 {
                for x in [left + dx, left + side - 1 - dx] {
                    if x < width {
                        buf[y * width + x] = 0;
                    }
                }
            }
            let x = left + dy * side.saturating_sub(1) / side.saturating_sub(1).max(1);
            for dx in 0..2 {
                if x + dx < width {
                    buf[y * width + x + dx] = 0;
                }
            }
        }
    }

    /// A vertical stroke and a foot. The wide end is not centered on the tip.
    fn paint_letter_l(
        buf: &mut [u8],
        width: usize,
        height: usize,
        left: usize,
        top: usize,
        side: usize,
    ) {
        for dy in 0..side {
            let y = top + dy;
            if y >= height {
                continue;
            }
            for dx in 0..2 {
                if left + dx < width {
                    buf[y * width + left + dx] = 0;
                }
            }
        }
        let y = top + side - 1;
        if y < height {
            for dx in 0..side {
                if left + dx < width {
                    buf[y * width + left + dx] = 0;
                }
            }
        }
    }

    /// Two strokes that meet at a point and never close the wide end.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]
    fn paint_letter_v(
        buf: &mut [u8],
        width: usize,
        height: usize,
        left: usize,
        top: usize,
        side: usize,
    ) {
        if side < 2 {
            return;
        }
        for dy in 0..side {
            let t = 1.0 - dy as f64 / (side as f64 - 1.0);
            let half = (t * (side as f64) / 2.0).round() as isize;
            let mid = left + side / 2;
            let y = top + dy;
            if y >= height {
                continue;
            }
            for dx in [half.saturating_neg(), half] {
                let x = mid as isize + dx;
                if x >= 0 && (x as usize) < width {
                    buf[y * width + x as usize] = 0;
                    if dx != 0 {
                        let inner = mid as isize + dx.signum() * dx.abs().saturating_sub(1);
                        if inner >= 0 && (inner as usize) < width {
                            buf[y * width + inner as usize] = 0;
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn triangle_marks_reads_hollow_white_and_filled_black() {
        let frame = [50, 50, 350, 350];
        let g = page_with(500, 420, |buf| {
            paint_triangle(buf, 500, 420, 360, 180, 28, false, true);
        });
        assert_eq!(triangle_marks(&g, &[frame]), vec![Some(true)]);
        let g = page_with(500, 420, |buf| {
            paint_triangle(buf, 500, 420, 360, 180, 28, true, true);
        });
        assert_eq!(triangle_marks(&g, &[frame]), vec![Some(false)]);
    }

    #[test]
    fn triangle_marks_reads_either_orientation() {
        let frame = [50, 50, 350, 350];
        // Hollow, point up or down, is White. Filled, either way, is Black.
        for up in [true, false] {
            let g = page_with(500, 420, |buf| {
                paint_triangle(buf, 500, 420, 362, 160, 28, false, up);
            });
            assert_eq!(
                triangle_marks(&g, &[frame]),
                vec![Some(true)],
                "hollow up={up}"
            );
            let g = page_with(500, 420, |buf| {
                paint_triangle(buf, 500, 420, 362, 160, 28, true, up);
            });
            assert_eq!(
                triangle_marks(&g, &[frame]),
                vec![Some(false)],
                "filled up={up}"
            );
        }
        // Filled ▼ by the bottom rank is outside the Quality Chess pair, and still Black.
        let (g, f) = marked_triangles(&[(360, 310, 28, true, false)]);
        assert_eq!(triangle_marker(&g, f), None);
        assert_eq!(triangle_marks(&g, &[f]), vec![Some(false)]);
    }

    #[test]
    fn triangle_marks_reads_the_right_band_and_the_caption() {
        let frame = [50, 50, 350, 350];
        // Mid-rank on the right is not the top or bottom Quality Chess place.
        let (g, f) = marked_triangles(&[(360, 160, 28, false, true)]);
        assert_eq!(triangle_marker(&g, f), None);
        assert_eq!(triangle_marks(&g, &[f]), vec![Some(true)]);
        // The caption line under the board, not the band beside it.
        let g = page_with(500, 460, |buf| {
            paint_triangle(buf, 500, 460, 160, 362, 24, true, false);
        });
        assert_eq!(triangle_marks(&g, &[frame]), vec![Some(false)]);
        // A triangle on the left of the board is not a side mark.
        let g = page_with(500, 420, |buf| {
            paint_triangle(buf, 500, 420, 8, 160, 28, false, true);
        });
        assert_eq!(triangle_marks(&g, &[frame]), vec![None]);
        // A caption triangle with another glyph beside it is not isolated.
        let g = page_with(500, 460, |buf| {
            paint_triangle(buf, 500, 460, 160, 362, 24, false, true);
            for y in 364..378 {
                for x in 190..204 {
                    buf[y * 500 + x] = 0;
                }
            }
        });
        assert_eq!(triangle_marks(&g, &[frame]), vec![None]);
    }

    #[test]
    fn triangle_marks_rejects_letter_shapes() {
        let frame = [50, 50, 350, 350];
        let reject = |g: &Gray, place: &str| {
            assert_eq!(triangle_marks(g, &[frame]), vec![None], "{place}");
        };
        let right = page_with(500, 420, |buf| paint_letter_a(buf, 500, 420, 360, 160, 28));
        reject(&right, "A beside the board");
        let caption = page_with(500, 460, |buf| paint_letter_a(buf, 500, 460, 160, 362, 28));
        reject(&caption, "A in the caption");
        let right = page_with(500, 420, |buf| paint_letter_n(buf, 500, 420, 360, 160, 28));
        reject(&right, "N beside the board");
        let caption = page_with(500, 460, |buf| paint_letter_n(buf, 500, 460, 160, 362, 28));
        reject(&caption, "N in the caption");
        let right = page_with(500, 420, |buf| paint_letter_l(buf, 500, 420, 360, 160, 28));
        reject(&right, "L beside the board");
        let caption = page_with(500, 460, |buf| paint_letter_l(buf, 500, 460, 160, 362, 28));
        reject(&caption, "L in the caption");
        let right = page_with(500, 420, |buf| paint_letter_v(buf, 500, 420, 360, 160, 28));
        reject(&right, "V beside the board");
        let caption = page_with(500, 460, |buf| paint_letter_v(buf, 500, 460, 160, 362, 28));
        reject(&caption, "V in the caption");
    }

    #[test]
    fn triangle_marks_abstains_when_shared_or_between_boards() {
        // The triangle sits in the gutter, about as close to each board.
        let left = [40, 40, 300, 300];
        let right = [340, 40, 600, 300];
        let g = page_with(640, 360, |buf| {
            paint_triangle(buf, 640, 360, 312, 70, 18, true, true);
        });
        assert_eq!(triangle_marks(&g, &[left, right]), vec![None, None]);
        // A triangle next to the first board, far from the second, belongs to the first.
        let far = [640, 40, 900, 300];
        let g = page_with(960, 360, |buf| {
            paint_triangle(buf, 960, 360, 308, 70, 18, false, false);
        });
        assert_eq!(triangle_marks(&g, &[left, far]), vec![Some(true), None]);
        // A caption under the upper board is closer to it than to the board below.
        let upper = [40, 40, 340, 340];
        let lower = [40, 520, 340, 820];
        let g = page_with(400, 880, |buf| {
            paint_triangle(buf, 400, 880, 150, 360, 22, false, true);
        });
        assert_eq!(triangle_marks(&g, &[upper, lower]), vec![Some(true), None]);
        // Halfway between two boards, the same caption is nobody's mark.
        let close = [40, 400, 340, 700];
        let g = page_with(400, 760, |buf| {
            paint_triangle(buf, 400, 760, 150, 358, 22, true, false);
        });
        assert_eq!(triangle_marks(&g, &[upper, close]), vec![None, None]);
    }

    /// How a synthetic square is painted. `Glyph` is a flat outline with a cross through it.
    #[derive(Clone, Copy)]
    enum MarkInk {
        Hollow,
        Filled,
        Gray,
        Glyph,
    }

    fn paint_square(
        buf: &mut [u8],
        width: usize,
        height: usize,
        left: usize,
        top: usize,
        side: usize,
        ink: MarkInk,
    ) {
        let thick = if side < 12 { 1 } else { 2 };
        for dy in 0..side {
            if top + dy >= height {
                break;
            }
            for dx in 0..side {
                if left + dx >= width {
                    break;
                }
                let border = dx < thick || dy < thick || dx + thick >= side || dy + thick >= side;
                let bar =
                    (dx > side / 3 && dx < side * 2 / 3) || (dy > side / 3 && dy < side * 2 / 3);
                let pixel = match ink {
                    MarkInk::Hollow => {
                        if border {
                            0
                        } else {
                            255
                        }
                    }
                    MarkInk::Filled => 0,
                    MarkInk::Gray => {
                        if border {
                            0
                        } else {
                            128
                        }
                    }
                    MarkInk::Glyph => {
                        if border || bar {
                            0
                        } else {
                            255
                        }
                    }
                };
                buf[(top + dy) * width + left + dx] = pixel;
            }
        }
    }

    fn paint_arrow(
        buf: &mut [u8],
        width: usize,
        height: usize,
        left: usize,
        top: usize,
        side: usize,
        attached: bool,
    ) {
        let (ax, ay) = if attached {
            (left + side / 2 - 1, top + side - 2)
        } else {
            (left + side + 6, top)
        };
        let tall = if attached { side / 2 + 4 } else { side };
        for dy in 0..tall {
            for dx in 0..2 {
                let (px, py) = (ax + dx, ay + dy);
                if px < width && py < height {
                    buf[py * width + px] = 0;
                }
            }
        }
    }

    fn page_with(width: usize, height: usize, paint: impl FnOnce(&mut [u8])) -> Gray {
        let mut luma = vec![255u8; width * height];
        paint(&mut luma);
        Gray::from_luma(width, height, &luma).expect("page")
    }

    #[test]
    fn side_marks_reads_hollow_white_and_filled_or_gray_black() {
        let frame = [40, 40, 340, 340];
        let g = page_with(500, 460, |buf| {
            paint_square(buf, 500, 460, 360, 290, 22, MarkInk::Hollow);
        });
        assert_eq!(side_marks(&g, &[frame]), vec![Some(true)]);
        let g = page_with(500, 460, |buf| {
            paint_square(buf, 500, 460, 360, 290, 22, MarkInk::Filled);
        });
        assert_eq!(side_marks(&g, &[frame]), vec![Some(false)]);
        let g = page_with(500, 460, |buf| {
            paint_square(buf, 500, 460, 360, 290, 22, MarkInk::Gray);
        });
        assert_eq!(side_marks(&g, &[frame]), vec![Some(false)]);
    }

    #[test]
    fn side_marks_reads_a_square_on_every_side_and_the_caption() {
        let frame = [40, 40, 340, 340];
        // Left-top, right-top, left-bottom, right-bottom, caption under the board.
        let spots = [
            (8, 20, 22, "left-top"),
            (360, 20, 22, "right-top"),
            (8, 280, 22, "left-bottom"),
            (360, 290, 22, "right-bottom"),
            (150, 352, 20, "caption"),
        ];
        for (x, y, side, band) in spots {
            for ink in [MarkInk::Hollow, MarkInk::Filled] {
                let g = page_with(500, 460, |buf| {
                    paint_square(buf, 500, 460, x, y, side, ink);
                });
                let want = matches!(ink, MarkInk::Hollow);
                assert_eq!(side_marks(&g, &[frame]), vec![Some(want)], "{band} {want}");
            }
        }
    }

    #[test]
    fn side_marks_tolerates_a_thin_arrow() {
        let frame = [40, 40, 340, 340];
        let g = page_with(500, 460, |buf| {
            paint_square(buf, 500, 460, 360, 290, 22, MarkInk::Hollow);
            paint_arrow(buf, 500, 460, 360, 290, 22, true);
        });
        assert_eq!(side_marks(&g, &[frame]), vec![Some(true)]);
        let g = page_with(500, 460, |buf| {
            paint_square(buf, 500, 460, 360, 290, 22, MarkInk::Filled);
            paint_arrow(buf, 500, 460, 360, 290, 22, false);
        });
        assert_eq!(side_marks(&g, &[frame]), vec![Some(false)]);
    }

    #[test]
    fn side_marks_rejects_a_glyph_with_a_varying_interior() {
        let frame = [40, 40, 340, 340];
        let g = page_with(500, 460, |buf| {
            paint_square(buf, 500, 460, 360, 290, 22, MarkInk::Glyph);
        });
        assert_eq!(side_marks(&g, &[frame]), vec![None]);
        // The glyph is not a mark, so a real square in another band still counts.
        let g = page_with(500, 460, |buf| {
            paint_square(buf, 500, 460, 8, 20, 22, MarkInk::Glyph);
            paint_square(buf, 500, 460, 360, 290, 22, MarkInk::Filled);
        });
        assert_eq!(side_marks(&g, &[frame]), vec![Some(false)]);
    }

    #[test]
    fn side_marks_abstains_when_marks_conflict() {
        let frame = [40, 40, 340, 340];
        // Two bands, two sides.
        let g = page_with(500, 460, |buf| {
            paint_square(buf, 500, 460, 8, 20, 22, MarkInk::Hollow);
            paint_square(buf, 500, 460, 360, 290, 22, MarkInk::Filled);
        });
        assert_eq!(side_marks(&g, &[frame]), vec![None]);
        // One band, two squares, even when they name the same side.
        let g = page_with(500, 460, |buf| {
            paint_square(buf, 500, 460, 360, 270, 18, MarkInk::Hollow);
            paint_square(buf, 500, 460, 400, 270, 18, MarkInk::Hollow);
        });
        assert_eq!(side_marks(&g, &[frame]), vec![None]);
        // The same side on two bands is one reading.
        let g = page_with(500, 460, |buf| {
            paint_square(buf, 500, 460, 8, 20, 22, MarkInk::Hollow);
            paint_square(buf, 500, 460, 150, 352, 20, MarkInk::Hollow);
        });
        assert_eq!(side_marks(&g, &[frame]), vec![Some(true)]);
    }

    #[test]
    fn side_marks_abstains_when_a_square_is_shared_or_between_boards() {
        // The square sits in the gutter and in both boards' bands.
        let left = [40, 40, 300, 300];
        let right = [340, 40, 600, 300];
        let g = page_with(640, 360, |buf| {
            paint_square(buf, 640, 360, 312, 50, 16, MarkInk::Filled);
        });
        assert_eq!(side_marks(&g, &[left, right]), vec![None, None]);
        // A square next to the first board, far from the second, belongs to the first.
        let far = [640, 40, 900, 300];
        let g = page_with(960, 360, |buf| {
            paint_square(buf, 960, 360, 308, 50, 14, MarkInk::Hollow);
        });
        assert_eq!(side_marks(&g, &[left, far]), vec![Some(true), None]);
        // A caption tucked under the upper board is closer to it than to the board below.
        let lower = [40, 520, 340, 820];
        let upper = [40, 40, 340, 340];
        let g = page_with(400, 880, |buf| {
            paint_square(buf, 400, 880, 150, 360, 20, MarkInk::Hollow);
        });
        assert_eq!(side_marks(&g, &[upper, lower]), vec![Some(true), None]);
    }

    #[test]
    fn side_marks_accepts_a_square_near_three_percent_of_the_board() {
        // 8 px on a 260 px board is just over 0.03. One pixel smaller is not a mark.
        let frame = [30, 30, 290, 290];
        let g = page_with(400, 400, |buf| {
            paint_square(buf, 400, 400, 8, 250, 8, MarkInk::Hollow);
        });
        assert_eq!(side_marks(&g, &[frame]), vec![Some(true)]);
        let g = page_with(400, 400, |buf| {
            paint_square(buf, 400, 400, 8, 250, 7, MarkInk::Hollow);
        });
        assert_eq!(side_marks(&g, &[frame]), vec![None]);
    }

    #[test]
    fn side_survives_the_png_text_chunk() {
        let image = image::GrayImage::from_raw(2, 2, vec![0, 255, 255, 0]).unwrap();
        let mut png = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        assert_eq!(stated_side(&png), None);
        assert_eq!(stated_caption_side(&png), None);
        assert_eq!(stated_mark_side(&png), None);
        let marked = with_mark_side(&png, true);
        assert_eq!(stated_mark_side(&marked), Some(true));
        assert_eq!(stated_side(&marked), None);
        let captioned = with_caption_side(&png, true);
        assert_eq!(stated_caption_side(&captioned), Some(true));
        assert_eq!(stated_side(&captioned), None);
        let both = with_side(&with_placement(&png, "8/8/8/8/8/8/8/K6k"), false);
        assert_eq!(stated_side(&both), Some(false));
        assert_eq!(
            stated_placement(&both).as_deref(),
            Some("8/8/8/8/8/8/8/K6k")
        );
        assert!(image::load_from_memory(&both).is_ok());
    }
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

    /// A white canvas with empty bordered boards (gray dark squares) at the given corners.
    fn canvas(width: usize, height: usize, boards: &[(usize, usize)], side: usize) -> Gray {
        let mut px = vec![1.0f32; width * height];
        for &(x0, y0) in boards {
            for y in 0..side + 2 {
                for x in 0..side + 2 {
                    let border = x == 0 || y == 0 || x == side + 1 || y == side + 1;
                    let (r, c) = ((y.max(1) - 1) * 8 / side, (x.max(1) - 1) * 8 / side);
                    px[(y0 + y) * width + x0 + x] = if border {
                        0.0
                    } else if dark_square(r.min(7), c.min(7)) {
                        0.7
                    } else {
                        1.0
                    };
                }
            }
        }
        Gray { width, height, px }
    }

    #[test]
    fn a_small_board_beside_wide_margins_is_located() {
        let g = canvas(520, 300, &[(131, 22)], 256);
        let frame = locate(&g).expect("board inside its border lines");
        for (got, want) in frame.iter().zip([132, 23, 388, 279]) {
            assert!((got - want).abs() <= 3, "{frame:?}");
        }
    }

    #[test]
    fn fallback_uses_second_only_when_first_abstains() {
        assert_eq!(
            Fallback {
                primary: &Fixed(Some("new")),
                fallback: &Fixed(Some("old")),
            }
            .placement(&[]),
            Some("new".into())
        );
        assert_eq!(
            Fallback {
                primary: &Fixed(None),
                fallback: &Fixed(Some("old")),
            }
            .placement(&[]),
            Some("old".into())
        );
        assert_eq!(
            Fallback {
                primary: &Fixed(None),
                fallback: &Fixed(None),
            }
            .placement(&[]),
            None
        );
    }

    #[test]
    fn several_boards_in_one_image_abstain() {
        let g = canvas(620, 300, &[(10, 22), (330, 22)], 256);
        assert_eq!(locate(&g), None);
    }

    /// A diagram with a thick light (gold-like) rim and coordinates painted inside the rim — the
    /// dark-line detector skips the rim, so the inset fallback must still find the 8x8 field.
    #[test]
    fn a_board_inside_a_light_rim_is_located() {
        let size = 312usize;
        let rim = 14usize;
        let band = 12usize; // coordinate strip inside the rim
        let mut px = vec![0.55f32; size * size]; // light-brown rim
        let inner = rim + band;
        let field = size - 2 * inner;
        for y in 0..size {
            for x in 0..size {
                if x < rim || y < rim || x >= size - rim || y >= size - rim {
                    px[y * size + x] = 0.52; // gold-ish
                    continue;
                }
                if x < inner || y < inner || x >= size - inner || y >= size - inner {
                    px[y * size + x] = 0.85; // coordinate band
                    continue;
                }
                let col = (x - inner) * 8 / field;
                let row = (y - inner) * 8 / field;
                let dark = (row + col) % 2 == 1;
                px[y * size + x] = if dark { 0.78 } else { 0.96 };
            }
        }
        let g = Gray {
            width: size,
            height: size,
            px,
        };
        let frame = locate_with_light_rim(&g).expect("board inside light rim");
        let (w, h) = (frame[2] - frame[0], frame[3] - frame[1]);
        assert!(w >= 200 && h >= 200, "frame too small: {frame:?}");
        let rim_i = i64::try_from(rim).unwrap();
        assert!(frame[0] >= rim_i, "{frame:?}");
        assert!(frame[1] >= rim_i, "{frame:?}");
    }
}
