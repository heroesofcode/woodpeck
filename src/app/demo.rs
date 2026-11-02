//! Reproducible screenshot data uses an isolated SQLite database.
use crate::{
    puzzle::Outcome,
    service::AppService,
    sets::{BuilderParams, generate},
    source::{CancelFlag, import_starter},
    stats::{Attempt, AttemptKind},
    store::Store,
};
pub fn service() -> Result<AppService, Box<dyn std::error::Error>> {
    let mut store = Store::memory()?;
    import_starter(&mut store, false, &CancelFlag::default(), |_| {})?;
    let params = BuilderParams {
        name: "Pattern practice".into(),
        preset_id: None,
        size: 20,
        target_cycles: 5,
        ..Default::default()
    };
    let generated = generate(&store, &params, 42)?;
    let now = chrono::Local::now().timestamp();
    let id = store.create_set(&params, &generated.puzzles, now - 86_400 * 7, 42)?;
    for number in 1..=3 {
        let cycle = store.start_cycle(id, now - 86_400 * (4 - number))?;
        for (i, puzzle) in generated
            .puzzles
            .iter()
            .take(if number == 3 { 7 } else { 20 })
            .enumerate()
        {
            store.record_attempt(&Attempt {
                puzzle_id: puzzle.id.clone(),
                set_id: id,
                cycle_id: Some(cycle.id),
                kind: AttemptKind::Cycle,
                outcome: if (i + number as usize).is_multiple_of(6) {
                    Outcome::Incorrect
                } else if i == 4 && number == 1 {
                    Outcome::Skipped
                } else {
                    Outcome::Correct
                },
                time_ms: ((26_000 / number) as u64) + (i as u64 * 321),
                moves_played: vec![],
                created_at: now - 86_400 * (4 - number) + i64::try_from(i)?,
            })?;
        }
    }
    let book = book();
    let book_id = store.save_book(&book, "demo-open-games-v1", now)?;
    store.save_reading_progress(
        book_id,
        &crate::store::ReadingProgress {
            page_index: 0,
            node_id: Some(5),
            updated_at: now,
        },
    )?;
    for (index, (title, text)) in [
        (
            "The Queen’s Pawn Centre",
            "1. d4 d5 2. c4 e6 3. Nc3 Nf6 4. Bg5 Be7",
        ),
        (
            "Develop with a Purpose",
            "1. Nf3 d5 2. g3 Nf6 3. Bg2 e6 4. O-O Be7",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        use crate::books::document::{BlockKind, Document, Page};
        let mut page = Page::default();
        page.push(title, BlockKind::Heading);
        page.push(
            "Use these short lines to study the centre and the activity of each piece.",
            BlockKind::Paragraph,
        );
        page.push(text, BlockKind::Paragraph);
        let book = crate::books::resolve::resolve(Document {
            title: title.into(),
            author: "Woodpeck study notes".into(),
            format: "epub".into(),
            pages: vec![page],
            assets: vec![],
        });
        store.save_book(&book, &format!("demo-study-{index}"), now - 3600)?;
    }
    Ok(AppService::new(store)?)
}

/// Stable, offline states for fixed-frame welcome screenshots.
pub(super) fn welcome(app: &mut super::Woodpeck, screen: super::Screen) {
    use super::{Screen, library::WelcomeStep};
    use crate::source::{ImportProgress, LibrarySize};
    let accepted = 116_480;
    let profile = LibrarySize::Medium.histogram();
    let total = profile.iter().sum::<u64>();
    let mut bands = profile.map(|count| count * accepted / total);
    bands[5] += accepted - bands.iter().sum::<u64>();
    let progress = ImportProgress {
        bytes: 15_340_000,
        scanned: 307_000,
        accepted,
        rejected: 190_520,
        bands,
    };
    if matches!(
        screen,
        Screen::Welcome | Screen::WelcomeProgress | Screen::WelcomeDone
    ) {
        app.screen = Screen::Welcome;
        app.library.selected = LibrarySize::Medium;
        app.library.step = match screen {
            Screen::WelcomeProgress => WelcomeStep::Downloading,
            Screen::WelcomeDone => WelcomeStep::Done,
            _ => WelcomeStep::Choose,
        };
        app.library.progress = progress;
        if screen == Screen::WelcomeDone {
            app.library.count = 200_000;
            app.library.bands = *LibrarySize::Medium.histogram();
            app.library.progress.accepted = 200_000;
            app.library.progress.scanned = 527_077;
            app.library.progress.bytes = LibrarySize::Medium.download_bytes();
        }
    }
}

/// Drive the real engine through the solution before capturing solved feedback.
pub(super) fn solve(puzzle: &crate::puzzle::Puzzle, engine: &mut crate::puzzle::Engine) {
    let mut time = std::time::Duration::from_secs(60);
    for uci in puzzle.moves.split_whitespace().skip(1).step_by(2) {
        engine.update(time);
        let _ = engine.play(uci, time);
        time += std::time::Duration::from_secs(1);
    }
    engine.update(time);
}

/// Original instructional text, generated entirely in memory.
fn book() -> crate::books::model::Book {
    use crate::books::document::{BlockKind, Document, Page};
    let mut first = Page::default();
    first.push("1 · The open game", BlockKind::Heading);
    first.push("A small guide to active pieces", BlockKind::Heading);
    first.push("The first moves are a conversation about the centre. Place a pawn where it makes room for your pieces, then develop with a purpose.", BlockKind::Paragraph);
    first.push(
        "1. e4 e5 2. Nf3 Nc6 3. Bc4 (3. Bb5 a6 4. Ba4) Bc5 4. c3 Nf6",
        BlockKind::Paragraph,
    );
    first.push("White prepares a broad pawn centre. Before advancing, ask which piece still needs a useful square. The quiet move can carry the strongest idea.", BlockKind::Paragraph);
    first.push("Look at the alternatives", BlockKind::Heading);
    first.push("The bishop can aim at f7 or put pressure on the knight defending e5. Select a move to compare the positions; use the arrows to follow a line.", BlockKind::Paragraph);
    let mut second = Page::default();
    second.push("2 · Build before you attack", BlockKind::Heading);
    second.push("5. d3 d6 6. O-O O-O", BlockKind::Paragraph);
    second.push("Both kings are safe. Now improve the least active piece and watch for a chance to open the centre.", BlockKind::Paragraph);
    crate::books::resolve::resolve(Document {
        title: "First Steps in the Open Game".into(),
        author: "Woodpeck study notes".into(),
        format: "epub".into(),
        pages: vec![first, second],
        assets: vec![],
    })
}
