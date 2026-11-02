//! Library covers: the book's own cover, else one cached from an online lookup, else a
//! generated cover drawn from the title so every book has something recognizable.
use super::{Woodpeck, reader::asset_image};
use crate::books::model::Book;
use eframe::egui::{self, Color32, Ui};
use std::{collections::HashMap, sync::mpsc};

/// Retry an online lookup that found nothing after this long (30 days).
const RETRY_MISS: i64 = 30 * 24 * 3600;
/// Width and height of a cover on the shelf, in points.
pub(super) const COVER: egui::Vec2 = egui::vec2(150.0, 225.0);

#[derive(Default)]
pub(super) struct Covers {
    /// Decoded covers by book; `None` when the book has no usable picture.
    textures: HashMap<i64, Option<egui::TextureHandle>>,
    /// Online lookups by book: image bytes (empty for a miss) and when they ran.
    online: Option<HashMap<i64, (Vec<u8>, i64)>>,
    loading: bool,
    lookups: Option<mpsc::Receiver<(i64, String, Vec<u8>)>>,
    tried: std::collections::HashSet<i64>,
}

impl Woodpeck {
    /// Loads cached lookups once, then looks up covers for books that need one, in the
    /// background and one book at a time. Lookups never run in tests or screenshot mode.
    pub(super) fn update_covers(&mut self, ctx: &egui::Context) {
        let mut received = Vec::new();
        if let Some(lookups) = &self.books.covers.lookups {
            loop {
                match lookups.try_recv() {
                    Ok(result) => received.push(result),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.books.covers.lookups = None;
                        break;
                    }
                }
            }
        }
        for (id, title, image) in received {
            // The book may have been deleted meanwhile and its id given to another one.
            if !self
                .books
                .entries
                .iter()
                .any(|(entry, book, _)| *entry == id && book.document.title == title)
            {
                continue;
            }
            let now = chrono::Utc::now().timestamp();
            if let Some(online) = &mut self.books.covers.online {
                online.insert(id, (image.clone(), now));
            }
            self.books.covers.textures.remove(&id);
            self.background(move |s| s.save_book_cover(id, &image), |_, _| {});
        }
        if self.books.covers.online.is_none() {
            if !self.books.covers.loading {
                self.books.covers.loading = true;
                self.background(crate::service::LocalService::book_covers, |app, result| {
                    app.books.covers.loading = false;
                    // Covers drawn before the cache arrived were generated; decode again.
                    app.books.covers.textures.clear();
                    app.books.covers.online = Some(
                        result
                            .unwrap_or_default()
                            .into_iter()
                            .map(|(id, image, at)| (id, (image, at)))
                            .collect(),
                    );
                });
            }
            return;
        }
        if self.books.covers.lookups.is_some() || cfg!(test) || self.screenshot.is_some() {
            return;
        }
        let covers = &mut self.books.covers;
        let Some(online) = &covers.online else {
            return;
        };
        let now = chrono::Utc::now().timestamp();
        let wanted: Vec<_> = self
            .books
            .entries
            .iter()
            .filter(|(id, book, _)| {
                book.document.cover.is_none()
                    && !covers.tried.contains(id)
                    && online
                        .get(id)
                        .is_none_or(|(image, at)| image.is_empty() && now - at > RETRY_MISS)
            })
            .map(|(id, book, _)| {
                (
                    *id,
                    book.document.title.clone(),
                    book.document.author.clone(),
                    book.document.isbn.clone(),
                )
            })
            .collect();
        if wanted.is_empty() {
            return;
        }
        covers.tried.extend(wanted.iter().map(|w| w.0));
        let (send, receive) = mpsc::channel();
        covers.lookups = Some(receive);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            for (index, (id, title, author, isbn)) in wanted.into_iter().enumerate() {
                if index > 0 {
                    // One request a second at most, to stay well inside Open Library's limits.
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
                // Offline, throttled or refused: stop and leave the rest for the next session
                // instead of recording misses.
                let Ok(image) = crate::books::cover::lookup(&title, &author, isbn.as_deref())
                else {
                    return;
                };
                if send.send((id, title, image.unwrap_or_default())).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        });
    }
}

impl Covers {
    /// Draws the cover for one book at `size`; returns the click response.
    pub(super) fn show(
        &mut self,
        ui: &mut Ui,
        id: i64,
        book: &Book,
        size: egui::Vec2,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let online = self.online.as_ref();
        let texture = self.textures.entry(id).or_insert_with(|| {
            let own = book
                .document
                .cover
                .as_ref()
                .and_then(|path| book.document.assets.iter().find(|a| &a.path == path))
                .map(|a| a.bytes.as_slice());
            let fetched = online
                .and_then(|o| o.get(&id))
                .map(|(image, _)| image.as_slice())
                .filter(|i| !i.is_empty());
            // An own cover that fails to decode gives way to a fetched one.
            own.and_then(|bytes| asset_image(bytes, 480))
                .or_else(|| fetched.and_then(|bytes| asset_image(bytes, 480)))
                .map(|image| {
                    ui.ctx().load_texture(
                        format!("cover-{id}"),
                        image,
                        egui::TextureOptions::LINEAR,
                    )
                })
        });
        let corner = 6;
        if let Some(texture) = texture {
            // Fill the frame like a book jacket, cropping rather than letterboxing.
            let [w, h] = texture.size().map(|v| v as f32);
            let scale = (rect.width() / w).max(rect.height() / h);
            let (u, v) = (rect.width() / (w * scale), rect.height() / (h * scale));
            let uv = egui::Rect::from_center_size(egui::pos2(0.5, 0.5), egui::vec2(u, v));
            egui::Image::new(&*texture)
                .uv(uv)
                .corner_radius(corner)
                .paint_at(ui, rect);
        } else {
            generated(ui, rect, &book.document.title, &book.document.author);
        }
        if response.hovered() {
            ui.painter().rect_stroke(
                rect,
                corner,
                ui.visuals().selection.stroke,
                egui::StrokeKind::Inside,
            );
        }
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    }
    /// Drops everything known about a deleted book, whose id a later import may reuse.
    pub(super) fn forget(&mut self, id: i64) {
        self.textures.remove(&id);
        self.tried.remove(&id);
        if let Some(online) = &mut self.online {
            online.remove(&id);
        }
    }
}

/// A plain cover from the title: a color picked from the title, a strip of board squares,
/// and the title and author set on it.
fn generated(ui: &Ui, rect: egui::Rect, title: &str, author: &str) {
    let hash = title.bytes().fold(2_166_136_261_u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(16_777_619)
    });
    let palette = [
        Color32::from_rgb(46, 74, 61),
        Color32::from_rgb(70, 52, 40),
        Color32::from_rgb(40, 56, 82),
        Color32::from_rgb(86, 44, 48),
        Color32::from_rgb(58, 58, 64),
        Color32::from_rgb(76, 66, 36),
    ];
    let fill = palette[hash as usize % palette.len()];
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6, fill);
    let square = rect.width() / 8.0;
    for i in 0..8 {
        let light = i % 2 == 0;
        let cell = egui::Rect::from_min_size(
            egui::pos2(rect.left() + square * i as f32, rect.top() + square * 0.6),
            egui::vec2(square, square * 0.5),
        );
        painter.rect_filled(
            cell,
            0,
            if light {
                Color32::from_white_alpha(46)
            } else {
                Color32::from_black_alpha(40)
            },
        );
    }
    // Type and margins follow the cover size, so small covers stay legible.
    let scale = rect.width() / COVER.x;
    let margin = 12.0 * scale;
    let width = rect.width() - margin * 2.0;
    let title = painter.layout(
        title.trim().to_owned(),
        egui::FontId::proportional(17.0 * scale),
        Color32::WHITE,
        width,
    );
    let top = rect.top() + square * 1.6;
    painter.galley(egui::pos2(rect.left() + margin, top), title, Color32::WHITE);
    let author = painter.layout(
        author.trim().to_owned(),
        egui::FontId::proportional((12.0 * scale).max(8.0)),
        Color32::from_white_alpha(190),
        width,
    );
    let y = rect.bottom() - margin - author.size().y;
    painter.galley(egui::pos2(rect.left() + margin, y), author, Color32::WHITE);
}
