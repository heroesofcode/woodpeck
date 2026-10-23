//! Embedded cburnett SVGs, rasterized at the actual physical square size.
use eframe::egui::{self, TextureHandle};
use shakmaty::{Color, Piece, Role};
use std::collections::HashMap;
const SVG: [&[u8]; 12] = [
    include_bytes!("../../assets/pieces/cburnett/wP.svg"),
    include_bytes!("../../assets/pieces/cburnett/wN.svg"),
    include_bytes!("../../assets/pieces/cburnett/wB.svg"),
    include_bytes!("../../assets/pieces/cburnett/wR.svg"),
    include_bytes!("../../assets/pieces/cburnett/wQ.svg"),
    include_bytes!("../../assets/pieces/cburnett/wK.svg"),
    include_bytes!("../../assets/pieces/cburnett/bP.svg"),
    include_bytes!("../../assets/pieces/cburnett/bN.svg"),
    include_bytes!("../../assets/pieces/cburnett/bB.svg"),
    include_bytes!("../../assets/pieces/cburnett/bR.svg"),
    include_bytes!("../../assets/pieces/cburnett/bQ.svg"),
    include_bytes!("../../assets/pieces/cburnett/bK.svg"),
];
#[derive(Default)]
pub struct Pieces {
    cache: HashMap<(usize, u32, bool), TextureHandle>,
}
impl Pieces {
    pub fn texture(
        &mut self,
        ctx: &egui::Context,
        piece: Piece,
        size: f32,
        smooth: bool,
    ) -> Option<&TextureHandle> {
        let role = match piece.role {
            Role::Pawn => 0,
            Role::Knight => 1,
            Role::Bishop => 2,
            Role::Rook => 3,
            Role::Queen => 4,
            Role::King => 5,
        };
        let index = role + if piece.color == Color::Black { 6 } else { 0 };
        let pixels = (size * ctx.pixels_per_point()).ceil().clamp(16.0, 1024.0) as u32;
        if !self.cache.contains_key(&(index, pixels, smooth)) {
            let tree =
                resvg::usvg::Tree::from_data(SVG[index], &resvg::usvg::Options::default()).ok()?;
            let mut pixmap = resvg::tiny_skia::Pixmap::new(pixels, pixels)?;
            resvg::render(
                &tree,
                resvg::tiny_skia::Transform::from_scale(
                    pixels as f32 / tree.size().width(),
                    pixels as f32 / tree.size().height(),
                ),
                &mut pixmap.as_mut(),
            );
            let image = egui::ColorImage::from_rgba_premultiplied(
                [pixels as usize, pixels as usize],
                pixmap.data(),
            );
            if self.cache.len() > 96 {
                self.cache.clear();
            }
            self.cache.insert(
                (index, pixels, smooth),
                ctx.load_texture(
                    format!("piece-{index}-{pixels}"),
                    image,
                    if smooth {
                        egui::TextureOptions::LINEAR
                    } else {
                        egui::TextureOptions::NEAREST
                    },
                ),
            );
        }
        self.cache.get(&(index, pixels, smooth))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_embedded_pieces_rasterize_without_external_resources() {
        for svg in SVG {
            let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default()).unwrap();
            let mut pixels = resvg::tiny_skia::Pixmap::new(90, 90).unwrap();
            resvg::render(
                &tree,
                resvg::tiny_skia::Transform::from_scale(
                    90.0 / tree.size().width(),
                    90.0 / tree.size().height(),
                ),
                &mut pixels.as_mut(),
            );
            assert!(pixels.data().as_chunks::<4>().0.iter().any(|p| p[3] > 0));
        }
    }
}
