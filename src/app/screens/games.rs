//! Games hub and Board-scan drill UI (desktop, web desktop, and phone).
use super::super::{Screen, Woodpeck, board::TapHighlight, style, widgets};
use crate::games::{
    self, BlindPhase, BlindSession, BlindTick, DrillMode, Highlight, SafeSession, SafeStatus,
    VisionOutcome, VisionSession, VisionStatus,
};
use crate::puzzle::Engine;
use eframe::egui::{self, RichText, Ui};
use shakmaty::{Color, Square};
use std::time::Duration;

impl Woodpeck {
    pub(in crate::app) fn games_hub(&mut self, ui: &mut Ui) {
        widgets::title(
            ui,
            "TRAINING GAMES",
            "Short drills for vision and feel",
            "Pick a drill. They work the same on desktop and on your phone.",
        );
        let wide = ui.available_width() >= 720.0;
        let card_width = if wide {
            ((ui.available_width() - 12.0) / 2.0).max(260.0)
        } else {
            ui.available_width()
        };
        egui::Grid::new("games-catalog")
            .num_columns(if wide { 2 } else { 1 })
            .spacing([12.0, 12.0])
            .min_col_width(card_width)
            .show(ui, |ui| {
                for (index, game) in games::CATALOG.iter().enumerate() {
                    self.game_card(ui, game, card_width);
                    if !wide || index % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
    }

    fn game_card(&mut self, ui: &mut Ui, game: &games::GameInfo, width: f32) {
        widgets::card(ui)
            .fill(
                ui.visuals()
                    .widgets
                    .noninteractive
                    .bg_fill
                    .gamma_multiply(if game.available { 1.0 } else { 0.7 }),
            )
            .show(ui, |ui| {
                ui.set_width((width - 1.0).max(0.0));
                ui.label(RichText::new(game.title).strong().size(20.0));
                ui.add_space(4.0);
                ui.label(game.blurb);
                ui.add_space(10.0);
                if game.available {
                    if widgets::primary(ui, "Play").clicked() {
                        match game.id {
                            "board-scan" => self.start_board_scan(DrillMode::HangingPieces),
                            "safe-move" => self.start_safe_move(),
                            "blind-line" => self.start_blind_line(),
                            _ => {}
                        }
                    }
                } else {
                    ui.label(
                        RichText::new("Coming in the next update")
                            .italics()
                            .color(ui.visuals().weak_text_color()),
                    );
                }
            });
    }

    pub(in crate::app) fn start_board_scan(&mut self, mode: DrillMode) {
        let seed = self
            .origin
            .elapsed()
            .as_millis()
            .wrapping_add(u128::from(self.frames)) as u64;
        let mut session = VisionSession::start(mode, seed.max(1));
        let now = self.now();
        let limit = session.round.time_left(Duration::ZERO);
        session.round.deadline = now.saturating_add(limit);
        self.game_vision = Some(session);
        self.games_board = super::super::board::BoardWidget::default();
        self.navigate(Screen::BoardScan);
    }

    pub(in crate::app) fn board_scan_screen(&mut self, ui: &mut Ui) {
        let narrow = ui.available_width() < 640.0;
        self.board_scan_header(ui, narrow);
        if self.game_vision.is_none() {
            ui.label("Starting…");
            return;
        }
        let now = self.now();
        if let Some(session) = self.game_vision.as_mut() {
            session.tick(now);
        }
        self.board_scan_stats(ui, narrow);
        self.board_scan_board(ui, narrow);
        self.board_scan_footer(ui);
    }

    fn board_scan_header(&mut self, ui: &mut Ui, narrow: bool) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("< Games").clicked() {
                self.game_vision = None;
                self.navigate(Screen::Games);
            }
            ui.label(
                RichText::new("Board scan")
                    .strong()
                    .size(if narrow { 18.0 } else { 22.0 }),
            );
        });
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            for mode in DrillMode::ALL {
                let selected = self.game_vision.as_ref().is_some_and(|s| s.mode == mode);
                if ui.selectable_label(selected, mode.label()).clicked() && !selected {
                    self.start_board_scan(mode);
                }
            }
        });
        ui.add_space(8.0);
    }

    fn board_scan_stats(&self, ui: &mut Ui, narrow: bool) {
        let Some(session) = self.game_vision.as_ref() else {
            return;
        };
        ui.label(RichText::new(session.round.mode.prompt()).size(if narrow { 14.0 } else { 15.0 }));
        ui.add_space(6.0);
        let secs = session.round.time_left(self.now()).as_secs_f32().ceil() as u32;
        let accent = style::color(self.palette.semantic().accent);
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Score {}", session.score));
            ui.separator();
            ui.label(format!("Streak {}", session.streak));
            ui.separator();
            ui.label(format!("Best {}", session.best_streak));
            ui.separator();
            ui.label(format!("Left {}", session.round.remaining_targets()));
            ui.separator();
            let timer = RichText::new(format!("{secs}s")).strong();
            ui.label(if session.round.status == VisionStatus::Playing {
                timer
            } else {
                timer.color(accent)
            });
        });
        ui.add_space(8.0);
    }

    fn board_scan_board(&mut self, ui: &mut Ui, narrow: bool) {
        let Some(session) = self.game_vision.as_ref() else {
            return;
        };
        let board_budget = if narrow {
            ui.available_width()
                .min((ui.available_height() - 120.0).max(180.0))
        } else {
            ui.available_height()
                .min(ui.available_width() * 0.62)
                .clamp(280.0, 560.0)
        };
        let position = session.round.position();
        let white_bottom = session.round.white_bottom;
        let tap_highlights: Vec<(Square, TapHighlight)> = session
            .round
            .highlights()
            .iter()
            .map(|(sq, h)| (*sq, tap_highlight(*h)))
            .collect();
        let theme = self.settings.board_theme;
        let rendering = self.settings.piece_rendering;
        let tapped = self.games_board.show_tap(
            ui,
            &position,
            board_budget,
            theme,
            rendering,
            white_bottom,
            &tap_highlights,
        );
        if let Some(square) = tapped {
            let outcome = self.game_vision.as_mut().unwrap().tap(square);
            if outcome == VisionOutcome::Miss {
                self.toast = Some(("Not a target.".into(), web_time::Instant::now()));
            }
        }
        ui.add_space(10.0);
    }

    fn board_scan_footer(&mut self, ui: &mut Ui) {
        let Some(status) = self.game_vision.as_ref().map(|s| s.round.status) else {
            return;
        };
        match status {
            VisionStatus::Playing => {
                ui.label("Tap each matching piece. Empty squares are ignored.");
                ui.ctx().request_repaint_after(Duration::from_millis(100));
            }
            VisionStatus::Cleared => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Clear!").strong());
                    if widgets::primary(ui, "Next board").clicked() {
                        let now = self.now();
                        self.game_vision.as_mut().unwrap().next_round(now);
                    }
                });
                ui.ctx().request_repaint_after(Duration::from_millis(16));
            }
            VisionStatus::TimedOut => {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Time's up. Missed targets are marked in yellow.");
                    if widgets::primary(ui, "Try another").clicked() {
                        let now = self.now();
                        self.game_vision.as_mut().unwrap().next_round(now);
                    }
                });
            }
        }
    }

    pub(in crate::app) fn start_safe_move(&mut self) {
        let seed = self
            .origin
            .elapsed()
            .as_millis()
            .wrapping_add(u128::from(self.frames)) as u64;
        let session = SafeSession::start(seed.max(1));
        let engine = Engine::sandbox(session.round.position());
        self.game_safe = Some(session);
        self.game_engine = Some(engine);
        self.games_board = super::super::board::BoardWidget::default();
        self.navigate(Screen::SafeMove);
    }

    pub(in crate::app) fn safe_move_screen(&mut self, ui: &mut Ui) {
        let narrow = ui.available_width() < 640.0;
        ui.horizontal_wrapped(|ui| {
            if ui.button("< Games").clicked() {
                self.game_safe = None;
                self.game_engine = None;
                self.navigate(Screen::Games);
            }
            ui.label(
                RichText::new("Safe move")
                    .strong()
                    .size(if narrow { 18.0 } else { 22.0 }),
            );
        });
        ui.add_space(8.0);
        if self.game_safe.is_none() || self.game_engine.is_none() {
            ui.label("Starting…");
            return;
        }
        let (score, streak, best, status) = {
            let s = self.game_safe.as_ref().unwrap();
            (s.score, s.streak, s.best_streak, s.round.status)
        };
        ui.label(
            RichText::new("Play any solid move. Hanging a piece ends the streak.")
                .size(if narrow { 14.0 } else { 15.0 }),
        );
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Score {score}"));
            ui.separator();
            ui.label(format!("Streak {streak}"));
            ui.separator();
            ui.label(format!("Best {best}"));
        });
        ui.add_space(8.0);

        let board_budget = if narrow {
            ui.available_width()
                .min((ui.available_height() - 120.0).max(180.0))
        } else {
            ui.available_height()
                .min(ui.available_width() * 0.62)
                .clamp(280.0, 560.0)
        };

        let played = {
            let engine = self.game_engine.as_ref().unwrap();
            self.games_board.show(
                ui,
                engine,
                board_budget,
                self.settings.board_theme,
                self.settings.piece_rendering,
            )
        };
        if let Some(uci) = played
            && status == SafeStatus::Playing
        {
            let outcome = self.game_safe.as_mut().unwrap().submit(&uci);
            match outcome {
                SafeStatus::Cleared | SafeStatus::Blundered => {
                    let ok = outcome == SafeStatus::Cleared;
                    let engine = self.game_engine.as_mut().unwrap();
                    let _ = engine.play_any(&uci);
                    engine.resolve_sandbox(ok);
                    for event in engine.take_sounds() {
                        if self.screenshot.is_none() {
                            crate::sound::play(event, &self.settings);
                        }
                    }
                    self.games_board.player_moved();
                    let toast = if ok {
                        "Safe."
                    } else {
                        "Blunder — streak lost."
                    };
                    self.toast = Some((toast.into(), web_time::Instant::now()));
                }
                SafeStatus::Playing => {}
            }
        }
        ui.add_space(10.0);
        let status = self.game_safe.as_ref().unwrap().round.status;
        match status {
            SafeStatus::Playing => {
                ui.label("Drag or tap a move. Any non-blunder keeps the streak alive.");
            }
            SafeStatus::Cleared | SafeStatus::Blundered => {
                let label = if status == SafeStatus::Cleared {
                    "Next position"
                } else {
                    "Try another"
                };
                if widgets::primary(ui, label).clicked() {
                    self.game_safe.as_mut().unwrap().next_round();
                    let position = self.game_safe.as_ref().unwrap().round.position();
                    self.game_engine = Some(Engine::sandbox(position));
                    self.games_board = super::super::board::BoardWidget::default();
                }
            }
        }
    }

    pub(in crate::app) fn start_blind_line(&mut self) {
        let seed = self
            .origin
            .elapsed()
            .as_millis()
            .wrapping_add(u128::from(self.frames)) as u64;
        let now = self.now();
        let session = BlindSession::start(seed.max(1), now);
        let engine = Engine::sandbox(session.round.start_position());
        self.game_blind = Some(session);
        self.game_engine = Some(engine);
        self.games_board = super::super::board::BoardWidget::default();
        self.navigate(Screen::BlindLine);
    }

    #[allow(clippy::too_many_lines)]
    pub(in crate::app) fn blind_line_screen(&mut self, ui: &mut Ui) {
        let narrow = ui.available_width() < 640.0;
        ui.horizontal_wrapped(|ui| {
            if ui.button("< Games").clicked() {
                self.game_blind = None;
                self.game_engine = None;
                self.navigate(Screen::Games);
            }
            ui.label(
                RichText::new("Blind line")
                    .strong()
                    .size(if narrow { 18.0 } else { 22.0 }),
            );
        });
        ui.add_space(8.0);
        if self.game_blind.is_none() || self.game_engine.is_none() {
            ui.label("Starting…");
            return;
        }

        let now = self.now();
        match self.game_blind.as_mut().unwrap().tick(now) {
            Some(BlindTick::Preview(uci)) => {
                let engine = self.game_engine.as_mut().unwrap();
                let _ = engine.play_any(&uci);
                for event in engine.take_sounds() {
                    if self.screenshot.is_none() {
                        crate::sound::play(event, &self.settings);
                    }
                }
                self.games_board.player_moved();
                // Keep preview read-only so phone/desktop taps cannot disturb the line.
                let white = engine.orientation() != Some(Color::Black);
                self.games_board.reader_orientation = Some(white);
            }
            Some(BlindTick::ReadyToAnswer) => {
                let side = self.game_engine.as_ref().unwrap().orientation();
                let position = self
                    .game_blind
                    .as_ref()
                    .unwrap()
                    .round
                    .position_after_preview();
                let mut engine = Engine::sandbox(position);
                if let Some(side) = side {
                    engine.lock_orientation(side);
                }
                self.game_engine = Some(engine);
                self.games_board = super::super::board::BoardWidget::default();
            }
            None => {}
        }
        if self.game_blind.as_ref().unwrap().round.phase == BlindPhase::Watching {
            let white = self.game_engine.as_ref().unwrap().orientation() != Some(Color::Black);
            self.games_board.reader_orientation = Some(white);
        }

        let (score, streak, best, phase) = {
            let s = self.game_blind.as_ref().unwrap();
            (s.score, s.streak, s.best_streak, s.round.phase)
        };
        let hint = match phase {
            BlindPhase::Watching => "Watch the line…",
            BlindPhase::Concealed => "Hold it in your head…",
            BlindPhase::Answering => "Play the next move.",
            BlindPhase::Cleared => "Correct.",
            BlindPhase::Missed => "Not that move.",
        };
        ui.label(RichText::new(hint).size(if narrow { 14.0 } else { 15.0 }));
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Score {score}"));
            ui.separator();
            ui.label(format!("Streak {streak}"));
            ui.separator();
            ui.label(format!("Best {best}"));
        });
        ui.add_space(8.0);

        let board_budget = if narrow {
            ui.available_width()
                .min((ui.available_height() - 120.0).max(180.0))
        } else {
            ui.available_height()
                .min(ui.available_width() * 0.62)
                .clamp(280.0, 560.0)
        };

        if phase == BlindPhase::Concealed {
            let size = egui::vec2(board_budget, board_budget);
            ui.allocate_ui(size, |ui| {
                ui.set_min_size(size);
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new("…")
                            .size(48.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                });
            });
        } else {
            let interactive = phase == BlindPhase::Answering;
            let played = {
                let engine = self.game_engine.as_ref().unwrap();
                self.games_board.show(
                    ui,
                    engine,
                    board_budget,
                    self.settings.board_theme,
                    self.settings.piece_rendering,
                )
            };
            if let Some(uci) = played
                && interactive
            {
                let outcome = self.game_blind.as_mut().unwrap().submit(&uci);
                let ok = outcome == BlindPhase::Cleared;
                let engine = self.game_engine.as_mut().unwrap();
                let _ = engine.play_any(&uci);
                engine.resolve_sandbox(ok);
                for event in engine.take_sounds() {
                    if self.screenshot.is_none() {
                        crate::sound::play(event, &self.settings);
                    }
                }
                self.games_board.player_moved();
                self.toast = Some((
                    if ok {
                        "Nice memory.".into()
                    } else {
                        "Miss — streak lost.".into()
                    },
                    web_time::Instant::now(),
                ));
            }
        }

        ui.add_space(10.0);
        let phase = self.game_blind.as_ref().unwrap().round.phase;
        match phase {
            BlindPhase::Watching | BlindPhase::Concealed => {
                ui.label("The board plays the line for you. Wait for your turn.");
                ui.ctx().request_repaint_after(Duration::from_millis(50));
            }
            BlindPhase::Answering => {
                ui.label("Drag or tap the next move. Works with mouse and touch.");
            }
            BlindPhase::Cleared | BlindPhase::Missed => {
                let label = if phase == BlindPhase::Cleared {
                    "Next line"
                } else {
                    "Try another"
                };
                if widgets::primary(ui, label).clicked() {
                    let now = self.now();
                    self.game_blind.as_mut().unwrap().next_round(now);
                    let position = self.game_blind.as_ref().unwrap().round.start_position();
                    self.game_engine = Some(Engine::sandbox(position));
                    self.games_board = super::super::board::BoardWidget::default();
                }
            }
        }
    }
}

fn tap_highlight(h: Highlight) -> TapHighlight {
    match h {
        Highlight::Selected => TapHighlight::Selected,
        Highlight::Correct => TapHighlight::Correct,
        Highlight::Wrong => TapHighlight::Wrong,
        Highlight::Missed => TapHighlight::Missed,
    }
}
