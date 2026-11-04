//! Local book collection and cancellable import presentation.
use super::{Screen, Woodpeck, reader::Reader, widgets};
#[cfg(not(target_arch = "wasm32"))]
use crate::books::import::{PageSource, TextImporter};
use crate::{
    books::{import::ImportError, model::Book},
    source::CancelFlag,
    store::ReadingProgress,
};
use eframe::egui::{self, Ui};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
        mpsc,
    },
};

/// Asks the system file chooser for a book: the XDG desktop portal on Linux (Hyprland, GNOME,
/// KDE…), with zenity as a fallback, and the native dialogs on macOS and Windows. `None` means
/// cancelled or no chooser available; the path field stays as the fallback either way.
///
/// The dialog is created on the UI thread (macOS requires it) but awaited on a worker, so no
/// modal loop ever runs inside the window's event handling.
#[cfg(not(any(test, target_arch = "wasm32")))]
fn pick_book() -> impl std::future::Future<Output = Option<PathBuf>> + Send + 'static {
    let chosen = rfd::AsyncFileDialog::new()
        .set_title("Import a chess book")
        .add_filter("EPUB or PDF", &["epub", "pdf", "EPUB", "PDF"])
        .pick_file();
    async move { chosen.await.map(|file| file.path().to_path_buf()) }
}
#[cfg(any(test, target_arch = "wasm32"))]
fn pick_book() -> impl std::future::Future<Output = Option<PathBuf>> + Send + 'static {
    std::future::ready(None)
}

struct ImportTask {
    cancel: CancelFlag,
    result: mpsc::Receiver<Result<(Book, String), String>>,
    stage: Arc<AtomicU8>,
}

pub(super) type Entry = (i64, Book, Option<ReadingProgress>);
#[derive(Default)]
pub(super) struct Library {
    pub entries: Vec<Entry>,
    pub reader: Option<Reader>,
    path: String,
    choosing: bool,
    picking: Option<mpsc::Receiver<Option<PathBuf>>>,
    deleting: Option<i64>,
    import: Option<ImportTask>,
    message: String,
    pub covers: super::covers::Covers,
}
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(super) fn import_error(error: &ImportError) -> String {
    match error {
        ImportError::NoTextLayer => {
            "This PDF has no text layer (it looks scanned). Scanned books aren't supported yet."
                .into()
        }
        ImportError::Invalid(_) => {
            format!("Couldn't read this book. It may be damaged or DRM-protected. {error}")
        }
        _ => error.to_string(),
    }
}
impl Woodpeck {
    pub(super) fn poll_books(&mut self, ctx: &egui::Context) {
        if let Some(picking) = &self.books.picking {
            match picking.try_recv() {
                Ok(Some(path)) => {
                    self.books.picking = None;
                    self.books.path = path.display().to_string();
                    self.start_import(ctx, path);
                }
                Ok(None) | Err(mpsc::TryRecvError::Disconnected) => self.books.picking = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let result = self
            .books
            .import
            .as_ref()
            .and_then(|task| match task.result.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                    "Book import stopped unexpectedly. Please try again.".into(),
                )),
            });
        if let Some(result) = result {
            let task = self.books.import.take().unwrap();
            let cancel = task.cancel;
            if !cancel.is_cancelled() {
                match result {
                    Ok((book, hash)) => self.background(
                        move |service| {
                            let id = service.save_book(&book, &hash)?;
                            Ok((id, service.books()?))
                        },
                        |app, result| match result {
                            Ok((id, entries)) => {
                                let entry = entries.iter().find(|e| e.0 == id).cloned();
                                app.books.entries = entries;
                                app.books.choosing = false;
                                if let Some(entry) = entry {
                                    let summary = entry.1.summary();
                                    app.open_book(entry);
                                    if let Some(reader) = &mut app.books.reader {
                                        reader.notice = Some(import_summary(summary));
                                    }
                                }
                            }
                            Err(error) => app.books.message = error.to_string(),
                        },
                    ),
                    Err(error) => self.books.message = error,
                }
            }
        }
    }
    pub(super) fn refresh_books(&mut self, open: bool) {
        self.background(
            crate::service::LocalService::books,
            move |app, result| match result {
                Ok(entries) => {
                    app.books.entries = entries;
                    if open && let Some(entry) = app.books.entries.first().cloned() {
                        app.open_book(entry);
                    }
                }
                Err(error) => app.error(error),
            },
        );
    }
    fn open_book(&mut self, (id, book, progress): Entry) {
        if self
            .books
            .reader
            .as_ref()
            .is_some_and(|reader| reader.id() == id)
        {
            self.screen = Screen::Reader;
            return;
        }
        self.screen = Screen::Books;
        self.save_reader();
        let mut reader = Reader::new(id, book, progress);
        if self
            .screenshot
            .as_ref()
            .is_some_and(|s| s.screen == Screen::ReaderVariations)
        {
            reader.preview_variations();
        }
        self.books.reader = Some(reader);
        self.screen = Screen::Reader;
    }
    fn choose_book(&mut self, ctx: &egui::Context) {
        self.books.choosing = true;
        if self.books.picking.is_some() || self.screenshot.is_some() {
            return;
        }
        let (send, receive) = mpsc::channel();
        let ctx = ctx.clone();
        let chosen = pick_book();
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            let _ = send.send(pollster::block_on(chosen));
            ctx.request_repaint();
        });
        #[cfg(target_arch = "wasm32")]
        {
            drop((chosen, ctx));
            let _ = send.send(None);
        }
        self.books.picking = Some(receive);
    }
    fn start_import(&mut self, ctx: &egui::Context, path: PathBuf) {
        if self.books.import.is_some() {
            return;
        }
        let cancel = CancelFlag::default();
        let flag = cancel.clone();
        let (send, receive) = mpsc::channel();
        let stage = Arc::new(AtomicU8::new(0));
        let worker_stage = stage.clone();
        let ctx = ctx.clone();
        #[cfg(target_arch = "wasm32")]
        {
            drop((path, flag, worker_stage, ctx));
            let _ = send.send(Err("Books are not available in the web version yet.".into()));
        }
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            use sha2::{Digest, Sha256};
            let result = (|| {
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                flag.check().map_err(|e| e.to_string())?;
                worker_stage.store(1, Ordering::Relaxed);
                ctx.request_repaint();
                let document = TextImporter.document(&path).map_err(|e| import_error(&e))?;
                flag.check().map_err(|e| e.to_string())?;
                worker_stage.store(2, Ordering::Relaxed);
                ctx.request_repaint();
                let book = crate::books::resolve::resolve(document);
                flag.check().map_err(|e| e.to_string())?;
                Ok((book, format!("{:x}", Sha256::digest(bytes))))
            })();
            let _ = send.send(result);
            ctx.request_repaint();
        });
        self.books.import = Some(ImportTask {
            cancel,
            result: receive,
            stage,
        });
        self.books.message.clear();
    }
    fn book_import(&mut self, ui: &mut Ui) {
        if ui.button("Import book…").clicked() {
            self.choose_book(ui.ctx());
        }
        if self.books.picking.is_some() {
            ui.weak("Choose a book in the file dialog…");
        }
        if self.books.choosing {
            let mut import = false;
            widgets::card(ui).show(ui, |ui| {
                ui.label("Or type the EPUB or PDF file path");
                let field = ui.text_edit_singleline(&mut self.books.path);
                ui.small(
                    "Text-layer books only. DRM-protected and scanned books aren't supported.",
                );
                let ready = self.books.import.is_none() && !self.books.path.trim().is_empty();
                import = ui.add_enabled(ready, egui::Button::new("Import")).clicked()
                    || (ready
                        && field.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            });
            if import {
                let path = PathBuf::from(self.books.path.trim());
                self.start_import(ui.ctx(), path);
            }
        }
        if let Some(task) = &self.books.import {
            let stage = task.stage.load(Ordering::Relaxed);
            ui.add(
                egui::ProgressBar::new(f32::from(stage + 1) / 4.0)
                    .animate(true)
                    .text(match stage {
                        0 => "Reading file…",
                        1 => "Extracting pages…",
                        _ => "Resolving chess positions…",
                    }),
            );
            if task.cancel.is_cancelled() {
                ui.weak("Cancelling…");
            } else if ui.button("Cancel").clicked() {
                task.cancel.cancel();
            }
        }
    }
    /// One book on the shelf: cover, title, author, what was found in it and reading
    /// progress. Returns true when the reader should open it.
    fn book_card(&mut self, ui: &mut Ui, entry: &Entry, compact: bool) -> bool {
        if !compact {
            let open = self
                .books
                .covers
                .show(ui, entry.0, &entry.1, super::covers::COVER)
                .on_hover_text("Open book")
                .clicked();
            ui.add_space(8.0);
            return self.book_details(ui, entry, false) | open;
        }
        // Narrow windows: a small cover beside the details keeps the actions in view.
        ui.horizontal_top(|ui| {
            let open = self
                .books
                .covers
                .show(ui, entry.0, &entry.1, super::covers::COVER * 0.5)
                .on_hover_text("Open book")
                .clicked();
            ui.vertical(|ui| self.book_details(ui, entry, true)).inner | open
        })
        .inner
    }
    fn book_details(&mut self, ui: &mut Ui, (id, book, progress): &Entry, compact: bool) -> bool {
        let mut open = false;
        ui.add(egui::Label::new(egui::RichText::new(&book.document.title).strong()).truncate())
            .on_hover_text(&book.document.title);
        if !book.document.author.is_empty() {
            ui.add(egui::Label::new(egui::RichText::new(&book.document.author).weak()).truncate());
        }
        if compact {
            ui.horizontal(|ui| {
                open |= ui.button("Open book >").clicked();
                if ui.small_button("Delete…").clicked() {
                    self.books.deleting = Some(*id);
                }
            });
        }
        ui.small(format!(
            "{} · {} pages",
            book.document.format.to_uppercase(),
            book.document.pages.len(),
        ));
        ui.small(card_summary(book.summary()));
        let pages = book.document.pages.len().max(1) as f32;
        let (fraction, status) = progress
            .as_ref()
            .map_or((0.0, "Ready to read".to_owned()), |p| {
                (
                    (p.page_index + 1) as f32 / pages,
                    format!(
                        "Page {} of {} · {}",
                        p.page_index + 1,
                        book.document.pages.len(),
                        chrono::DateTime::from_timestamp(p.updated_at, 0)
                            .map_or_else(String::new, |d| d.format("%b %d").to_string())
                    ),
                )
            });
        ui.small(status);
        ui.add(
            egui::ProgressBar::new(fraction)
                .desired_width(ui.available_width())
                .desired_height(4.0),
        );
        if !compact {
            ui.horizontal(|ui| {
                open |= ui.button("Open book >").clicked();
                if ui.small_button("Delete…").clicked() {
                    self.books.deleting = Some(*id);
                }
            });
        }
        open
    }
    pub(super) fn books_screen(&mut self, ui: &mut Ui) {
        widgets::caption(ui, "LIBRARY / READ & EXPLORE");
        ui.heading("Your chess bookshelf");
        ui.label("Read at your own pace. Follow every line on the board.");
        ui.add_space(16.0);
        self.book_import(ui);
        if !self.books.message.is_empty() {
            ui.label(&self.books.message);
        }
        ui.add_space(20.0);
        if self.books.entries.is_empty() {
            ui.label("Your books will appear here after importing.");
        }
        self.update_covers(ui.ctx());
        let mut open = None;
        // A grid of book cards: as many columns as fit, each a cover with its details.
        let gap = 18.0;
        let width = super::covers::COVER.x + 24.0;
        let columns = (((ui.available_width() + gap) / (width + gap)).floor() as usize).max(1);
        let compact = columns < 2;
        let width = if compact { ui.available_width() } else { width };
        let entries = std::mem::take(&mut self.books.entries);
        for row in entries.chunks(columns) {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for entry in row {
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(width);
                            if self.book_card(ui, entry, compact) {
                                open = Some(entry.clone());
                            }
                        },
                    );
                }
            });
            ui.add_space(gap);
        }
        self.books.entries = entries;
        if let Some(entry) = open {
            self.open_book(entry);
        }
        if let Some(id) = self.books.deleting {
            egui::Modal::new(egui::Id::unique("delete-book")).show(ui.ctx(), |ui| {
                ui.heading("Delete this book?");
                ui.label("The book and its reading progress will be removed.");
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.books.deleting = None;
                    }
                    if ui.button("Delete book").clicked() {
                        self.books.deleting = None;
                        self.books.covers.forget(id);
                        self.background(
                            move |s| s.delete_book(id),
                            |app, result| match result {
                                Ok(()) => app.refresh_books(false),
                                Err(e) => app.error(e),
                            },
                        );
                    }
                });
            });
        }
    }
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}
/// The short line on a library card: what the board can show and what is left unchecked.
fn card_summary(summary: crate::books::model::Summary) -> String {
    let positions = count(summary.positions, "position", "positions");
    if summary.unresolved == 0 {
        positions
    } else {
        format!("{positions} · {} unresolved", summary.unresolved)
    }
}
/// What an import found, in plain words, shown when the new book opens.
pub(super) fn import_summary(summary: crate::books::model::Summary) -> String {
    let positions = count(summary.positions, "position", "positions");
    let unresolved = count(summary.unresolved, "passage", "passages");
    match (summary.positions, summary.unresolved) {
        (0, 0) => "No chess moves or diagrams were found in this book.".to_owned(),
        (_, 0) => {
            format!("Found {positions}, all checked. Click any move or diagram to set the board.")
        }
        (0, _) => format!(
            "No position could be checked. {unresolved} with moves are marked ··· in the text; they never move the board."
        ),
        _ => format!(
            "Found {positions}. {unresolved} could not be checked: they are marked ··· in the text and never move the board."
        ),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_summaries_read_plainly() {
        use crate::books::model::Summary;
        let found = |positions, unresolved| Summary {
            positions,
            passages: 10,
            unresolved,
        };
        assert_eq!(
            import_summary(found(312, 14)),
            "Found 312 positions. 14 passages could not be checked: they are marked ··· in the text and never move the board."
        );
        assert_eq!(
            import_summary(found(1, 0)),
            "Found 1 position, all checked. Click any move or diagram to set the board."
        );
        assert!(import_summary(found(0, 0)).starts_with("No chess moves"));
        assert!(import_summary(found(0, 1)).contains("1 passage with moves"));
        assert_eq!(
            card_summary(found(312, 14)),
            "312 positions · 14 unresolved"
        );
        assert_eq!(card_summary(found(5, 0)), "5 positions");
    }
    #[test]
    fn readable_import_errors() {
        assert_eq!(
            import_error(&ImportError::NoTextLayer),
            "This PDF has no text layer (it looks scanned). Scanned books aren't supported yet."
        );
        assert!(import_error(&ImportError::Unsupported).contains("EPUB or PDF"));
        assert!(
            import_error(&ImportError::Invalid("broken archive".into())).contains("DRM-protected")
        );
        assert!(
            import_error(&ImportError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "File not found"
            )))
            .contains("File not found")
        );
    }
}
