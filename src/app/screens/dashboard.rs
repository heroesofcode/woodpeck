use super::super::{Screen, Woodpeck, style, widgets};
use eframe::egui::{self, RichText, Ui};
impl Woodpeck {
    pub(in crate::app) fn dashboard(&mut self, ui: &mut Ui) {
        widgets::title(
            ui,
            "YOUR TRAINING SPACE",
            "Small steps. Lasting patterns.",
            "A fixed set. A little repetition. A sharper chess mind.",
        );
        let accent = style::color(self.palette.semantic().accent);
        if self.pending > 0 {
            widgets::card(ui)
                .fill(accent.gamma_multiply(0.09))
                .stroke(egui::Stroke::new(1.0, accent.gamma_multiply(0.4)))
                .show(ui, |ui| {
                    ui.set_width((ui.available_width() - 1.0).max(0.0));
                    widgets::caption(ui, "TURN MISSES INTO INSTINCT");
                    ui.label(
                        RichText::new(format!("{} puzzles waiting in Review", self.pending))
                            .strong()
                            .size(21.0),
                    );
                    ui.label("Give the patterns you missed another look.");
                    if widgets::primary(ui, "Open Review  >").clicked() {
                        self.navigate(Screen::Review);
                    }
                });
            ui.add_space(16.0);
        }
        ui.horizontal(|ui| {
            ui.heading("Your sets");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("+ New set").clicked() {
                    self.navigate(Screen::Builder);
                }
            });
        });
        ui.add_space(6.0);
        if self.sets.is_empty() {
            widgets::card(ui).show(ui, |ui| {
                ui.heading("Make your first set");
                ui.label("Start with 50–100 puzzles, a little below your peak. Your positions stay fixed as you work through each cycle.");
                if widgets::primary(ui, "Build a training set").clicked() {
                    self.navigate(Screen::Builder);
                }
            });
        }
        for set in self.sets.clone() {
            self.dashboard_set(ui, &set);
            ui.add_space(6.0);
        }
        ui.add_space(16.0);
        widgets::caption(ui, "LOCAL & READY");
        if self.service.is_busy() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(&self.status);
            });
        } else {
            ui.label(&self.status);
        }
        ui.add_space(20.0);
        ui.separator();
        ui.small("Puzzles from the Lichess puzzle database (CC0)");
        ui.small("Your practice stays on this device.");
    }
    /// Theme chips for a set card: included themes in the accent color, excluded ones as "not …".
    fn dashboard_set_themes(&self, ui: &mut Ui, params: &crate::sets::BuilderParams) {
        let Some(summary) = params.theme_summary() else {
            return;
        };
        let semantic = self.palette.semantic();
        let (accent, avoid) = (
            style::color(semantic.accent),
            style::color(semantic.incorrect),
        );
        let chips: Vec<(String, egui::Color32)> = params
            .include_themes
            .iter()
            .map(|t| (crate::sets::theme_name(t), accent))
            .chain(
                params
                    .exclude_themes
                    .iter()
                    .map(|t| (format!("not {}", crate::sets::theme_name(t)), avoid)),
            )
            .collect();
        let (shown, hidden) = widgets::chip_overflow(&chips, 4);
        ui.add_space(3.0);
        ui.horizontal_wrapped(|ui| {
            if params.theme_match == crate::sets::ThemeMatch::All {
                widgets::caption(ui, "ALL OF");
            }
            for (text, color) in shown {
                widgets::chip(ui, text.as_str(), *color);
            }
            if hidden > 0 {
                widgets::chip(ui, format!("+{hidden}"), ui.visuals().weak_text_color());
            }
        })
        .response
        .on_hover_text(summary);
    }
    fn dashboard_set(&mut self, ui: &mut Ui, set: &crate::service::SetSummary) {
        let accent = style::color(self.palette.semantic().accent);
        widgets::card(ui).show(ui, |ui| {
            ui.set_width((ui.available_width() - 1.0).max(0.0));
            ui.horizontal_wrapped(|ui| {
                widgets::chip(
                    ui,
                    if set.completed {
                        "COMPLETED"
                    } else {
                        "IN PRACTICE"
                    },
                    accent,
                );
                widgets::caption(
                    ui,
                    format!(
                        "{} PUZZLES · RATED {}–{}",
                        set.set.params.size, set.set.params.rating_min, set.set.params.rating_max
                    ),
                );
            });
            self.dashboard_set_themes(ui, &set.set.params);
            ui.add_space(5.0);
            ui.label(RichText::new(&set.set.params.name).size(23.0).strong());
            ui.label(format!(
                "Cycle {} / {} · {} of {} positions",
                set.latest_cycle.as_ref().map_or(1, |c| c.number),
                set.set.params.target_cycles,
                set.stats.attempted(),
                set.set.params.size
            ));
            ui.add(
                egui::ProgressBar::new(
                    set.stats.attempted() as f32 / set.set.params.size.max(1) as f32,
                )
                .desired_height(5.0),
            );
            ui.horizontal_wrapped(|ui| {
                ui.small(format!(
                    "{:.0}% accuracy",
                    set.stats.accuracy(set.set.params.size)
                ));
                ui.small(format!(
                    "{} solving time",
                    widgets::duration(set.stats.time_ms)
                ));
                if set.review_pending > 0 {
                    widgets::chip(ui, format!("{} in Review", set.review_pending), accent);
                }
            });
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if widgets::primary(ui, "Continue  >").clicked() {
                    self.selected_set = Some(set.set.id);
                    self.screen = Screen::Training;
                    self.resume_selected();
                }
                if ui.button("Progress").clicked() {
                    self.selected_set = Some(set.set.id);
                    self.navigate(Screen::Progress);
                }
                if ui.small_button("Delete").clicked() {
                    self.delete = Some(set.set.id);
                }
            });
        });
    }
}
