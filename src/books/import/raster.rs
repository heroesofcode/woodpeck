//! Boards drawn without a picture of their own: scanned pages (one picture per page), vector
//! drawings and chess-font or glyph-built diagrams. The page is rendered, boards are found on
//! the raster, and each board crop joins the page as a picture, so it goes through the same
//! diagram gate as an embedded diagram image.
use super::fontboard::FontBoard;
use super::picture::Placed;
use crate::books::diagram::{self, Gray};
use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::hayro_syntax::page::Rotation;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings};
use std::sync::Arc;

/// Rendered page width in pixels: about 170 dpi for a 6x9 inch page.
const WIDTH: f32 = 1100.0;
/// Largest rendered side, so an oversized page cannot exhaust memory.
const MAX_SIDE: f32 = 3000.0;

/// A PDF opened for rendering, with a cache shared by its pages.
pub(super) struct Renderer {
    pdf: Pdf,
}

impl Renderer {
    pub(super) fn new(bytes: &[u8]) -> Option<Self> {
        Pdf::new(Arc::new(bytes.to_vec()))
            .ok()
            .map(|pdf| Self { pdf })
    }

    /// Page area in square points, if the page exists.
    pub(super) fn area(&self, index: usize) -> Option<f64> {
        let pages = self.pdf.pages();
        let (w, h) = pages.get(index)?.render_dimensions();
        Some(f64::from(w) * f64::from(h))
    }

    /// Board crops found on page `index` (0-based), top to bottom, as PNG pictures.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        clippy::too_many_lines
    )]
    /// A board also read from diagram-font glyphs is stored with that placement, which
    /// then takes precedence over reading its pixels.
    pub(super) fn boards(&self, index: usize, fonts: &[FontBoard]) -> Vec<Placed> {
        let pages = self.pdf.pages();
        let Some(page) = pages.get(index) else {
            return Vec::new();
        };
        // Crops are mapped back through the unrotated crop box; a rotated page is left as is.
        if !matches!(page.rotation(), Rotation::None) {
            return Vec::new();
        }
        let (pw, ph) = page.render_dimensions();
        if !(pw > 1.0 && ph > 1.0) {
            return Vec::new();
        }
        let (bottom, origin) = (page.crop_box().y0, page.crop_box().x0);
        let scale = (WIDTH / pw).min(MAX_SIDE / pw).min(MAX_SIDE / ph);
        let cache = RenderCache::new();
        let pixmap = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            hayro::render(
                page,
                &cache,
                &InterpreterSettings::default(),
                &RenderSettings::default(),
                &PixmapSettings {
                    x_scale: scale,
                    y_scale: scale,
                    bg_color: WHITE,
                },
            )
        }));
        let Ok(pixmap) = pixmap else {
            return Vec::new();
        };
        let (w, h) = (usize::from(pixmap.width()), usize::from(pixmap.height()));
        let luma: Vec<u8> = pixmap
            .data_as_u8_slice()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| ((u32::from(p[0]) * 3 + u32::from(p[1]) * 6 + u32::from(p[2])) / 10) as u8)
            .collect();
        let Some(gray) = Gray::from_luma(w, h, &luma) else {
            return Vec::new();
        };
        let detected = diagram::page_boards(&gray);
        let mut out = Vec::new();
        let mut boards = Vec::new();
        for frame in &detected {
            let frame = *frame;
            let Some(crop) = gray.crop(frame) else {
                continue;
            };
            let (Ok(width), Ok(height)) = (u32::try_from(crop.width), u32::try_from(crop.height))
            else {
                continue;
            };
            let Some(image) = image::GrayImage::from_raw(width, height, crop.luma()) else {
                continue;
            };
            let mut bytes = Vec::new();
            if image
                .write_to(
                    &mut std::io::Cursor::new(&mut bytes),
                    image::ImageFormat::Png,
                )
                .is_err()
            {
                continue;
            }
            // Page space has its origin at the bottom; the crop's top edge is its height.
            let top = bottom + f64::from(ph) * (1.0 - frame[1] as f64 / h as f64);
            let side = (frame[2] - frame[0]) as f64 / f64::from(scale);
            let left = origin + frame[0] as f64 / f64::from(scale);
            let low = top - (frame[3] - frame[1]) as f64 / f64::from(scale);
            // The glyph board whose centre lies inside this crop.
            let font = fonts.iter().find(|b| {
                let (cx, cy) = (
                    f64::midpoint(b.left, b.right),
                    f64::midpoint(b.top, b.bottom),
                );
                (left..left + side).contains(&cx) && (low..top).contains(&cy)
            });
            if let Some(font) = font {
                bytes = diagram::with_placement(&bytes, &font.placement);
            }
            // The crop carries a margin; the marker is placed against the board itself.
            let board = diagram::locate(&crop).map_or(frame, |f| {
                let (x, y) = (frame[0].max(0), frame[1].max(0));
                [x + f[0], y + f[1], x + f[2], y + f[3]]
            });
            boards.push(board);
            out.push(Placed {
                left,
                bottom: low,
                right: left + side,
                top,
                area: side * side,
                bytes,
                rendered: true,
                scan_side: None,
                scan_move: None,
                scan_replay: Vec::new(),
            });
        }
        // Squares and triangles are decided with every board in view, so a mark shared by two
        // diagrams abstains.
        stamp_boards(&gray, &boards, &mut out);
        attach_scan_captions(&gray, index, pw, ph, origin, bottom, &detected, &mut out);
        out
    }
}

/// OCR turn captions for this render. A dropped detector frame abstains the whole page.
#[allow(
    clippy::cast_precision_loss,
    clippy::similar_names,
    clippy::too_many_arguments
)]
fn attach_scan_captions(
    gray: &Gray,
    index: usize,
    pdf_width: f32,
    pdf_height: f32,
    crop_x0: f64,
    crop_y0: f64,
    detected: &[diagram::Frame],
    placed: &mut [Placed],
) {
    if detected.is_empty() || detected.len() != placed.len() {
        return;
    }
    let Ok(page_index) = u32::try_from(index) else {
        return;
    };
    let width = gray.width as f64;
    if !(pdf_width > 1.0 && width.is_finite()) {
        return;
    }
    let facts = super::scan_caption::RenderFacts {
        page_index,
        pdf_width: f64::from(pdf_width),
        pdf_height: f64::from(pdf_height),
        scale: width / f64::from(pdf_width),
        crop_x0,
        crop_y0,
    };
    let targets: Vec<_> = placed
        .iter()
        .map(|picture| super::scan_caption::ScanTarget {
            confirmed: super::scan_caption::stronger_side_present(&picture.bytes),
            eligible: true,
        })
        .collect();
    let Some(pending) = super::scan_caption::pending_sides(gray, facts, detected, &targets) else {
        return;
    };
    for ((picture, side), printed) in placed.iter_mut().zip(pending.sides).zip(pending.moves) {
        picture.scan_side = side;
        if side.is_none() {
            picture.scan_move = printed;
        }
        picture.scan_replay.clone_from(&pending.replayed);
    }
}

/// A bottom-right square or a Quality Chess triangle keeps the marker channel. Any other
/// square outranks a triangle found elsewhere.
fn stamp_boards(gray: &Gray, boards: &[diagram::Frame], placed: &mut [Placed]) {
    let squares = diagram::side_marks(gray, boards);
    let triangles = diagram::triangle_marks(gray, boards);
    for ((picture, board), (square, triangle)) in placed
        .iter_mut()
        .zip(boards)
        .zip(squares.into_iter().zip(triangles))
    {
        picture.bytes = stamp_side(
            gray,
            *board,
            square,
            triangle,
            std::mem::take(&mut picture.bytes),
        );
    }
}

/// Bottom-right and Quality Chess triangles keep the channel that can disagree with the text.
/// Any other printed square is stored aside. A triangle elsewhere is stored only when neither
/// of those fired, and the resolver applies it only while the side is unknown.
fn stamp_side(
    gray: &Gray,
    board: diagram::Frame,
    square: Option<bool>,
    triangle: Option<bool>,
    bytes: Vec<u8>,
) -> Vec<u8> {
    if let Some(white) =
        diagram::side_marker(gray, board).or_else(|| diagram::triangle_marker(gray, board))
    {
        diagram::with_side(&bytes, white)
    } else if let Some(white) = square {
        diagram::with_mark_side(&bytes, white)
    } else if let Some(white) = triangle {
        diagram::with_triangle_side(&bytes, white)
    } else {
        bytes
    }
}
