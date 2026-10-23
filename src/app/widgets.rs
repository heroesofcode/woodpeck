//! Small visual building blocks and presentation calculations.
use eframe::egui::{self, Color32, RichText, Ui};
pub fn duration(ms: u64) -> String {
    let seconds = ms / 1000;
    if seconds >= 60 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}
pub fn relative_time(now: i64, then: i64) -> String {
    let days = now.saturating_sub(then).max(0) / 86400;
    if days == 0 {
        "Today".into()
    } else {
        format!("{days}d ago")
    }
}
pub fn delta_time(percent: f64) -> String {
    format!(
        "{:.0}% {}",
        percent.abs(),
        if percent >= 0.0 { "less" } else { "more" }
    )
}
pub fn delta_points(points: f64) -> String {
    format!("{points:+.0} points")
}
pub fn chip_overflow<T>(items: &[T], limit: usize) -> (&[T], usize) {
    let count = items.len().min(limit);
    (&items[..count], items.len() - count)
}
pub fn caption(ui: &mut Ui, text: impl Into<String>) {
    ui.label(
        RichText::new(text)
            .size(11.0)
            .strong()
            .color(ui.visuals().weak_text_color()),
    );
}
pub fn chip(ui: &mut Ui, text: impl Into<String>, color: Color32) {
    let padding = ui.spacing().button_padding;
    let height = ui.spacing().interact_size.y;
    ui.spacing_mut().button_padding = egui::vec2(8.0, 4.0);
    ui.spacing_mut().interact_size.y = 23.0;
    ui.add(
        egui::Button::new(RichText::new(text).size(11.0).strong().color(color))
            .fill(color.gamma_multiply(0.13))
            .stroke(egui::Stroke::NONE)
            .corner_radius(6)
            .wrap_mode(egui::TextWrapMode::Extend)
            .sense(egui::Sense::hover()),
    );
    ui.spacing_mut().button_padding = padding;
    ui.spacing_mut().interact_size.y = height;
}
pub fn card(ui: &Ui) -> egui::Frame {
    egui::Frame::new()
        .fill(ui.visuals().panel_fill)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(14)
        .inner_margin(20)
}
pub fn primary(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add_sized(
        [ui.available_width().min(260.0), 40.0],
        egui::Button::new(
            RichText::new(text)
                .strong()
                .color(Color32::from_rgb(20, 28, 19)),
        )
        .fill(ui.visuals().selection.bg_fill)
        .corner_radius(9),
    )
}
pub fn title(ui: &mut Ui, eyebrow: &str, title: &str, subtitle: &str) {
    caption(ui, eyebrow);
    ui.add_space(6.0);
    ui.label(RichText::new(title).size(28.0).strong());
    ui.label(RichText::new(subtitle).color(ui.visuals().weak_text_color()));
    ui.add_space(22.0);
}
pub fn stat(ui: &mut Ui, label: &str, value: impl Into<String>, detail: &str) {
    card(ui).show(ui, |ui| {
        ui.set_min_width((ui.available_width() - 40.0).max(0.0));
        caption(ui, label);
        ui.add_space(10.0);
        ui.label(RichText::new(value).size(27.0).strong());
        ui.small(detail);
    });
}
pub fn nav_icon(painter: &egui::Painter, rect: egui::Rect, index: usize, color: Color32) {
    let center = rect.center();
    let stroke = egui::Stroke::new(1.7, color);
    let point = |x, y| center + egui::vec2(x, y);
    match index {
        0 => {
            for x in [-6.0, 3.0] {
                for y in [-6.0, 3.0] {
                    painter.rect_stroke(
                        egui::Rect::from_min_size(point(x, y), egui::vec2(5.0, 5.0)),
                        1,
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }
        1 => {
            painter.add(egui::Shape::closed_line(
                vec![point(-5.0, -7.0), point(7.0, 0.0), point(-5.0, 7.0)],
                stroke,
            ));
        }
        2 => {
            painter.circle_stroke(center, 8.0, stroke);
            painter.circle_stroke(center, 3.0, stroke);
        }
        3 => {
            for (x, h) in [(-7.0, 5.0), (-1.0, 10.0), (5.0, 15.0)] {
                painter.rect_filled(
                    egui::Rect::from_min_size(point(x, 7.0 - h), egui::vec2(4.0, h)),
                    1,
                    color,
                );
            }
        }
        4 => {
            painter.line_segment([point(-7.0, 0.0), point(7.0, 0.0)], stroke);
            painter.line_segment([point(0.0, -7.0), point(0.0, 7.0)], stroke);
        }
        _ => {
            painter.circle_stroke(center, 6.0, stroke);
            painter.circle_stroke(center, 2.0, stroke);
            for angle in 0..8 {
                let radians = angle as f32 * std::f32::consts::TAU / 8.0;
                let direction = egui::vec2(radians.cos(), radians.sin());
                painter.line_segment([center + direction * 6.0, center + direction * 9.0], stroke);
            }
        }
    }
}
pub fn logo(ui: &mut Ui, accent: Color32) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(25.0, 28.0), egui::Sense::hover());
    let p = ui.painter();
    let c = r.center();
    p.circle_filled(c + egui::vec2(-2.0, 4.0), 8.0, accent);
    p.circle_filled(c + egui::vec2(1.0, -5.0), 6.0, accent);
    p.add(egui::Shape::convex_polygon(
        vec![
            c + egui::vec2(6.0, -7.0),
            c + egui::vec2(13.0, -4.0),
            c + egui::vec2(6.0, -2.0),
        ],
        accent,
        egui::Stroke::NONE,
    ));
    p.circle_filled(c + egui::vec2(2.0, -6.0), 1.5, ui.visuals().panel_fill);
}

pub fn week_activity(today: i64, monday_index: u32, days: &[i64]) -> [bool; 7] {
    let start = today - i64::from(monday_index.min(6));
    std::array::from_fn(|index| days.contains(&(start + i64::try_from(index).unwrap_or(0))))
}

pub fn comparison(
    ui: &mut Ui,
    label: &str,
    first: &str,
    latest: &str,
    delta: Option<&str>,
    accent: Color32,
) {
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .corner_radius(10)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            caption(ui, label);
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(first)
                        .size(22.0)
                        .color(ui.visuals().weak_text_color()),
                );
                ui.label("to");
                ui.label(RichText::new(latest).size(28.0).strong());
            });
            if let Some(delta) = delta {
                chip(ui, delta, accent);
            } else {
                ui.small("Your first-cycle baseline");
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presentation() {
        assert_eq!(
            week_activity(100, 4, &[96, 98, 100]),
            [true, false, true, false, true, false, false]
        );
        assert_eq!(duration(171_000), "2m 51s");
        assert_eq!(relative_time(259_200, 0), "3d ago");
        assert_eq!(relative_time(0, 10), "Today");
        assert_eq!(delta_time(33.3), "33% less");
        assert_eq!(delta_time(-20.0), "20% more");
        assert_eq!(delta_points(20.0), "+20 points");
        assert_eq!(chip_overflow(&[1, 2, 3, 4], 2), (&[1, 2][..], 2));
    }
}
