//! One typeset paragraph, including move hit regions, cached until its appearance changes.
use super::reader_flow::PageRuns;
use eframe::egui::{self, Color32, FontId, Ui};
use std::{ops::Range, sync::Arc};

pub(super) fn serif(size: f32, strong: bool) -> FontId {
    FontId::new(
        size,
        egui::FontFamily::Name(if strong { "book-semibold" } else { "book" }.into()),
    )
}
#[derive(PartialEq)]
struct Key {
    width: f32,
    size: f32,
    pixels: f32,
    color: Color32,
    accent: Color32,
    selection: Color32,
}
pub(super) struct Paragraph {
    key: Key,
    galley: Arc<egui::Galley>,
    links: Vec<(egui::Rect, usize)>,
    unresolved: Vec<String>,
}
impl Paragraph {
    pub fn show(
        cache: &mut Option<Self>,
        ui: &mut Ui,
        page: &PageRuns,
        span: Range<usize>,
        size: f32,
        selected: Option<usize>,
        follow: bool,
    ) -> Option<usize> {
        let key = Key {
            width: ui.available_width(),
            size,
            pixels: ui.ctx().pixels_per_point(),
            color: ui.visuals().text_color(),
            accent: ui.visuals().hyperlink_color,
            selection: ui.visuals().selection.bg_fill,
        };
        if cache.as_ref().is_none_or(|p| p.key != key) {
            *cache = Some(Self::new(ui, page, span, key));
        }
        let paragraph = cache.as_ref().unwrap();
        let (rect, response) =
            ui.allocate_exact_size(paragraph.galley.size(), egui::Sense::hover());
        if ui.is_rect_visible(rect) {
            for (hit, node) in &paragraph.links {
                if Some(*node) == selected {
                    ui.painter().rect_filled(
                        hit.translate(rect.min.to_vec2()),
                        2,
                        paragraph.key.selection,
                    );
                }
            }
            ui.painter()
                .galley(rect.min, paragraph.galley.clone(), paragraph.key.color);
        }
        if !paragraph.unresolved.is_empty() {
            response.on_hover_text(format!(
                "Couldn't follow these moves: {}",
                paragraph.unresolved.join("; ")
            ));
        }
        let mut clicked = None;
        for (index, (hit, node)) in paragraph.links.iter().enumerate() {
            let hit = hit.translate(rect.min.to_vec2());
            let response = ui
                .interact(
                    hit,
                    ui.scope_id().with(("book-move", index, node)),
                    egui::Sense::click(),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.clicked() {
                clicked = Some(*node);
            }
            if follow && Some(*node) == selected {
                response.scroll_to_me(Some(egui::Align::Center));
            }
        }
        clicked
    }
    fn new(ui: &Ui, page: &PageRuns, span: Range<usize>, key: Key) -> Self {
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = key.width;
        let mut links = Vec::new();
        let mut characters = 0;
        for (run, node) in &page.runs {
            let start = run.start.max(span.start);
            let end = run.end.min(span.end);
            if start >= end {
                continue;
            }
            let text = &page.page.text[start..end];
            let count = text.chars().count();
            if let Some(id) = node {
                links.push((characters..characters + count, *id));
            }
            job.append(
                text,
                0.0,
                egui::TextFormat {
                    font_id: if node.is_some() {
                        FontId::proportional(key.size * 0.9)
                    } else {
                        serif(key.size, false)
                    },
                    color: if node.is_some() {
                        key.accent
                    } else {
                        key.color
                    },
                    line_height: Some(key.size * 1.5),
                    ..Default::default()
                },
            );
            characters += count;
        }
        let galley = ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));
        let mut regions = Vec::new();
        let mut character = 0;
        for row in &galley.rows {
            for (range, node) in &links {
                let a = range.start.saturating_sub(character).min(row.glyphs.len());
                let b = range.end.saturating_sub(character).min(row.glyphs.len());
                if a < b {
                    let first = &row.glyphs[a];
                    let last = &row.glyphs[b - 1];
                    regions.push((
                        egui::Rect::from_min_max(
                            row.pos + egui::vec2(first.pos.x, 0.0),
                            row.pos + egui::vec2(last.pos.x + last.advance_width, row.size.y),
                        ),
                        *node,
                    ));
                }
            }
            character += row.glyphs.len() + usize::from(row.ends_with_newline);
        }
        let unresolved = page
            .unresolved
            .iter()
            .filter(|(r, _)| r.start < span.end && r.end > span.start)
            .map(|(_, reason)| reason.clone())
            .collect();
        Self {
            key,
            galley,
            links: regions,
            unresolved,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::books::{
        document::{BlockKind, Document, Page},
        resolve,
    };
    #[test]
    fn wrapped_move_links_survive_font_changes_and_pointer_clicks() {
        let mut source = Page::default();
        source.push("Préparation: 1. e4 e5 2. Nf3 Nc6", BlockKind::Paragraph);
        let book = resolve::resolve(Document {
            title: String::new(),
            author: String::new(),
            format: "epub".into(),
            pages: vec![source],
            assets: vec![],
            cover: None,
            isbn: None,
        });
        let page = PageRuns::new(&book, 0);
        let ctx = egui::Context::default();
        super::super::style::fonts(&ctx);
        let mut cache = None;
        for size in [18.0, 26.0] {
            let mut origin = egui::Pos2::ZERO;
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.set_width(150.0);
                origin = ui.cursor().min;
                Paragraph::show(
                    &mut cache,
                    ui,
                    &page,
                    0..page.page.text.len() - 1,
                    size,
                    None,
                    false,
                );
            });
            output.drop_without_applying_deltas();
            let paragraph = cache.as_ref().unwrap();
            assert!(paragraph.galley.rows.len() >= 2);
            let (rect, id) = paragraph.links.last().copied().unwrap();
            assert_eq!(book.nodes[id].san, "Nc6");
            let pos = rect.center() + origin.to_vec2();
            let mut clicked = None;
            for pressed in [true, false] {
                let output = ctx.run_ui(
                    egui::RawInput {
                        events: vec![
                            egui::Event::PointerMoved(pos),
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                        ..Default::default()
                    },
                    |ui| {
                        ui.set_width(150.0);
                        clicked = Paragraph::show(
                            &mut cache,
                            ui,
                            &page,
                            0..page.page.text.len() - 1,
                            size,
                            None,
                            false,
                        );
                    },
                );
                output.drop_without_applying_deltas();
            }
            assert_eq!(clicked, Some(id));
        }
    }
}
