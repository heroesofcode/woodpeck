//! Shared library presentation and welcome state. Imports keep running after navigation.
use super::{Screen, Woodpeck, style, widgets};
use crate::{
    service::{Job, JobHandle},
    source::{ImportProgress, LICHESS_DATABASE_URL, LibrarySize, RATING_BANDS},
};
use eframe::egui::{self, Color32, RichText, Ui};
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum WelcomeStep {
    #[default]
    Choose,
    Downloading,
    Done,
    Error,
}
pub(super) struct Library {
    pub selected: LibrarySize,
    pub step: WelcomeStep,
    pub job: Option<JobHandle>,
    pub progress: ImportProgress,
    pub count: u64,
    pub bands: [u64; RATING_BANDS],
    pub message: String,
    pub entered: Instant,
}
impl Default for Library {
    fn default() -> Self {
        Self {
            selected: LibrarySize::Medium,
            step: WelcomeStep::Choose,
            job: None,
            progress: ImportProgress::default(),
            count: 0,
            bands: [0; RATING_BANDS],
            message: String::new(),
            entered: Instant::now(),
        }
    }
}
impl Woodpeck {
    pub(super) fn welcome_step(&mut self, step: WelcomeStep) {
        self.library.step = step;
        self.library.entered = Instant::now();
    }
    fn finish_onboarding(&mut self) -> bool {
        match self.service.set_local_setting("onboarding_done", "true") {
            Ok(()) => true,
            Err(e) => {
                self.error(e);
                false
            }
        }
    }
    pub(super) fn start_library(&mut self, size: LibrarySize) {
        if !self.finish_onboarding() {
            return;
        }
        let job = if size == LibrarySize::Starter {
            Job::ImportStarter { force: true }
        } else {
            Job::ImportLibrary {
                location: LICHESS_DATABASE_URL.into(),
                size,
                limit: size.limit(),
            }
        };
        match self.service.submit(job) {
            Ok(handle) => {
                self.library.selected = size;
                self.library.job = Some(handle);
                self.library.progress = ImportProgress::default();
                self.library.message.clear();
                self.welcome_step(WelcomeStep::Downloading);
            }
            Err(e) => {
                self.library.message = e.to_string();
                self.welcome_step(WelcomeStep::Error);
            }
        }
    }
    pub(super) fn show_welcome(&mut self) {
        if self.library.job.is_none() {
            self.welcome_step(WelcomeStep::Choose);
        }
        self.screen = Screen::Welcome;
    }
    pub(super) fn welcome(&mut self, ui: &mut Ui) {
        let accent = style::color(self.palette.semantic().accent);
        let age = self.library.entered.elapsed().as_secs_f32();
        let settled = self.screenshot.is_some() || ui.style().animation_time == 0.0;
        let entry = if settled { 1.0 } else { (age / 0.45).min(1.0) };
        let eased = 1.0 - (1.0 - entry).powi(3);
        if entry < 1.0 {
            ui.ctx().request_repaint();
        }
        ui.add_space(12.0 + 16.0 * (1.0 - eased));
        ui.set_opacity(eased);
        ui.horizontal(|ui| {
            widgets::logo(ui, accent);
            ui.label(RichText::new("woodpeck").size(23.0).strong());
            widgets::chip(ui, "YOUR NEXT CHAPTER", accent);
        });
        ui.add_space(24.0);
        match self.library.step {
            WelcomeStep::Choose => self.welcome_choices(ui),
            WelcomeStep::Downloading => {
                widgets::title(
                    ui,
                    "MAKING ROOM FOR PROGRESS",
                    "Your library is growing",
                    "A little preparation. Thousands of patterns to discover.",
                );
                self.library_progress(ui);
                ui.add_space(16.0);
                if widgets::primary(ui, "Continue in the background").clicked()
                    && self.finish_onboarding()
                {
                    self.navigate(Screen::Builder);
                }
                if ui.button("Cancel").clicked() {
                    self.cancel_library();
                }
                ui.small("Committed puzzles stay on this computer if you cancel.");
            }
            WelcomeStep::Done => {
                self.celebrate(ui, eased);
                widgets::title(
                    ui,
                    "READY WHEN YOU ARE",
                    "Your library is ready",
                    "Build a set. Repeat the patterns. Make them second nature.",
                );
                ui.label(
                    RichText::new(format!(
                        "{} puzzles available offline",
                        widgets::grouped_count(self.library.count)
                    ))
                    .size(24.0)
                    .strong(),
                );
                ui.small(format!(
                    "{} accepted this import · {:.1} MB read",
                    widgets::grouped_count(self.library.progress.accepted),
                    self.library.progress.bytes as f64 / 1_000_000.0
                ));
                histogram(ui, &self.library.bands, None, accent, 90.0);
                ui.add_space(20.0);
                if widgets::primary(ui, "Start training").clicked() {
                    self.navigate(Screen::Builder);
                }
            }
            WelcomeStep::Error => {
                widgets::title(
                    ui,
                    "LET’S TRY AGAIN",
                    "Download interrupted",
                    "Your committed puzzles are safe. You can retry whenever you’re ready.",
                );
                ui.colored_label(
                    style::color(self.palette.semantic().incorrect),
                    &self.library.message,
                );
                if widgets::primary(ui, "Retry download").clicked() {
                    self.start_library(self.library.selected);
                }
                if ui.button("Choose another size").clicked() {
                    self.welcome_step(WelcomeStep::Choose);
                }
                if ui.button("Start with the starter pack").clicked() {
                    self.skip_welcome();
                }
            }
        }
        self.welcome_keyboard(ui);
    }
    fn welcome_choices(&mut self, ui: &mut Ui) {
        widgets::title(
            ui,
            "A WORLD OF PATTERNS",
            "Build your puzzle library",
            "Choose how much room you’d like to explore. You can always add more later.",
        );
        ui.label("Free puzzles from Lichess, filtered for quality and playable solutions.");
        ui.hyperlink_to(
            "Lichess puzzle database · CC0",
            "https://database.lichess.org/#puzzles",
        );
        ui.small("Stored only on this computer. Nothing is uploaded. Sync never carries your puzzle library.");
        ui.add(egui::Label::new(format!("Data location: {}", self.display_data_path())).wrap());
        ui.add_space(22.0);
        self.size_cards(ui, true);
        ui.add_space(18.0);
        if !self.library.message.is_empty() {
            ui.label(&self.library.message);
            ui.strong(format!(
                "{} puzzles available offline",
                widgets::grouped_count(self.library.count)
            ));
        }
        ui.add_enabled_ui(!self.service.is_busy(), |ui| {
            let label = if self.library.selected == LibrarySize::Starter {
                "Use starter pack".into()
            } else {
                format!("Download {}", self.library.selected.label())
            };
            if widgets::primary(ui, &label).clicked() {
                if self.library.selected == LibrarySize::Starter {
                    self.skip_welcome();
                } else {
                    self.start_library(self.library.selected);
                }
            }
        });
        if ui.button("Start with the starter pack").clicked() {
            self.skip_welcome();
        }
        ui.small("Tab / arrows to choose · Enter to start · Esc to skip");
        ui.small("Time estimates assume 5–50 Mbit/s, plus local import time.");
    }
    pub(super) fn display_data_path(&self) -> String {
        if self.screenshot.is_some() {
            "$XDG_DATA_HOME/woodpeck/woodpeck.db".into()
        } else {
            self.data_path.display().to_string()
        }
    }
    fn skip_welcome(&mut self) {
        if self.finish_onboarding() {
            self.navigate(Screen::Builder);
        }
    }
    fn cancel_library(&self) {
        if let Some(job) = &self.library.job {
            job.cancel.cancel();
        }
    }
    fn welcome_keyboard(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx();
        if ctx.input(|i| i.modifiers.any()) {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            if self.library.step == WelcomeStep::Downloading {
                self.cancel_library();
            } else {
                self.skip_welcome();
            }
        }
        if self.library.step != WelcomeStep::Choose {
            return;
        }
        let next = ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)
        });
        let prev = ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)
        });
        if next || prev {
            let index = LibrarySize::ALL
                .iter()
                .position(|s| *s == self.library.selected)
                .unwrap_or(1);
            let index = (index + if next { 1 } else { 2 }) % 3;
            self.library.selected = LibrarySize::ALL[index];
            ctx.memory_mut(|m| {
                // The cards use a cyclic order; suppress egui's spatial arrow navigation.
                m.move_focus(egui::FocusDirection::None);
                m.request_focus(egui::Id::unique((
                    "library-card",
                    self.library.selected.key(),
                )));
            });
        }
        // Let focused buttons handle Enter themselves; an unfocused Enter starts the choice.
        if !ctx.egui_wants_keyboard_input()
            && !self.service.is_busy()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
        {
            if self.library.selected == LibrarySize::Starter {
                self.skip_welcome();
            } else {
                self.start_library(self.library.selected);
            }
        }
    }
    pub(super) fn size_cards(&mut self, ui: &mut Ui, starter: bool) {
        let sizes = if starter {
            &LibrarySize::ALL[..]
        } else {
            &LibrarySize::ALL[1..]
        };
        if ui.available_width() >= sizes.len() as f32 * 230.0 {
            ui.columns(sizes.len(), |columns| {
                for (index, size) in sizes.iter().enumerate() {
                    self.size_card(&mut columns[index], *size, index);
                }
            });
        } else {
            for (index, size) in sizes.iter().enumerate() {
                self.size_card(ui, *size, index);
                ui.add_space(6.0);
            }
        }
    }
    fn size_card(&mut self, ui: &mut Ui, size: LibrarySize, index: usize) {
        let accent = style::color(self.palette.semantic().accent);
        let selected = self.library.selected == size;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 224.0),
            egui::Sense::hover(),
        );
        let response = ui.interact(
            rect,
            egui::Id::unique(("library-card", size.key())),
            egui::Sense::click(),
        );
        if response.clicked() {
            response.request_focus();
        }
        if response.clicked() || response.has_focus() {
            self.library.selected = size;
        }
        if response.gained_focus() {
            response.scroll_to_me(Some(egui::Align::Center));
        }
        let settled = self.screenshot.is_some() || ui.style().animation_time == 0.0;
        let age = (self.library.entered.elapsed().as_secs_f32() - index as f32 * 0.07).max(0.0);
        let entry = if settled { 1.0 } else { (age / 0.4).min(1.0) };
        if entry < 1.0 {
            ui.ctx().request_repaint();
        }
        let lift = if settled {
            0.0
        } else {
            ui.ctx()
                .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.15)
                * 3.0
        };
        let rect = rect.translate(egui::vec2(0.0, (1.0 - entry).powi(3) * 10.0 - lift));
        let mut painter = ui.painter().clone();
        painter.multiply_opacity(entry);
        painter.rect(
            rect,
            14,
            if selected {
                accent.gamma_multiply(0.08)
            } else {
                ui.visuals().panel_fill
            },
            egui::Stroke::new(
                if selected || response.has_focus() {
                    2.0
                } else {
                    1.0
                },
                if selected || response.has_focus() {
                    accent
                } else {
                    ui.visuals().widgets.noninteractive.bg_stroke.color
                },
            ),
            egui::StrokeKind::Inside,
        );
        self.paint_size_details(ui, &painter, rect, size);
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::RadioButton, true, selected, size.label())
        });
        if response.has_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
            && !self.service.is_busy()
        {
            if size == LibrarySize::Starter {
                self.skip_welcome();
            } else {
                self.start_library(size);
            }
        }
    }
    fn paint_size_details(
        &self,
        ui: &Ui,
        painter: &egui::Painter,
        rect: egui::Rect,
        size: LibrarySize,
    ) {
        let accent = style::color(self.palette.semantic().accent);
        let x = rect.left() + 18.0;
        let text = ui.visuals().text_color();
        let weak = ui.visuals().weak_text_color();
        let line = |y, label: &str, font, color| {
            painter.text(
                egui::pos2(x, rect.top() + y),
                egui::Align2::LEFT_TOP,
                label,
                egui::FontId::proportional(font),
                color,
            );
        };
        line(16.0, size.label(), 22.0, text);
        if size == LibrarySize::Medium {
            line(45.0, "RECOMMENDED", 10.0, accent);
        } else {
            line(
                45.0,
                if size == LibrarySize::Starter {
                    "A GENTLE START"
                } else {
                    "THE COMPLETE COLLECTION"
                },
                10.0,
                weak,
            );
        }
        line(65.0, &format!("{} puzzles", size.puzzles()), 17.0, text);
        line(
            94.0,
            &format!(
                "{} download",
                if size == LibrarySize::Starter {
                    "0 MB".into()
                } else {
                    format!("≈{:.0} MB", size.download_bytes() as f64 / 1_000_000.0)
                }
            ),
            13.0,
            weak,
        );
        line(114.0, &format!("{} on disk", size.disk()), 13.0, weak);
        line(134.0, size.time(), 13.0, weak);
        let chart = egui::Rect::from_min_size(
            egui::pos2(x, rect.top() + 163.0),
            egui::vec2(rect.width() - 36.0, 33.0),
        );
        let bands = size.histogram().map(|count| count as f32);
        paint_histogram(painter, chart, &bands, accent, histogram_max(&bands));
        for (x, align, label) in [
            (rect.left() + 18.0, egui::Align2::LEFT_TOP, "400"),
            (rect.center().x, egui::Align2::CENTER_TOP, "RATING"),
            (rect.right() - 18.0, egui::Align2::RIGHT_TOP, "2800+"),
        ] {
            painter.text(
                egui::pos2(x, rect.top() + 202.0),
                align,
                label,
                egui::FontId::proportional(9.0),
                weak,
            );
        }
    }
    pub(super) fn library_progress(&mut self, ui: &mut Ui) {
        let progress = self.library.progress;
        let accent = style::color(self.palette.semantic().accent);
        let fraction = if self.library.selected == LibrarySize::Full {
            progress.bytes as f32 / self.library.selected.download_bytes() as f32
        } else {
            progress.accepted as f32 / self.library.selected.limit().unwrap_or(1) as f32
        }
        .clamp(0.0, 1.0);
        let animated = self.screenshot.is_none() && ui.style().animation_time > 0.0;
        let job = self.library.job.as_ref().map_or(0, |job| job.id);
        let accepted = animated_count(ui, ("accepted", job), progress.accepted, animated);
        let scanned = animated_count(ui, ("scanned", job), progress.scanned, animated);
        let bytes = animated_count(ui, ("bytes", job), progress.bytes, animated);
        let bands = std::array::from_fn(|index| {
            animated_count(ui, ("band", job, index), progress.bands[index], animated)
        });
        let expected = expected_histogram(self.library.selected);
        let fraction = if animated {
            ui.ctx().animate_value_with_time(
                egui::Id::unique(("library-fraction", job)),
                fraction,
                0.3,
            )
        } else {
            fraction
        };
        widgets::card(ui).show(ui, |ui| {
            ui.set_width((ui.available_width() - 1.0).max(0.0));
            ui.horizontal_wrapped(|ui| {
                widgets::chip(ui, self.library.selected.label(), accent);
                ui.label("Downloading & filtering for quality");
            });
            ui.add_space(10.0);
            ui.label(
                RichText::new(format!(
                    "{} puzzles accepted",
                    widgets::grouped_count(accepted)
                ))
                .size(26.0)
                .strong(),
            );
            ui.label(format!(
                "{:.1} MB {} · {} rows scanned",
                bytes as f64 / 1_000_000.0,
                if self.library.selected == LibrarySize::Starter {
                    "read"
                } else {
                    "downloaded"
                },
                widgets::grouped_count(scanned)
            ));
            ui.add(
                egui::ProgressBar::new(fraction)
                    .animate(animated && self.library.job.is_some())
                    .show_percentage(),
            );
            widgets::caption(ui, "PATTERNS AT EVERY LEVEL");
            histogram(ui, &bands, Some(&expected), accent, 100.0);
        });
    }
    fn celebrate(&self, ui: &mut Ui, eased: f32) {
        let accent = style::color(self.palette.semantic().accent);
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 110.0),
            egui::Sense::hover(),
        );
        let center = rect.center();
        let p = ui.painter();
        p.circle_filled(center, 34.0 * eased, accent.gamma_multiply(0.13));
        p.line_segment(
            [
                center + egui::vec2(-13.0, 0.0),
                center + egui::vec2(-3.0, 10.0),
            ],
            egui::Stroke::new(4.0, accent),
        );
        p.line_segment(
            [
                center + egui::vec2(-3.0, 10.0),
                center + egui::vec2(16.0, -12.0),
            ],
            egui::Stroke::new(4.0, accent),
        );
        for i in 0..16 {
            let angle = i as f32 * std::f32::consts::TAU / 16.0;
            let position = center + egui::vec2(angle.cos(), angle.sin()) * (42.0 + 14.0 * eased);
            p.rect_filled(
                egui::Rect::from_center_size(position, egui::vec2(4.0, 4.0)),
                1,
                if i % 2 == 0 {
                    accent
                } else {
                    style::color(self.palette.semantic().correct)
                },
            );
        }
    }
}
/// Restart each counter from its displayed value when another worker report arrives.
/// Cubic easing preserves continuity even when reports arrive faster than the transition.
fn animated_count(
    ui: &Ui,
    key: impl std::hash::Hash + std::fmt::Debug,
    target: u64,
    animated: bool,
) -> u64 {
    if !animated {
        return target;
    }
    let id = egui::Id::unique(("library-counter", key));
    let now = ui.input(|i| i.time);
    let (from, to, started) = ui
        .data(|data| data.get_temp::<(f64, u64, f64)>(id))
        .unwrap_or((0.0, target, now));
    let t = ((now - started) / 0.35).clamp(0.0, 1.0);
    let current = from + (to as f64 - from) * (1.0 - (1.0 - t).powi(3));
    let next = if to == target {
        (from, to, started)
    } else {
        (current, target, now)
    };
    ui.data_mut(|data| data.insert_temp(id, next));
    if t < 1.0 || to != target {
        ui.ctx().request_repaint();
    }
    // The eased count stays nonnegative and within the u64 endpoints; round for display.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let rounded = current.round() as u64;
    rounded
}
/// Scale the measured profile to the selected import's expected accepted total.
fn expected_histogram(size: LibrarySize) -> [f32; RATING_BANDS] {
    let profile = size.histogram();
    let total = profile.iter().sum::<u64>();
    let expected = size.limit().unwrap_or(total);
    profile.map(|count| count as f32 * expected as f32 / total.max(1) as f32)
}
fn histogram_max(bands: &[f32; RATING_BANDS]) -> f32 {
    bands.iter().copied().fold(1.0, f32::max)
}
pub(super) fn histogram(
    ui: &mut Ui,
    bands: &[u64; RATING_BANDS],
    expected: Option<&[f32; RATING_BANDS]>,
    accent: Color32,
    height: f32,
) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::hover(),
    );
    let rect = rect.shrink2(egui::vec2(0.0, 10.0));
    let bands = bands.map(|count| count as f32);
    let max = histogram_max(expected.unwrap_or(&bands));
    if let Some(expected) = expected {
        paint_histogram(
            ui.painter(),
            rect,
            expected,
            accent.gamma_multiply(0.18),
            max,
        );
    }
    paint_histogram(ui.painter(), rect, &bands, accent, max);
    ui.horizontal(|ui| {
        ui.small("400");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.small("2800+ · rating");
        });
    });
}
fn paint_histogram(
    p: &egui::Painter,
    rect: egui::Rect,
    bands: &[f32; RATING_BANDS],
    accent: Color32,
    max: f32,
) {
    let width = rect.width() / RATING_BANDS as f32;
    for (index, count) in bands.iter().enumerate() {
        if *count <= 0.0 {
            continue;
        }
        let height = (count / max).min(1.0) * rect.height();
        let bar = egui::Rect::from_min_max(
            egui::pos2(rect.left() + index as f32 * width, rect.bottom() - height),
            egui::pos2(
                rect.left() + (index + 1) as f32 * width - 3.0,
                rect.bottom(),
            ),
        );
        p.rect_filled(bar, 2, accent.gamma_multiply(0.35 + index as f32 / 24.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chart_bars(
        bands: &[u64; RATING_BANDS],
        expected: Option<&[f32; RATING_BANDS]>,
    ) -> Vec<egui::epaint::RectShape> {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            histogram(ui, bands, expected, Color32::WHITE, 100.0);
        });
        let bars = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(bar) => Some(bar.clone()),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        bars
    }

    #[test]
    fn live_chart_grows_toward_fixed_ghosts_while_committed_chart_fits_its_data() {
        for size in [LibrarySize::Medium, LibrarySize::Full] {
            let expected = expected_histogram(size);
            let total = size.histogram().iter().sum::<u64>();
            let expected_total = size.limit().unwrap_or(total);
            assert!((expected.iter().sum::<f32>() - expected_total as f32).abs() < 1.0);
            let early = size
                .histogram()
                .map(|count| count * (expected_total * 7 / 100) / total);
            let late = size
                .histogram()
                .map(|count| count * (expected_total * 98 / 100) / total);
            let early_bars = chart_bars(&early, Some(&expected));
            let late_bars = chart_bars(&late, Some(&expected));
            assert_eq!(early_bars.len(), 2 * RATING_BANDS);
            assert_eq!(late_bars.len(), 2 * RATING_BANDS);
            for index in 0..RATING_BANDS {
                let ghost = &early_bars[index];
                let early = &early_bars[index + RATING_BANDS];
                let late = &late_bars[index + RATING_BANDS];
                assert_eq!(ghost.rect, late_bars[index].rect);
                assert!(ghost.fill.a() < early.fill.a());
                assert!(early.rect.height() < late.rect.height());
                assert!(late.rect.height() < ghost.rect.height());
                assert!((early.rect.bottom() - ghost.rect.bottom()).abs() < 0.001);
            }
            for bands in [early, late] {
                let committed = chart_bars(&bands, None);
                assert_eq!(committed.len(), RATING_BANDS);
                let tallest = committed
                    .iter()
                    .map(|bar| bar.rect.height())
                    .fold(0.0, f32::max);
                assert!((tallest - 80.0).abs() < 0.001);
            }
            assert_eq!(
                chart_bars(&[0; RATING_BANDS], Some(&expected)).len(),
                RATING_BANDS
            );
        }
    }
}
