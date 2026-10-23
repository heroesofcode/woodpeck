//! Native, responsive desktop interface over `AppService`.
mod board;
mod charts;
mod demo;
mod pieces;
mod screens;
mod shell;
mod shortcuts;
mod style;
mod widgets;
use crate::{
    puzzle::{Engine, Outcome, Puzzle, State},
    review::{ReviewCard, ReviewSort},
    service::{AppService, Event, Job, JobOutput, SetCreation, SetSummary, TrainingPosition},
    sets::BuilderParams,
    settings::Settings,
    stats::{Attempt, AttemptKind},
    theme::Palette,
    timer::{Clock, Timer, TimerConditions},
};
use eframe::egui::{self, Ui};
use std::{
    cell::Cell,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Screen {
    #[default]
    Dashboard,
    Training,
    Review,
    Progress,
    Builder,
    Settings,
}
impl Screen {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "dashboard" => Some(Self::Dashboard),
            "training" => Some(Self::Training),
            "review" => Some(Self::Review),
            "progress" => Some(Self::Progress),
            "builder" => Some(Self::Builder),
            "settings" => Some(Self::Settings),
            _ => None,
        }
    }
    const ALL: [(Self, &'static str); 6] = [
        (Self::Dashboard, "Dashboard"),
        (Self::Training, "Train"),
        (Self::Review, "Review"),
        (Self::Progress, "Progress"),
        (Self::Builder, "New set"),
        (Self::Settings, "Settings"),
    ];
}
#[derive(Clone, Debug)]
pub struct Screenshot {
    pub path: PathBuf,
    pub screen: Screen,
    pub size: [f32; 2],
}
#[derive(Clone)]
struct FrameClock(Rc<Cell<Duration>>);
impl Clock for FrameClock {
    fn now(&self) -> Duration {
        self.0.get()
    }
}
struct Session {
    puzzle: Puzzle,
    engine: Engine,
    set_id: i64,
    cycle_id: Option<i64>,
    position: usize,
    recorded: bool,
    finished_at: Option<Duration>,
}
pub struct Woodpeck {
    service: AppService,
    settings: Settings,
    screen: Screen,
    sets: Vec<SetSummary>,
    reviews: Vec<ReviewCard>,
    pending: u32,
    selected_set: Option<i64>,
    session: Option<Session>,
    board: board::BoardWidget,
    timer: Timer<FrameClock>,
    clock: FrameClock,
    origin: Instant,
    builder: BuilderParams,
    creation: Option<SetCreation>,
    theme_search: String,
    review_theme: String,
    review_sort: ReviewSort,
    status: String,
    job_fraction: f32,
    cycle_marks: Vec<Outcome>,
    toast: Option<(String, Instant)>,
    delete: Option<i64>,
    collapsed: bool,
    export_path: String,
    sync_remote: String,
    data_path: PathBuf,
    theme: style::ThemeWatcher,
    palette: Palette,
    screenshot: Option<Screenshot>,
    frames: u32,
    capture_requested: bool,
    capture_result: Option<std::sync::mpsc::Sender<Result<(), String>>>,
    clearing_card: Option<(ReviewCard, Duration)>,
}
impl Woodpeck {
    pub fn new(
        ctx: &egui::Context,
        service: AppService,
        data_path: PathBuf,
        screenshot: Option<Screenshot>,
    ) -> Self {
        let settings = service.settings().unwrap_or_default();
        let sync_remote = service.sync_status().remote;
        let origin = Instant::now();
        let theme = style::ThemeWatcher::new(settings.color_theme);
        let palette = crate::theme::select(settings.color_theme, None);
        style::fonts(ctx);
        style::apply(ctx, &palette);
        let clock = FrameClock(Rc::new(Cell::new(Duration::ZERO)));
        let screen = screenshot.as_ref().map_or(Screen::Dashboard, |s| s.screen);
        let mut builder = crate::sets::PRESETS[0].builder(settings.user_rating);
        builder.shuffle_each_cycle = settings.shuffle_each_cycle;
        let mut app = Self {
            service,
            settings,
            screen,
            sets: vec![],
            reviews: vec![],
            pending: 0,
            selected_set: None,
            session: None,
            board: board::BoardWidget::default(),
            timer: Timer::new(clock.clone()),
            clock,
            origin,
            builder,
            creation: None,
            theme_search: String::new(),
            review_theme: String::new(),
            review_sort: ReviewSort::MostRecent,
            status: "Preparing local puzzles…".into(),
            job_fraction: 0.0,
            cycle_marks: vec![],
            toast: None,
            delete: None,
            collapsed: false,
            export_path: std::env::temp_dir()
                .join("woodpeck-set.pgn")
                .display()
                .to_string(),
            sync_remote,
            data_path,
            theme,
            palette,
            screenshot,
            frames: 0,
            capture_requested: false,
            capture_result: None,
            clearing_card: None,
        };
        app.refresh();
        if matches!(screen, Screen::Training) {
            app.resume_selected();
        }
        if matches!(screen, Screen::Review) {
            app.select_first_review();
        }
        if matches!(screen, Screen::Builder) && app.screenshot.is_some() {
            // Show the theme picker in use in reproducible screenshots.
            for key in ["fork", "pin", "mate", "mate"] {
                app.builder.cycle_theme(key);
            }
            app.builder.theme_match = crate::sets::ThemeMatch::All;
        }
        app
    }
    fn now(&self) -> Duration {
        self.clock.now()
    }
    fn error(&mut self, error: impl std::fmt::Display) {
        self.toast = Some((error.to_string(), Instant::now()));
    }
    fn refresh(&mut self) {
        match self.service.list_sets() {
            Ok(sets) => self.sets = sets,
            Err(e) => self.error(e),
        }
        match self.service.review_queue(
            (!self.review_theme.is_empty()).then_some(self.review_theme.as_str()),
            self.review_sort,
        ) {
            Ok(r) => self.reviews = r,
            Err(e) => self.error(e),
        }
        self.pending = self.service.review_count().unwrap_or(0);
        self.cycle_marks = self
            .session
            .as_ref()
            .and_then(|s| s.cycle_id)
            .and_then(|id| self.service.cycle_outcomes(id).ok())
            .unwrap_or_default();
        if self
            .selected_set
            .is_none_or(|id| !self.sets.iter().any(|s| s.set.id == id))
        {
            self.selected_set = self.sets.first().map(|s| s.set.id);
        }
    }
    fn navigate(&mut self, screen: Screen) {
        self.screen = screen;
        if screen == Screen::Training && self.session.as_ref().is_none_or(|s| s.cycle_id.is_none())
        {
            self.resume_selected();
        }
        if screen == Screen::Review && self.session.as_ref().is_none_or(|s| s.cycle_id.is_some()) {
            self.select_first_review();
        }
    }
    fn load(&mut self, puzzle: Puzzle, set_id: i64, cycle_id: Option<i64>, position: usize) {
        let engine = Engine::new(&puzzle, self.now(), true);
        self.session = Some(Session {
            puzzle,
            engine,
            set_id,
            cycle_id,
            position,
            recorded: false,
            finished_at: None,
        });
        self.timer.reset();
        self.board.reset();
        self.clearing_card = None;
    }
    fn load_training(&mut self, p: TrainingPosition) {
        self.selected_set = Some(p.cycle.set_id);
        if let Some(puzzle) = p.puzzle {
            self.load(puzzle, p.cycle.set_id, Some(p.cycle.id), p.position);
        } else {
            self.session = None;
        }
        self.refresh();
    }
    fn resume_selected(&mut self) {
        if let Some(id) = self.selected_set {
            if self.sets.iter().any(|s| s.set.id == id && s.completed) {
                self.session = None;
                self.timer.reset();
                self.board.reset();
                return;
            }
            match self
                .service
                .start_or_resume_cycle(id, chrono::Local::now().timestamp())
            {
                Ok(p) => self.load_training(p),
                Err(e) => self.error(e),
            }
        }
    }
    fn select_first_review(&mut self) {
        if let Some(card) = self.reviews.first().cloned() {
            self.load(
                card.puzzle,
                card.item.set_id,
                None,
                card.position as usize - 1,
            );
        } else {
            self.session = None;
        }
    }
    fn advance(&mut self) {
        if self.screen == Screen::Review {
            self.refresh();
            self.select_first_review();
        } else if let Some(id) = self.session.as_ref().and_then(|s| s.cycle_id) {
            match self.service.current_puzzle(id) {
                Ok(p) => self.load_training(p),
                Err(e) => self.error(e),
            }
        }
    }
    fn action(&mut self, action: shortcuts::Action) {
        use shortcuts::Action;
        let now = self.now();
        match action {
            Action::Timer => self.timer.toggle_visible(),
            Action::Close => {
                self.delete = None;
                self.board.promotion = None;
            }
            Action::Advance => self.advance(),
            Action::Promote(role) => {
                if let Some(uci) = self.board.promote(role) {
                    self.play(&uci);
                }
            }
            Action::Skip => {
                if let Some(s) = &mut self.session {
                    let _ = s.engine.skip();
                }
            }
            Action::Previous | Action::NextMove => {
                if let Some(w) = self.session.as_mut().and_then(|s| s.engine.walkthrough()) {
                    if action == Action::Previous {
                        w.previous();
                    } else if w.is_complete() {
                        self.advance();
                    } else {
                        w.next();
                    }
                }
            }
        }
        self.clock.0.set(now);
    }
    fn play(&mut self, uci: &str) {
        let now = self.now();
        if let Some(s) = &mut self.session {
            let _ = s.engine.play(uci, now);
        }
    }
    fn record(&mut self) {
        let Some(s) = &self.session else {
            return;
        };
        let Some(outcome) = s.engine.outcome() else {
            return;
        };
        if s.recorded {
            return;
        }
        let attempt = Attempt {
            puzzle_id: s.puzzle.id.clone(),
            set_id: s.set_id,
            cycle_id: s.cycle_id,
            kind: if s.cycle_id.is_some() {
                AttemptKind::Cycle
            } else {
                AttemptKind::Review
            },
            outcome,
            time_ms: self.timer.recorded_ms(),
            moves_played: s.engine.moves_played().to_vec(),
            created_at: chrono::Local::now().timestamp(),
        };
        match self.service.record_attempt(&attempt) {
            Ok(_) => {
                let now = self.now();
                if let Some(s) = &mut self.session {
                    s.recorded = true;
                    s.finished_at = Some(now);
                }
                if attempt.kind == AttemptKind::Review {
                    if outcome == Outcome::Correct {
                        self.clearing_card = self
                            .reviews
                            .iter()
                            .find(|c| {
                                c.item.set_id == attempt.set_id && c.puzzle.id == attempt.puzzle_id
                            })
                            .cloned()
                            .map(|c| (c, now));
                    }
                    self.toast = Some((
                        if outcome == Outcome::Correct {
                            "Cleared from Review"
                        } else {
                            "Still pending"
                        }
                        .into(),
                        Instant::now(),
                    ));
                } else if outcome.is_miss() {
                    self.toast = Some(("Added to Review".into(), Instant::now()));
                }
                self.refresh();
            }
            Err(e) => self.error(e),
        }
    }
    fn poll(&mut self) {
        if self.service.poll_sync() {
            self.refresh();
            if let Some(session) = &self.session
                && !session.recorded
            {
                if let Some(cycle) = session.cycle_id {
                    if let Ok(position) = self.service.current_puzzle(cycle)
                        && position.puzzle.as_ref().map(|p| &p.id) != Some(&session.puzzle.id)
                    {
                        self.load_training(position);
                    }
                } else if !self.reviews.iter().any(|card| {
                    card.puzzle.id == session.puzzle.id && card.item.set_id == session.set_id
                }) {
                    self.select_first_review();
                }
            }
            if let Ok(settings) = self.service.settings()
                && settings != self.settings
            {
                self.theme = style::ThemeWatcher::new(settings.color_theme);
                self.settings = settings;
            }
        }

        for event in self.service.poll_events() {
            match event {
                Event::Started { .. } => self.status = "Working in the background…".into(),
                Event::ImportProgress { progress, .. } => {
                    self.job_fraction = (progress.bytes as f32 / 300_000_000.0).clamp(0.0, 0.99);
                    self.status = format!(
                        "{} rows scanned · {} puzzles imported · {:.1} MB",
                        progress.scanned,
                        progress.accepted,
                        progress.bytes as f64 / 1_000_000.0
                    );
                }
                Event::FetchProgress {
                    batches, inserted, ..
                } => {
                    self.status =
                        format!("Fetching from Lichess · {batches} batches · {inserted} puzzles");
                }
                Event::Finished { result, .. } => {
                    match result {
                        Ok(JobOutput::SetCreated(c)) => self.created(c),
                        Ok(_) => {
                            self.status = format!(
                                "{} puzzles available offline",
                                self.service.puzzle_count().unwrap_or(0)
                            );
                        }
                        Err(e) => {
                            self.status = e.to_string();
                            self.error(e);
                        }
                    }
                    self.refresh();
                }
            }
        }
    }
    fn created(&mut self, c: SetCreation) {
        if let Some(id) = c.set_id {
            self.selected_set = Some(id);
            self.screen = Screen::Training;
            self.resume_selected();
            if c.widening_steps > 0 {
                self.toast = Some((
                    format!(
                        "Rating band widened to {}–{} to fill your set",
                        c.used_band.0, c.used_band.1
                    ),
                    Instant::now(),
                ));
            }
        }
        self.creation = Some(c);
    }
    fn submit(&mut self, job: Job) {
        self.job_fraction = 0.0;
        if let Err(e) = self.service.submit(job) {
            self.error(e);
        }
    }
    fn keyboard(&mut self, ctx: &egui::Context) {
        use shortcuts::Key;
        let keys = [
            (egui::Key::S, Key::S),
            (egui::Key::T, Key::T),
            (egui::Key::ArrowLeft, Key::Left),
            (egui::Key::ArrowRight, Key::Right),
            (egui::Key::Space, Key::Space),
            (egui::Key::Enter, Key::Enter),
            (egui::Key::Escape, Key::Escape),
            (egui::Key::Q, Key::Q),
            (egui::Key::R, Key::R),
            (egui::Key::B, Key::B),
            (egui::Key::N, Key::N),
        ];
        let solving = matches!(self.screen, Screen::Training | Screen::Review);
        for (egui_key, key) in keys {
            if ctx.input(|i| i.key_pressed(egui_key)) {
                let walk = self
                    .session
                    .as_ref()
                    .is_some_and(|s| s.engine.state() == State::FailedReview);
                let advance = self
                    .session
                    .as_ref()
                    .is_some_and(|s| s.engine.outcome().is_some());
                let modified = ctx.input(|i| {
                    i.modifiers.alt
                        || i.modifiers.ctrl
                        || i.modifiers.mac_cmd
                        || i.modifiers.command
                });
                let blocked = !solving || self.delete.is_some() || egui::Popup::is_any_open(ctx);
                if let Some(action) = shortcuts::dispatch(
                    key,
                    shortcuts::Context {
                        editing: ctx.egui_wants_keyboard_input(),
                        modified,
                        blocked,
                        walkthrough: walk,
                        advance,
                        promotion: self.board.promotion.is_some(),
                    },
                ) {
                    self.action(action);
                }
            }
        }
    }
}
impl eframe::App for Woodpeck {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.clock.0.set(self.origin.elapsed());
        let now = self.now();
        if let Some(p) = self.theme.poll() {
            style::apply(ctx, &p);
            self.palette = p;
        }
        self.poll();
        let solving = matches!(self.screen, Screen::Training | Screen::Review);
        let conditions = ctx.input(|i| TimerConditions {
            player_turn: self
                .session
                .as_ref()
                .is_some_and(|s| s.engine.state() == State::PlayerTurn),
            interactive: solving && self.delete.is_none(),
            focused: i.viewport().focused.unwrap_or(i.focused),
            minimized: i.viewport().minimized.unwrap_or(false),
            promotion_open: self.board.promotion.is_some(),
        });
        self.timer.set_conditions(conditions);
        if solving {
            if let Some(s) = &mut self.session {
                s.engine.update(now);
            }
            self.record();
        }
        if self.settings.auto_advance
            && solving
            && self.session.as_ref().is_some_and(|s| {
                s.engine.state() == State::Complete
                    && s.finished_at
                        .is_some_and(|t| now.saturating_sub(t) > Duration::from_millis(650))
            })
        {
            self.advance();
        }
        ctx.request_repaint_after(if solving || self.service.is_busy() {
            Duration::from_millis(100)
        } else {
            Duration::from_secs(1)
        });
    }
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}
pub fn run(screenshot: Option<Screenshot>) -> Result<(), Box<dyn std::error::Error>> {
    let path = AppService::default_database_path().ok_or("Could not locate data directory")?;
    let service = if screenshot.is_some() {
        demo::service()?
    } else {
        AppService::open(&path)?
    };
    let sync_shutdown = service.sync_shutdown();
    let capture = screenshot.is_some();
    let (sender, receiver) = std::sync::mpsc::channel();
    let size = screenshot.as_ref().map_or([1280.0, 820.0], |s| s.size);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("woodpeck")
            .with_app_id("woodpeck")
            .with_inner_size(size)
            .with_min_inner_size([340.0, 480.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "woodpeck",
        options,
        Box::new(move |cc| {
            let mut app = Woodpeck::new(&cc.egui_ctx, service, path, screenshot);
            app.capture_result = Some(sender);
            Ok(Box::new(app))
        }),
    );
    if let Some(shutdown) = sync_shutdown {
        shutdown.finish();
    }
    result?;
    if capture {
        receiver
            .try_recv()
            .map_err(|_| "Window closed before the screenshot was saved")?
            .map_err(std::io::Error::other)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
