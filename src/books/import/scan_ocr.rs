//! Latin OCR for one rendered page, using the bundled ocrs weights.
//!
//! The engine is pure Rust (`rten`). A missing or unloadable model returns no lines, and the
//! caption binder then leaves the side unknown. This module does not run in the browser build.

use super::scan_caption::OcrLine;
use crate::books::diagram::Gray;
use ocrs_cjk::{ImageSource, OcrEngine, OcrEngineParams, TextItem};
use rten::Model;
use std::sync::OnceLock;

static ENGINE: OnceLock<Option<OcrEngine>> = OnceLock::new();

/// Lines found on `gray`, in raster pixels (y down). `None` when the engine cannot run.
pub(super) fn recognize(gray: &Gray) -> Option<Vec<OcrLine>> {
    let engine = ENGINE
        .get_or_init(|| {
            let detection =
                Model::load_static_slice(include_bytes!("../../../assets/ocr/text-detection.onnx"))
                    .ok()?;
            let recognition = Model::load_static_slice(include_bytes!(
                "../../../assets/ocr/text-recognition.onnx"
            ))
            .ok()?;
            OcrEngine::new(OcrEngineParams {
                detection_model: Some(detection),
                recognition_model: Some(recognition),
                ..OcrEngineParams::default()
            })
            .ok()
        })
        .as_ref()?;
    read_lines(engine, gray).ok()
}

fn read_lines(engine: &OcrEngine, gray: &Gray) -> Result<Vec<OcrLine>, ()> {
    let bytes = gray.luma();
    let width = u32::try_from(gray.width).map_err(|_| ())?;
    let height = u32::try_from(gray.height).map_err(|_| ())?;
    let source = ImageSource::from_bytes(&bytes, (width, height)).map_err(|_| ())?;
    let input = engine.prepare_input(source).map_err(|_| ())?;
    let words = engine.detect_words(&input).map_err(|_| ())?;
    let rows = engine.find_text_lines(&input, &words);
    let recognized = engine.recognize_text(&input, &rows).map_err(|_| ())?;
    let mut lines = Vec::new();
    for line in recognized.into_iter().flatten() {
        let text = line.to_string();
        if text.trim().is_empty() {
            continue;
        }
        let rect = line.bounding_rect();
        lines.push(OcrLine {
            text,
            confidence: line.confidence(),
            left: f64::from(rect.left()),
            top: f64::from(rect.top()),
            right: f64::from(rect.right()),
            bottom: f64::from(rect.bottom()),
        });
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    const DETECTION_SHA256: &str =
        "a917b23dbd9524b465df7e922641b3ff2981623df4ded5a0234004ef2fee7cfe";
    const RECOGNITION_SHA256: &str =
        "86c145c2edb96c8caed5b1ebb8f44d706408922211451c309c625157dd6061c5";

    #[test]
    fn bundled_weights_match_the_published_ocrs_models() {
        let detection = include_bytes!("../../../assets/ocr/text-detection.onnx");
        let recognition = include_bytes!("../../../assets/ocr/text-recognition.onnx");
        assert_eq!(hex(&Sha256::digest(detection)), DETECTION_SHA256);
        assert_eq!(hex(&Sha256::digest(recognition)), RECOGNITION_SHA256);
    }

    /// One synthetic scan: a bordered checkerboard and a Source Serif line drawn into the
    /// page image. Standard PDF fonts are not embedded, so the caption is painted here.
    #[test]
    fn a_rendered_page_reads_one_explicit_turn_line() {
        let (width, height) = (860_usize, 640_usize);
        let bytes = gray_page(&paint_caption(width, height), width, height);
        let renderer = crate::books::import::raster::Renderer::new(&bytes).expect("renderer");
        let placed = renderer.boards(0, &[]);
        assert_eq!(placed.len(), 1, "the painted board is the census");
        assert_eq!(placed[0].scan_side, Some(true));
    }

    fn paint_caption(width: usize, height: usize) -> Vec<u8> {
        use ab_glyph::{Font, FontVec, ScaleFont, point};
        let mut luma = vec![255_u8; width * height];
        let (left, top, side) = (80_i64, 70_i64, 280_i64);
        fill(
            &mut luma,
            width,
            left - 3,
            top - 3,
            left + side + 3,
            top + side + 3,
            0,
        );
        let cell = side / 8;
        for row in 0..8 {
            for col in 0..8 {
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
        let font = FontVec::try_from_vec(
            include_bytes!("../../../assets/fonts/SourceSerif4-Regular.ttf").to_vec(),
        )
        .expect("font");
        let phrase = "White to move";
        let scaled = font.as_scaled(46.0);
        let gray = crate::books::diagram::Gray::from_luma(width, height, &luma).expect("board");
        let crop = crate::books::diagram::page_boards(&gray)
            .into_iter()
            .next()
            .expect("one crop");
        let inner = crate::books::diagram::locate(&gray.crop(crop).expect("crop image"))
            .expect("board inside the crop");
        let board_left = crop[0].max(0) + inner[0];
        let board_right = crop[0].max(0) + inner[2];
        let board_bottom = crop[1].max(0) + inner[3];
        let board_width = (board_right - board_left) as f32;
        // Glyph tops sit inside the detector padding, past the minimum gap under the board.
        let gap = 0.045 * board_width + 2.0;
        assert!(
            board_bottom < crop[3] && (board_bottom as f32 + gap) < crop[3] as f32,
            "the caption starts in the crop padding, below the located board"
        );
        let glyph_top = phrase
            .chars()
            .filter_map(|ch| {
                let glyph = scaled
                    .glyph_id(ch)
                    .with_scale_and_position(scaled.scale(), point(0.0, 0.0));
                font.outline_glyph(glyph)
                    .map(|outlined| outlined.px_bounds().min.y)
            })
            .fold(f32::MAX, f32::min);
        let baseline = board_bottom as f32 + gap - glyph_top;
        let advances: f32 = phrase
            .chars()
            .map(|ch| scaled.h_advance(scaled.glyph_id(ch)))
            .sum();
        let mut caret = (board_left + board_right) as f32 / 2.0 - advances / 2.0;
        for ch in phrase.chars() {
            let id = scaled.glyph_id(ch);
            let glyph = id.with_scale_and_position(scaled.scale(), point(caret, baseline));
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|x, y, coverage| {
                    darken(
                        &mut luma,
                        width,
                        bounds.min.x + x as f32,
                        bounds.min.y + y as f32,
                        coverage,
                    );
                });
            }
            caret += scaled.h_advance(id);
        }
        luma
    }

    fn gray_page(luma: &[u8], width: usize, height: usize) -> Vec<u8> {
        use lopdf::{Object, Stream, dictionary};
        let image = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => i64::try_from(width).unwrap_or(0),
                "Height" => i64::try_from(height).unwrap_or(0),
                "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8,
            },
            luma.to_vec(),
        );
        let mut doc = lopdf::Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let image_id = doc.add_object(image);
        let content = format!("q {width} 0 0 {height} 0 0 cm /Im0 Do Q");
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![
                0.into(),
                0.into(),
                i64::try_from(width).unwrap_or(0).into(),
                i64::try_from(height).unwrap_or(0).into(),
            ],
            "Contents" => content_id,
            "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => image_id } },
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page.into()],
                "Count" => 1,
            }),
        );
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("pdf");
        bytes
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

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn darken(luma: &mut [u8], width: usize, x: f32, y: f32, coverage: f32) {
        let (Ok(x), Ok(y)) = (usize::try_from(x as i32), usize::try_from(y as i32)) else {
            return;
        };
        if x >= width || y >= luma.len() / width {
            return;
        }
        let ink = (255.0 * (1.0 - coverage.clamp(0.0, 1.0))).round() as u8;
        let pixel = &mut luma[y * width + x];
        *pixel = (*pixel).min(ink);
    }

    fn hex(bytes: &[u8]) -> String {
        use std::fmt::Write as _;
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            let _ = write!(out, "{byte:02x}");
        }
        out
    }
}
