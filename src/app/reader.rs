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
use std::{rc::Rc, time::Duration};
use web_time::Instant;

use super::{
    reader_flow::PageRuns,
    reader_type::{Paragraph, serif},
};

/// About 68 characters in the embedded reading face at the default size.
const MEASURE: f32 = 660.0;

fn heading(ui: &mut Ui, text: &str, rank: u8, first: bool, after_heading: bool) {
    if first {
        ui.add_space(12.0);
    } else if !after_heading {
        ui.add_space(if rank == 0 { 24.0 } else { 14.0 });
    }
    let size = if after_heading {
        20.0
    } else {
        heading_size(rank)
    }
    .min(ui.available_width() / 7.0);
    let rich = RichText::new(text.trim())
        .font(serif(size, rank > 0 && !after_heading))
        .line_height(Some(size * 1.2));
    ui.add(egui::Label::new(if after_heading { rich.weak() } else { rich }).wrap());
    ui.add_space(if rank == 0 { 14.0 } else { 4.0 });
}
fn heading_size(rank: u8) -> f32 {
    match rank {
        0 => 36.0,
        1 => 27.0,
        2 => 23.0,
        _ => 20.0,
    }
}

/// Neutral paper and ink remain independent of saturated desktop accent palettes.
fn paper(ui: &mut Ui, theme: crate::settings::ReadingTheme) -> egui::Color32 {
    use crate::settings::ReadingTheme;
    let dark =
        theme == ReadingTheme::Dark || (theme == ReadingTheme::Auto && ui.visuals().dark_mode);
    let (background, ink, muted, accent, selection) = if dark {
        (
            0x0024_2625,
            0x00e5_e2da,
            0x00ab_aea8,
            0x00a9_ccba,
            0x0036_4b40,
        )
    } else if theme == ReadingTheme::Sepia {
        (
            0x00f3_e9d5,
            0x0030_2b23,
            0x0071_634f,
            0x0030_5e50,
            0x00d7_ddc7,
        )
    } else {
        (
            0x00fa_f9f6,
            0x0029_2d2a,
            0x006b_706a,
            0x0030_654e,
            0x00dc_e9df,
        )
    };
    let rgb = |hex: u32| {
        let bytes = hex.to_be_bytes();
        egui::Color32::from_rgb(bytes[1], bytes[2], bytes[3])
    };
    let visuals = ui.visuals_mut();
    *visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.override_text_color = Some(rgb(ink));
    visuals.weak_text_color = Some(rgb(muted));
    visuals.hyperlink_color = rgb(accent);
    visuals.selection.bg_fill = rgb(selection);
    visuals.selection.stroke.color = rgb(accent);
    rgb(background)
}

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
    /// Text runs of the page on screen, rebuilt only when the page changes.
    runs: std::collections::VecDeque<(usize, Rc<PageRuns>)>,
    paragraphs: std::collections::HashMap<(usize, usize), Option<Paragraph>>,
    /// The side to move the reader chose for boards the book does not confirm (true: White).
    sides: std::collections::HashMap<String, bool>,
    /// Chapter headings across the book: (page, level, title).
    contents: Vec<(usize, u8, String)>,
    /// Highest heading level the book uses, so its chapters get the chapter style.
    top_level: u8,
    /// What the import found, shown once above the book until dismissed.
    pub(super) notice: Option<String>,
}
/// The heading level that marks chapters: the highest imported level used more than once.
fn chapter_level(pages: &[crate::books::document::Page]) -> u8 {
    pages
        .iter()
        .flat_map(|page| &page.blocks)
        // Only imported levels count: unknown ones (older imports) read as sections.
        .filter(|b| b.kind == BlockKind::Heading && b.level > 0)
        .map(crate::books::document::Block::heading_level)
        .fold([0usize; 7], |mut counts, level| {
            counts[usize::from(level)] += 1;
            counts
        })
        .iter()
        .enumerate()
        // A level used once is usually the book title; chapters repeat.
        .find(|(_, count)| **count >= 2)
        .and_then(|(level, _)| u8::try_from(level).ok())
        .unwrap_or(1)
}
/// Decodes a PNG, JPEG or SVG asset, scaled down so its longer side is at most `max` pixels.
pub(super) fn asset_image(bytes: &[u8], max: u32) -> Option<egui::ColorImage> {
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
        let image = if decoded.width().max(decoded.height()) > max {
            decoded.thumbnail(max, max)
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
    let limit = f32::from(u16::try_from(max.min(1024)).unwrap_or(1024));
    let scale = (limit / tree.size().width().max(tree.size().height())).min(1.0);
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
            runs: std::collections::VecDeque::new(),
            paragraphs: std::collections::HashMap::new(),
            sides: std::collections::HashMap::new(),
            contents: Vec::new(),
            top_level: 1,
            notice: None,
        };
        reader.contents = reader
            .book
            .document
            .pages
            .iter()
            .enumerate()
            .flat_map(|(index, page)| {
                page.blocks
                    .iter()
                    .filter(|b| b.kind == BlockKind::Heading && b.heading_level() <= 3)
                    .filter_map(move |b| {
                        let title = page.text.get(b.span.clone())?.trim();
                        (!title.is_empty()).then(|| (index, b.heading_level(), title.to_owned()))
                    })
            })
            .collect();
        reader.top_level = chapter_level(&reader.book.document.pages);
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
        // The first line on the page that has moves, else its first passage of any kind.
        let on_page = |f: &&crate::books::model::Fragment| f.page == self.page;
        self.fragment = self
            .book
            .fragments
            .iter()
            .position(|f| on_page(&f) && !f.nodes.is_empty())
            .or_else(|| self.book.fragments.iter().position(|f| on_page(&f)))
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
            // An unresolved passage has no verified position: its start is only where the
            // last good line stopped, so the board says so instead of showing it.
            if matches!(f.status, Status::Unresolved(_)) {
                self.fen.clear();
            } else {
                self.fen.clone_from(&f.start_fen);
            }
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
    fn page_runs(&mut self) -> Rc<PageRuns> {
        if let Some((_, runs)) = self.runs.iter().find(|(page, _)| *page == self.page) {
            return runs.clone();
        }
        let runs = Rc::new(PageRuns::new(&self.book, self.page));
        if self.runs.len() >= 6
            && let Some((page, _)) = self.runs.pop_front()
        {
            self.paragraphs.retain(|(p, _), _| *p != page);
        }
        self.runs.push_back((self.page, runs.clone()));
        runs
    }
    fn paragraph(
        &mut self,
        ui: &mut Ui,
        page: &PageRuns,
        span: &std::ops::Range<usize>,
        size: f32,
        follow: bool,
    ) {
        let text = &page.page.text[span.clone()];
        if crate::books::reflow::caption(text)
            && !page
                .runs
                .iter()
                .any(|(run, node)| node.is_some() && run.start < span.end && run.end > span.start)
        {
            ui.vertical_centered(|ui| {
                ui.add(egui::Label::new(RichText::new(text).size(size * 0.8).weak()).wrap());
            });
            return;
        }
        let cache = self.paragraphs.entry((self.page, span.start)).or_default();
        if let Some(id) = Paragraph::show(cache, ui, page, span.clone(), size, self.node, follow) {
            self.select(id);
        }
    }
    fn illustration(
        &mut self,
        ui: &mut Ui,
        block: &crate::books::document::Block,
        settings: &crate::settings::Settings,
    ) {
        match &block.kind {
            BlockKind::Image { path, alt } => {
                if !self.textures.contains_key(path)
                    && let Some(asset) = self.book.document.assets.iter().find(|a| &a.path == path)
                    && let Some(color) = asset_image(&asset.bytes, 1600)
                {
                    self.textures.insert(
                        path.clone(),
                        ui.ctx()
                            .load_texture(path, color, egui::TextureOptions::default()),
                    );
                }
                if let Some(texture) = self.textures.get(path) {
                    ui.vertical_centered(|ui| {
                        ui.add(
                            egui::Image::new(texture)
                                .max_width(ui.available_width())
                                .max_height(260.0),
                        );
                        if !alt.trim().is_empty() {
                            ui.small(alt);
                        }
                    });
                } else {
                    ui.weak(alt);
                }
            }
            BlockKind::Diagram { fen } | BlockKind::RecognizedDiagram { fen, .. } => {
                if let Some(position) = crate::books::resolve::position(fen) {
                    ui.vertical_centered(|ui| {
                        let mut board = BoardWidget::default();
                        board.reader_orientation = Some(self.white);
                        board.show(
                            ui,
                            &Engine::reading(position),
                            ui.available_width().min(220.0),
                            settings.board_theme,
                            settings.piece_rendering,
                        );
                        if board.clicked(ui) || ui.small_button("Explore diagram >").clicked() {
                            self.fen.clone_from(fen);
                            self.node = None;
                            self.dirty = Some(Instant::now());
                        }
                        if let BlockKind::RecognizedDiagram { alt, .. } = &block.kind
                            && !alt.trim().is_empty()
                        {
                            ui.small(alt);
                        }
                    });
                }
            }
            BlockKind::UnconfirmedDiagram {
                path,
                alt,
                placement,
            } => self.unconfirmed(ui, path, alt, placement, settings),
            _ => {}
        }
    }
    /// A board read from pixels whose side to move the book does not confirm. Nothing is
    /// assumed: exploring it waits until the reader picks a side.
    fn unconfirmed(
        &mut self,
        ui: &mut Ui,
        path: &str,
        alt: &str,
        placement: &str,
        settings: &crate::settings::Settings,
    ) {
        let at = |white: bool| {
            let fen = format!("{placement} {} - - 0 1", if white { 'w' } else { 'b' });
            crate::books::resolve::position(&fen).map(|p| (fen, p))
        };
        let chosen = self.sides.get(path).copied();
        let Some((_, shown)) = chosen
            .and_then(at)
            .or_else(|| at(true))
            .or_else(|| at(false))
        else {
            return;
        };
        ui.vertical_centered(|ui| {
            let mut board = BoardWidget::default();
            board.reader_orientation = Some(self.white);
            board.show(
                ui,
                &Engine::reading(shown),
                ui.available_width().min(220.0),
                settings.board_theme,
                settings.piece_rendering,
            );
            match chosen {
                None => {
                    ui.colored_label(ui.visuals().warn_fg_color, "⚠ Side to move not confirmed")
                        .on_hover_text(
                            "The pieces were read from the picture, but the book's text \
                             doesn't say who moves. Choose a side to explore it.",
                        );
                }
                Some(white) => {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        if white {
                            "⚠ White to move (your choice, not confirmed)"
                        } else {
                            "⚠ Black to move (your choice, not confirmed)"
                        },
                    );
                }
            }
            ui.horizontal(|ui| {
                for (white, label) in [(true, "White to move"), (false, "Black to move")] {
                    if let Some((fen, _)) = at(white)
                        && ui.selectable_label(chosen == Some(white), label).clicked()
                    {
                        self.sides.insert(path.to_owned(), white);
                        self.fen = fen;
                        self.node = None;
                        self.dirty = Some(Instant::now());
                    }
                }
            });
            if let Some((fen, _)) = chosen.and_then(at)
                && ui.small_button("Explore diagram >").clicked()
            {
                self.fen = fen;
                self.node = None;
                self.dirty = Some(Instant::now());
            }
            if !alt.trim().is_empty() {
                ui.small(alt);
            }
        });
    }
    fn text(&mut self, ui: &mut Ui, height: f32, settings: &crate::settings::Settings) {
        if self.book.document.pages.get(self.page).is_none() {
            ui.label("This book has no pages.");
            return;
        }
        let runs = self.page_runs();
        let page = &runs.page;
        let size = f32::from(settings.reader_font_size.clamp(16, 28));
        let margin = if ui.available_width() < 400.0 { 18 } else { 36 };
        ui.scope(|ui| {
            let background = paper(ui, settings.reader_theme);
            egui::Frame::new()
                .fill(background)
                .corner_radius(4)
                .inner_margin(margin)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    let height = (height - f32::from(margin) * 2.0).max(80.0);
                    let follow = std::mem::take(&mut self.follow);
                    egui::ScrollArea::vertical()
                        .id_salt(("book-page", self.id, self.page))
                        .max_height(height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let width = ui.available_width().min(MEASURE * size / 20.0);
                            let inset = ((ui.available_width() - width) / 2.0).max(0.0);
                            let rect = egui::Rect::from_min_size(
                                ui.cursor().min + egui::vec2(inset, 0.0),
                                egui::vec2(width, 0.0),
                            );
                            ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                                ui.set_width(width);
                                ui.spacing_mut().item_spacing.y = 0.0;
                                let blocks = &page.blocks;
                                let mut previous_heading = false;
                                for (index, block) in blocks.iter().enumerate() {
                                    match &block.kind {
                                        BlockKind::Image { .. }
                                        | BlockKind::Diagram { .. }
                                        | BlockKind::RecognizedDiagram { .. }
                                        | BlockKind::UnconfirmedDiagram { .. } => {
                                            self.illustration(ui, block, settings);
                                        }
                                        BlockKind::Heading => heading(
                                            ui,
                                            page.text.get(block.span.clone()).unwrap_or_default(),
                                            if block.level == 0 && index == 0 {
                                                0
                                            } else {
                                                block.heading_level().saturating_sub(self.top_level)
                                            },
                                            index == 0,
                                            previous_heading,
                                        ),
                                        _ => {
                                            ui.push_id(index, |ui| {
                                                self.paragraph(
                                                    ui,
                                                    &runs,
                                                    &block.span,
                                                    size,
                                                    follow,
                                                );
                                            });
                                        }
                                    }
                                    previous_heading = block.kind == BlockKind::Heading;
                                    ui.add_space(if previous_heading { 8.0 } else { size * 0.65 });
                                }
                                for (_, reason) in &runs.unresolved {
                                    ui.weak("··· Unresolved moves").on_hover_text(format!(
                                        "Couldn't follow these moves: {reason}"
                                    ));
                                }
                                ui.add_space(20.0);
                            });
                        });
                });
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
impl Reader {
    fn layout(&mut self, ui: &mut Ui, mode: LayoutMode, settings: &crate::settings::Settings) {
        let height = ui.available_height();
        let controls_and_moves = if self.children().len() > 1 {
            300.0
        } else {
            260.0
        };
        if mode == LayoutMode::Columns {
            // Reserve a bounded analysis pane; the reading page gets the remaining width
            // and centers its text within a comfortable measure.
            let total = ui.available_width();
            let gap = 28.0;
            let pane = (total * 0.32).clamp(260.0, 420.0);
            let board = (height - controls_and_moves).min(pane).max(180.0);
            let text_width = total - pane - gap;
            let area = ui.available_rect_before_wrap();
            let column = |x: f32, width: f32| {
                egui::UiBuilder::new()
                    .max_rect(egui::Rect::from_min_size(
                        egui::pos2(area.left() + x, area.top()),
                        egui::vec2(width, height),
                    ))
                    .layout(egui::Layout::top_down(egui::Align::Min))
            };
            ui.scope_builder(column(0.0, text_width), |ui| {
                self.text(ui, height, settings);
            });
            // The controls and move list get the whole remaining column, not just the board's
            // width, so a short window does not cramp them.
            let pane = (total - text_width - gap).max(board);
            ui.scope_builder(column(text_width + gap, pane), |ui| {
                widgets::caption(ui, "EXPLORE THE POSITION");
                ui.add_space(8.0);
                self.position(ui, board, settings);
            });
            ui.allocate_rect(area, egui::Sense::hover());
        } else {
            // Short windows scroll the board and controls together, preserving readable
            // squares and access to the move list instead of squeezing the board to an icon.
            egui::ScrollArea::vertical()
                .id_salt(("reader-stacked", self.id))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let size = ui.available_width().min(320.0);
                    let text_height = if ui.available_width() < 500.0 {
                        (height * 0.7).clamp(260.0, 420.0)
                    } else {
                        (height * 0.45).clamp(260.0, 420.0)
                    };
                    self.text(ui, text_height, settings);
                    ui.separator();
                    ui.allocate_ui_with_layout(
                        egui::vec2(size, size + controls_and_moves),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| self.position(ui, size, settings),
                    );
                });
        }
    }
    fn toolbar(&mut self, ui: &mut Ui, settings: &mut crate::settings::Settings) {
        if let Some(notice) = &self.notice {
            let mut dismiss = false;
            widgets::card(ui).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(notice);
                    dismiss = ui.small_button("Got it").clicked();
                });
            });
            if dismiss {
                self.notice = None;
            }
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().button_padding = egui::vec2(10.0, 6.0);
            if ui
                .add_enabled(self.page > 0, egui::Button::new("‹"))
                .on_hover_text("Previous page · Page Up")
                .clicked()
            {
                self.change_page(self.page - 1);
            }
            ui.menu_button(
                format!("{} / {}", self.page + 1, self.book.document.pages.len()),
                |ui| {
                    ui.label("Go to page");
                    let mut page = self.page + 1;
                    if ui
                        .add(
                            egui::DragValue::new(&mut page)
                                .range(1..=self.book.document.pages.len().max(1)),
                        )
                        .changed()
                    {
                        self.change_page(page - 1);
                    }
                },
            );
            if ui
                .add_enabled(
                    self.page + 1 < self.book.document.pages.len(),
                    egui::Button::new("›"),
                )
                .on_hover_text("Next page · Page Down")
                .clicked()
            {
                self.change_page(self.page + 1);
            }
            self.contents_menu(ui);
            ui.menu_button("Aa", |ui| {
                ui.spacing_mut().slider_width = 160.0;
                ui.label("Reading appearance");
                ui.add(
                    egui::Slider::new(&mut settings.reader_font_size, 16..=28).text("Text size"),
                );
                ui.horizontal(|ui| {
                    use crate::settings::ReadingTheme;
                    for (theme, name) in [
                        (ReadingTheme::Auto, "Auto"),
                        (ReadingTheme::Light, "Light"),
                        (ReadingTheme::Sepia, "Sepia"),
                        (ReadingTheme::Dark, "Dark"),
                    ] {
                        ui.selectable_value(&mut settings.reader_theme, theme, name);
                    }
                });
            })
            .response
            .on_hover_text("Text size and paper color");
        });
        let fraction = (self.page + 1) as f32 / self.book.document.pages.len().max(1) as f32;
        ui.add(
            egui::ProgressBar::new(fraction)
                .desired_height(2.0)
                .desired_width(ui.available_width()),
        );
        ui.add_space(6.0);
    }
    fn contents_menu(&mut self, ui: &mut Ui) {
        if self.contents.is_empty() {
            return;
        }
        let mut jump = None;
        ui.menu_button("Contents", |ui| {
            egui::ScrollArea::vertical()
                .max_height(420.0)
                .show(ui, |ui| {
                    for (page, level, title) in &self.contents {
                        ui.horizontal(|ui| {
                            ui.add_space(f32::from(level.saturating_sub(1)) * 14.0);
                            let text = RichText::new(title);
                            let text = if *level == 1 { text.strong() } else { text };
                            if ui
                                .selectable_label(*page == self.page, text)
                                .on_hover_text(format!("Page {}", page + 1))
                                .clicked()
                            {
                                jump = Some(*page);
                                ui.close();
                            }
                        });
                    }
                });
        });
        if let Some(page) = jump {
            self.change_page(page);
        }
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
        let wide = ui.available_width() >= 700.0;
        let back = ui
            .horizontal(|ui| {
                let back = ui.button("< Library").clicked();
                if wide && let Some(reader) = &self.books.reader {
                    ui.add_space(12.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(&reader.book.document.title).font(serif(23.0, false)),
                        )
                        .truncate(),
                    );
                }
                back
            })
            .inner;
        if back {
            self.navigate(Screen::Books);
            return;
        }
        let Some(reader) = &mut self.books.reader else {
            ui.label("Open a book from your Library.");
            return;
        };
        let previous = reader.node;
        reader.keyboard(ui);
        let settings_before = self.settings.clone();
        if !wide {
            ui.add(
                egui::Label::new(
                    RichText::new(&reader.book.document.title).font(serif(23.0, false)),
                )
                .truncate(),
            );
        }
        if ui.available_width() >= 500.0 {
            widgets::caption(ui, &reader.book.document.author);
        }
        reader.toolbar(ui, &mut self.settings);
        reader.layout(ui, mode, &self.settings);
        if previous != reader.node && reader.node.is_some() && self.screenshot.is_none() {
            crate::sound::play(crate::sound::Event::Move, &self.settings);
        }
        if settings_before != self.settings {
            let settings = self.settings.clone();
            self.background(
                move |service| service.set_settings(&settings),
                |app, result| {
                    if let Err(error) = result {
                        app.error(error);
                    }
                },
            );
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
    #[test]
    fn chapters_are_the_repeated_imported_level() {
        let mut page = Page::default();
        page.heading("Book title", 1);
        page.heading("Chapter one", 2);
        page.heading("Chapter two", 2);
        page.heading("A section", 3);
        page.heading("Another section", 3);
        assert_eq!(chapter_level(std::slice::from_ref(&page)), 2);
        // Headings without a level (older imports, the demo) stay sections.
        let mut old = Page::default();
        old.push("Subtitle", BlockKind::Heading);
        old.push("Another", BlockKind::Heading);
        assert_eq!(chapter_level(&[old]), 1);
    }
    fn book() -> Book {
        let mut first = Page::default();
        first.push("1. e4 (1. d4 d5) e5", BlockKind::Paragraph);
        let mut second = Page::default();
        second.push("2. Nf3 Nc6", BlockKind::Paragraph);
        crate::books::resolve::resolve(Document {
            cover: None,
            isbn: None,
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
        assert_eq!(asset_image(svg, 1600).unwrap().size, [20, 10]);
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::new(8, 8)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        assert_eq!(asset_image(png.get_ref(), 1600).unwrap().size, [8, 8]);
        assert!(asset_image(b"broken image", 1600).is_none());
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
    fn unresolved_passages_show_no_board() {
        let mut first = Page::default();
        first.push("1. e4 e5 2. Nf3", BlockKind::Paragraph);
        let mut second = Page::default();
        second.push("14. Qh5 Kd7", BlockKind::Paragraph);
        second.push("2... Nc6", BlockKind::Paragraph);
        let book = crate::books::resolve::resolve(Document {
            cover: None,
            isbn: None,
            title: "Reader test".into(),
            author: String::new(),
            format: "epub".into(),
            pages: vec![first, second],
            assets: vec![],
        });
        let mut reader = Reader::new(1, book, None);
        reader.change_page(1);
        // The page opens on its line with moves, not on the unresolved passage before it.
        assert_eq!(reader.book.fragments[reader.fragment].nodes.len(), 1);
        let unresolved = reader
            .book
            .fragments
            .iter()
            .position(|f| matches!(f.status, Status::Unresolved(_)))
            .unwrap();
        reader.fragment = unresolved;
        reader.start();
        assert!(crate::books::resolve::position(&reader.fen).is_none());
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

    #[test]
    fn page_cache_reuses_reflow_and_evicts_its_layout_together() {
        let mut book = book();
        book.document.pages.resize(8, Page::default());
        let mut reader = Reader::new(1, book, None);
        let first = reader.page_runs();
        reader.paragraphs.insert((0, 0), None);
        reader.change_page(1);
        reader.page_runs();
        reader.change_page(0);
        assert!(Rc::ptr_eq(&first, &reader.page_runs()));
        for page in 2..8 {
            reader.change_page(page);
            reader.page_runs();
        }
        assert_eq!(reader.runs.len(), 6);
        assert!(!reader.paragraphs.contains_key(&(0, 0)));
        reader.change_page(0);
        assert!(!Rc::ptr_eq(&first, &reader.page_runs()));
    }
}
