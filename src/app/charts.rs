//! Painter charts with finite, independently tested scale calculation.
use super::widgets;
use crate::stats::CycleSummary;
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Stroke, StrokeKind, Ui};
pub fn normalized(value: f64, maximum: f64) -> f32 {
    if !value.is_finite() || !maximum.is_finite() || maximum <= 0.0 {
        0.0
    } else {
        (value / maximum).clamp(0.0, 1.0) as f32
    }
}
pub fn maximum(values: impl IntoIterator<Item = f64>) -> f64 {
    values
        .into_iter()
        .filter(|v| v.is_finite())
        .fold(1.0, f64::max)
}
pub fn chart(ui: &mut Ui, cycles: &[CycleSummary], set_size: u32, accuracy: bool, color: Color32) {
    let width = ui.available_width();
    let (area, _) = ui.allocate_exact_size(egui::vec2(width, 230.0), egui::Sense::hover());
    let rect = Rect::from_min_max(
        area.min + egui::vec2(40.0, 14.0),
        area.max - egui::vec2(12.0, 30.0),
    );
    let max = if accuracy {
        100.0
    } else {
        maximum(cycles.iter().map(|c| c.stats.time_ms as f64)) * 1.15
    };
    let p = ui.painter();
    let text = ui.visuals().weak_text_color();
    for i in 0..=4 {
        let y = rect.bottom() - rect.height() * i as f32 / 4.0;
        p.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            ui.visuals().widgets.noninteractive.bg_stroke,
        );
        p.text(
            Pos2::new(rect.left() - 8.0, y),
            egui::Align2::RIGHT_CENTER,
            if accuracy {
                format!("{}%", i * 25)
            } else {
                widgets::duration((max * f64::from(i) / 4.0) as u64)
            },
            FontId::proportional(10.0),
            text,
        );
    }
    if cycles.is_empty() {
        p.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Your first cycle starts the story",
            FontId::proportional(12.0),
            text,
        );
        return;
    }
    let spacing = rect.width() / cycles.len().max(1) as f32;
    let mut previous = None;
    for (index, c) in cycles.iter().enumerate() {
        let x = rect.left() + spacing * (index as f32 + 0.5);
        let value = if accuracy {
            c.stats.accuracy(set_size)
        } else {
            c.stats.time_ms as f64
        };
        let y = rect.bottom() - normalized(value, max) * rect.height();
        if accuracy {
            let r = Rect::from_min_max(
                Pos2::new(x - spacing.min(64.0) * 0.28, y),
                Pos2::new(x + spacing.min(64.0) * 0.28, rect.bottom()),
            );
            p.rect_filled(
                r,
                4,
                if c.complete {
                    color
                } else {
                    color.gamma_multiply(0.06)
                },
            );
            p.rect_stroke(r, 4, Stroke::new(2.0, color), StrokeKind::Inside);
        } else {
            let point = Pos2::new(x, y);
            if let Some(prev) = previous {
                p.line_segment([prev, point], Stroke::new(2.5, color));
            }
            p.circle_filled(
                point,
                4.5,
                if c.complete {
                    color
                } else {
                    ui.visuals().panel_fill
                },
            );
            p.circle_stroke(point, 4.5, Stroke::new(2.0, color));
            previous = Some(point);
        }
        p.text(
            Pos2::new(x, rect.bottom() + 14.0),
            egui::Align2::CENTER_CENTER,
            format!("Cycle {}", c.number),
            FontId::proportional(11.0),
            text,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_clamped_chart_scales() {
        assert_eq!(normalized(50.0, 100.0), 0.5);
        for (v, m) in [(1.0, 0.0), (f64::NAN, 1.0), (2.0, f64::INFINITY)] {
            assert_eq!(normalized(v, m), 0.0);
        }
        assert_eq!(normalized(200.0, 100.0), 1.0);
        assert_eq!(maximum([0.0, f64::NAN, 250.0]), 250.0);
        assert_eq!(maximum([]), 1.0);
    }
}
