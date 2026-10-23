use super::super::{Woodpeck, style, widgets};
use crate::{
    service::Job,
    settings::{BoardTheme, ColorTheme, PieceRendering},
    source::LICHESS_DATABASE_URL,
};
use eframe::egui::{self, Ui};
impl Woodpeck {
    pub(in crate::app) fn settings_screen(&mut self, ui: &mut Ui) {
        widgets::title(
            ui,
            "FEEL AT HOME",
            "Settings",
            "A quiet space for focused practice. Make it yours.",
        );
        let before = self.settings.clone();
        widgets::card(ui).show(ui, |ui| {
            ui.set_width((ui.available_width() - 1.0).max(0.0));
            ui.heading("Appearance");
            widgets::caption(ui, "BOARD THEME");
            let columns = ((ui.available_width() + 10.0) / 72.0).floor().max(1.0) as usize;
            egui::Grid::new("board-themes")
                .num_columns(columns)
                .spacing([12.0, 12.0])
                .show(ui, |ui| {
                    for (index, theme) in BoardTheme::ALL.into_iter().enumerate() {
                        let colors = theme.colors();
                        let selected = self.settings.board_theme == theme;
                        let response = ui
                            .vertical(|ui| {
                                let (r, response) = ui.allocate_exact_size(
                                    egui::vec2(60.0, 42.0),
                                    egui::Sense::click(),
                                );
                                for y in 0..2 {
                                    for x in 0..3 {
                                        ui.painter().rect_filled(
                                            egui::Rect::from_min_size(
                                                r.min
                                                    + egui::vec2(x as f32 * 20.0, y as f32 * 20.0),
                                                egui::vec2(20.0, 20.0),
                                            ),
                                            0,
                                            style::color(if (x + y) % 2 == 0 {
                                                colors.light
                                            } else {
                                                colors.dark
                                            }),
                                        );
                                    }
                                }
                                if selected {
                                    ui.painter().rect_stroke(
                                        r.expand(2.0),
                                        3,
                                        egui::Stroke::new(
                                            2.0,
                                            style::color(self.palette.semantic().accent),
                                        ),
                                        egui::StrokeKind::Outside,
                                    );
                                }
                                ui.small(format!("{theme:?}"));
                                response
                            })
                            .inner;
                        if response.clicked() {
                            self.settings.board_theme = theme;
                        }
                        if (index + 1).is_multiple_of(columns) {
                            ui.end_row();
                        }
                    }
                });
            ui.add_space(10.0);
            widgets::caption(ui, "COLOR THEME");
            egui::ComboBox::from_id_salt("color-theme")
                .selected_text(format!("{:?}", self.settings.color_theme))
                .show_ui(ui, |ui| {
                    for theme in [
                        ColorTheme::System,
                        ColorTheme::Omarchy,
                        ColorTheme::Light,
                        ColorTheme::Dark,
                    ] {
                        ui.selectable_value(
                            &mut self.settings.color_theme,
                            theme,
                            format!("{theme:?}"),
                        );
                    }
                });
            ui.small(
                "System follows Omarchy when available. Desktop changes reload automatically.",
            );
            widgets::caption(ui, "PIECE RENDERING");
            egui::ComboBox::from_id_salt("pieces")
                .selected_text(match self.settings.piece_rendering {
                    PieceRendering::Vector => "Cburnett · smooth",
                    PieceRendering::Cburnett => "Cburnett · crisp",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.settings.piece_rendering,
                        PieceRendering::Vector,
                        "Cburnett · smooth",
                    );
                    ui.selectable_value(
                        &mut self.settings.piece_rendering,
                        PieceRendering::Cburnett,
                        "Cburnett · crisp",
                    );
                });
            ui.small("Embedded SVG pieces by Colin M.L. Burnett.");
        });
        ui.add_space(8.0);
        widgets::card(ui).show(ui, |ui| {
            ui.set_width((ui.available_width() - 1.0).max(0.0));
            ui.heading("Practice");
            ui.checkbox(&mut self.settings.completion_sound, "Completion sound");
            ui.checkbox(&mut self.settings.show_theme_labels, "Show tactical labels");
            ui.checkbox(
                &mut self.settings.auto_advance,
                "Auto-advance to the next puzzle",
            );
            ui.checkbox(
                &mut self.settings.shuffle_each_cycle,
                "Shuffle each cycle by default",
            );
            widgets::caption(ui, "YOUR PUZZLE RATING");
            ui.spacing_mut().slider_width = (ui.available_width() - 80.0).clamp(80.0, 240.0);
            ui.add(egui::Slider::new(
                &mut self.settings.user_rating,
                400..=3000,
            ));
            ui.small("Used to recommend a comfortable training range.");
        });
        ui.add_space(8.0);
        widgets::card(ui).show(ui, |ui| {
            ui.set_width((ui.available_width() - 1.0).max(0.0));
            ui.heading("Puzzle library");
            ui.strong(format!(
                "{} puzzles available offline",
                self.service.puzzle_count().unwrap_or(0)
            ));
            ui.label(&self.status);
            if self.service.is_busy() {
                ui.add(
                    egui::ProgressBar::new(self.job_fraction)
                        .animate(true)
                        .text(&self.status),
                );
                if ui.button("Cancel import / download").clicked() {
                    self.service.cancel_all();
                }
            }
            ui.add_enabled_ui(!self.service.is_busy(), |ui| {
                if ui
                    .button("Download full Lichess puzzle database (~300 MB)")
                    .clicked()
                {
                    self.submit(Job::ImportFull {
                        location: LICHESS_DATABASE_URL.into(),
                    });
                }
                if ui.button("Re-import starter pack").clicked() {
                    self.submit(Job::ImportStarter { force: true });
                }
            });
            ui.add_space(8.0);
            widgets::caption(ui, "DATA LOCATION");
            ui.add(egui::Label::new(self.data_path.display().to_string()).wrap());
        });
        ui.add_space(8.0);
        widgets::card(ui).show(ui, |ui| {
            ui.set_width((ui.available_width() - 1.0).max(0.0));
            ui.heading("Export a set");
            ui.label("Save your fixed positions and solution lines as PGN.");
            self.set_selector(ui, "export-set");
            ui.add(
                egui::TextEdit::singleline(&mut self.export_path)
                    .desired_width(ui.available_width())
                    .hint_text("/path/to/set.pgn"),
            );
            if ui
                .add_enabled(
                    self.selected_set.is_some() && !self.export_path.trim().is_empty(),
                    egui::Button::new("Export set PGN"),
                )
                .clicked()
                && let Some(id) = self.selected_set
            {
                match self
                    .service
                    .export_pgn(id, std::path::Path::new(&self.export_path))
                {
                    Ok(()) => self.toast = Some(("PGN exported".into(), std::time::Instant::now())),
                    Err(e) => self.error(e),
                }
            }
        });
        if before != self.settings {
            if let Err(e) = self.service.set_settings(&self.settings) {
                self.error(e);
            }
            if before.color_theme != self.settings.color_theme {
                self.theme = style::ThemeWatcher::new(self.settings.color_theme);
                let p = crate::theme::select(self.settings.color_theme, None);
                style::apply(ui.ctx(), &p);
                self.palette = p;
            }
        }
        ui.add_space(18.0);
        ui.small("Puzzles from the Lichess puzzle database (CC0)");
        ui.small("cburnett pieces · GPLv2+ / CC BY-SA 3.0 · Colin M.L. Burnett");
    }
}
