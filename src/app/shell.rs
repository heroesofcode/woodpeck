//! Responsive native shell, navigation, dialogs, and screenshot delivery.
use super::{Screen, Woodpeck, style, widgets};
use crate::layout::{self, Navigation};
use eframe::egui::{self, RichText, Ui};
use std::time::Duration;
impl Woodpeck {
    pub(super) fn navigation(&mut self, ui: &mut Ui, mode: Navigation) {
        let accent = style::color(self.palette.semantic().accent);
        if mode == Navigation::TopBar {
            self.top_navigation(ui);
            return;
        }
        let expanded = mode == Navigation::Sidebar && !self.collapsed;
        egui::Panel::left("navigation")
            .exact_size(if expanded { 216.0 } else { 56.0 })
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(if expanded { 16 } else { 7 }),
            )
            .show(ui, |ui| {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    widgets::logo(ui, accent);
                    if expanded {
                        ui.label(RichText::new("woodpeck").size(22.0).strong());
                    }
                });
                ui.add_space(34.0);
                self.side_navigation_entries(ui, expanded);
                self.sync_indicator(ui);
                ui.add_space(
                    (ui.available_height() - if expanded { 218.0 } else { 50.0 }).max(10.0),
                );
                if expanded {
                    self.streak(ui);
                }
                if ui
                    .button(if expanded { "<  Collapse sidebar" } else { ">" })
                    .clicked()
                {
                    self.collapsed = !self.collapsed;
                }
            });
    }
    pub(super) fn draw(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.keyboard(&ctx);
        let window = ui.max_rect().size();
        let mut layout = layout::calculate(window.x, window.y);
        if self.collapsed && layout.navigation == Navigation::Sidebar {
            layout = layout::calculate(window.x + 160.0, window.y);
            layout.navigation_width = 56.0;
        }
        self.navigation(ui, layout.navigation);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(style::color(self.palette.semantic().background))
                    .inner_margin(if window.x < 640.0 { 14 } else { 28 }),
            )
            .show(ui, |ui| {
                let width = ui.available_width().min(1680.0);
                let margin = ((ui.available_width() - width) / 2.0).max(0.0);
                let content = egui::Rect::from_min_size(
                    ui.cursor().min + egui::vec2(margin, 0.0),
                    egui::vec2(width, ui.available_height()),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(content), |ui| {
                    if matches!(self.screen, Screen::Training | Screen::Review) {
                        self.solving(ui, layout.mode, layout.panel_width);
                    } else {
                        egui::ScrollArea::vertical()
                            .id_salt("screen-scroll")
                            .show(ui, |ui| match self.screen {
                                Screen::Dashboard => self.dashboard(ui),
                                Screen::Builder => self.builder_screen(ui),
                                Screen::Progress => self.progress_screen(ui),
                                Screen::Settings => self.settings_screen(ui),
                                _ => {}
                            });
                    }
                });
            });
        self.toast(ui, window);
        self.delete_dialog(ui, window);
        self.capture(ui);
    }
    pub(super) fn sync_indicator(&self, ui: &mut Ui) {
        use crate::sync::Status;
        let config = self.service.sync_status();
        let colors = self.palette.semantic();
        let color = match config.status {
            Status::Disabled => colors.muted,
            Status::Synced => colors.correct,
            Status::Pending | Status::Offline => colors.warning,
            Status::Syncing | Status::Merged => colors.accent,
            Status::Error => colors.incorrect,
        };
        let label = if self.collapsed || ui.available_width() < 110.0 {
            "•".to_owned()
        } else {
            format!("Sync · {}", config.status.label())
        };
        ui.colored_label(style::color(color), label).on_hover_text(
            config
                .last_error
                .as_deref()
                .unwrap_or(config.status.label()),
        );
    }
    fn top_navigation(&mut self, ui: &mut Ui) {
        let accent = style::color(self.palette.semantic().accent);
        egui::Panel::top("navigation")
            .frame(
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(10),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    widgets::logo(ui, accent);
                    ui.label(RichText::new("woodpeck").strong().size(18.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        widgets::chip(ui, format!("{} Review", self.pending), accent);
                        self.sync_indicator(ui);
                    });
                });
                ui.horizontal(|ui| {
                    for (index, (screen, label)) in Screen::ALL.into_iter().enumerate() {
                        let selected = self.screen == screen;
                        let color = if selected || screen == Screen::Review {
                            accent
                        } else {
                            ui.visuals().weak_text_color()
                        };
                        let response = ui.add_sized(
                            [36.0, 30.0],
                            egui::Button::new("")
                                .fill(if selected {
                                    accent.gamma_multiply(0.15)
                                } else {
                                    egui::Color32::TRANSPARENT
                                })
                                .stroke(egui::Stroke::NONE),
                        );
                        widgets::nav_icon(ui.painter(), response.rect, index, color);
                        if response.on_hover_text(label).clicked() {
                            self.navigate(screen);
                        }
                    }
                });
            });
    }
    fn side_navigation_entries(&mut self, ui: &mut Ui, expanded: bool) {
        let accent = style::color(self.palette.semantic().accent);
        for (index, (screen, label)) in Screen::ALL.into_iter().enumerate() {
            let selected = self.screen == screen;
            let color = if selected || screen == Screen::Review {
                accent
            } else {
                ui.visuals().text_color()
            };
            let response = ui.add_sized(
                [ui.available_width(), 42.0],
                egui::Button::new("")
                    .fill(if selected {
                        accent.gamma_multiply(0.12)
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .stroke(egui::Stroke::NONE)
                    .corner_radius(9),
            );
            let icon = egui::Rect::from_center_size(
                egui::pos2(response.rect.left() + 20.0, response.rect.center().y),
                egui::vec2(20.0, 20.0),
            );
            widgets::nav_icon(ui.painter(), icon, index, color);
            if expanded {
                ui.painter().text(
                    egui::pos2(response.rect.left() + 40.0, response.rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    label,
                    egui::FontId::proportional(14.0),
                    color,
                );
            }
            if screen == Screen::Review && self.pending > 0 {
                let center = if expanded {
                    egui::pos2(response.rect.right() - 18.0, response.rect.center().y)
                } else {
                    response.rect.right_top() + egui::vec2(-5.0, 6.0)
                };
                ui.painter()
                    .circle_filled(center, if expanded { 11.0 } else { 8.0 }, accent);
                ui.painter().text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    self.pending.to_string(),
                    egui::FontId::proportional(10.0),
                    style::color(self.palette.semantic().background),
                );
            }
            if response.on_hover_text(label).clicked() {
                self.navigate(screen);
            }
        }
    }
    fn streak(&mut self, ui: &mut Ui) {
        let accent = style::color(self.palette.semantic().accent);
        let m = self.service.motivation().unwrap_or_default();
        widgets::caption(ui, "KEEP YOUR RHYTHM");
        let today = i64::from(chrono::Datelike::num_days_from_ce(
            &chrono::Local::now().date_naive(),
        ));
        ui.label(RichText::new(format!("{} day streak", m.streak_on(today))).strong());
        let weekday =
            chrono::Datelike::weekday(&chrono::Local::now().date_naive()).num_days_from_monday();
        let activity = widgets::week_activity(today, weekday, &m.recent_days);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 7.0;
            for (index, day) in ["M", "T", "W", "T", "F", "S", "S"].into_iter().enumerate() {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(19.0, 24.0), egui::Sense::hover());
                let center = rect.center();
                ui.painter().circle_filled(
                    center,
                    8.0,
                    if activity[index] {
                        accent.gamma_multiply(0.2)
                    } else {
                        ui.visuals().extreme_bg_color
                    },
                );
                ui.painter().text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    day,
                    egui::FontId::proportional(9.0),
                    if activity[index] {
                        accent
                    } else {
                        ui.visuals().weak_text_color()
                    },
                );
            }
        });
        ui.separator();
        widgets::caption(ui, format!("LEVEL {} · {}", m.level(), m.rank()));
        ui.add(egui::ProgressBar::new(m.level_progress()).desired_height(4.0));
        ui.small(format!("{} XP · keep showing up", m.xp));
    }
    fn toast(&mut self, ui: &mut Ui, window: egui::Vec2) {
        let ctx = ui.ctx().clone();
        if let Some((message, then)) = &self.toast
            && then.elapsed() < Duration::from_secs(5)
        {
            egui::Area::new(egui::Id::unique("toast"))
                .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -18.0])
                .order(egui::Order::Foreground)
                .show(&ctx, |ui| {
                    widgets::card(ui)
                        .stroke(egui::Stroke::new(
                            1.0,
                            style::color(self.palette.semantic().accent),
                        ))
                        .show(ui, |ui| {
                            ui.set_max_width((window.x - 56.0).max(100.0));
                            ui.label(RichText::new(message).strong());
                        });
                });
        }
    }
    fn delete_dialog(&mut self, ui: &mut Ui, window: egui::Vec2) {
        let ctx = ui.ctx().clone();
        if let Some(id) = self.delete {
            egui::Modal::new(egui::Id::unique("delete-set")).show(&ctx, |ui| {
                ui.set_max_width((window.x - 64.0).min(340.0));
                ui.heading("Delete this set?");
                ui.label(
                    "The set, its attempts, and its Review entries will be permanently removed.",
                );
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.delete = None;
                    }
                    if ui
                        .button(
                            RichText::new("Delete set")
                                .color(style::color(self.palette.semantic().incorrect)),
                        )
                        .clicked()
                    {
                        match self.service.delete_set(id) {
                            Ok(()) => {
                                self.delete = None;
                                self.session = None;
                                self.refresh();
                            }
                            Err(e) => self.error(e),
                        }
                    }
                });
            });
        }
    }
    fn capture(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.frames += 1;
        if self.frames > 12
            && !self.capture_requested
            && let Some(screenshot) = &self.screenshot
        {
            self.capture_requested = true;
            let path = screenshot.path.clone();
            let ctx = ctx.clone();
            let result_sender = self.capture_result.take();
            ui.ctx().request_screenshot(move |image| {
                let pixels: Vec<u8> = image
                    .pixels
                    .iter()
                    .flat_map(egui::Color32::to_array)
                    .collect();
                let result = image::save_buffer_with_format(
                    &path,
                    &pixels,
                    image.width() as u32,
                    image.height() as u32,
                    image::ColorType::Rgba8,
                    image::ImageFormat::Png,
                )
                .map_err(|e| e.to_string());
                if let Some(sender) = result_sender {
                    let _ = sender.send(result);
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            });
        }
        if self.screenshot.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
}
