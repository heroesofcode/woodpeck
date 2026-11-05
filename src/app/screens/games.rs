//! Games hub and Board-scan drill UI (desktop, web desktop, and phone).
use super::super::{Screen, Woodpeck, board::TapHighlight, style, widgets};
use crate::games::{
    self, BlindPhase, BlindSession, BlindTick, DrillMode, Highlight, HumanSession, HumanStatus,
    SafeSession, SafeStatus, VisionOutcome, VisionSession, VisionStatus,
};
use crate::puzzle::Engine;
use eframe::egui::{self, RichText, Ui};
use shakmaty::{Color, Position, Square};
use std::time::Duration;

impl Woodpeck {
    pub(in crate::app) fn games_hub(&mut self, ui: &mut Ui) {
        widgets::title(
            ui,
            "TRAINING GAMES",
            "Short drills for vision and feel",
            "Pick a drill. They work the same on desktop and on your phone.",
        );
        // widgets::card adds 2×20 inner margin (+ stroke). Size columns to the
        // available width so a 2×2 grid never overflows the content pane.
        let avail = ui.available_width();
        let cols = if avail >= 2.0 * (200.0 + CARD_INSET) + GAP {
            2
        } else {
            1
        };
        let col = ((avail - GAP * (cols as f32 - 1.0)) / cols as f32)
            .floor()
            .max(1.0);
        let inner = (col - CARD_INSET).max(120.0);
        egui::Grid::new("games-catalog")
            .num_columns(cols)
            .spacing([GAP, GAP])
            .min_col_width(col)
            .max_col_width(col)
            .show(ui, |ui| {
                for (index, game) in games::CATALOG.iter().enumerate() {
                    self.game_card(ui, game, inner);
                    if cols == 1 || index % 2 == 1 {
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
                ui.set_width(width.max(0.0));
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
                            "human-move" => self.start_human_move(),
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
        let board_budget = game_board_size(ui, narrow);
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
        let tapped = with_centered_board(ui, board_budget, |ui, size| {
            self.games_board.show_tap(
                ui,
                &position,
                size,
                theme,
                rendering,
                white_bottom,
                &tap_highlights,
            )
        });
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

        let board_budget = game_board_size(ui, narrow);
        let theme = self.settings.board_theme;
        let rendering = self.settings.piece_rendering;
        let played = {
            let engine = self.game_engine.as_ref().unwrap();
            with_centered_board(ui, board_budget, |ui, size| {
                self.games_board.show(ui, engine, size, theme, rendering)
            })
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
            BlindPhase::Watching => "Watch the line…".to_string(),
            BlindPhase::Concealed => "Hold it in your head…".to_string(),
            BlindPhase::Answering => "Play the next move.".to_string(),
            BlindPhase::Cleared => "Correct.".to_string(),
            BlindPhase::Missed => {
                let san = self.game_blind.as_ref().unwrap().round.answer_san();
                format!("Not that move. Answer: {san}")
            }
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

        let board_budget = game_board_size(ui, narrow);
        let theme = self.settings.board_theme;
        let rendering = self.settings.piece_rendering;

        if phase == BlindPhase::Concealed {
            with_centered_board(ui, board_budget, |ui, size| {
                let size = egui::vec2(size, size);
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
            });
        } else {
            let interactive = phase == BlindPhase::Answering;
            let played = {
                let engine = self.game_engine.as_ref().unwrap();
                with_centered_board(ui, board_budget, |ui, size| {
                    self.games_board.show(ui, engine, size, theme, rendering)
                })
            };
            if let Some(uci) = played
                && interactive
            {
                let outcome = self.game_blind.as_mut().unwrap().submit(&uci);
                let ok = outcome == BlindPhase::Cleared;
                if ok {
                    let engine = self.game_engine.as_mut().unwrap();
                    let _ = engine.play_any(&uci);
                    engine.resolve_sandbox(true);
                    for event in engine.take_sounds() {
                        if self.screenshot.is_none() {
                            crate::sound::play(event, &self.settings);
                        }
                    }
                    self.games_board.player_moved();
                    self.toast = Some(("Nice memory.".into(), web_time::Instant::now()));
                } else {
                    // Reveal the expected move: reseat, play the answer, highlight from/to + SAN.
                    let answer = self.game_blind.as_ref().unwrap().round.answer.clone();
                    let san = self.game_blind.as_ref().unwrap().round.answer_san();
                    let before = self
                        .game_blind
                        .as_ref()
                        .unwrap()
                        .round
                        .position_after_preview();
                    let side = self.game_engine.as_ref().unwrap().orientation();
                    let mut engine = Engine::sandbox(before.clone());
                    if let Some(side) = side {
                        engine.lock_orientation(side);
                    }
                    self.games_board = super::super::board::BoardWidget::default();
                    self.games_board.seed_board(before.board());
                    let _ = engine.play_any(&answer);
                    engine.resolve_sandbox(false);
                    for event in engine.take_sounds() {
                        if self.screenshot.is_none() {
                            crate::sound::play(event, &self.settings);
                        }
                    }
                    self.game_engine = Some(engine);
                    self.toast = Some((
                        format!("Miss — answer was {san}."),
                        web_time::Instant::now(),
                    ));
                }
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

    pub(in crate::app) fn start_human_move(&mut self) {
        let seed = self
            .origin
            .elapsed()
            .as_millis()
            .wrapping_add(u128::from(self.frames)) as u64;
        let session = HumanSession::start(seed.max(1));
        let engine = Engine::sandbox(session.round.position());
        self.game_human = Some(session);
        self.game_engine = Some(engine);
        self.games_board = super::super::board::BoardWidget::default();
        self.navigate(Screen::HumanMove);
    }

    #[allow(clippy::too_many_lines)]
    pub(in crate::app) fn human_move_screen(&mut self, ui: &mut Ui) {
        let narrow = ui.available_width() < 640.0;
        ui.horizontal_wrapped(|ui| {
            if ui.button("< Games").clicked() {
                self.game_human = None;
                self.game_engine = None;
                self.navigate(Screen::Games);
            }
            ui.label(
                RichText::new("Human move")
                    .strong()
                    .size(if narrow { 18.0 } else { 22.0 }),
            );
        });
        ui.add_space(8.0);
        if self.game_human.is_none() || self.game_engine.is_none() {
            ui.label("Starting…");
            return;
        }
        let (score, streak, best, status) = {
            let s = self.game_human.as_ref().unwrap();
            (s.score, s.streak, s.best_streak, s.round.status)
        };
        ui.label(
            RichText::new("Play the natural move a club player would pick.").size(if narrow {
                14.0
            } else {
                15.0
            }),
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

        let board_budget = game_board_size(ui, narrow);
        let theme = self.settings.board_theme;
        let rendering = self.settings.piece_rendering;
        let played = {
            let engine = self.game_engine.as_ref().unwrap();
            with_centered_board(ui, board_budget, |ui, size| {
                self.games_board.show(ui, engine, size, theme, rendering)
            })
        };
        if let Some(uci) = played
            && status == HumanStatus::Playing
        {
            let outcome = self.game_human.as_mut().unwrap().submit(&uci);
            match outcome {
                HumanStatus::Cleared | HumanStatus::Missed => {
                    let ok = outcome == HumanStatus::Cleared;
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
                            "Natural.".into()
                        } else {
                            "Not the human move — streak lost.".into()
                        },
                        web_time::Instant::now(),
                    ));
                }
                HumanStatus::Playing => {}
            }
        }
        ui.add_space(10.0);
        let status = self.game_human.as_ref().unwrap().round.status;
        match status {
            HumanStatus::Playing => {
                ui.label("Drag or tap. Same controls on desktop and phone.");
            }
            HumanStatus::Cleared | HumanStatus::Missed => {
                let label = if status == HumanStatus::Cleared {
                    "Next position"
                } else {
                    "Try another"
                };
                if widgets::primary(ui, label).clicked() {
                    self.game_human.as_mut().unwrap().next_round();
                    let position = self.game_human.as_ref().unwrap().round.position();
                    self.game_engine = Some(Engine::sandbox(position));
                    self.games_board = super::super::board::BoardWidget::default();
                }
            }
        }
    }
}

/// Card frame inset (2×20 inner margin + stroke) reserved when sizing hub columns.
const CARD_INSET: f32 = 42.0;
const GAP: f32 = 12.0;
const BOARD_ACTION_RESERVE: f32 = 72.0;

fn tap_highlight(h: Highlight) -> TapHighlight {
    match h {
        Highlight::Selected => TapHighlight::Selected,
        Highlight::Correct => TapHighlight::Correct,
        Highlight::Wrong => TapHighlight::Wrong,
        Highlight::Missed => TapHighlight::Missed,
    }
}

/// Square board that fills the content width on phone, stays capped on desktop,
/// and leaves room for the primary action when height is scarce.
fn game_board_size(ui: &Ui, narrow: bool) -> f32 {
    let width = ui.available_width();
    // Always leave room for Next / Try another under the board (play screens are
    // no longer inside a ScrollArea).
    let height = (ui.available_height() - BOARD_ACTION_RESERVE).max(160.0);
    if narrow {
        width.min(height)
    } else {
        height.min(width * 0.62).min(560.0).min(width).max(0.0)
    }
}

fn with_centered_board<R>(ui: &mut Ui, size: f32, add: impl FnOnce(&mut Ui, f32) -> R) -> R {
    // `game_board_size` already reserved the action row; do not subtract again.
    let size = size
        .min(ui.available_width())
        .min(ui.available_height())
        .max(0.0);
    let pad = ((ui.available_width() - size) * 0.5).max(0.0);
    let mut out = None;
    ui.horizontal(|ui| {
        ui.add_space(pad);
        // Allocate an exact square so padding does not steal width from the board
        // (unlike set_max_width on the row, which is measured from the row origin).
        ui.allocate_ui(egui::vec2(size, size), |ui| {
            out = Some(add(ui, size));
        });
    });
    out.expect("board closure ran")
}
