use super::super::{Screen, Woodpeck, charts, style, widgets};
use eframe::egui::{self, RichText, Ui};
impl Woodpeck {
    pub(in crate::app) fn set_selector(&mut self, ui: &mut Ui, id: &str) {
        egui::ComboBox::from_id_salt(id)
            .width(ui.available_width().min(340.0) - 12.0)
            .selected_text(
                self.sets
                    .iter()
                    .find(|s| Some(s.set.id) == self.selected_set)
                    .map_or("Choose a set", |s| s.set.params.name.as_str()),
            )
            .show_ui(ui, |ui| {
                for s in &self.sets {
                    ui.selectable_value(&mut self.selected_set, Some(s.set.id), &s.set.params.name);
                }
            });
    }
    pub(in crate::app) fn progress_screen(&mut self, ui: &mut Ui) {
        widgets::title(
            ui,
            "THE BIGGER PICTURE",
            "See your practice pay off",
            "Less time calculating. More patterns you simply recognize.",
        );
        self.set_selector(ui, "progress-set");
        ui.add_space(12.0);
        let Some(id) = self.selected_set else {
            ui.label("Your progress starts with a training set.");
            if widgets::primary(ui, "Build a set").clicked() {
                self.navigate(Screen::Builder);
            }
            return;
        };
        let p = match self.service.progress(id) {
            Ok(p) => p,
            Err(e) => {
                self.error(e);
                return;
            }
        };
        let accent = style::color(self.palette.semantic().accent);
        widgets::card(ui).show(ui, |ui| {
            ui.set_width((ui.available_width() - 1.0).max(0.0));
            widgets::caption(ui, "CYCLE PROGRESS");
            let latest = p.cycles.iter().rev().find(|c| c.complete);
            let first = p.cycles.iter().find(|c| c.complete);
            ui.label(
                RichText::new(latest.map_or_else(
                    || "Your first cycle is in progress".into(),
                    |c| format!("Cycle {} compared with cycle 1", c.number),
                ))
                .size(22.0)
                .strong(),
            );
            if let Some((first, latest)) = first.zip(latest) {
                ui.add_space(10.0);
                let time_delta = p
                    .report
                    .latest_vs_first
                    .and_then(|d| d.time_reduction_percent)
                    .map(widgets::delta_time);
                let accuracy_delta = p
                    .report
                    .latest_vs_first
                    .map(|d| widgets::delta_points(d.accuracy_points));
                let panels = |uis: &mut [Ui]| {
                    widgets::comparison(
                        &mut uis[0],
                        "COMPLETION TIME",
                        &widgets::duration(first.stats.time_ms),
                        &widgets::duration(latest.stats.time_ms),
                        time_delta.as_deref(),
                        accent,
                    );
                    widgets::comparison(
                        &mut uis[1],
                        "ACCURACY",
                        &format!("{:.0}%", first.stats.accuracy(first.stats.attempted())),
                        &format!("{:.0}%", latest.stats.accuracy(latest.stats.attempted())),
                        accuracy_delta.as_deref(),
                        accent,
                    );
                };
                if ui.available_width() > 600.0 {
                    ui.columns(2, panels);
                } else {
                    widgets::comparison(
                        ui,
                        "COMPLETION TIME",
                        &widgets::duration(first.stats.time_ms),
                        &widgets::duration(latest.stats.time_ms),
                        time_delta.as_deref(),
                        accent,
                    );
                    widgets::comparison(
                        ui,
                        "ACCURACY",
                        &format!("{:.0}%", first.stats.accuracy(first.stats.attempted())),
                        &format!("{:.0}%", latest.stats.accuracy(latest.stats.attempted())),
                        accuracy_delta.as_deref(),
                        accent,
                    );
                }
            } else {
                ui.label("Finish your first cycle to establish a baseline.");
            }
        });
        ui.add_space(10.0);
        let latest = p.cycles.last();
        let total = self
            .sets
            .iter()
            .find(|s| s.set.id == id)
            .map_or(0, |s| s.set.params.size);
        let detail = latest.map_or_else(String::new, |c| {
            format!("{} of {} in cycle {}", c.stats.attempted(), total, c.number)
        });
        let tiles = [
            (
                "PUZZLES ATTEMPTED",
                p.report.puzzles_attempted.to_string(),
                detail.as_str(),
            ),
            (
                "ACCURACY",
                format!("{:.1}%", p.report.accuracy.percent()),
                "across your training cycles",
            ),
            (
                "AVERAGE SOLVE TIME",
                widgets::duration(p.report.average_ms as u64),
                "per attempted puzzle",
            ),
            (
                "BEST CYCLE",
                p.report
                    .best_cycle
                    .map_or_else(|| "—".into(), |c| widgets::duration(c.stats.time_ms)),
                "fastest completion",
            ),
        ];
        let columns = if ui.available_width() > 1000.0 {
            4
        } else if ui.available_width() > 500.0 {
            2
        } else {
            1
        };
        for row in tiles.chunks(columns) {
            ui.columns(columns, |uis| {
                for (i, (label, value, detail)) in row.iter().enumerate() {
                    widgets::stat(&mut uis[i], label, value.clone(), detail);
                }
            });
        }
        ui.add_space(15.0);
        ui.heading("Cycle performance");
        ui.label("Solve time and accuracy for each cycle. Hollow marks show a cycle in progress.");
        if ui.available_width() > 700.0 {
            ui.columns(2, |uis| {
                widgets::card(&uis[0]).show(&mut uis[0], |ui| {
                    ui.heading("Time by cycle");
                    charts::chart(
                        ui,
                        &p.cycles,
                        total,
                        false,
                        style::color(
                            self.palette
                                .color("blue")
                                .unwrap_or(self.palette.semantic().accent),
                        ),
                    );
                });
                widgets::card(&uis[1]).show(&mut uis[1], |ui| {
                    ui.heading("Accuracy by cycle");
                    charts::chart(ui, &p.cycles, total, true, accent);
                });
            });
        } else {
            for accuracy in [false, true] {
                widgets::card(ui).show(ui, |ui| {
                    ui.heading(if accuracy {
                        "Accuracy by cycle"
                    } else {
                        "Time by cycle"
                    });
                    charts::chart(ui, &p.cycles, total, accuracy, accent);
                });
            }
        }
        ui.add_space(12.0);
        widgets::card(ui).show(ui, |ui| {
            ui.set_width((ui.available_width() - 1.0).max(0.0));
            ui.heading("Performance by theme");
            ui.add_space(6.0);
            egui::Grid::new("themes-table")
                .num_columns(3)
                .spacing([16.0, 12.0])
                .striped(true)
                .show(ui, |ui| {
                    widgets::caption(ui, "THEME");
                    widgets::caption(ui, "ATTEMPTS");
                    widgets::caption(ui, "ACCURACY");
                    ui.end_row();
                    for (theme, accuracy) in &p.report.by_theme {
                        ui.label(crate::sets::theme_label(theme));
                        ui.label(accuracy.attempts.to_string());
                        ui.colored_label(accent, format!("{:.0}%", accuracy.percent()));
                        ui.end_row();
                    }
                });
        });
        widgets::card(ui).show(ui, |ui| {
            ui.set_width((ui.available_width() - 1.0).max(0.0));
            ui.heading("Most-missed positions");
            ui.label("Find the positions you miss most often.");
            for missed in p.report.most_missed.iter().take(10) {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(format!("Position {}", missed.position));
                    ui.small(format!(
                        "{} misses · {:.0}% accuracy",
                        missed.misses,
                        missed.accuracy.percent()
                    ));
                    if ui.link("Lichess").clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(format!(
                            "https://lichess.org/training/{}",
                            missed.puzzle_id
                        )));
                    }
                });
            }
            if p.report.most_missed.is_empty() {
                ui.label("No missed positions yet.");
            }
        });
    }
}
