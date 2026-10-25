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
        ui.add(
            egui::Label::new(
                RichText::new(name)
                    .size(11.0)
                    .strong()
                    .color(ui.visuals().weak_text_color()),
            )
            .truncate(),
        );
        ui.add_space(8.0);
        if self.loading && self.session.is_none() {
            ui.spinner();
            return;
        }
        if self.session.is_none() {
            if review {
                self.review_empty(ui);
            } else {
                self.cycle_complete(ui);
            }
            return;
        }
        let state = self.session.as_ref().unwrap().engine.state();
        let actions = match state {
            State::FailedReview => 180.0,
            State::Complete | State::Skipped => 128.0,
            State::Invalid => 144.0,
            _ => 112.0,
        } + if self.record_error { 52.0 } else { 0.0 };
        let geometry = crate::layout::solving(
            ui.available_width(),
            ui.available_height(),
            mode,
            panel_width,
            actions,
        );
        let origin = ui.cursor().min;
        let board_rect = egui::Rect::from_min_size(
            origin + egui::vec2((geometry.board_column - geometry.board_size) / 2.0, 0.0),
            egui::vec2(
                geometry.board_size,
                geometry.board_size + crate::layout::BOARD_FOOTER,
            ),
        );
        let panel_origin = if mode == LayoutMode::Columns {
            origin + egui::vec2(geometry.board_column + geometry.gap, 0.0)
        } else {
            origin + egui::vec2(0.0, board_rect.height() + geometry.gap)
        };
        let panel_rect = egui::Rect::from_min_size(
            panel_origin,
            egui::vec2(geometry.panel_width, geometry.panel_height),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(board_rect), |ui| {
            self.board_area(ui, geometry.board_size);
        });
        let details = egui::Rect::from_min_size(
            panel_origin,
            egui::vec2(geometry.panel_width, geometry.details_height),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(details), |ui| {
            egui::ScrollArea::vertical()
                .id_salt("solve-details")
                .max_height(geometry.details_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if review {
                        self.review_panel(ui);
                    } else {
                        self.training_panel(ui, geometry.compact);
                    }
                });
        });
        let actions_rect = egui::Rect::from_min_max(
            egui::pos2(
                panel_rect.left(),
                panel_rect.bottom() - geometry.action_height,
            ),
            panel_rect.max,
        );
        self.pinned_actions(ui, actions_rect);
    }

    fn pinned_actions(&mut self, ui: &mut Ui, actions_rect: egui::Rect) {
        ui.scope_builder(egui::UiBuilder::new().max_rect(actions_rect), |ui| {
            egui::Frame::new()
                .fill(ui.visuals().panel_fill)
                .corner_radius(10)
                .inner_margin(8)
                .show(ui, |ui| {
                    ui.set_width((ui.available_width() - 1.0).max(0.0));
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
                    ui.spacing_mut().button_padding = egui::vec2(8.0, 3.0);
                    ui.spacing_mut().interact_size.y = 24.0;
                    ui.style_mut().override_font_id = Some(egui::FontId::proportional(13.0));
                    self.solve_controls(ui);
                    ui.horizontal(|ui| {
                        if ui.small_button("× Exit").clicked() {
                            self.navigate(Screen::Dashboard);
                        }
                        if let Some(session) = &self.session {
                            ui.hyperlink_to(
                                "Open on Lichess",
                                format!("https://lichess.org/training/{}", session.puzzle.id),
                            );
                        }
                    });
                });
        });
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
        ui.add_space(4.0);
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
        ui.add(
            egui::Label::new(
                RichText::new(text)
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
            )
            .truncate(),
        );
    }
    fn training_panel(&mut self, ui: &mut Ui, compact: bool) {
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
        if compact {
            self.compact_training_panel(ui, &set, &puzzle, position, color);
            return;
        }
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
                    ui.strong(format!("{:.0}%", set.stats.accuracy()));
                });
            });
            self.segments(ui, set.set.params.size);
            ui.add_space(7.0);
            self.timer_row(ui, color, false);
            ui.add_space(7.0);
            ui.separator();
            ui.add_space(10.0);
            self.training_stats(ui, &set);
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
            self.solve_membership_note(ui, false);
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            widgets::caption(ui, "S  SKIP");
            widgets::caption(ui, "T  TIMER");
            widgets::caption(ui, "ENTER  NEXT");
        });
    }
    fn compact_training_panel(
        &mut self,
        ui: &mut Ui,
        set: &crate::service::SetSummary,
        puzzle: &crate::puzzle::Puzzle,
        position: usize,
        color: Option<shakmaty::Color>,
    ) {
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.horizontal(|ui| {
            ui.strong(format!("{} / {}", position + 1, set.set.params.size));
            ui.label(
                RichText::new(if self.timer.visible() {
                    self.timer.display(false)
                } else {
                    "—:—".into()
                })
                .monospace()
                .size(21.0),
            );
            if ui
                .small_button("T")
                .on_hover_text("Show / hide timer")
                .clicked()
            {
                self.timer.toggle_visible();
            }
            ui.small(if color == Some(shakmaty::Color::Black) {
                "Black"
            } else {
                "White"
            });
        });
        ui.collapsing(
            format!(
                "Cycle {} · {:.0}% · Details",
                set.latest_cycle.as_ref().map_or(1, |c| c.number),
                set.stats.accuracy()
            ),
            |ui| {
                ui.strong(&set.set.params.name);
                self.training_stats(ui, set);
                ui.small(format!("Rating {}", puzzle.rating));
                if self.settings.show_theme_labels {
                    ui.horizontal_wrapped(|ui| {
                        for theme in &puzzle.themes {
                            ui.small(crate::sets::theme_label(theme));
                        }
                    });
                }
                self.solve_membership_note(ui, false);
            },
        );
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
    fn segments(&self, ui: &mut Ui, total: u32) {
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
        if self.record_error {
            ui.colored_label(ui.visuals().error_fg_color, "Progress could not be saved.");
            if ui.button("Retry saving").clicked() {
                self.record_error = false;
                self.record();
            }
        }
        let Some(s) = &mut self.session else {
            return;
        };
        let state = s.engine.state();
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
                ui.label(RichText::new("Puzzle unavailable").strong().size(17.0));
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
                    format!("{:.0}%", set.stats.accuracy()),
                    "Skips count as misses",
                );
                widgets::stat(
                    ui,
                    "AVERAGE",
                    widgets::duration(set.stats.average_ms() as u64),
                    "per answered puzzle; skips included",
                );
                self.cycle_comparisons(ui, &set);
                ui.label(format!("{} puzzles waiting in Review", self.pending));
                if !set.completed {
                    if widgets::primary(ui, "Start next cycle").clicked() {
                        let id = set.set.id;
                        self.request_training(move |service| {
                            service.next_cycle(id, chrono::Local::now().timestamp())
                        });
                    }
                } else if set.set.params.target_cycles < 10
                    && widgets::primary(ui, "Run another cycle").clicked()
                {
                    let id = set.set.id;
                    self.request_training(move |service| {
                        service.extend_cycle(id, chrono::Local::now().timestamp())
                    });
                }
                if ui.button("Create a harder set").clicked() {
                    self.builder.set_range(crate::sets::normalize_range(
                        set.set.params.rating_min.saturating_add(100),
                        set.set.params.rating_max.saturating_add(100),
                    ));
                    self.navigate(Screen::Builder);
                }
            });
        });
    }
    fn training_stats(&self, ui: &mut Ui, set: &crate::service::SetSummary) {
        widgets::caption(ui, "THIS CYCLE");
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(format!("{:.0}%", set.stats.accuracy()))
                        .size(26.0)
                        .strong(),
                );
                widgets::caption(ui, "ACCURACY");
            });
            ui.add_space(18.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(widgets::duration(set.stats.average_ms() as u64))
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
    }
    pub(in crate::app) fn solve_membership_note(&self, ui: &mut Ui, is_review: bool) {
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
    }

    fn cycle_comparisons(&self, ui: &mut Ui, set: &crate::service::SetSummary) {
        if let Some(p) = self.progress.get(&set.set.id) {
            for (label, baseline) in [
                ("vs cycle 1", p.cycles.first()),
                ("vs previous", p.cycles.iter().rev().nth(1)),
            ] {
                if let Some(base) = baseline {
                    let d = compare(set.stats, base.stats);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(label);
                        if let Some(t) = d.average_time_reduction_percent {
                            widgets::chip(
                                ui,
                                format!("{} avg / puzzle", widgets::delta_time(t)),
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
    }
}
