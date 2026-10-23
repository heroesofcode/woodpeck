use super::super::{Screen, Woodpeck, shortcuts::Action, style, widgets};
use crate::{
    layout::LayoutMode,
    puzzle::{Outcome, State},
    stats::compare,
};
use eframe::egui::{self, RichText, Ui};
impl Woodpeck {
    pub(in crate::app) fn solving(&mut self, ui: &mut Ui, mode: LayoutMode, panel_width: f32) {
        let review = self.screen == Screen::Review;
        let name = if review {
            "REVIEW / REINFORCE YOUR PATTERNS".into()
        } else {
            self.sets
                .iter()
                .find(|s| Some(s.set.id) == self.selected_set)
                .map_or_else(
                    || "TRAIN / BUILD YOUR RHYTHM".into(),
                    |s| format!("TRAIN / {}", s.set.params.name.to_uppercase()),
                )
        };
        ui.horizontal(|ui| {
            widgets::caption(ui, name);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("×  Exit").clicked() {
                    self.navigate(Screen::Dashboard);
                }
            });
        });
        ui.add_space(15.0);
        if self.session.is_none() {
            if review {
                self.review_empty(ui);
            } else {
                self.cycle_complete(ui);
            }
            return;
        }
        let geometry = crate::layout::solving(
            ui.available_width(),
            ui.available_height(),
            mode,
            panel_width,
        );
        if mode == LayoutMode::Columns {
            let panel_width = geometry.panel_width;
            let board_width = geometry.board_column;
            let board_size = geometry.board_size;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 24.0;
                ui.allocate_ui_with_layout(
                    egui::vec2(board_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        self.board_area(ui, board_size);
                    },
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(panel_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("solve-panel")
                            .show(ui, |ui| {
                                if review {
                                    self.review_panel(ui);
                                } else {
                                    self.training_panel(ui);
                                }
                            });
                    },
                );
            });
        } else {
            let size = geometry.board_size;
            egui::ScrollArea::vertical()
                .id_salt("stacked-solve")
                .show(ui, |ui| {
                    self.board_area(ui, size);
                    ui.add_space(8.0);
                    if review {
                        self.review_panel(ui);
                    } else {
                        self.training_panel(ui);
                    }
                });
        }
    }
    fn board_area(&mut self, ui: &mut Ui, size: f32) {
        let played = self.session.as_ref().and_then(|s| {
            self.board.show(
                ui,
                &s.engine,
                size,
                self.settings.board_theme,
                self.settings.piece_rendering,
            )
        });
        if let Some(uci) = played {
            self.play(&uci);
        }
        ui.add_space(9.0);
        let text = self
            .session
            .as_ref()
            .map_or("", |s| match s.engine.state() {
                State::PlayerTurn => "Find the best move. Take your time.",
                State::OpponentTurn | State::Loading => "Opponent is moving…",
                State::Complete => "Well played. Pattern reinforced.",
                State::Incorrect | State::FailedReview => "Review the line, then make it yours.",
                State::Skipped => "Saved to Review. You can come back to it.",
                State::Invalid => "This puzzle could not be loaded safely.",
            });
        ui.label(
            RichText::new(text)
                .size(12.0)
                .color(ui.visuals().weak_text_color()),
        );
    }
    fn training_panel(&mut self, ui: &mut Ui) {
        let Some(session) = &self.session else {
            return;
        };
        let Some(set) = self
            .sets
            .iter()
            .find(|s| s.set.id == session.set_id)
            .cloned()
        else {
            return;
        };
        let puzzle = session.puzzle.clone();
        let position = session.position;
        let color = session.engine.orientation();
        widgets::card(ui).show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 7.0);
            ui.set_min_width((ui.available_width() - 1.0).max(0.0));
            ui.label(RichText::new(&set.set.params.name).size(20.0).strong());
            widgets::caption(
                ui,
                format!(
                    "CYCLE {} OF {} · {} PUZZLES",
                    set.latest_cycle.as_ref().map_or(1, |c| c.number),
                    set.set.params.target_cycles,
                    set.set.params.size
                ),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                widgets::caption(
                    ui,
                    format!("PUZZLE {} / {}", position + 1, set.set.params.size),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.strong(format!("{:.0}%", set.stats.accuracy(set.set.params.size)));
                });
            });
            self.segments(ui, set.stats, set.set.params.size);
            ui.add_space(7.0);
            self.timer_row(ui, color, false);
            ui.add_space(7.0);
            ui.separator();
            ui.add_space(10.0);
            widgets::caption(ui, "THIS CYCLE");
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(format!("{:.0}%", set.stats.accuracy(set.set.params.size)))
                            .size(26.0)
                            .strong(),
                    );
                    widgets::caption(ui, "ACCURACY");
                });
                ui.add_space(18.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(widgets::duration(
                            set.stats.average_ms(set.set.params.size) as u64
                        ))
                        .size(26.0)
                        .strong(),
                    );
                    widgets::caption(ui, "AVG / PUZZLE");
                });
            });
            ui.horizontal_wrapped(|ui| {
                widgets::chip(
                    ui,
                    format!("{} correct", set.stats.correct),
                    style::color(self.palette.semantic().correct),
                );
                widgets::chip(
                    ui,
                    format!("{} missed", set.stats.incorrect),
                    style::color(self.palette.semantic().incorrect),
                );
                ui.small(format!("{} skipped", set.stats.skipped));
            });
            ui.add_space(7.0);
            ui.separator();
            ui.add_space(10.0);
            widgets::caption(ui, "THIS PUZZLE");
            ui.horizontal(|ui| {
                widgets::chip(
                    ui,
                    format!("{}", puzzle.rating),
                    style::color(self.palette.semantic().warning),
                );
                ui.small("Puzzle rating");
            });
            if self.settings.show_theme_labels {
                ui.horizontal_wrapped(|ui| {
                    for theme in &puzzle.themes {
                        widgets::chip(
                            ui,
                            crate::sets::theme_label(theme),
                            style::color(self.palette.semantic().accent),
                        );
                    }
                });
            }
            ui.add_space(7.0);
            ui.separator();
            ui.add_space(8.0);
            self.solve_controls(ui);
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            widgets::caption(ui, "S  SKIP");
            widgets::caption(ui, "T  TIMER");
            widgets::caption(ui, "ENTER  NEXT");
        });
    }
    pub(in crate::app) fn timer_row(
        &mut self,
        ui: &mut Ui,
        color: Option<shakmaty::Color>,
        tenths: bool,
    ) {
        egui::Frame::new()
            .fill(ui.visuals().extreme_bg_color)
            .corner_radius(9)
            .inner_margin(12)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(if self.timer.visible() {
                            self.timer.display(tenths)
                        } else {
                            "—:—".into()
                        })
                        .monospace()
                        .size(if tenths { 23.0 } else { 28.0 })
                        .strong(),
                    );
                    let eye = ui.add_sized(
                        [26.0, 26.0],
                        egui::Button::new("").stroke(egui::Stroke::NONE),
                    );
                    let center = eye.rect.center();
                    let stroke = egui::Stroke::new(1.4, ui.visuals().weak_text_color());
                    ui.painter().add(egui::Shape::closed_line(
                        vec![
                            center + egui::vec2(-7.0, 0.0),
                            center + egui::vec2(-3.0, -4.0),
                            center + egui::vec2(3.0, -4.0),
                            center + egui::vec2(7.0, 0.0),
                            center + egui::vec2(3.0, 4.0),
                            center + egui::vec2(-3.0, 4.0),
                        ],
                        stroke,
                    ));
                    ui.painter().circle_stroke(center, 2.0, stroke);
                    if eye.on_hover_text("Show / hide timer (T)").clicked() {
                        self.timer.toggle_visible();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.vertical(|ui| {
                            widgets::caption(ui, "YOU PLAY");
                            ui.strong(if color == Some(shakmaty::Color::Black) {
                                "Black"
                            } else {
                                "White"
                            });
                        });
                    });
                });
            });
    }
    fn segments(&self, ui: &mut Ui, _stats: crate::stats::CycleStats, total: u32) {
        let (r, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 7.0), egui::Sense::hover());
        let gap = if total < 60 { 2.0 } else { 0.5 };
        let width = r.width() / total.max(1) as f32;
        for i in 0..total {
            let color = match self.cycle_marks.get(i as usize) {
                Some(Outcome::Correct) => style::color(self.palette.semantic().correct),
                Some(Outcome::Incorrect) => style::color(self.palette.semantic().incorrect),
                Some(Outcome::Skipped | Outcome::Invalid) => {
                    style::color(self.palette.semantic().muted)
                }
                None => ui.visuals().extreme_bg_color,
            };
            ui.painter().rect_filled(
                egui::Rect::from_min_size(
                    r.min + egui::vec2(i as f32 * width, 0.0),
                    egui::vec2((width - gap).max(0.5), 7.0),
                ),
                2,
                color,
            );
        }
    }
    pub(in crate::app) fn solve_controls(&mut self, ui: &mut Ui) {
        let Some(s) = &mut self.session else {
            return;
        };
        let state = s.engine.state();
        let puzzle_id = s.puzzle.id.clone();
        let is_review = s.cycle_id.is_none();
        match state {
            State::Complete => {
                ui.label(
                    RichText::new(if is_review {
                        "Cleared from Review"
                    } else {
                        "Puzzle complete"
                    })
                    .strong()
                    .size(19.0)
                    .color(style::color(self.palette.semantic().correct)),
                );
                if widgets::primary(ui, "Next puzzle  >").clicked() {
                    self.advance();
                }
            }
            State::FailedReview => {
                ui.label(RichText::new("Review the missed line").strong().size(17.0));
                if let Some(w) = s.engine.walkthrough() {
                    let (n, total) = w.progress();
                    widgets::caption(ui, format!("SOLUTION {n} / {total}"));
                    ui.add(
                        egui::ProgressBar::new(n as f32 / total.max(1) as f32).desired_height(4.0),
                    );
                    let complete = w.is_complete();
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(n > 0, egui::Button::new("< Previous"))
                            .clicked()
                        {
                            self.action(Action::Previous);
                        }
                        if ui
                            .button(if complete { "Done" } else { "Next >" })
                            .clicked()
                        {
                            self.action(Action::NextMove);
                        }
                    });
                    if complete {
                        ui.label("Solution complete");
                    }
                }
                if ui.button("Skip solution & next puzzle").clicked() {
                    self.advance();
                }
            }
            State::Skipped => {
                ui.strong("Puzzle skipped");
                if widgets::primary(ui, "Next puzzle  >").clicked() {
                    self.advance();
                }
            }
            State::Invalid => {
                ui.heading("Puzzle unavailable");
                ui.label("This puzzle could not be loaded safely.");
                if widgets::primary(ui, "Report and continue").clicked() {
                    self.advance();
                }
            }
            _ => {
                if ui
                    .add_enabled(
                        state == State::PlayerTurn,
                        egui::Button::new("Skip puzzle")
                            .min_size(egui::vec2(ui.available_width(), 38.0)),
                    )
                    .clicked()
                {
                    self.action(Action::Skip);
                }
            }
        }
        if is_review
            && self.session.as_ref().is_some_and(|s| {
                matches!(
                    s.engine.outcome(),
                    Some(Outcome::Incorrect | Outcome::Skipped)
                )
            })
        {
            ui.colored_label(
                style::color(self.palette.semantic().warning),
                "Still pending",
            );
        }
        if !is_review
            && self
                .session
                .as_ref()
                .is_some_and(|s| s.engine.outcome().is_some_and(Outcome::is_miss))
        {
            ui.colored_label(
                style::color(self.palette.semantic().accent),
                "Added to Review",
            );
        }
        ui.add_space(6.0);
        if ui.link("Open on Lichess").clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(format!(
                "https://lichess.org/training/{puzzle_id}"
            )));
        }
    }
    fn cycle_complete(&mut self, ui: &mut Ui) {
        let Some(set) = self
            .sets
            .iter()
            .find(|s| Some(s.set.id) == self.selected_set)
            .cloned()
        else {
            widgets::title(
                ui,
                "YOUR NEXT STEP",
                "Ready when you are",
                "Create a set to start building lasting patterns.",
            );
            if widgets::primary(ui, "Create your first set").clicked() {
                self.navigate(Screen::Builder);
            }
            return;
        };
        egui::ScrollArea::vertical().show(ui, |ui| {
            widgets::title(
                ui,
                "A LITTLE STRONGER",
                "Cycle complete",
                "Repetition turns calculation into recognition.",
            );
            widgets::card(ui).show(ui, |ui| {
                ui.heading(&set.set.params.name);
                ui.label(format!(
                    "Cycle {} of {}",
                    set.latest_cycle.as_ref().map_or(1, |c| c.number),
                    set.set.params.target_cycles
                ));
                widgets::stat(
                    ui,
                    "TOTAL TIME",
                    widgets::duration(set.stats.time_ms),
                    "Your focused solving time",
                );
                widgets::stat(
                    ui,
                    "ACCURACY",
                    format!("{:.0}%", set.stats.accuracy(set.set.params.size)),
                    "Skips count as misses",
                );
                widgets::stat(
                    ui,
                    "AVERAGE",
                    widgets::duration(set.stats.average_ms(set.set.params.size) as u64),
                    "per puzzle",
                );
                if let Ok(p) = self.service.progress(set.set.id) {
                    for (label, baseline) in [
                        ("vs cycle 1", p.cycles.first()),
                        ("vs previous", p.cycles.iter().rev().nth(1)),
                    ] {
                        if let Some(base) = baseline {
                            let d = compare(set.stats, base.stats, set.set.params.size);
                            ui.horizontal_wrapped(|ui| {
                                ui.label(label);
                                if let Some(t) = d.time_reduction_percent {
                                    widgets::chip(
                                        ui,
                                        widgets::delta_time(t),
                                        style::color(self.palette.semantic().accent),
                                    );
                                }
                                widgets::chip(
                                    ui,
                                    widgets::delta_points(d.accuracy_points),
                                    style::color(self.palette.semantic().accent),
                                );
                            });
                        }
                    }
                }
                ui.label(format!("{} puzzles waiting in Review", self.pending));
                if !set.completed {
                    if widgets::primary(ui, "Start next cycle").clicked() {
                        match self
                            .service
                            .next_cycle(set.set.id, chrono::Local::now().timestamp())
                        {
                            Ok(p) => self.load_training(p),
                            Err(e) => self.error(e),
                        }
                    }
                } else if set.set.params.target_cycles < 10
                    && widgets::primary(ui, "Run another cycle").clicked()
                {
                    match self
                        .service
                        .extend_cycle(set.set.id, chrono::Local::now().timestamp())
                    {
                        Ok(p) => self.load_training(p),
                        Err(e) => self.error(e),
                    }
                }
                if ui.button("Create a harder set").clicked() {
                    self.builder.target_rating = (set.set.params.target_rating + 100).min(3000);
                    self.builder.restrict_recommended = false;
                    self.navigate(Screen::Builder);
                }
            });
        });
    }
}
