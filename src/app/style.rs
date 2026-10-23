//! Maps desktop palettes and fontconfig onto egui.
use crate::theme::{Mode, Palette, Rgb};
use eframe::egui::{self, Color32, FontFamily, TextStyle};
pub fn color(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.0, rgb.1, rgb.2)
}
pub fn apply(ctx: &egui::Context, palette: &Palette) {
    let c = palette.semantic();
    let mut visuals = if palette.mode == Mode::Light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    let panel = if c.panel == c.background {
        if palette.mode == Mode::Light {
            Rgb::WHITE
        } else {
            c.background.mix(c.text, 0.035)
        }
    } else {
        c.panel
    };
    visuals.panel_fill = color(panel);
    visuals.window_fill = color(panel);
    visuals.slider_trailing_fill = true;
    visuals.extreme_bg_color = color(c.background);
    visuals.faint_bg_color = color(c.background);
    visuals.override_text_color = Some(color(c.text));
    visuals.selection.bg_fill = color(c.accent);
    visuals.selection.stroke = egui::Stroke::new(1.0, color(c.text));
    visuals.hyperlink_color = color(c.accent);
    visuals.warn_fg_color = color(c.warning);
    visuals.error_fg_color = color(c.incorrect);
    let border = color(c.text.mix(c.background, 0.87));
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, border);
    visuals.widgets.inactive.weak_bg_fill = color(c.background);
    visuals.widgets.inactive.bg_fill = color(c.background.mix(c.text, 0.16));
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, border);
    visuals.widgets.hovered.weak_bg_fill = color(c.selection);
    visuals.widgets.hovered.bg_fill = color(c.selection);
    visuals.widgets.active.bg_fill = color(c.accent);
    ctx.set_visuals(visuals);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(10.0, 10.0);
        s.spacing.button_padding = egui::vec2(12.0, 9.0);
        s.spacing.interact_size = egui::vec2(34.0, 34.0);
        s.spacing.slider_width = 240.0;
        s.text_styles.insert(
            TextStyle::Body,
            egui::FontId::new(14.0, FontFamily::Proportional),
        );
        s.text_styles.insert(
            TextStyle::Button,
            egui::FontId::new(14.0, FontFamily::Proportional),
        );
        s.text_styles.insert(
            TextStyle::Heading,
            egui::FontId::new(22.0, FontFamily::Proportional),
        );
        for w in [
            &mut s.visuals.widgets.inactive,
            &mut s.visuals.widgets.hovered,
            &mut s.visuals.widgets.active,
        ] {
            w.corner_radius = egui::CornerRadius::same(8);
        }
    });
}
pub fn fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.families.insert(
        FontFamily::Name("coordinates".into()),
        fonts
            .families
            .get(&FontFamily::Proportional)
            .cloned()
            .unwrap_or_default(),
    );
    for (name, family) in [
        ("sans-serif", FontFamily::Proportional),
        ("monospace", FontFamily::Monospace),
        (
            "sans-serif:style=Bold",
            FontFamily::Name("coordinates".into()),
        ),
    ] {
        if let Some(bytes) = crate::theme::font_file(name).and_then(|p| std::fs::read(p).ok()) {
            fonts
                .font_data
                .insert(name.into(), egui::FontData::from_owned(bytes).into());
            fonts
                .families
                .entry(family)
                .or_default()
                .insert(0, name.into());
        }
    }
    ctx.set_fonts(fonts);
}

/// Filesystem polling stays off the frame thread, including symlink swaps.
pub struct ThemeWatcher {
    receiver: std::sync::mpsc::Receiver<Palette>,
    last: Option<Palette>,
}
impl ThemeWatcher {
    pub fn new(choice: crate::settings::ColorTheme) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("woodpeck-theme".into())
            .spawn(move || {
                let home = std::env::var_os("HOME")
                    .map_or_else(std::path::PathBuf::new, std::path::PathBuf::from);
                let mut poller = crate::theme::ThemePoller::new(home, choice);
                let origin = std::time::Instant::now();
                let mut palette = crate::theme::select(choice, None);
                loop {
                    if let Some(next) = poller.poll(origin.elapsed()) {
                        palette = next;
                    }
                    if sender.send(palette.clone()).is_err() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
            });
        Self {
            receiver,
            last: None,
        }
    }
    pub fn poll(&mut self) -> Option<Palette> {
        let palette = self.receiver.try_iter().last()?;
        if self.last.as_ref() == Some(&palette) {
            return None;
        }
        self.last = Some(palette.clone());
        Some(palette)
    }
}
