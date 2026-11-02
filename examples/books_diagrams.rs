//! Renders diagram images for generated book fixtures from FEN placements read on stdin:
//! `[{"file": "d00.png" | "d00.jpg", "placement": "...", "style": "screen" | "printed", "square": 28, "seed": 1}]`.
//! Images are drawn independently of the recognizer, from the app's piece SVGs.
// The shared renderer converts between pixel sizes and colour channels.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
#[path = "support/diagram_render.rs"]
mod render;
use serde_json::Value;
use std::{
    io::{self, Read},
    path::Path,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .ok_or("usage: books_diagrams <out dir>")?;
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let items: Vec<Value> = serde_json::from_str(&input)?;
    for item in items {
        let style = match item["style"].as_str() {
            Some("printed") => render::Style::Printed,
            _ => render::Style::Screen,
        };
        let png = render::render(
            item["placement"].as_str().ok_or("placement")?,
            style,
            u32::try_from(item["square"].as_u64().unwrap_or(28))?,
            item["seed"].as_u64().unwrap_or(1),
        );
        let file = item["file"].as_str().ok_or("file")?;
        let path = Path::new(&out).join(file);
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("jpg"))
        {
            // Scanned books ship JPEGs; lossy blocks also keep the fixture small.
            let image = image::load_from_memory(&png)?.into_luma8();
            let mut jpeg = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 65)
                .encode_image(&image)?;
            std::fs::write(path, jpeg)?;
        } else {
            std::fs::write(path, png)?;
        }
    }
    Ok(())
}
