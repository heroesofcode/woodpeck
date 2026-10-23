use super::super::{Woodpeck, style, widgets};
use crate::review::ReviewSort;
use eframe::egui::{self, RichText, Ui};
impl Woodpeck {
    pub(in crate::app) fn review_empty(&mut self, ui: &mut Ui) {
        widgets::title(
            ui,
            "ALL CLEAR",
            "Nothing to review — nice work",
            "Every pattern you revisit is one more pattern that sticks.",
        );
        self.review_filters(ui);
        if !self.review_theme.is_empty() {
            ui.label("No pending puzzles match this theme.");
            if ui.button("Show all themes").clicked() {
                self.review_theme.clear();
                self.refresh();
                self.select_first_review();
            }
        }
        if widgets::primary(ui, "Back to training").clicked() {
            self.navigate(super::super::Screen::Training);
        }
    }
    fn review_filters(&mut self, ui: &mut Ui) {
        let old = (self.review_theme.clone(), self.review_sort);
        widgets::caption(ui, "FILTER QUEUE");
        egui::ComboBox::from_id_salt("review-theme")
            .width((ui.available_width() - 12.0).max(80.0))
            .selected_text(if self.review_theme.is_empty() {
                "All themes".into()
            } else {
                crate::sets::theme_label(&self.review_theme)
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.review_theme, String::new(), "All themes");
                let all = self
                    .service
                    .review_queue(None, ReviewSort::MostRecent)
                    .unwrap_or_default();
                let themes: std::collections::BTreeSet<_> =
                    all.iter().flat_map(|c| c.puzzle.themes.iter()).collect();
                for theme in themes {
                    ui.selectable_value(
                        &mut self.review_theme,
                        theme.clone(),
                        crate::sets::theme_label(theme),
                    );
                }
            });
        egui::ComboBox::from_id_salt("review-sort")
            .width((ui.available_width() - 12.0).max(80.0))
            .selected_text(if self.review_sort == ReviewSort::MostRecent {
                "Most recent miss"
            } else {
                "Lowest accuracy"
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.review_sort,
                    ReviewSort::MostRecent,
                    "Most recent miss",
                );
                ui.selectable_value(
                    &mut self.review_sort,
                    ReviewSort::LowestAccuracy,
                    "Lowest accuracy",
                );
            });
        if old != (self.review_theme.clone(), self.review_sort) {
            self.refresh();
            self.select_first_review();
        }
    }
    pub(in crate::app) fn review_panel(&mut self, ui: &mut Ui) {
        widgets::card(ui).show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 7.0);
            ui.set_min_width((ui.available_width() - 1.0).max(0.0));
            ui.label(RichText::new("Review queue").size(22.0).strong());
            widgets::caption(ui, format!("— {} PENDING", self.pending));
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(self.reviews.len().to_string())
                        .size(38.0)
                        .strong()
                        .color(style::color(self.palette.semantic().accent)),
                );
                widgets::caption(ui, "READY");
            });
            self.timer_row(
                ui,
                self.session.as_ref().and_then(|s| s.engine.orientation()),
                true,
            );
            self.solve_controls(ui);
            ui.separator();
            self.review_filters(ui);
            ui.add_space(4.0);
            self.clearing_review(ui);
            let mut chosen = None;
            for card in self.reviews.clone() {
                let response = self.review_card(ui, &card);
                if ui
                    .interact(
                        response.rect,
                        response.id.with("select"),
                        egui::Sense::click(),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    chosen = Some(card);
                }
            }
            if let Some(card) = chosen {
                self.load(
                    card.puzzle,
                    card.item.set_id,
                    None,
                    card.position as usize - 1,
                );
            }
        });
    }
    fn clearing_review(&self, ui: &mut Ui) {
        if let Some((card, then)) = &self.clearing_card {
            let fraction = (self.now().saturating_sub(*then).as_secs_f32() / 0.45).clamp(0.0, 1.0);
            if fraction < 1.0 {
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 80.0 * (1.0 - fraction)),
                    egui::Sense::hover(),
                );
                let painter = ui.painter().with_clip_rect(rect);
                let color =
                    style::color(self.palette.semantic().correct).gamma_multiply(1.0 - fraction);
                painter.rect_stroke(
                    rect,
                    10,
                    egui::Stroke::new(1.0, color),
                    egui::StrokeKind::Inside,
                );
                painter.text(
                    rect.min + egui::vec2(12.0, 16.0),
                    egui::Align2::LEFT_TOP,
                    &card.set_name,
                    egui::FontId::proportional(14.0),
                    color,
                );
                painter.text(
                    rect.min + egui::vec2(12.0, 42.0),
                    egui::Align2::LEFT_TOP,
                    "Cleared from Review",
                    egui::FontId::proportional(12.0),
                    color,
                );
                ui.ctx().request_repaint();
            }
        }
    }
    fn review_card(&self, ui: &mut Ui, card: &crate::review::ReviewCard) -> egui::Response {
        let selected = self
            .session
            .as_ref()
            .is_some_and(|s| s.set_id == card.item.set_id && s.puzzle.id == card.puzzle.id);
        let accent = style::color(self.palette.semantic().accent);
        widgets::card(ui)
            .inner_margin(14)
            .stroke(egui::Stroke::new(
                if selected { 1.5 } else { 1.0 },
                if selected {
                    accent
                } else {
                    ui.visuals().widgets.noninteractive.bg_stroke.color
                },
            ))
            .show(ui, |ui| {
                ui.set_width((ui.available_width() - 1.0).max(0.0));
                ui.label(RichText::new(&card.set_name).strong().size(15.0));
                ui.small(format!(
                    "Position {} · {}",
                    card.position,
                    widgets::relative_time(
                        chrono::Local::now().timestamp(),
                        card.item.last_missed_at
                    )
                ));
                ui.horizontal_wrapped(|ui| {
                    widgets::chip(
                        ui,
                        card.puzzle.rating.to_string(),
                        style::color(self.palette.semantic().warning),
                    );
                    let (chips, overflow) = widgets::chip_overflow(&card.puzzle.themes, 2);
                    for t in chips {
                        widgets::chip(ui, crate::sets::theme_label(t), accent);
                    }
                    if overflow > 0 {
                        widgets::chip(ui, format!("+{overflow}"), ui.visuals().weak_text_color());
                    }
                });
                widgets::caption(
                    ui,
                    format!(
                        "ACCURACY   {:.0}% · {}/{}",
                        card.accuracy.percent(),
                        card.accuracy.correct,
                        card.accuracy.attempts
                    ),
                );
                ui.add(
                    egui::ProgressBar::new(card.accuracy.percent() as f32 / 100.0)
                        .desired_height(4.0),
                );
            })
            .response
    }
}
