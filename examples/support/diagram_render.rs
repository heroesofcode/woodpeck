//! Synthetic diagram images from a FEN placement, drawn with the app's piece set. Two looks:
//! a flat screen board and a printed one with hatched dark squares, a frame and paper noise.
use resvg::{tiny_skia, usvg};
use std::path::Path;

#[derive(Clone, Copy)]
pub enum Style {
    Screen,
    Printed,
}

fn piece(letter: char, size: u32) -> Option<Vec<(f32, f32)>> {
    let color = if letter.is_ascii_uppercase() {
        'w'
    } else {
        'b'
    };
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "assets/pieces/cburnett/{color}{}.svg",
        letter.to_ascii_uppercase()
    ));
    let data = std::fs::read(path).ok()?;
    let tree = usvg::Tree::from_data(&data, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
    let scale = size as f32 / tree.size().width().max(tree.size().height());
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(
        pixmap
            .pixels()
            .iter()
            .map(|p| {
                let c = p.demultiply();
                let luma = (0.2126 * f32::from(c.red())
                    + 0.7152 * f32::from(c.green())
                    + 0.0722 * f32::from(c.blue()))
                    / 255.0;
                (luma, f32::from(c.alpha()) / 255.0)
            })
            .collect(),
    )
}

/// PNG bytes of a grayscale diagram, White at the bottom.
pub fn render(placement: &str, style: Style, square: u32, mut seed: u64) -> Vec<u8> {
    let (margin, border) = match style {
        Style::Screen => (0, 0),
        Style::Printed => (14, 3),
    };
    let board = square * 8;
    let side = board + 2 * (margin + border);
    let mut px = vec![1.0f32; (side * side) as usize];
    let at = |x: u32, y: u32| (y * side + x) as usize;
    for y in margin..side - margin {
        for x in margin..side - margin {
            px[at(x, y)] = 0.08;
        }
    }
    let origin = margin + border;
    let squares: Vec<Option<char>> = placement
        .split('/')
        .flat_map(|row| {
            row.chars().flat_map(|c| {
                c.to_digit(10)
                    .map_or_else(|| vec![Some(c)], |n| vec![None; n as usize])
            })
        })
        .collect();
    for (i, content) in squares.iter().enumerate().take(64) {
        let (row, col) = (i as u32 / 8, i as u32 % 8);
        let dark = (row + col) % 2 == 1;
        let (x0, y0) = (origin + col * square, origin + row * square);
        for y in 0..square {
            for x in 0..square {
                px[at(x0 + x, y0 + y)] = match (style, dark) {
                    (Style::Screen, false) => 0.86,
                    (Style::Screen, true) => 0.56,
                    (Style::Printed, false) => 0.96,
                    (Style::Printed, true) => {
                        if (x0 + x + y0 + y) % 5 < 2 {
                            0.2
                        } else {
                            0.94
                        }
                    }
                };
            }
        }
        if let Some(letter) = content
            && let Some(sprite) = piece(*letter, square)
        {
            for y in 0..square {
                for x in 0..square {
                    let (luma, alpha) = sprite[(y * square + x) as usize];
                    let p = &mut px[at(x0 + x, y0 + y)];
                    *p = *p * (1.0 - alpha) + luma * alpha;
                }
            }
        }
    }
    if matches!(style, Style::Printed) {
        for p in &mut px {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            *p += ((seed % 1000) as f32 / 1000.0 - 0.5) * 0.1;
        }
    }
    let bytes: Vec<u8> = px
        .iter()
        .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect();
    let image = image::GrayImage::from_raw(side, side, bytes).expect("sized buffer");
    let mut out = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut out, image::ImageFormat::Png)
        .expect("png encoding");
    out.into_inner()
}
