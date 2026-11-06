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
        let columns = if ui.available_width() >= 600.0 { 2 } else { 1 };
        let width = ((ui.available_width() - GAP * (columns - 1) as f32) / columns as f32).floor();
        for row in games::CATALOG.chunks(columns) {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                for game in row {
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, 208.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| self.game_card(ui, game, width),
                    );
                }
            });
            ui.add_space(GAP);
        }
    }

    fn game_card(&mut self, ui: &mut Ui, game: &games::GameInfo, width: f32) {
        let frame = widgets::card(ui).inner_margin(16);
        let inner = (width - frame.total_margin().sum().x).max(0.0);
        frame.show(ui, |ui| {
            ui.set_width(inner);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            widgets::caption(
                ui,
                if game.available {
                    "READY TO PLAY"
                } else {
                    "COMING SOON"
                },
            );
            ui.add_space(6.0);
            ui.label(RichText::new(game.title).strong().size(22.0));
            ui.add_space(4.0);
            ui.allocate_ui(egui::vec2(inner, 64.0), |ui| {
                ui.set_min_height(64.0);
                ui.label(game.blurb);
            });
            ui.add_space(10.0);
            let play = ui.add_enabled(
                game.available,
                egui::Button::new(
                    RichText::new(if game.available {
                        "Play  >"
                    } else {
                        "Coming soon"
                    })
                    .strong()
                    .color(if game.available {
                        egui::Color32::from_rgb(20, 28, 19)
                    } else {
                        ui.visuals().weak_text_color()
                    }),
                )
                .fill(if game.available {
                    ui.visuals().selection.bg_fill
                } else {
                    ui.visuals().faint_bg_color
                })
                .min_size(egui::vec2(inner, 44.0)),
            );
            if play.clicked() {
                match game.id {
                    "board-scan" => self.start_board_scan(DrillMode::HangingPieces),
                    "safe-move" => self.start_safe_move(),
                    "blind-line" => self.start_blind_line(),
                    "human-move" => self.start_human_move(),
                    _ => {}
                }
            }
        });
    }

    fn game_seed(&self) -> u64 {
        if self.screenshot.is_some() {
            1
        } else {
            self.origin
                .elapsed()
                .as_millis()
                .wrapping_add(u128::from(self.frames)) as u64
        }
    }

    pub(in crate::app) fn start_board_scan(&mut self, mode: DrillMode) {
        let seed = self.game_seed();
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
        self.board_scan_header(ui);
        if self.game_vision.is_none() {
            ui.label("Starting…");
            return;
        }
        let now = self.now();
        if let Some(session) = self.game_vision.as_mut() {
            session.tick(now);
        }
        self.board_scan_stats(ui, narrow);
        self.board_scan_board(ui);
        self.board_scan_footer(ui);
    }

    fn board_scan_header(&mut self, ui: &mut Ui) {
        if game_header(ui, "Board scan") {
            self.game_vision = None;
            self.game_engine = None;
            self.toast = None;
            self.navigate(Screen::Games);
            return;
        }
        let mut mode = self
            .game_vision
            .as_ref()
            .map_or(DrillMode::HangingPieces, |s| s.mode);
        let previous = mode;
        ui.scope(|ui| {
            ui.spacing_mut().interact_size.y = 36.0;
            egui::ComboBox::from_id_salt("scan-mode")
                .selected_text(mode.label())
                .show_ui(ui, |ui| {
                    for choice in DrillMode::ALL {
                        ui.selectable_value(&mut mode, choice, choice.label());
                    }
                });
        });
        if mode != previous {
            self.start_board_scan(mode);
        }
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
            let timer = RichText::new(if session.round.status == VisionStatus::Playing {
                format!("{secs}s left")
            } else {
                "Round over".into()
            })
            .strong();
            ui.label(if session.round.status == VisionStatus::Playing {
                timer.color(if secs <= 5 {
                    style::color(self.palette.semantic().incorrect)
                } else {
                    ui.visuals().text_color()
                })
            } else {
                timer.color(accent)
            });
        });
        ui.add_space(8.0);
    }

    fn board_scan_board(&mut self, ui: &mut Ui) {
        let Some(session) = self.game_vision.as_ref() else {
            return;
        };
        let board_budget = game_board_size(ui);
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
                self.toast = Some((
                    "Not a target — streak reset. Keep scanning.".into(),
                    web_time::Instant::now(),
                ));
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
                ui.label("Scan both colors. A wrong tap resets your streak.");
                ui.ctx().request_repaint_after(Duration::from_millis(100));
            }
            VisionStatus::Cleared => {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Board cleared. Nice scan!").strong());
                    if next_button(ui, "Next board") {
                        let now = self.now();
                        self.game_vision.as_mut().unwrap().next_round(now);
                    }
                });
            }
            VisionStatus::TimedOut => {
                ui.vertical(|ui| {
                    ui.label("Time’s up. Yellow marks the missed targets.");
                    if next_button(ui, "Try another") {
                        let now = self.now();
                        self.game_vision.as_mut().unwrap().next_round(now);
                    }
                });
            }
        }
    }

    pub(in crate::app) fn start_safe_move(&mut self) {
        let seed = self.game_seed();
        let session = SafeSession::start(seed.max(1));
        let engine = Engine::sandbox(session.round.position());
        self.game_safe = Some(session);
        self.game_engine = Some(engine);
        self.games_board = super::super::board::BoardWidget::default();
        self.navigate(Screen::SafeMove);
    }

    pub(in crate::app) fn safe_move_screen(&mut self, ui: &mut Ui) {
        let narrow = ui.available_width() < 640.0;
        if game_header(ui, "Safe move") {
            self.game_safe = None;
            self.game_engine = None;
            self.toast = None;
            self.navigate(Screen::Games);
            return;
        }
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

        let board_budget = game_board_size(ui);
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
                if next_button(ui, label) {
                    self.game_safe.as_mut().unwrap().next_round();
                    let position = self.game_safe.as_ref().unwrap().round.position();
                    self.game_engine = Some(Engine::sandbox(position));
                    self.games_board = super::super::board::BoardWidget::default();
                }
            }
        }
    }

    pub(in crate::app) fn start_blind_line(&mut self) {
        let seed = self.game_seed();
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
        if game_header(ui, "Blind line") {
            self.game_blind = None;
            self.game_engine = None;
            self.toast = None;
            self.navigate(Screen::Games);
            return;
        }
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

        let board_budget = game_board_size(ui);
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
                if next_button(ui, label) {
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
        let seed = self.game_seed();
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
        if game_header(ui, "Human move") {
            self.game_human = None;
            self.game_engine = None;
            self.toast = None;
            self.navigate(Screen::Games);
            return;
        }
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

        let board_budget = game_board_size(ui);
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
                if next_button(ui, label) {
                    self.game_human.as_mut().unwrap().next_round();
                    let position = self.game_human.as_ref().unwrap().round.position();
                    self.game_engine = Some(Engine::sandbox(position));
                    self.games_board = super::super::board::BoardWidget::default();
                }
            }
        }
    }
}

const GAP: f32 = 12.0;
const BOARD_ACTION_RESERVE: f32 = 104.0;

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
fn game_board_size(ui: &Ui) -> f32 {
    ui.available_width()
        .min((ui.available_height() - BOARD_ACTION_RESERVE).max(0.0))
        .min(560.0)
}

fn game_header(ui: &mut Ui, title: &str) -> bool {
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
    let mut back = false;
    ui.horizontal_wrapped(|ui| {
        back = ui
            .add_sized([88.0, 40.0], egui::Button::new("< Games"))
            .clicked();
        ui.label(RichText::new(title).strong().size(22.0));
    });
    ui.add_space(8.0);
    back || game_key(ui, egui::Key::Escape)
}

fn game_key(ui: &Ui, key: egui::Key) -> bool {
    !ui.ctx().egui_wants_keyboard_input()
        && !egui::Popup::is_any_open(ui.ctx())
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key))
}

fn next_button(ui: &mut Ui, label: &str) -> bool {
    widgets::primary(ui, label)
        .on_hover_text("Enter to continue")
        .clicked()
        || game_key(ui, egui::Key::Enter)
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
