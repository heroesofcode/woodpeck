use super::super::{Woodpeck, style, widgets};
use crate::{
    service::Job,
    sets::{
        CYCLE_CARDS, PRESETS, PresetKind, ThemeMatch, ThemeSelection, difficulty_caption,
        search_themes, theme_name,
    },
};
use eframe::egui::{self, RichText, Ui};
impl Woodpeck {
    pub(in crate::app) fn builder_screen(&mut self, ui: &mut Ui) {
        widgets::title(
            ui,
            "MAKE IT YOURS",
            "Build a training set",
            "Choose your focus. Keep the positions. Watch the patterns stick.",
        );
        if ui.available_width() > 800.0 {
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(170.0, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        for (i, preset) in PRESETS.into_iter().enumerate() {
                            if i == 1 {
                                ui.add_space(15.0);
                                widgets::caption(ui, "WEAKNESS");
                            }
                            if matches!(preset.kind, PresetKind::Level(800, 1000)) {
                                ui.add_space(15.0);
                                widgets::caption(ui, "LEVEL");
                            }
                            let selected = self.builder.preset_id.as_deref() == Some(preset.id);
                            if ui
                                .add_sized(
                                    [160.0, 36.0],
                                    egui::Button::new(preset.label).selected(selected),
                                )
                                .on_hover_text(preset.description)
                                .clicked()
                            {
                                self.builder = preset.builder(self.settings.user_rating);
                                self.builder.shuffle_each_cycle = self.settings.shuffle_each_cycle;
                                self.creation = None;
                            }
                        }
                    },
                );
                ui.add_space(18.0);
                ui.vertical(|ui| {
                    self.builder_form(ui);
                });
            });
        } else {
            egui::ComboBox::from_id_salt("preset")
                .width(ui.available_width() - 12.0)
                .selected_text(
                    self.builder
                        .preset_id
                        .as_deref()
                        .and_then(crate::sets::Preset::find)
                        .map_or("Custom set", |p| p.label),
                )
                .show_ui(ui, |ui| {
                    for p in PRESETS {
                        if ui
                            .selectable_label(
                                self.builder.preset_id.as_deref() == Some(p.id),
                                p.label,
                            )
                            .clicked()
                        {
                            self.builder = p.builder(self.settings.user_rating);
                            self.builder.shuffle_each_cycle = self.settings.shuffle_each_cycle;
                            self.creation = None;
                        }
                    }
                });
            ui.add_space(12.0);
            self.builder_form(ui);
        }
    }
    fn builder_form(&mut self, ui: &mut Ui) {
        widgets::card(ui).show(ui, |ui| {
            ui.set_width((ui.available_width() - 1.0).max(0.0));
            self.builder_basics(ui);
            ui.add_space(14.0);
            ui.separator();
            ui.add_space(12.0);
            self.builder_difficulty(ui);
            ui.add_space(14.0);
            ui.separator();
            ui.add_space(12.0);
            self.builder_volume(ui);
            self.builder_create(ui);
        });
    }
    fn builder_basics(&mut self, ui: &mut Ui) {
        ui.heading("Basic info");
        ui.label("Name your set and pick the tactical themes to include or avoid.");
        ui.add_space(8.0);
        widgets::caption(ui, "SET NAME");
        ui.add(
            egui::TextEdit::singleline(&mut self.builder.name).desired_width(ui.available_width()),
        );
        ui.add_space(6.0);
        self.builder_themes(ui);
    }
    /// Searchable include/exclude picker. Each click cycles a theme: include → exclude → off.
    fn builder_themes(&mut self, ui: &mut Ui) {
        let semantic = self.palette.semantic();
        let (include, exclude) = (
            style::color(semantic.correct),
            style::color(semantic.incorrect),
        );
        let snapshot = |b: &crate::sets::BuilderParams| {
            (
                b.include_themes.clone(),
                b.exclude_themes.clone(),
                b.theme_match,
            )
        };
        let before = snapshot(&self.builder);
        widgets::caption(ui, "THEMES");
        ui.small("Click a theme to include it, again to exclude it, and once more to clear it.");
        ui.add_space(4.0);
        self.builder_theme_chips(ui, include, exclude);
        self.builder_theme_picker(ui, include, exclude);
        if self.builder.include_themes.len() < 2 {
            // "All" is only meaningful (and only shown) with two or more included themes.
            self.builder.theme_match = ThemeMatch::Any;
        }
        if before != snapshot(&self.builder) {
            // A shortfall notice describes the old filter; drop it once themes change.
            self.creation = None;
        }
    }
    fn builder_theme_chips(&mut self, ui: &mut Ui, include: egui::Color32, exclude: egui::Color32) {
        let mut removed = None;
        if self.builder.include_themes.is_empty() && self.builder.exclude_themes.is_empty() {
            ui.label(RichText::new("Any theme").color(ui.visuals().weak_text_color()));
        } else {
            ui.horizontal_wrapped(|ui| {
                let chosen = self
                    .builder
                    .include_themes
                    .iter()
                    .map(|t| (t, true))
                    .chain(self.builder.exclude_themes.iter().map(|t| (t, false)));
                for (key, included) in chosen {
                    let (sign, color) = if included {
                        ("+", include)
                    } else {
                        ("−", exclude)
                    };
                    if widgets::removable_chip(ui, &format!("{sign} {}", theme_name(key)), color)
                        .on_hover_text("Remove")
                        .clicked()
                    {
                        removed = Some(key.clone());
                    }
                }
                if ui.small_button("Clear all").clicked() {
                    self.builder.include_themes.clear();
                    self.builder.exclude_themes.clear();
                }
            });
        }
        if let Some(key) = removed {
            self.builder.clear_theme(&key);
        }
        if self.builder.include_themes.len() > 1 {
            ui.horizontal_wrapped(|ui| {
                ui.small("Included themes:");
                for (mode, label, hint) in [
                    (
                        ThemeMatch::Any,
                        "Match any",
                        "Puzzles need at least one included theme",
                    ),
                    (
                        ThemeMatch::All,
                        "Match all",
                        "Puzzles need every included theme",
                    ),
                ] {
                    ui.selectable_value(&mut self.builder.theme_match, mode, label)
                        .on_hover_text(hint);
                }
            });
        }
    }
    fn builder_theme_picker(
        &mut self,
        ui: &mut Ui,
        include: egui::Color32,
        exclude: egui::Color32,
    ) {
        ui.add_space(4.0);
        ui.add(
            egui::TextEdit::singleline(&mut self.theme_search)
                .hint_text("Search themes…")
                .desired_width(ui.available_width().min(400.0)),
        );
        egui::ScrollArea::vertical()
            .id_salt("theme-picker")
            .max_height(150.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let mut any = false;
                    for (key, label) in search_themes(&self.theme_search) {
                        any = true;
                        let state = self.builder.theme_selection(key);
                        let color = match state {
                            ThemeSelection::Included => Some(include),
                            ThemeSelection::Excluded => Some(exclude),
                            ThemeSelection::Neutral => None,
                        };
                        let mut text = RichText::new(label).size(12.0);
                        if state == ThemeSelection::Excluded {
                            text = text.strikethrough();
                        }
                        let mut button = egui::Button::new(
                            text.color(color.unwrap_or_else(|| ui.visuals().text_color())),
                        )
                        .corner_radius(6);
                        if let Some(color) = color {
                            button = button
                                .fill(color.gamma_multiply(0.15))
                                .stroke(egui::Stroke::new(1.0, color));
                        }
                        let hover = match state {
                            ThemeSelection::Neutral => "Click to include",
                            ThemeSelection::Included => "Included · click to exclude",
                            ThemeSelection::Excluded => "Excluded · click to clear",
                        };
                        if ui.add(button).on_hover_text(hover).clicked() {
                            self.builder.cycle_theme(key);
                        }
                    }
                    if !any {
                        ui.small("No theme matches your search.");
                    }
                });
            });
    }
    fn builder_difficulty(&mut self, ui: &mut Ui) {
        use crate::sets::{
            MIN_RANGE_WIDTH, RANGE_STEP, RATING_BOUNDS, RangeHandle, drag_range, quick_ranges,
        };
        let rating = self.settings.user_rating;
        let accent = style::color(self.palette.semantic().accent);
        let before = self.builder.range();
        ui.heading("Difficulty");
        ui.label(format!(
            "Choose the puzzle rating range. A little below your peak works best; your puzzle rating is {rating}."
        ));
        ui.add_space(8.0);
        widgets::caption(
            ui,
            format!(
                "RATING RANGE · {}–{}",
                self.builder.rating_min, self.builder.rating_max
            ),
        );
        if let Some((handle, value)) = widgets::range_slider(
            ui,
            self.builder.range(),
            RATING_BOUNDS,
            RANGE_STEP,
            accent,
            Some((rating, "You")),
        ) {
            self.builder
                .set_range(drag_range(self.builder.range(), handle, value));
        }
        ui.horizontal_wrapped(|ui| {
            let (mut min, mut max) = self.builder.range();
            ui.label("Min");
            let min_edit = ui
                .add(
                    egui::DragValue::new(&mut min)
                        .range(RATING_BOUNDS.0..=RATING_BOUNDS.1 - MIN_RANGE_WIDTH)
                        .speed(RANGE_STEP),
                )
                .on_hover_text("Lowest puzzle rating, inclusive");
            ui.label("Max");
            let max_edit = ui
                .add(
                    egui::DragValue::new(&mut max)
                        .range(RATING_BOUNDS.0 + MIN_RANGE_WIDTH..=RATING_BOUNDS.1)
                        .speed(RANGE_STEP),
                )
                .on_hover_text("Highest puzzle rating, inclusive");
            if min_edit.changed() {
                self.builder
                    .set_range(drag_range(self.builder.range(), RangeHandle::Min, min));
            } else if max_edit.changed() {
                self.builder
                    .set_range(drag_range(self.builder.range(), RangeHandle::Max, max));
            }
            widgets::chip(ui, difficulty_caption(self.builder.range(), rating), accent);
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (label, range) in quick_ranges(rating) {
                if ui
                    .selectable_label(self.builder.range() == range, label)
                    .on_hover_text(format!("{}–{}", range.0, range.1))
                    .clicked()
                {
                    self.builder.set_range(range);
                }
            }
        });
        ui.small(format!(
            "Both ends are inclusive. Ranges span {}–{} and are at least {MIN_RANGE_WIDTH} points wide; the range is never widened for you.",
            RATING_BOUNDS.0, RATING_BOUNDS.1
        ));
        if self.builder.range() != before {
            // A shortfall notice describes the old range; drop it once the range changes.
            self.creation = None;
        }
    }
    fn builder_volume(&mut self, ui: &mut Ui) {
        ui.heading("Training volume");
        ui.label("Start with 50–100 puzzles if you're unsure.");
        widgets::caption(ui, "TOTAL PUZZLES");
        ui.spacing_mut().slider_width = (ui.available_width() - 74.0).clamp(100.0, 500.0);
        ui.add(egui::Slider::new(&mut self.builder.size, 20..=500));
        ui.add_space(6.0);
        widgets::caption(ui, "TRAINING CYCLES");
        let width = ((ui.available_width() - 20.0) / 3.0).max(50.0);
        ui.horizontal(|ui| {
            for (cycles, label) in CYCLE_CARDS {
                let selected = self.builder.target_cycles == cycles;
                let response =
                    ui.add_sized(
                        [width, 76.0],
                        egui::Button::new(
                            RichText::new(format!("{cycles} cycles\n{label}"))
                                .size(if width < 100.0 { 11.0 } else { 14.0 }),
                        )
                        .wrap()
                        .selected(selected)
                        .corner_radius(10),
                    );
                if response.clicked() {
                    self.builder.target_cycles = cycles;
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Custom cycles");
            ui.add(egui::DragValue::new(&mut self.builder.target_cycles).range(1..=10));
        });
        ui.checkbox(&mut self.builder.shuffle_each_cycle, "Shuffle each cycle");
        ui.add_space(8.0);
        ui.small(
            "Positions stay fixed across cycles. Puzzle ratings are separate from playing ratings.",
        );
        ui.add_space(12.0);
        let (minimum, maximum) = self.builder.estimated_time();
        widgets::caption(ui, "EST. COMPLETION TIME");
        ui.strong(format!(
            "{:.1}–{:.1} hours",
            minimum.as_secs_f64() / 3600.0,
            maximum.as_secs_f64() / 3600.0
        ));
    }
    fn builder_create(&mut self, ui: &mut Ui) {
        if widgets::primary(ui, "Create Set  >").clicked() {
            let now = chrono::Local::now().timestamp();
            match self
                .service
                .create_set(&self.builder, now.unsigned_abs(), now)
            {
                Ok(c) => self.created(c),
                Err(e) => self.error(e),
            }
        }
        if let Some(c) = &self.creation
            && c.set_id.is_none()
        {
            ui.add_space(10.0);
            ui.colored_label(
                style::color(self.palette.semantic().warning),
                format!(
                    "{} puzzles short. Found {} of {} rated {}–{}{}. Widen the range, change the themes, or fetch more.",
                    c.shortfall(),
                    c.available,
                    c.requested,
                    c.range.0,
                    c.range.1,
                    self.builder
                        .theme_summary()
                        .map_or_else(String::new, |t| format!(" matching {t}")),
                ),
            );
            if ui
                .add_enabled(
                    !self.service.is_busy(),
                    egui::Button::new("Fetch more from Lichess"),
                )
                .clicked()
            {
                let now = chrono::Local::now().timestamp();
                self.submit(Job::CreateSetWithFallback {
                    params: self.builder.clone(),
                    seed: now.unsigned_abs(),
                    now,
                });
            }
        }
        if self.service.is_busy() {
            ui.horizontal_wrapped(|ui| {
                ui.spinner();
                ui.label(&self.status);
            });
            ui.add(
                egui::ProgressBar::new(0.0)
                    .animate(true)
                    .text("Background import / fetch"),
            );
        }
    }
}
