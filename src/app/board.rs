//! Responsive chessboard painting and pointer interaction.
use super::{pieces::Pieces, style::color};
use crate::{
    puzzle::{Engine, State},
    settings::{BoardTheme, PieceRendering},
};
use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use shakmaty::{Board, Color, Position, Square, uci::UciMove};

pub fn hit_square(point: [f32; 2], origin: [f32; 2], size: f32, white: bool) -> Option<Square> {
    if size <= 0.0 || !size.is_finite() {
        return None;
    }
    let x = (point[0] - origin[0]) / size;
    let y = (point[1] - origin[1]) / size;
    if !(0.0..1.0).contains(&x) || !(0.0..1.0).contains(&y) {
        return None;
    }
    let file = (x * 8.0).floor() as u32;
    let rank = (y * 8.0).floor() as u32;
    Some(Square::new(if white {
        (7 - rank) * 8 + file
    } else {
        rank * 8 + 7 - file
    }))
}
fn square_rect(rect: Rect, square: Square, white: bool) -> Rect {
    let file = u32::from(square.file());
    let rank = u32::from(square.rank());
    let (x, y) = if white {
        (file, 7 - rank)
    } else {
        (7 - file, rank)
    };
    Rect::from_min_size(
        rect.min + Vec2::new(x as f32, y as f32) * rect.width() / 8.0,
        Vec2::splat(rect.width() / 8.0),
    )
}
#[derive(Default)]
pub struct BoardWidget {
    pieces: Pieces,
    selected: Option<Square>,
    pub promotion: Option<String>,
    previous: Option<Board>,
    moving: Vec<(Square, Square)>,
    last_move: Vec<Square>,
    changed: f64,
    flash: Option<(f64, bool)>,
    last_state: Option<State>,
}
impl BoardWidget {
    pub fn reset(&mut self) {
        self.selected = None;
        self.promotion = None;
        self.previous = None;
        self.moving.clear();
        self.last_move.clear();
        self.last_state = None;
        self.flash = None;
    }
    pub fn promote(&mut self, role: char) -> Option<String> {
        self.promotion.take().map(|s| format!("{s}{role}"))
    }
    pub fn show(
        &mut self,
        ui: &mut Ui,
        engine: &Engine,
        size: f32,
        theme: BoardTheme,
        rendering: PieceRendering,
    ) -> Option<String> {
        let position = engine.position()?;
        let white = engine.orientation() != Some(Color::Black);
        let size = size.min(ui.available_width()).max(1.0);
        let (allocation, response) =
            ui.allocate_exact_size(Vec2::new(size, size + 6.0), Sense::click_and_drag());
        let rect = Rect::from_min_size(allocation.min, Vec2::splat(size));
        let sq = size / 8.0;
        let now = ui.input(|i| i.time);
        let board = position.board();
        if self.previous.as_ref() != Some(board) {
            self.moving.clear();
            self.last_move.clear();
            if let Some(old) = &self.previous {
                for to in Square::ALL {
                    if let Some(piece) = board.piece_at(to)
                        && old.piece_at(to) != Some(piece)
                        && let Some(from) = Square::ALL.into_iter().find(|&from| {
                            old.piece_at(from) == Some(piece) && board.piece_at(from) != Some(piece)
                        })
                    {
                        self.moving.push((from, to));
                        self.last_move.extend([from, to]);
                    }
                }
            }
            self.changed = now;
            self.previous = Some(board.clone());
        }
        if self.last_state != Some(engine.state()) {
            if matches!(
                engine.state(),
                State::Incorrect | State::Skipped | State::Complete
            ) {
                self.flash = Some((now, engine.state() == State::Complete));
            }
            self.last_state = Some(engine.state());
        }
        let c = theme.colors();
        let painter = ui.painter();
        painter.rect_filled(allocation, 8, color(c.lip));
        for square in Square::ALL {
            let r = square_rect(rect, square, white);
            let dark = (u32::from(square.file()) + u32::from(square.rank())).is_multiple_of(2);
            painter.rect_filled(r, 0, if dark { color(c.dark) } else { color(c.light) });
            if self.last_move.contains(&square) {
                painter.rect_filled(r, 0, Color32::from_rgba_unmultiplied(210, 217, 54, 78));
            }
            if self.selected == Some(square) {
                painter.rect_filled(r, 0, Color32::from_rgba_unmultiplied(226, 222, 60, 145));
            }
            if position.is_check() && board.king_of(position.turn()) == Some(square) {
                for level in (1..=5).rev() {
                    painter.circle_filled(
                        r.center(),
                        sq * (0.17 + level as f32 * 0.055),
                        Color32::from_rgba_unmultiplied(216, 52, 48, 24),
                    );
                }
            }
            let label_color = if dark { color(c.light) } else { color(c.dark) };
            let font = FontId::new(
                (sq * 0.19).clamp(9.0, 15.0),
                egui::FontFamily::Name("coordinates".into()),
            );
            if (white && square.file() == shakmaty::File::A)
                || (!white && square.file() == shakmaty::File::H)
            {
                painter.text(
                    r.min + Vec2::new(4.0, 3.0),
                    Align2::LEFT_TOP,
                    format!("{}", u32::from(square.rank()) + 1),
                    font.clone(),
                    label_color,
                );
            }
            if (white && square.rank() == shakmaty::Rank::First)
                || (!white && square.rank() == shakmaty::Rank::Eighth)
            {
                painter.text(
                    r.max - Vec2::new(4.0, 3.0),
                    Align2::RIGHT_BOTTOM,
                    format!("{}", square.file().char()),
                    font,
                    label_color,
                );
            }
        }
        if engine.state() == State::PlayerTurn
            && let Some(from) = self.selected
        {
            for m in position
                .legal_moves()
                .iter()
                .filter(|m| m.from() == Some(from))
            {
                let r = square_rect(rect, m.to(), white);
                if board.piece_at(m.to()).is_some() {
                    painter.circle_stroke(
                        r.center(),
                        sq * 0.43,
                        Stroke::new(sq * 0.07, Color32::from_black_alpha(65)),
                    );
                } else {
                    painter.circle_filled(r.center(), sq * 0.12, Color32::from_black_alpha(65));
                }
            }
        }
        let fraction = ((now - self.changed) / 0.14).clamp(0.0, 1.0) as f32;
        let dragging = response.dragged() && self.selected.is_some();
        for square in Square::ALL {
            if dragging && self.selected == Some(square) {
                continue;
            }
            if let Some(piece) = board.piece_at(square) {
                let mut r = square_rect(rect, square, white);
                if let Some((from, _)) = self.moving.iter().find(|(_, to)| *to == square) {
                    r = r.translate(
                        (square_rect(rect, *from, white).min - r.min) * (1.0 - fraction),
                    );
                }
                if let Some(texture) =
                    self.pieces
                        .texture(ui.ctx(), piece, sq, rendering == PieceRendering::Vector)
                {
                    painter.image(
                        texture.id(),
                        r.shrink(sq * 0.055),
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
            }
        }
        if dragging
            && let (Some(from), Some(pointer)) = (self.selected, response.interact_pointer_pos())
            && let Some(piece) = board.piece_at(from)
            && let Some(texture) =
                self.pieces
                    .texture(ui.ctx(), piece, sq, rendering == PieceRendering::Vector)
        {
            let center = Pos2::new(
                pointer
                    .x
                    .clamp(rect.left() + sq / 2.0, rect.right() - sq / 2.0),
                pointer
                    .y
                    .clamp(rect.top() + sq / 2.0, rect.bottom() - sq / 2.0),
            );
            painter.image(
                texture.id(),
                Rect::from_center_size(center, Vec2::splat(sq * 0.9)),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        if fraction < 1.0 {
            ui.ctx().request_repaint();
        }
        if let Some((then, correct)) = self.flash
            && now - then < 0.35
        {
            painter.rect_filled(
                rect,
                0,
                if correct {
                    Color32::from_rgba_unmultiplied(
                        81,
                        195,
                        98,
                        ((1.0 - (now - then) / 0.35) * 65.0) as u8,
                    )
                } else {
                    Color32::from_rgba_unmultiplied(
                        230,
                        55,
                        55,
                        ((1.0 - (now - then) / 0.35) * 80.0) as u8,
                    )
                },
            );
            ui.ctx().request_repaint();
        }
        let hit = |p: Pos2| hit_square([p.x, p.y], [rect.left(), rect.top()], size, white);
        let interactive = engine.state() == State::PlayerTurn && self.promotion.is_none();
        if interactive
            && response.drag_started()
            && let Some(origin) = ui.input(|i| i.pointer.press_origin())
            && let Some(square) = hit(origin)
            && board.color_at(square) == Some(position.turn())
        {
            self.selected = Some(square);
        }
        let mut played = None;
        if interactive
            && (response.clicked() || response.drag_stopped())
            && let Some(to) = response.interact_pointer_pos().and_then(hit)
        {
            if let Some(from) = self.selected
                && from != to
            {
                let candidates: Vec<_> = position
                    .legal_moves()
                    .into_iter()
                    .filter(|m| m.from() == Some(from) && m.to() == to)
                    .collect();
                if let Some(m) = candidates.first() {
                    if m.promotion().is_some() {
                        self.promotion = Some(format!("{from}{to}"));
                    } else {
                        played = Some(
                            UciMove::from_move(*m, shakmaty::CastlingMode::Standard).to_string(),
                        );
                    }
                    self.selected = None;
                } else {
                    self.selected = (board.color_at(to) == Some(position.turn())).then_some(to);
                }
            } else {
                self.selected = (board.color_at(to) == Some(position.turn())).then_some(to);
            }
        }
        if self.promotion.is_some() {
            let overlay = Rect::from_center_size(rect.center(), Vec2::new(size.min(340.0), 90.0));
            painter.rect_filled(overlay, 12, ui.visuals().window_fill);
            painter.rect_stroke(
                overlay,
                12,
                Stroke::new(2.0, ui.visuals().selection.bg_fill),
                StrokeKind::Inside,
            );
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(overlay.shrink(12.0)),
                |ui| {
                    ui.label("Promote to");
                    ui.horizontal(|ui| {
                        for (label, role) in [("Q", 'q'), ("R", 'r'), ("B", 'b'), ("N", 'n')] {
                            if ui.button(label).clicked() {
                                played = self.promote(role);
                            }
                        }
                    });
                },
            );
        }
        played
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn orientation_edges_and_scaling() {
        assert_eq!(
            hit_square([10.0, 20.0], [10.0, 20.0], 800.0, true),
            Some(Square::A8)
        );
        assert_eq!(
            hit_square([10.0, 20.0], [10.0, 20.0], 800.0, false),
            Some(Square::H1)
        );
        assert_eq!(
            hit_square([809.9, 819.9], [10.0, 20.0], 800.0, true),
            Some(Square::H1)
        );
        for p in [[810.0, 20.0], [9.0, 20.0], [10.0, 820.0]] {
            assert_eq!(hit_square(p, [10.0, 20.0], 800.0, true), None);
        }
        assert_eq!(hit_square([0.0, 0.0], [0.0, 0.0], 0.0, true), None);
        for white in [true, false] {
            for square in Square::ALL {
                let r = square_rect(
                    Rect::from_min_size(Pos2::new(12.0, 20.0), Vec2::splat(320.0)),
                    square,
                    white,
                );
                assert_eq!(
                    hit_square([r.center().x, r.center().y], [12.0, 20.0], 320.0, white),
                    Some(square)
                );
            }
        }
    }
}
