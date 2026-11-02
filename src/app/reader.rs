//! Reader navigation follows persisted source nodes rather than replaying prose.
use super::{Screen, Woodpeck, board::BoardWidget, widgets};
use crate::{
    books::{
        document::BlockKind,
        model::{Book, Status},
    },
    layout::LayoutMode,
    puzzle::Engine,
    store::ReadingProgress,
};
use eframe::egui::{self, RichText, Ui};
use std::time::{Duration, Instant};

pub(super) struct Reader {
    id: i64,
    book: Book,
    page: usize,
    node: Option<usize>,
    fragment: usize,
    choice: usize,
    white: bool,
    fen: String,
    board: BoardWidget,
    dirty: Option<Instant>,
    follow: bool,
    follow_moves: bool,
    textures: std::collections::HashMap<String, egui::TextureHandle>,
}
fn asset_image(bytes: &[u8]) -> Option<egui::ColorImage> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    if reader.format().is_some() {
        let mut reader = reader;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(8192);
        limits.max_image_height = Some(8192);
        limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        let decoded = reader.decode().ok()?;
        let image = if decoded.width().max(decoded.height()) > 1600 {
            decoded.thumbnail(1600, 1600)
        } else {
            decoded
        }
        .to_rgba8();
        return Some(egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        ));
    }
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).ok()?;
    let scale = (1024.0 / tree.size().width().max(tree.size().height())).min(1.0);
    let width = (tree.size().width() * scale).ceil().max(1.0) as u32;
    let height = (tree.size().height() * scale).ceil().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(egui::ColorImage::from_rgba_premultiplied(
        [width as usize, height as usize],
        pixmap.data(),
    ))
}

fn move_label(node: &crate::books::model::Node) -> String {
    use shakmaty::Position;
    crate::books::resolve::position(&node.fen).map_or_else(
        || node.san.clone(),
        |position| {
            let black = position.turn().is_white();
            let number = position.fullmoves().get().saturating_sub(u32::from(black));
            format!("{number}{} {}", if black { "..." } else { "." }, node.san)
        },
    )
}

impl Reader {
    pub(super) fn id(&self) -> i64 {
        self.id
    }
    pub fn new(id: i64, book: Book, progress: Option<ReadingProgress>) -> Self {
        let page = progress
            .as_ref()
            .map_or(0, |p| p.page_index as usize)
            .min(book.document.pages.len().saturating_sub(1));
        let node = progress
            .and_then(|p| p.node_id)
            .map(|n| n as usize)
            .filter(|n| book.nodes.get(*n).is_some_and(|n| n.page == page));
        let mut reader = Self {
            id,
            book,
            page,
            node: None,
            fragment: 0,
            choice: 0,
            white: true,
            fen: crate::books::resolve::fen(&shakmaty::Chess::default()),
            board: BoardWidget::default(),
            dirty: None,
            follow: false,
            follow_moves: false,
            textures: std::collections::HashMap::default(),
        };
        if let Some(node) = node {
            reader.select(node);
        } else {
            reader.change_page(page);
        }
        reader.dirty = None;
        reader.follow = false;
        reader
    }
    pub(super) fn preview_variations(&mut self) {
        if let Some(id) = self
            .book
            .nodes
            .iter()
            .find(|node| {
                self.book
                    .nodes
                    .iter()
                    .filter(|child| child.parent == Some(node.id))
                    .count()
                    > 1
            })
            .map(|node| node.id)
        {
            self.select(id);
            self.follow = false;
        }
    }
    fn children(&self) -> Vec<usize> {
        if let Some(parent) = self.node {
            self.book
                .nodes
                .iter()
                .filter(|n| n.parent == Some(parent))
                .map(|n| n.id)
                .collect()
        } else {
            self.book
                .fragments
                .get(self.fragment)
                .map_or_else(Vec::new, |f| {
                    let parent = f.nodes.first().and_then(|id| self.book.nodes[*id].parent);
                    f.nodes
                        .iter()
                        .copied()
                        .filter(|id| self.book.nodes[*id].parent == parent)
                        .collect()
                })
        }
    }
    fn select(&mut self, id: usize) {
        if let Some(n) = self.book.nodes.get(id) {
            self.node = Some(id);
            self.page = n.page;
            self.fen.clone_from(&n.fen);
            self.fragment = self
                .book
                .fragments
                .iter()
                .position(|f| f.nodes.contains(&id))
                .unwrap_or(0);
            self.choice = 0;
            self.follow = true;
            self.follow_moves = true;
            self.dirty = Some(Instant::now());
        }
    }
    fn change_page(&mut self, page: usize) {
        self.page = page.min(self.book.document.pages.len().saturating_sub(1));
        self.node = None;
        self.fragment = self
            .book
            .fragments
            .iter()
            .position(|f| f.page == self.page)
            .unwrap_or(usize::MAX);
        self.start();
    }
    fn start(&mut self) {
        if let Some(mut root) = self.node {
            while let Some(parent) = self.book.nodes[root].parent {
                root = parent;
            }
            self.fragment = self
                .book
                .fragments
                .iter()
                .position(|f| f.nodes.contains(&root))
                .unwrap_or(self.fragment);
            self.page = self.book.nodes[root].page;
        }
        self.node = None;
        self.choice = 0;
        if let Some(f) = self.book.fragments.get(self.fragment) {
            self.fen.clone_from(&f.start_fen);
        }
        self.dirty = Some(Instant::now());
    }
    fn forward(&mut self) {
        if let Some(id) = self.children().get(self.choice).copied() {
            self.select(id);
        }
    }
    fn back(&mut self) {
        if let Some(id) = self.node {
            if let Some(parent) = self.book.nodes[id].parent {
                self.select(parent);
            } else {
                self.start();
            }
        }
    }
    fn end(&mut self) {
        while !self.children().is_empty() {
            self.forward();
        }
    }
    fn progress(&self) -> ReadingProgress {
        ReadingProgress {
            page_index: u32::try_from(self.page).unwrap_or(u32::MAX),
            node_id: self.node.and_then(|n| u32::try_from(n).ok()),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
    fn keyboard(&mut self, ui: &Ui) {
        if ui.ctx().egui_wants_keyboard_input() || ui.input(|i| i.modifiers.any()) {
            return;
        }
        for key in [
            egui::Key::ArrowLeft,
            egui::Key::ArrowRight,
            egui::Key::ArrowUp,
            egui::Key::ArrowDown,
            egui::Key::Home,
            egui::Key::End,
            egui::Key::PageUp,
            egui::Key::PageDown,
        ] {
            if ui.input(|i| i.key_pressed(key)) {
                match key {
                    egui::Key::ArrowLeft => self.back(),
                    egui::Key::ArrowRight => self.forward(),
                    egui::Key::Home => self.start(),
                    egui::Key::End => self.end(),
                    egui::Key::ArrowUp => self.choice = self.choice.saturating_sub(1),
                    egui::Key::ArrowDown => {
                        self.choice =
                            (self.choice + 1).min(self.children().len().saturating_sub(1));
                    }
                    egui::Key::PageUp => self.change_page(self.page.saturating_sub(1)),
                    egui::Key::PageDown => self.change_page(self.page + 1),
                    _ => {}
                }
            }
        }
    }
    fn paragraph(&mut self, ui: &mut Ui, text: &str, span: &std::ops::Range<usize>, follow: bool) {
        let runs = self.book.text_runs(self.page).unwrap_or_default();
        let mut selected = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.spacing_mut().button_padding.x = 1.0;
            for run in runs {
                let start = run.span.start.max(span.start);
                let end = run.span.end.min(span.end);
                if start >= end {
                    continue;
                }
                let text = &text[start..end];
                if let Some(id) = run.node_id {
                    let response = ui.selectable_label(
                        self.node == Some(id),
                        RichText::new(text)
                            .size(17.0)
                            .color(if self.node == Some(id) {
                                ui.visuals().selection.stroke.color
                            } else {
                                ui.visuals().hyperlink_color
                            }),
                    );
                    if self.node == Some(id) && follow {
                        response.scroll_to_me(Some(egui::Align::Center));
                    }
                    if response.clicked() {
                        selected = Some(id);
                    }
                } else {
                    let reason = self.book.fragments.iter().find_map(|f| {
                        if f.page != self.page || f.span.start >= end || f.span.end <= start {
                            return None;
                        }
                        match &f.status {
                            Status::Partial(reason) | Status::Unresolved(reason) => Some(reason),
                            Status::Resolved => None,
                        }
                    });
                    let rich = RichText::new(text).size(17.0);
                    let response = ui.add(
                        egui::Label::new(if reason.is_some() {
                            rich.weak().underline()
                        } else {
                            rich
                        })
                        .wrap(),
                    );
                    if let Some(reason) = reason {
                        response.on_hover_text(format!("Couldn't follow these moves: {reason}"));
                    }
                }
            }
        });
        if let Some(id) = selected {
            self.select(id);
        }
    }
    fn text(&mut self, ui: &mut Ui, height: f32) {
        let Some(page) = self.book.document.pages.get(self.page).cloned() else {
            ui.label("This book has no pages.");
            return;
        };
        let follow = std::mem::take(&mut self.follow);
        egui::ScrollArea::vertical()
            .id_salt(("book-page", self.id, self.page))
            .max_height(height)
            .show(ui, |ui| {
                let blocks = if page.blocks.is_empty() {
                    vec![crate::books::document::Block {
                        span: 0..page.text.len(),
                        kind: BlockKind::Paragraph,
                    }]
                } else {
                    page.blocks.clone()
                };
                for block in blocks {
                    match &block.kind {
                        BlockKind::Image { path, alt } => {
                            if !self.textures.contains_key(path)
                                && let Some(asset) =
                                    self.book.document.assets.iter().find(|a| &a.path == path)
                                && let Some(color) = asset_image(&asset.bytes)
                            {
                                self.textures.insert(
                                    path.clone(),
                                    ui.ctx().load_texture(
                                        path,
                                        color,
                                        egui::TextureOptions::default(),
                                    ),
                                );
                            }
                            if let Some(texture) = self.textures.get(path) {
                                ui.add(
                                    egui::Image::new(texture)
                                        .max_width(ui.available_width())
                                        .max_height(260.0),
                                );
                            } else {
                                ui.weak(alt);
                            }
                        }
                        BlockKind::Diagram { fen } => {
                            if let Some(position) = crate::books::resolve::position(fen) {
                                let mut board = BoardWidget::default();
                                board.reader_orientation = Some(self.white);
                                board.show(
                                    ui,
                                    &Engine::reading(position),
                                    180.0,
                                    crate::settings::Settings::default().board_theme,
                                    crate::settings::Settings::default().piece_rendering,
                                );
                                if board.clicked(ui)
                                    || ui.small_button("Explore diagram >").clicked()
                                {
                                    self.fen.clone_from(fen);
                                    self.node = None;
                                    self.dirty = Some(Instant::now());
                                }
                            }
                        }
                        BlockKind::Heading => {
                            ui.heading(page.text.get(block.span.clone()).unwrap_or_default());
                        }
                        _ => {
                            self.paragraph(ui, &page.text, &block.span, follow);
                        }
                    }
                    ui.add_space(14.0);
                }
                for fragment in self.book.fragments.iter().filter(|f| f.page == self.page) {
                    if let Status::Partial(reason) | Status::Unresolved(reason) = &fragment.status {
                        ui.weak("··· Unresolved moves")
                            .on_hover_text(format!("Couldn't follow these moves: {reason}"));
                    }
                }
            });
    }
    fn position(&mut self, ui: &mut Ui, size: f32, settings: &crate::settings::Settings) {
        if let Some(position) = crate::books::resolve::position(&self.fen) {
            self.board.reader_orientation = Some(self.white);
            self.board.show(
                ui,
                &Engine::reading(position),
                size,
                settings.board_theme,
                settings.piece_rendering,
            );
        } else {
            ui.weak("Select a resolved move or diagram to explore a position.");
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("|<")
                .on_hover_text("Start of line · Home")
                .clicked()
            {
                self.start();
            }
            if ui.button("<").on_hover_text("Back · Left").clicked() {
                self.back();
            }
            if ui.button(">").on_hover_text("Forward · Right").clicked() {
                self.forward();
            }
            if ui.button(">|").on_hover_text("End of line · End").clicked() {
                self.end();
            }
            if ui.button("Flip board").clicked() {
                self.white = !self.white;
            }
        });
        let children = self.children();
        if children.len() > 1 {
            ui.label(RichText::new("Choose next move · Up / Down, then Right").strong());
            ui.horizontal_wrapped(|ui| {
                for (index, id) in children.iter().enumerate() {
                    if ui
                        .add(
                            egui::Button::new(&self.book.nodes[*id].san)
                                .selected(index == self.choice),
                        )
                        .clicked()
                    {
                        self.select(*id);
                    }
                }
            });
        }
        let follow_moves = std::mem::take(&mut self.follow_moves);
        egui::ScrollArea::vertical()
            .id_salt(("reader-moves", self.id))
            .max_height(ui.available_height().clamp(100.0, 220.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().interact_size.y = 22.0;
                ui.spacing_mut().button_padding.y = 2.0;
                if let Some(fragment) = self.book.fragments.get(self.fragment) {
                    let ids = fragment.nodes.clone();
                    let mut jump = None;
                    for id in ids {
                        let n = &self.book.nodes[id];
                        let mut depth = 0;
                        let mut child = id;
                        let mut parent = n.parent;
                        while let Some(p) = parent {
                            if self
                                .book
                                .nodes
                                .iter()
                                .find(|n| n.parent == Some(p))
                                .is_some_and(|first| first.id != child)
                            {
                                depth += 1;
                            }
                            child = p;
                            parent = self.book.nodes[p].parent;
                        }
                        ui.horizontal(|ui| {
                            ui.add_space((depth as f32 * 12.0).min(72.0));
                            let response =
                                ui.selectable_label(self.node == Some(id), move_label(n));
                            if self.node == Some(id) && follow_moves {
                                response.scroll_to_me(Some(egui::Align::Center));
                            }
                            if response.clicked() {
                                jump = Some(id);
                            }
                        });
                    }
                    if let Some(id) = jump {
                        self.select(id);
                    }
                }
            });
    }
}
impl Woodpeck {
    pub(super) fn save_reader(&mut self) {
        let Some(reader) = &mut self.books.reader else {
            return;
        };
        if let Some(changed) = reader.dirty {
            if changed.elapsed() < Duration::from_millis(450) && self.screen == Screen::Reader {
                self.ctx.request_repaint_after(Duration::from_millis(450));
                return;
            }
            reader.dirty = None;
            let id = reader.id;
            let progress = reader.progress();
            let saved = progress.clone();
            self.background(
                move |s| s.save_reading_progress(id, &progress),
                move |app, result| {
                    if let Err(error) = result {
                        app.error(error);
                    } else if let Some(entry) = app.books.entries.iter_mut().find(|e| e.0 == id) {
                        entry.2 = Some(saved);
                    }
                },
            );
        }
    }
    pub(super) fn reader_screen(&mut self, ui: &mut Ui, mode: LayoutMode) {
        if ui.button("< Library").clicked() {
            self.navigate(Screen::Books);
        }
        let Some(reader) = &mut self.books.reader else {
            ui.label("Open a book from your Library.");
            return;
        };
        let previous = reader.node;
        reader.keyboard(ui);
        widgets::caption(ui, &reader.book.document.author);
        ui.heading(&reader.book.document.title);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(reader.page > 0, egui::Button::new("< Previous page"))
                .clicked()
            {
                reader.change_page(reader.page - 1);
            }
            let mut page = reader.page + 1;
            if ui
                .add(
                    egui::DragValue::new(&mut page)
                        .range(1..=reader.book.document.pages.len().max(1)),
                )
                .changed()
            {
                reader.change_page(page - 1);
            }
            ui.label(format!("/ {}", reader.book.document.pages.len()));
            if ui
                .add_enabled(
                    reader.page + 1 < reader.book.document.pages.len(),
                    egui::Button::new("Next page >"),
                )
                .clicked()
            {
                reader.change_page(reader.page + 1);
            }
        });
        ui.add_space(16.0);
        let height = ui.available_height();
        let controls_and_moves = if reader.children().len() > 1 {
            300.0
        } else {
            260.0
        };
        if mode == LayoutMode::Columns {
            ui.columns(2, |columns| {
                reader.text(&mut columns[0], height);
                let size = columns[1]
                    .available_width()
                    .min((height - controls_and_moves).max(80.0));
                reader.position(&mut columns[1], size, &self.settings);
            });
        } else {
            let text_height = (height * 0.32).max(80.0);
            reader.text(ui, text_height);
            ui.separator();
            let size = ui
                .available_width()
                .min((height - text_height - controls_and_moves).max(80.0));
            reader.position(ui, size, &self.settings);
        }
        if previous != reader.node && reader.node.is_some() && self.screenshot.is_none() {
            crate::sound::play(crate::sound::Event::Move, &self.settings);
        }
        self.save_reader();
    }
}

impl Drop for Woodpeck {
    fn drop(&mut self) {
        if let Some(reader) = &self.books.reader
            && reader.dirty.is_some()
        {
            let id = reader.id;
            let progress = reader.progress();
            let _ = self
                .service
                .background(move |s| s.save_reading_progress(id, &progress));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::books::document::{Document, Page};
    fn book() -> Book {
        let mut first = Page::default();
        first.push("1. e4 (1. d4 d5) e5", BlockKind::Paragraph);
        let mut second = Page::default();
        second.push("2. Nf3 Nc6", BlockKind::Paragraph);
        crate::books::resolve::resolve(Document {
            title: "Reader test".into(),
            author: String::new(),
            format: "epub".into(),
            pages: vec![first, second],
            assets: vec![],
        })
    }
    #[test]
    fn embedded_images_decode_and_invalid_assets_fall_back() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red"/></svg>"#;
        assert_eq!(asset_image(svg).unwrap().size, [20, 10]);
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::new(8, 8)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        assert_eq!(asset_image(png.get_ref()).unwrap().size, [8, 8]);
        assert!(asset_image(b"broken image").is_none());
    }
    #[test]
    fn variations_and_page_following() {
        let book = book();
        let e4 = book.nodes.iter().find(|n| n.san == "e4").unwrap().id;
        let d4 = book.nodes.iter().find(|n| n.san == "d4").unwrap().id;
        let mut reader = Reader::new(1, book, None);
        assert_eq!(reader.children(), vec![e4, d4]);
        reader.choice = 1;
        reader.forward();
        assert_eq!(reader.node, Some(d4));
        reader.forward();
        assert_eq!(reader.book.nodes[reader.node.unwrap()].san, "d5");
        reader.back();
        assert_eq!(reader.node, Some(d4));
        reader.start();
        reader.forward();
        assert_eq!(reader.node, Some(e4));
        reader.forward();
        reader.forward();
        assert_eq!(reader.page, 1);
        assert_eq!(reader.book.nodes[reader.node.unwrap()].san, "Nf3");
        reader.back();
        assert_eq!(reader.page, 0);
        reader.end();
        assert_eq!(reader.page, 1);
        assert_eq!(reader.book.nodes[reader.node.unwrap()].san, "Nc6");
    }
    #[test]
    fn progress_roundtrip_and_manual_page_change() {
        let book = book();
        let store = crate::store::Store::memory().unwrap();
        let id = store.save_book(&book, "reader-test", 0).unwrap();
        let mut reader = Reader::new(id, book.clone(), None);
        reader.end();
        let progress = reader.progress();
        store.save_reading_progress(id, &progress).unwrap();
        let mut restored = Reader::new(id, book, store.reading_progress(id).unwrap());
        assert!(
            !restored.follow,
            "restoring a move keeps the page headings visible"
        );
        assert!(
            restored.follow_moves,
            "restored moves remain visible in the move list"
        );
        assert_eq!(restored.page, reader.page);
        assert_eq!(restored.node, reader.node);
        assert_eq!(restored.fen, reader.fen);
        restored.change_page(0);
        assert_eq!(restored.node, None);
        assert_eq!(restored.page, 0);
        store
            .save_reading_progress(id, &restored.progress())
            .unwrap();
    }
}
