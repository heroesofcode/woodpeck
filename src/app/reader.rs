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
use std::{
    rc::Rc,
    time::{Duration, Instant},
};

/// Widest a line of book text is allowed to grow, in points (about 75 characters).
const MEASURE: f32 = 720.0;
const BODY_SIZE: f32 = 18.0;
/// Leading for body text, about 1.5 times the type size.
const LINE_HEIGHT: f32 = 27.0;

/// The page on screen: text runs (clickable moves or prose) and the fragments that could
/// not be followed, rebuilt only when the page changes.
struct PageRuns {
    runs: Vec<(std::ops::Range<usize>, Option<usize>)>,
    unresolved: Vec<(std::ops::Range<usize>, String)>,
}

/// Bold face for headings; it falls back to the regular face when the system has no bold.
fn bold(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name("bold".into()))
}

/// Text set in capitals throughout, which reads better letter-spaced at a smaller size.
fn all_caps(text: &str) -> bool {
    let letters = text.chars().filter(|c| c.is_alphabetic()).count();
    letters >= 4
        && text.chars().all(|c| !c.is_lowercase())
        && text
            .split(|c: char| !c.is_alphabetic())
            .any(|word| word.chars().count() >= 4)
}

/// One run of text in the given face, letter-spaced when it is set in capitals.
fn styled(
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    wrap: f32,
) -> egui::text::LayoutJob {
    let caps = all_caps(text);
    let size = font.size;
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            extra_letter_spacing: if caps { size * 0.08 } else { 0.0 },
            line_height: Some(size * 1.3),
            font_id: font,
            color,
            ..Default::default()
        },
    );
    job.wrap.max_width = wrap;
    job
}

/// A heading in the type scale. `rank` 0 is the book's top level (chapters), which opens
/// with a break; the levels below step down in size so sections stand apart from text.
fn heading(ui: &mut Ui, text: &str, rank: u8, first: bool, after_heading: bool) {
    let text = text.trim();
    if !first && !after_heading {
        if rank == 0 {
            ui.add_space(44.0);
            ui.separator();
            ui.add_space(36.0);
        } else {
            ui.add_space(if rank == 1 { 26.0 } else { 14.0 });
        }
    } else if first && rank == 0 {
        ui.add_space(28.0);
    }
    let size = heading_size(rank)
        * if rank >= 2 && all_caps(text) {
            0.9
        } else {
            1.0
        };
    let color = ui.visuals().strong_text_color();
    let job = styled(text, bold(size), color, ui.available_width());
    embolden(ui, job);
    if rank == 0 {
        // A short accent rule under chapter titles marks the chapter opening.
        ui.add_space(10.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(64.0, 3.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 1.5, ui.visuals().selection.stroke.color);
        ui.add_space(6.0);
    }
}

/// Lays out a heading and strikes it twice, a hair apart, so it reads heavier even when the
/// system only has a variable or regular face (egui cannot pick a weight from those).
fn embolden(ui: &mut Ui, job: egui::text::LayoutJob) {
    let size = job
        .sections
        .first()
        .map_or(BODY_SIZE, |s| s.format.font_id.size);
    let galley = ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));
    let (rect, _) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let color = ui.visuals().strong_text_color();
        let painter = ui.painter();
        painter.galley(rect.min, galley.clone(), color);
        painter.galley(rect.min + egui::vec2(size * 0.035, 0.0), galley, color);
    }
}

/// Type size for a heading rank, from chapter titles down to minor headings.
fn heading_size(rank: u8) -> f32 {
    match rank {
        0 => 40.0,
        1 => 29.0,
        2 => 23.0,
        _ => 20.0,
    }
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
    runs: Option<(usize, Rc<PageRuns>)>,
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
            runs: None,
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
        if let Some((page, runs)) = &self.runs
            && *page == self.page
        {
            return runs.clone();
        }
        let runs = Rc::new(PageRuns {
            runs: self
                .book
                .text_runs(self.page)
                .unwrap_or_default()
                .into_iter()
                .map(|run| (run.span, run.node_id))
                .collect(),
            unresolved: self
                .book
                .fragments
                .iter()
                .filter(|f| f.page == self.page)
                .filter_map(|f| match &f.status {
                    Status::Partial(reason) | Status::Unresolved(reason) => {
                        Some((f.span.clone(), reason.clone()))
                    }
                    Status::Resolved => None,
                })
                .collect(),
        });
        self.runs = Some((self.page, runs.clone()));
        runs
    }
    fn paragraph(&mut self, ui: &mut Ui, text: &str, span: &std::ops::Range<usize>, follow: bool) {
        let runs = self.page_runs();
        let plain = text.get(span.clone()).unwrap_or_default().trim();
        if all_caps(plain)
            && !runs
                .runs
                .iter()
                .any(|(r, n)| n.is_some() && r.start < span.end && r.end > span.start)
            && !runs
                .unresolved
                .iter()
                .any(|(r, _)| r.start < span.end && r.end > span.start)
        {
            // Display lines set in capitals (summaries, captions, run-in heads) read as
            // letter-spaced small capitals rather than as shouted body text.
            let short = plain.len() <= 80 && plain.split_whitespace().count() >= 3;
            let (font, color) = if short {
                (bold(BODY_SIZE * 0.9), ui.visuals().strong_text_color())
            } else {
                (
                    egui::FontId::proportional(BODY_SIZE * 0.82),
                    ui.visuals().weak_text_color(),
                )
            };
            let job = styled(plain, font, color, ui.available_width());
            if short {
                embolden(ui, job);
            } else {
                ui.add(egui::Label::new(job).wrap());
            }
            return;
        }
        let mut selected = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, LINE_HEIGHT - BODY_SIZE * 1.2);
            ui.spacing_mut().button_padding = egui::vec2(1.0, 0.0);
            ui.spacing_mut().interact_size.y = 0.0;
            for (run, node) in &runs.runs {
                let start = run.start.max(span.start);
                let end = run.end.min(span.end);
                if start >= end {
                    continue;
                }
                let text = &text[start..end];
                if let Some(id) = *node {
                    let response = ui.selectable_label(
                        self.node == Some(id),
                        RichText::new(text)
                            .size(BODY_SIZE)
                            .line_height(Some(LINE_HEIGHT))
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
                    let reason = runs
                        .unresolved
                        .iter()
                        .find(|(f, _)| f.start < end && f.end > start)
                        .map(|(_, reason)| reason);
                    let rich = RichText::new(text)
                        .size(BODY_SIZE)
                        .line_height(Some(LINE_HEIGHT));
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
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width().min(MEASURE));
                let blocks = if page.blocks.is_empty() {
                    vec![crate::books::document::Block {
                        span: 0..page.text.len(),
                        kind: BlockKind::Paragraph,
                        level: 0,
                    }]
                } else {
                    page.blocks.clone()
                };
                let mut previous_heading = false;
                for (index, block) in blocks.into_iter().enumerate() {
                    match &block.kind {
                        BlockKind::Image { path, alt } => {
                            if !self.textures.contains_key(path)
                                && let Some(asset) =
                                    self.book.document.assets.iter().find(|a| &a.path == path)
                                && let Some(color) = asset_image(&asset.bytes, 1600)
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
                        BlockKind::Diagram { fen } | BlockKind::RecognizedDiagram { fen, .. } => {
                            if matches!(block.kind, BlockKind::RecognizedDiagram { .. }) {
                                ui.weak("Position read from the book's diagram image");
                            }
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
                        BlockKind::Heading => heading(
                            ui,
                            page.text.get(block.span.clone()).unwrap_or_default(),
                            block.heading_level().saturating_sub(self.top_level),
                            index == 0,
                            previous_heading,
                        ),
                        _ => {
                            self.paragraph(ui, &page.text, &block.span, follow);
                        }
                    }
                    previous_heading = block.kind == BlockKind::Heading;
                    ui.add_space(if previous_heading { 10.0 } else { 14.0 });
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
impl Reader {
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
        if let Some(notice) = &reader.notice {
            let mut dismiss = false;
            widgets::card(ui).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(notice);
                    dismiss = ui.small_button("Got it").clicked();
                });
            });
            if dismiss {
                reader.notice = None;
            }
        }
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
            reader.contents_menu(ui);
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
            // The board takes what the height allows, up to about half the width; the text
            // keeps the rest and never grows past a comfortable line length.
            let total = ui.available_width();
            let gap = 24.0;
            let board = (height - controls_and_moves).min(total * 0.52).max(80.0);
            let text_width = (total - board - gap).clamp(200.0, MEASURE + 16.0);
            let area = ui.available_rect_before_wrap();
            let column = |x: f32, width: f32| {
                egui::UiBuilder::new()
                    .max_rect(egui::Rect::from_min_size(
                        egui::pos2(area.left() + x, area.top()),
                        egui::vec2(width, height),
                    ))
                    .layout(egui::Layout::top_down(egui::Align::Min))
            };
            ui.scope_builder(column(0.0, text_width), |ui| reader.text(ui, height));
            // The controls and move list get the whole remaining column, not just the board's
            // width, so a short window does not cramp them.
            let pane = (total - text_width - gap).max(board);
            ui.scope_builder(column(text_width + gap, pane), |ui| {
                reader.position(ui, board, &self.settings);
            });
            ui.allocate_rect(area, egui::Sense::hover());
        } else {
            // Stacked: a board big enough to read, and the rest of the height for the text.
            let reserve = controls_and_moves - 150.0;
            let size = ui
                .available_width()
                .min(height * 0.42)
                .min(420.0)
                .min(height - reserve - 100.0)
                .max(80.0);
            let text_height = (height - size - reserve).max(80.0);
            reader.text(ui, text_height);
            ui.separator();
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
    #[test]
    fn capitals_are_display_text_but_notation_is_not() {
        assert!(all_caps("DESCRIPTION OF THE CHESS-BOARD AND MEN."));
        assert!(all_caps("CHAPTER I."));
        assert!(!all_caps("Chapter I."));
        assert!(!all_caps("P-K4 P-K4"));
        assert!(!all_caps("1. R - K 4 K R - K 1"));
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
}
