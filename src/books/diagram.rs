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

/// Quality Chess–style triangle beside the board: outline ▲ by the bottom rank for White,
/// filled ▼ by the top rank for Black. Used only when [`side_marker`] abstains. `Some(true)` is
/// White. Anything doubtful (two markers, wrong place, not a triangle) abstains.
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

/// Key of the PNG text chunk holding a side read from a printed square on any side of the board.
/// Unlike [`SIDE_KEY`], the resolver may use it only while the side is still unknown.
const BAND_SIDE_KEY: &[u8] = b"woodpeck-band-side";

/// A PNG with a side from [`band_sides`] stored beside the pixels.
pub fn with_band_side(png: &[u8], white: bool) -> Vec<u8> {
    with_text(png, BAND_SIDE_KEY, if white { "w" } else { "b" })
}

/// The side stored by [`with_band_side`]: `Some(true)` for White to move.
pub fn stated_band_side(bytes: &[u8]) -> Option<bool> {
    match text_chunk(bytes, BAND_SIDE_KEY)?.as_slice() {
        b"w" => Some(true),
        b"b" => Some(false),
        _ => None,
    }
}

/// Writes a printed side onto `png`.
///
/// `known` is a side the bottom-right square or the triangle marker already confirmed. It is
/// stored with [`with_side`] and `band` is ignored, so this never replaces a confirmed side.
/// `band` is the four-side search and is stored only while `known` is empty.
pub fn stamp_printed_side(png: &[u8], known: Option<bool>, band: Option<bool>) -> Vec<u8> {
    match (known, band) {
        (Some(white), _) => with_side(png, white),
        (None, Some(white)) => with_band_side(png, white),
        (None, None) => png.to_vec(),
    }
}

/// Smallest printed square, as a fraction of the board. Real marks are sometimes only about
/// eight pixels, which the bottom-right check's `0.04` gate skips.
const MIN_MARK: f64 = 0.03;
/// Largest printed square, as a fraction of the board. A caption line is larger than this.
const MAX_MARK: f64 = 0.13;
/// Interior luma range above this is a glyph (a letter, the rank label "8"), not a flat square.
const INTERIOR_RANGE: f32 = 0.4;
/// Interior at least this light is the paper inside a hollow square (White).
const HOLLOW_LUMA: f32 = 0.80;
/// Interior at most this dark is a filled or gray-filled square (Black).
const FILLED_LUMA: f32 = 0.70;

/// Side squares printed around `frames`, one optional side per board.
///
/// Hollow is White. Filled or gray-filled is Black. A thin arrow attached to the square, or
/// sitting next to it, is not a second mark. The search covers the four corners and the caption
/// line under the board. The result is empty when the ink is doubtful: two marks in one band,
/// two bands that disagree, a mark about as close to another diagram as to its own, or an
/// interior that is not one flat tone. Callers must ignore the result when a side is already
/// confirmed.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
pub fn band_sides(g: &Gray, frames: &[Frame]) -> Vec<Option<bool>> {
    frames
        .iter()
        .map(|frame| side_around(g, *frame, frames))
        .collect()
}

/// Reads [`band_sides`] on one embedded diagram and stores it, leaving bytes untouched when the
/// side is already known or the image is not a single board. Page rasters use [`band_sides`]
/// directly so a mark outside the crop, and a mark shared by two boards, can be seen.
pub fn attach_band_side(bytes: &[u8]) -> Vec<u8> {
    if stated_side(bytes).is_some() || stated_band_side(bytes).is_some() {
        return bytes.to_vec();
    }
    let Some(gray) = Gray::decode(bytes) else {
        return bytes.to_vec();
    };
    let Some(frame) = locate_with_light_rim(&gray) else {
        return bytes.to_vec();
    };
    let Some(white) = band_sides(&gray, &[frame]).into_iter().next().flatten() else {
        return bytes.to_vec();
    };
    let Some(png) = as_png(bytes) else {
        return bytes.to_vec();
    };
    with_band_side(&png, white)
}

fn as_png(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(bytes.to_vec());
    }
    let image = image::load_from_memory(bytes).ok()?;
    let mut out = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .ok()?;
    Some(out)
}

fn side_around(g: &Gray, frame: Frame, frames: &[Frame]) -> Option<bool> {
    let board = (frame[2] - frame[0]).min(frame[3] - frame[1]) as f64;
    if board < 80.0 {
        return None;
    }
    let mut by_band: [Vec<bool>; 5] = std::array::from_fn(|_| Vec::new());
    for (index, core) in corner_bands(frame).into_iter().enumerate() {
        let search = search_band(core, index, board);
        for comp in ink_components(g, search, board) {
            let class = classify_mark(g, &comp, board);
            let (cx, cy, tone) = match class {
                Mark::Ignore => continue,
                Mark::Doubt { cx, cy } => (cx, cy, None),
                Mark::Square { white, cx, cy } => (cx, cy, Some(white)),
            };
            if !point_in(core, cx, cy) || inside_board(cx, cy, frame) > 1.0 {
                continue;
            }
            let own = outside_gap(cx, cy, frame);
            let other = frames
                .iter()
                .filter(|other| **other != frame)
                .map(|other| outside_gap(cx, cy, *other))
                .fold(f64::INFINITY, f64::min);
            match nearer_board(own, other, board) {
                // The ink belongs to the diagram it actually sits against.
                Near::Neighbor => {}
                // About as close to two diagrams, or not clearly hollow or filled.
                Near::Ambiguous => return None,
                Near::Own => by_band[index].push(tone?),
            }
        }
    }
    let mut side = None;
    for marks in by_band {
        // One band holding two squares is doubtful, even when they name the same side.
        if marks.len() > 1 {
            return None;
        }
        if let Some(white) = marks.first().copied() {
            if side.is_some_and(|found| found != white) {
                return None;
            }
            side = Some(white);
        }
    }
    side
}

/// Right-bottom, right-top, left-bottom, left-top, then the caption line under the board.
/// Each box starts one pixel outside the frame so the board's own border is not part of a mark
/// that sits against it.
fn corner_bands(frame: Frame) -> [[i64; 4]; 5] {
    let board = (frame[2] - frame[0]).min(frame[3] - frame[1]) as f64;
    let along = round_px(0.25 * board);
    let past = round_px(0.06 * board);
    let reach = round_px(0.40 * board);
    let (x0, y0, x1, y1) = (frame[0], frame[1], frame[2], frame[3]);
    let cap = round_px(0.22 * board);
    let widen = round_px(0.08 * board);
    [
        [x1 + 1, y1 - along, x1 + reach, y1 + past],
        [x1 + 1, y0 - past, x1 + reach, y0 + along],
        [x0 - reach, y1 - along, x0, y1 + past],
        [x0 - reach, y0 - past, x0, y0 + along],
        [x0 - widen, y1 + 2, x1 + widen, y1 + cap],
    ]
}

/// Extra room for a thin arrow, on the side facing away from the board.
fn search_band(core: [i64; 4], band: usize, board: f64) -> [i64; 4] {
    let pad = round_px(0.12 * board);
    match band {
        0 | 1 => [core[0], core[1] - pad, core[2] + pad, core[3] + pad],
        2 | 3 => [core[0] - pad, core[1] - pad, core[2], core[3] + pad],
        _ => [core[0] - pad, core[1], core[2] + pad, core[3] + pad],
    }
}

enum Near {
    Own,
    Neighbor,
    Ambiguous,
}

/// A mark clearly closer to another diagram is that diagram's. One about as close to both is
/// doubtful. `1.25` is the "about as close" test: the other board must be plainly farther.
fn nearer_board(own: f64, other: f64, board: f64) -> Near {
    if !other.is_finite() {
        return Near::Own;
    }
    if other < own * 0.8 {
        return Near::Neighbor;
    }
    if other <= own.mul_add(1.25, 0.02 * board) {
        Near::Ambiguous
    } else {
        Near::Own
    }
}

fn outside_gap(x: f64, y: f64, frame: Frame) -> f64 {
    let (left, top, right, bottom) = (
        frame[0] as f64,
        frame[1] as f64,
        frame[2] as f64,
        frame[3] as f64,
    );
    let dx = if x < left {
        left - x
    } else if x > right {
        x - right
    } else {
        0.0
    };
    let dy = if y < top {
        top - y
    } else if y > bottom {
        y - bottom
    } else {
        0.0
    };
    dx.hypot(dy)
}

/// How far `x`, `y` sits inside the frame. Zero when it is outside.
fn inside_board(x: f64, y: f64, frame: Frame) -> f64 {
    let (left, top, right, bottom) = (
        frame[0] as f64,
        frame[1] as f64,
        frame[2] as f64,
        frame[3] as f64,
    );
    if x < left || x > right || y < top || y > bottom {
        return 0.0;
    }
    (x - left).min(right - x).min(y - top).min(bottom - y)
}

fn point_in(rect: [i64; 4], x: f64, y: f64) -> bool {
    x >= rect[0] as f64 && x < rect[2] as f64 && y >= rect[1] as f64 && y < rect[3] as f64
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn round_px(v: f64) -> i64 {
    if !v.is_finite() {
        return 0;
    }
    let rounded = v.round();
    if rounded >= i64::MAX as f64 {
        return i64::MAX;
    }
    if rounded <= i64::MIN as f64 {
        return i64::MIN;
    }
    rounded as i64
}

enum Mark {
    Ignore,
    Doubt { cx: f64, cy: f64 },
    Square { white: bool, cx: f64, cy: f64 },
}

struct Ink {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    pts: Vec<(usize, usize)>,
}

#[allow(clippy::many_single_char_names)]
fn ink_components(g: &Gray, rect: [i64; 4], board: f64) -> Vec<Ink> {
    let Some((x0, y0, x1, y1)) = clamp_rect(rect, g.width, g.height) else {
        return Vec::new();
    };
    let (w, h) = (x1 - x0, y1 - y0);
    // A mark plus a short arrow. Anything longer is a border, a word, or another board.
    let limit = (MAX_MARK * board * 4.0 + 8.0).ceil();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let limit = limit.max(0.0) as usize;
    let mut seen = vec![false; w.saturating_mul(h)];
    let mut out = Vec::new();
    for start in 0..w * h {
        if seen[start] || !ink_at(g, x0 + start % w, y0 + start / w) {
            continue;
        }
        let mut stack = vec![start];
        seen[start] = true;
        let (mut bx0, mut by0, mut bx1, mut by1) = (w, h, 0usize, 0usize);
        let mut pts = Vec::new();
        let mut junk = false;
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            (bx0, by0, bx1, by1) = (bx0.min(x), by0.min(y), bx1.max(x + 1), by1.max(y + 1));
            if !junk {
                pts.push((x0 + x, y0 + y));
                if pts.len() > 8_000 || bx1 - bx0 > limit || by1 - by0 > limit {
                    junk = true;
                    pts.clear();
                }
            }
            for (nx, ny) in eight_neighbors(x, y, w, h) {
                let n = ny * w + nx;
                if !seen[n] && ink_at(g, x0 + nx, y0 + ny) {
                    seen[n] = true;
                    stack.push(n);
                }
            }
        }
        if !junk && !pts.is_empty() {
            out.push(Ink {
                x0: x0 + bx0,
                y0: y0 + by0,
                x1: x0 + bx1,
                y1: y0 + by1,
                pts,
            });
        }
    }
    out
}

fn ink_at(g: &Gray, x: usize, y: usize) -> bool {
    g.at(x, y) < 0.5
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
fn eight_neighbors(x: usize, y: usize, w: usize, h: usize) -> impl Iterator<Item = (usize, usize)> {
    let (x, y, w, h) = (x as isize, y as isize, w as isize, h as isize);
    [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ]
    .into_iter()
    .filter_map(move |(dx, dy)| {
        let (nx, ny) = (x + dx, y + dy);
        (nx >= 0 && ny >= 0 && nx < w && ny < h).then_some((nx as usize, ny as usize))
    })
}

fn clamp_rect(rect: [i64; 4], width: usize, height: usize) -> Option<(usize, usize, usize, usize)> {
    let clamp = |v: i64, max: usize| {
        if v <= 0 {
            0
        } else {
            usize::try_from(v).unwrap_or(max).min(max)
        }
    };
    let (x0, y0) = (clamp(rect[0], width), clamp(rect[1], height));
    let (x1, y1) = (clamp(rect[2], width), clamp(rect[3], height));
    (x1 > x0 + 1 && y1 > y0 + 1).then_some((x0, y0, x1, y1))
}

fn classify_mark(g: &Gray, comp: &Ink, board: f64) -> Mark {
    let (bw, bh) = (comp.x1 - comp.x0, comp.y1 - comp.y0);
    let (short, long) = (bw.min(bh) as f64, bw.max(bh) as f64);
    if short < MIN_MARK * board || short > MAX_MARK * board + 0.5 || long > short * 4.0 + 2.0 {
        return Mark::Ignore;
    }
    let window = if (bw as f64 - bh as f64).abs() <= 0.15 * long {
        Some((comp.x0, comp.y0, comp.x1, comp.y1))
    } else {
        arrow_window(comp)
    };
    let Some((x0, y0, x1, y1)) = window else {
        return Mark::Ignore;
    };
    judge_square(g, x0, y0, x1, y1, board)
}

/// The square at the thick end of a component, when the rest is a thin arrow.
fn arrow_window(comp: &Ink) -> Option<(usize, usize, usize, usize)> {
    let (bw, bh) = (comp.x1 - comp.x0, comp.y1 - comp.y0);
    let short = bw.min(bh);
    if short < 4 {
        return None;
    }
    let ends: [(usize, usize, usize, usize); 2] = if bh >= bw {
        [
            (comp.x0, comp.y0, comp.x0 + short, comp.y0 + short),
            (comp.x0, comp.y1 - short, comp.x0 + short, comp.y1),
        ]
    } else {
        [
            (comp.x0, comp.y0, comp.x0 + short, comp.y0 + short),
            (comp.x1 - short, comp.y0, comp.x1, comp.y0 + short),
        ]
    };
    let total = comp.pts.len();
    let mut best = None;
    let mut best_in = 0usize;
    for (x0, y0, x1, y1) in ends {
        let inside = comp
            .pts
            .iter()
            .filter(|(x, y)| (x0..x1).contains(x) && (y0..y1).contains(y))
            .count();
        if inside <= best_in || inside * 2 <= total {
            continue;
        }
        let mut outside = Vec::new();
        for &(x, y) in &comp.pts {
            if !(x0..x1).contains(&x) || !(y0..y1).contains(&y) {
                outside.push((x, y));
            }
        }
        if outside.is_empty() {
            best = Some((x0, y0, x1, y1));
            best_in = inside;
            continue;
        }
        let (mut ox0, mut oy0, mut ox1, mut oy1) = (usize::MAX, usize::MAX, 0usize, 0usize);
        for (x, y) in outside {
            (ox0, oy0, ox1, oy1) = (ox0.min(x), oy0.min(y), ox1.max(x + 1), oy1.max(y + 1));
        }
        let thin = ox1.saturating_sub(ox0).min(oy1.saturating_sub(oy0)) <= (short / 3).max(3);
        if thin {
            best = Some((x0, y0, x1, y1));
            best_in = inside;
        }
    }
    best
}

fn judge_square(g: &Gray, x0: usize, y0: usize, x1: usize, y1: usize, board: f64) -> Mark {
    let side = (x1 - x0).max(y1 - y0) as f64;
    if side < MIN_MARK * board || side > MAX_MARK * board + 0.5 || !edges_inked(g, x0, y0, x1, y1) {
        return Mark::Ignore;
    }
    let Some((mean, range)) = interior_tone(g, x0, y0, x1, y1) else {
        return Mark::Ignore;
    };
    // A letter or a rank digit varies inside; a printed square is one tone.
    if range >= INTERIOR_RANGE {
        return Mark::Ignore;
    }
    let (cx, cy) = (
        f64::midpoint(x0 as f64, x1 as f64),
        f64::midpoint(y0 as f64, y1 as f64),
    );
    if mean >= HOLLOW_LUMA {
        Mark::Square {
            white: true,
            cx,
            cy,
        }
    } else if mean <= FILLED_LUMA {
        Mark::Square {
            white: false,
            cx,
            cy,
        }
    } else {
        Mark::Doubt { cx, cy }
    }
}

fn edges_inked(g: &Gray, x0: usize, y0: usize, x1: usize, y1: usize) -> bool {
    let (w, h) = (x1 - x0, y1 - y0);
    if w < 4 || h < 4 {
        return false;
    }
    let thick = (w.min(h) / 5).max(1);
    let row = |y: usize| {
        let dark = (x0..x1).filter(|&x| ink_at(g, x, y)).count();
        dark as f32 / w as f32
    };
    let col = |x: usize| {
        let dark = (y0..y1).filter(|&y| ink_at(g, x, y)).count();
        dark as f32 / h as f32
    };
    // One straight inked line on each side. The thickest row or column of the strip is enough,
    // so a two-pixel border still counts when the strip is wider than the stroke.
    (y0..y0 + thick).any(|y| row(y) >= 0.72)
        && (y1 - thick..y1).any(|y| row(y) >= 0.72)
        && (x0..x0 + thick).any(|x| col(x) >= 0.72)
        && (x1 - thick..x1).any(|x| col(x) >= 0.72)
}

/// Mean and range of the inner part of the square, away from its border.
fn interior_tone(gray: &Gray, x0: usize, y0: usize, x1: usize, y1: usize) -> Option<(f32, f32)> {
    let (width, height) = (x1 - x0, y1 - y0);
    let (ix, iy) = (width * 3 / 10, height * 3 / 10);
    if ix == 0 || iy == 0 || x0 + ix >= x1 - ix || y0 + iy >= y1 - iy {
        return None;
    }
    let mut lo = 1.0f32;
    let mut hi = 0.0f32;
    let mut sum = 0.0f32;
    let mut count = 0u32;
    for y in (y0 + iy)..(y1 - iy) {
        for x in (x0 + ix)..(x1 - ix) {
            let tone = gray.at(x, y);
            lo = lo.min(tone);
            hi = hi.max(tone);
            sum += tone;
            count += 1;
        }
    }
    (count > 0).then_some((sum / count as f32, hi - lo))
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

    /// How a synthetic square is painted. `Glyph` is a hollow square with a bar through it.
    #[derive(Clone, Copy)]
    enum Paint {
        Hollow,
        Filled,
        Gray,
        Glyph,
    }

    struct Blob {
        x: usize,
        y: usize,
        side: usize,
        paint: Paint,
        /// A thin stroke attached to the bottom of the square.
        arrow: bool,
    }

    fn blob(x: usize, y: usize, side: usize, paint: Paint) -> Blob {
        Blob {
            x,
            y,
            side,
            paint,
            arrow: false,
        }
    }

    /// Blank page, board frames, and printed squares. Frames are outlines only.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]
    fn scene(width: usize, height: usize, frames: &[Frame], blobs: &[Blob]) -> Gray {
        let mut luma = vec![255u8; width * height];
        let set = |luma: &mut [u8], x: usize, y: usize, v: u8| {
            if x < width && y < height {
                luma[y * width + x] = v;
            }
        };
        for frame in frames {
            for x in frame[0].max(0)..=frame[2].min(width as i64 - 1) {
                let x = x as usize;
                set(&mut luma, x, frame[1].max(0) as usize, 0);
                set(&mut luma, x, frame[3].min(height as i64 - 1) as usize, 0);
            }
            for y in frame[1].max(0)..=frame[3].min(height as i64 - 1) {
                let y = y as usize;
                set(&mut luma, frame[0].max(0) as usize, y, 0);
                set(&mut luma, frame[2].min(width as i64 - 1) as usize, y, 0);
            }
        }
        for blob in blobs {
            let stroke = if blob.side < 14 { 1 } else { 2 };
            for dy in 0..blob.side {
                for dx in 0..blob.side {
                    let border = dx < stroke
                        || dy < stroke
                        || dx + stroke >= blob.side
                        || dy + stroke >= blob.side;
                    let bar = blob.side / 2 == dy || blob.side / 2 == dy + 1;
                    let value = match blob.paint {
                        Paint::Hollow => u8::from(!border).saturating_mul(255),
                        Paint::Filled => 0,
                        Paint::Gray => {
                            if border {
                                0
                            } else {
                                128
                            }
                        }
                        Paint::Glyph => u8::from(!(border || bar)).saturating_mul(255),
                    };
                    set(&mut luma, blob.x + dx, blob.y + dy, value);
                }
            }
            if blob.arrow {
                let ax = blob.x + blob.side / 2;
                for dy in 0..blob.side {
                    set(&mut luma, ax, blob.y + blob.side + dy, 0);
                    set(&mut luma, ax + 1, blob.y + blob.side + dy, 0);
                }
            }
        }
        Gray::from_luma(width, height, &luma).unwrap()
    }

    #[test]
    fn band_mark_reads_hollow_filled_and_gray() {
        let frame = [50, 50, 350, 350];
        let page = |paint| scene(500, 450, &[frame], &[blob(360, 300, 16, paint)]);
        assert_eq!(band_sides(&page(Paint::Hollow), &[frame]), vec![Some(true)]);
        assert_eq!(
            band_sides(&page(Paint::Filled), &[frame]),
            vec![Some(false)]
        );
        assert_eq!(band_sides(&page(Paint::Gray), &[frame]), vec![Some(false)]);
        // Eight pixels is below 0.03 of a 300 px board, and above it on a 240 px board.
        let large = scene(500, 450, &[frame], &[blob(360, 300, 8, Paint::Hollow)]);
        assert_eq!(band_sides(&large, &[frame]), vec![None]);
        let small_frame = [30, 30, 270, 270];
        let small = scene(
            420,
            360,
            &[small_frame],
            &[blob(280, 240, 8, Paint::Hollow)],
        );
        assert_eq!(band_sides(&small, &[small_frame]), vec![Some(true)]);
    }

    #[test]
    fn band_mark_is_read_on_every_side_and_the_caption() {
        let frame = [50, 50, 350, 350];
        let spots = [
            blob(360, 300, 16, Paint::Hollow), // right-bottom
            blob(360, 55, 16, Paint::Filled),  // right-top
            blob(10, 300, 16, Paint::Hollow),  // left-bottom
            blob(10, 55, 16, Paint::Gray),     // left-top
            blob(160, 360, 16, Paint::Filled), // caption line
        ];
        let expect = [
            Some(true),
            Some(false),
            Some(true),
            Some(false),
            Some(false),
        ];
        for (mark, side) in spots.into_iter().zip(expect) {
            let at = (mark.x, mark.y);
            let image = scene(500, 450, &[frame], &[mark]);
            assert_eq!(band_sides(&image, &[frame]), vec![side], "{at:?}");
        }
    }

    #[test]
    fn band_mark_keeps_a_square_with_a_thin_arrow() {
        let frame = [50, 50, 350, 350];
        let mut attached = blob(360, 290, 16, Paint::Hollow);
        attached.arrow = true;
        let image = scene(500, 450, &[frame], &[attached]);
        assert_eq!(band_sides(&image, &[frame]), vec![Some(true)]);
        // A thin stroke beside the square, not touching it, is not a second mark.
        let mut image = scene(500, 450, &[frame], &[blob(360, 290, 16, Paint::Gray)]);
        for y in 294..320 {
            image.px[y * image.width + 390] = 0.0;
            image.px[y * image.width + 391] = 0.0;
        }
        assert_eq!(band_sides(&image, &[frame]), vec![Some(false)]);
    }

    #[test]
    fn band_mark_rejects_a_glyph_like_blob() {
        let frame = [50, 50, 350, 350];
        // A bar through the square varies inside, the way a bold "8" or "B" does.
        let image = scene(500, 450, &[frame], &[blob(360, 290, 24, Paint::Glyph)]);
        assert_eq!(band_sides(&image, &[frame]), vec![None]);
        // The glyph does not veto a flat square in another band.
        let image = scene(
            500,
            450,
            &[frame],
            &[
                blob(360, 300, 16, Paint::Hollow),
                blob(8, 48, 24, Paint::Glyph),
            ],
        );
        assert_eq!(band_sides(&image, &[frame]), vec![Some(true)]);
    }

    #[test]
    fn band_mark_abstains_on_conflict_or_a_shared_gap() {
        let frame = [50, 50, 350, 350];
        // Two squares in the right-bottom band.
        let image = scene(
            520,
            450,
            &[frame],
            &[
                blob(360, 300, 16, Paint::Hollow),
                blob(410, 300, 16, Paint::Hollow),
            ],
        );
        assert_eq!(band_sides(&image, &[frame]), vec![None]);
        // Two bands that name different sides.
        let image = scene(
            500,
            450,
            &[frame],
            &[
                blob(360, 300, 16, Paint::Hollow),
                blob(10, 55, 16, Paint::Filled),
            ],
        );
        assert_eq!(band_sides(&image, &[frame]), vec![None]);
        // A square halfway between two boards is claimed by neither.
        let left = [40, 40, 240, 240];
        let right = [300, 40, 500, 240];
        let image = scene(
            540,
            280,
            &[left, right],
            &[blob(263, 200, 14, Paint::Hollow)],
        );
        assert_eq!(band_sides(&image, &[left, right]), vec![None, None]);
        // A square against the left board stays, even though the right board is on the page.
        let image = scene(
            540,
            280,
            &[left, right],
            &[blob(248, 200, 12, Paint::Filled)],
        );
        assert_eq!(band_sides(&image, &[left, right]), vec![Some(false), None]);
    }

    #[test]
    fn a_confirmed_printed_side_is_not_replaced_by_a_band_reading() {
        let image = image::GrayImage::from_raw(2, 2, vec![0, 255, 255, 0]).unwrap();
        let mut png = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let kept = stamp_printed_side(&png, Some(true), Some(false));
        assert_eq!(stated_side(&kept), Some(true));
        assert_eq!(stated_band_side(&kept), None);
        let filled = stamp_printed_side(&png, None, Some(false));
        assert_eq!(stated_side(&filled), None);
        assert_eq!(stated_band_side(&filled), Some(false));
        // An image that already carries a confirmed side is left alone.
        let again = attach_band_side(&kept);
        assert_eq!(stated_side(&again), Some(true));
        assert_eq!(stated_band_side(&again), None);
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
