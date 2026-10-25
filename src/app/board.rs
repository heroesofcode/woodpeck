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
    rect: Option<Rect>,
    pub promotion: Option<String>,
    previous: Option<Board>,
    moving: Vec<(Square, Square)>,
    last_move: Vec<Square>,
    changed: f64,
    snap_next: bool,
    flash: Option<(f64, bool)>,
    last_state: Option<State>,
}
impl BoardWidget {
    pub(super) fn drag_origin(&self, engine: &Engine) -> Pos2 {
        let position = engine.position().unwrap();
        let from = position.legal_moves().first().unwrap().from().unwrap();
        square_rect(
            self.rect.unwrap(),
            from,
            engine.orientation() != Some(Color::Black),
        )
        .center()
    }
    pub(super) fn is_selected(&self) -> bool {
        self.selected.is_some()
    }
    pub(super) fn player_moved(&mut self) {
        self.snap_next = true;
    }
    pub fn reset(&mut self) {
        self.snap_next = false;
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
        self.rect = Some(rect);
        let now = ui.input(|i| i.time);
        let board = position.board();
        self.update_board(board, engine.state(), now);
        self.paint_squares(ui, engine, rect, theme, white);
        self.paint_pieces(ui, board, rect, rendering, white, &response);
        self.paint_flash(ui, rect, now);
        let played = self.pointer_move(ui, engine, rect, white, &response);
        self.promotion_picker(ui, rect).or(played)
    }
    fn update_board(&mut self, board: &Board, state: State, now: f64) {
        if self.previous.as_ref() != Some(board) {
            self.moving.clear();
            self.last_move.clear();
            if let Some(old) = &self.previous {
                for to in Square::ALL {
                    if let Some(piece) = board.piece_at(to)
                        && old.piece_at(to) != Some(piece)
                        && let Some(from) = Square::ALL.into_iter().find(|&from| {
                            board.piece_at(from) != old.piece_at(from)
                                && (old.piece_at(from) == Some(piece)
                                    || (piece.role != shakmaty::Role::Pawn
                                        && to.rank()
                                            == if piece.color == Color::White {
                                                shakmaty::Rank::Eighth
                                            } else {
                                                shakmaty::Rank::First
                                            }
                                        && u32::from(to.rank()).abs_diff(u32::from(from.rank()))
                                            == 1
                                        && u32::from(to.file()).abs_diff(u32::from(from.file()))
                                            <= 1
                                        && old.piece_at(from)
                                            == Some(shakmaty::Piece {
                                                color: piece.color,
                                                role: shakmaty::Role::Pawn,
                                            })))
                        })
                    {
                        self.moving.push((from, to));
                        self.last_move.extend([from, to]);
                    }
                }
            }
            self.changed = if self.snap_next { now - 0.14 } else { now };
            self.snap_next = false;
            self.previous = Some(board.clone());
        }
        if self.last_state != Some(state) {
            if matches!(state, State::Incorrect | State::Skipped | State::Complete) {
                self.flash = Some((now, state == State::Complete));
            }
            self.last_state = Some(state);
        }
    }
    fn paint_squares(&self, ui: &Ui, engine: &Engine, rect: Rect, theme: BoardTheme, white: bool) {
        let Some(position) = engine.position() else {
            return;
        };
        let board = position.board();
        let sq = rect.width() / 8.0;
        let c = theme.colors();
        let painter = ui.painter();
        painter.rect_filled(
            rect.expand2(Vec2::new(0.0, 3.0))
                .translate(Vec2::new(0.0, 3.0)),
            8,
            color(c.lip),
        );
        let checked_king = position
            .is_check()
            .then(|| board.king_of(position.turn()))
            .flatten();
        let font = FontId::new(
            (sq * 0.19).clamp(9.0, 15.0),
            egui::FontFamily::Name("coordinates".into()),
        );
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
            if checked_king == Some(square) {
                for level in (1..=5).rev() {
                    painter.circle_filled(
                        r.center(),
                        sq * (0.17 + level as f32 * 0.055),
                        Color32::from_rgba_unmultiplied(216, 52, 48, 24),
                    );
                }
            }
            let label_color = if dark { color(c.light) } else { color(c.dark) };
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
                    font.clone(),
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
    }
    fn paint_pieces(
        &mut self,
        ui: &Ui,
        board: &Board,
        rect: Rect,
        rendering: PieceRendering,
        white: bool,
        response: &egui::Response,
    ) {
        let now = ui.input(|i| i.time);
        let sq = rect.width() / 8.0;
        let painter = ui.painter();
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
        if fraction < 1.0 && !self.moving.is_empty() {
            ui.ctx().request_repaint();
        }
    }
    fn paint_flash(&self, ui: &Ui, rect: Rect, now: f64) {
        let painter = ui.painter();
        if let Some((then, correct)) = self.flash
            && now - then < 0.22
        {
            let alpha = ((1.0 - (now - then) / 0.22) * 180.0) as u8;
            painter.rect_stroke(
                rect.shrink(1.5),
                0,
                Stroke::new(
                    3.0,
                    if correct {
                        Color32::from_rgba_unmultiplied(81, 195, 98, alpha)
                    } else {
                        Color32::from_rgba_unmultiplied(230, 55, 55, alpha)
                    },
                ),
                StrokeKind::Inside,
            );
            ui.ctx().request_repaint();
        }
    }
    fn pointer_move(
        &mut self,
        ui: &Ui,
        engine: &Engine,
        rect: Rect,
        white: bool,
        response: &egui::Response,
    ) -> Option<String> {
        let position = engine.position()?;
        let board = position.board();
        let size = rect.width();
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
                if let Some(m) = position
                    .legal_moves()
                    .into_iter()
                    .find(|m| m.from() == Some(from) && m.to() == to)
                {
                    if m.promotion().is_some() {
                        self.promotion = Some(format!("{from}{to}"));
                    } else {
                        played = Some(
                            UciMove::from_move(m, shakmaty::CastlingMode::Standard).to_string(),
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
        played
    }
    fn promotion_picker(&mut self, ui: &mut Ui, rect: Rect) -> Option<String> {
        let painter = ui.painter().clone();
        let size = rect.width();
        let mut played = None;
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

#[cfg(test)]
mod animation_tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn engine_moves_slide_including_promotions_and_player_moves_snap() {
        let mut puzzle = crate::puzzle::fixture();
        puzzle.fen = "7k/P7/8/8/8/8/8/7K w - - 0 1".into();
        puzzle.moves = "a7a8q h8g7".into();
        let mut engine = Engine::new(&puzzle, Duration::ZERO, false);
        let mut widget = BoardWidget::default();
        widget.update_board(engine.position().unwrap().board(), engine.state(), 0.0);
        assert_eq!(widget.moving.len(), 0);
        engine.update(Duration::from_millis(80));
        widget.update_board(engine.position().unwrap().board(), engine.state(), 0.08);
        assert_eq!(widget.moving, [(Square::A7, Square::A8)]);
        assert!((widget.changed - 0.08).abs() < f64::EPSILON);
        engine.update(Duration::from_millis(220));
        engine.play("h8g7", Duration::from_millis(300)).unwrap();
        widget.player_moved();
        widget.update_board(engine.position().unwrap().board(), engine.state(), 0.3);
        assert!((0.3 - widget.changed - 0.14).abs() < f64::EPSILON);
    }
}
