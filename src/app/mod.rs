//! Native, responsive desktop interface over `AppService`.
mod background;
#[doc(hidden)]
pub mod bench_support;
mod board;
mod charts;
mod demo;
mod frame_stats;
mod library;
mod pieces;
mod screens;
mod shell;
mod shortcuts;
mod style;
mod widgets;
use crate::{
    puzzle::{Engine, Outcome, Puzzle, State},
    review::{ReviewCard, ReviewSort},
    service::{AppService, Event, Job, JobOutput, SetCreation, SetSummary},
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Screen {
    #[default]
    Dashboard,
    Training,
    TrainingSolved,
    Review,
    Progress,
    Builder,
    Settings,
    Welcome,
    WelcomeProgress,
    WelcomeDone,
}
impl Screen {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "dashboard" => Some(Self::Dashboard),
            "training" => Some(Self::Training),
            "training-solved" => Some(Self::TrainingSolved),
            "review" => Some(Self::Review),
            "progress" => Some(Self::Progress),
            "builder" => Some(Self::Builder),
            "settings" => Some(Self::Settings),
            "welcome" => Some(Self::Welcome),
            "welcome-progress" => Some(Self::WelcomeProgress),
            "welcome-done" => Some(Self::WelcomeDone),
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
    ctx: egui::Context,
    background: Vec<background::Pending>,
    loading: bool,
    creating: bool,
    library_dirty: bool,
    load_generation: u64,
    saving: bool,
    record_error: bool,
    advance_pending: bool,
    prepared: Option<background::PreparedTraining>,
    all_reviews: Vec<ReviewCard>,
    motivation: crate::motivation::Motivation,
    progress: std::collections::BTreeMap<i64, crate::service::SetProgress>,
    library: library::Library,
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
    library_scroll_top: bool,
    theme_search: String,
    review_theme: String,
    review_sort: ReviewSort,
    status: String,
    status_generation: u64,
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
    frame_stats: frame_stats::FrameStats,
    frame_started: Instant,
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
        let wake_ctx = ctx.clone();
        service.set_waker(move || wake_ctx.request_repaint());
        let settings = Settings::default();
        let sync_remote = service.sync_status().remote;
        let origin = Instant::now();
        let theme = style::ThemeWatcher::new(settings.color_theme, ctx.clone());
        let palette = crate::theme::select(settings.color_theme, None);
        style::fonts(ctx);
        style::apply(ctx, &palette);
        let clock = FrameClock(Rc::new(Cell::new(Duration::ZERO)));
        let screen = screenshot.as_ref().map_or(Screen::Dashboard, |s| s.screen);
        let mut builder = crate::sets::PRESETS[0].builder(settings.user_rating);
        builder.shuffle_each_cycle = settings.shuffle_each_cycle;
        let mut app = Self {
            service,
            ctx: ctx.clone(),
            background: vec![],
            loading: false,
            creating: false,
            library_dirty: false,
            load_generation: 0,
            saving: false,
            record_error: false,
            advance_pending: false,
            prepared: None,
            all_reviews: vec![],
            motivation: crate::motivation::Motivation::default(),
            progress: std::collections::BTreeMap::new(),
            library: library::Library::default(),
            settings,
            screen: if screen == Screen::TrainingSolved {
                Screen::Training
            } else {
                screen
            },
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
            library_scroll_top: false,
            theme_search: String::new(),
            review_theme: String::new(),
            review_sort: ReviewSort::MostRecent,
            status: "Preparing local puzzles…".into(),
            status_generation: 0,
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
            frame_stats: frame_stats::FrameStats::default(),
            frame_started: Instant::now(),
            capture_requested: false,
            capture_result: None,
            clearing_card: None,
        };
        app.initialize();
        if app.screenshot.is_some() {
            demo::welcome(&mut app, screen);
        }
        if matches!(screen, Screen::Training | Screen::TrainingSolved) {
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
    fn navigate(&mut self, screen: Screen) {
        if self.screen != screen && self.loading {
            self.load_generation += 1;
            self.loading = false;
        }
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
        self.load_engine(puzzle, engine, set_id, cycle_id, position);
    }
    fn load_engine(
        &mut self,
        puzzle: Puzzle,
        engine: Engine,
        set_id: i64,
        cycle_id: Option<i64>,
        position: usize,
    ) {
        self.load_generation += 1;
        self.saving = false;
        self.record_error = false;
        self.advance_pending = false;
        self.prepared = None;
        self.loading = false;
        self.session = Some(Session {
            puzzle,
            engine,
            set_id,
            cycle_id,
            position,
            recorded: false,
            finished_at: None,
        });
        if self
            .screenshot
            .as_ref()
            .is_some_and(|s| s.screen == Screen::TrainingSolved)
            && let Some(session) = &mut self.session
        {
            demo::solve(&session.puzzle, &mut session.engine);
        }
        self.timer.reset();
        self.board.reset();
        self.clearing_card = None;
        self.ctx.request_repaint();
    }
    fn resume_selected(&mut self) {
        if let Some(id) = self.selected_set {
            if self.sets.iter().any(|s| s.set.id == id && s.completed) {
                self.session = None;
                self.timer.reset();
                self.board.reset();
                return;
            }
            self.request_training(move |service| {
                service.start_or_resume_cycle(id, chrono::Local::now().timestamp())
            });
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
        if self
            .session
            .as_ref()
            .is_some_and(|s| s.engine.outcome().is_none())
        {
            return;
        }
        if self.saving || self.session.as_ref().is_some_and(|s| !s.recorded) {
            self.advance_pending = true;
            return;
        }
        if self.screen == Screen::Review {
            self.select_first_review();
        } else if let Some(prepared) = self.prepared.take() {
            self.load_prepared(prepared);
        } else if !self.loading
            && let Some(id) = self.session.as_ref().and_then(|s| s.cycle_id)
        {
            self.request_training(move |service| service.current_puzzle(id));
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
        self.ctx.request_repaint();
    }
    fn play(&mut self, uci: &str) {
        let now = self.now();
        if let Some(s) = &mut self.session
            && s.engine.play(uci, now).is_ok()
        {
            for event in s.engine.take_sounds() {
                if self.screenshot.is_none() {
                    crate::sound::play(event, &self.settings);
                }
            }
            self.board.player_moved();
            self.ctx.request_repaint();
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
        if self.saving || self.record_error {
            return;
        }
        self.saving = true;
        let generation = self.load_generation;
        let now = self.now();
        if let Some(s) = &mut self.session {
            s.finished_at = Some(now);
        }
        let saved = attempt.clone();
        self.background(
            move |service| {
                service.record_attempt(&saved)?;
                let prepared = saved
                    .cycle_id
                    .map(|id| {
                        service
                            .current_puzzle(id)
                            .map(background::PreparedTraining::new)
                    })
                    .transpose();
                let view = background::ViewData::read(service, saved.cycle_id, false);
                Ok((prepared, view))
            },
            move |app, result| {
                match result {
                    Ok((prepared, view)) => {
                        // Saving still updates shared badges after navigation, but cannot replace a newer board.
                        let clearing = app
                            .reviews
                            .iter()
                            .find(|c| {
                                c.item.set_id == attempt.set_id && c.puzzle.id == attempt.puzzle_id
                            })
                            .cloned();
                        match view {
                            Ok(view) => app.apply_view(view),
                            Err(error) => app.error(error),
                        }
                        if generation != app.load_generation {
                            return;
                        }
                        app.saving = false;
                        if let Some(s) = &mut app.session {
                            s.recorded = true;
                        }
                        match prepared {
                            Ok(prepared) => app.prepared = prepared,
                            Err(error) => app.error(error),
                        }
                        app.record_feedback(&attempt, clearing);
                        if app.advance_pending {
                            app.advance();
                        }
                    }
                    Err(error) => {
                        if generation == app.load_generation {
                            app.saving = false;
                            app.record_error = true;
                        }
                        app.error(error);
                    }
                }
            },
        );
    }
    fn record_feedback(&mut self, attempt: &Attempt, clearing: Option<ReviewCard>) {
        if attempt.kind == AttemptKind::Review {
            if attempt.outcome == Outcome::Correct {
                self.clearing_card = clearing.map(|c| (c, self.now()));
            }
            self.toast = Some((
                if attempt.outcome == Outcome::Correct {
                    "Cleared from Review"
                } else {
                    "Still pending"
                }
                .into(),
                Instant::now(),
            ));
        } else if attempt.outcome.is_miss() {
            self.toast = Some(("Added to Review".into(), Instant::now()));
        }
    }
    fn poll(&mut self) {
        self.poll_background();
        if self.service.poll_sync() {
            self.refresh_after_sync();
            self.reload_settings();
        }

        for event in self.service.poll_events() {
            self.status_generation += 1;
            match event {
                Event::Started { .. } => self.status = "Working in the background…".into(),
                Event::ImportProgress { id, progress } => {
                    self.library_dirty |= progress.accepted > 0;
                    if self.library.job.as_ref().is_some_and(|job| job.id == id) {
                        self.library.progress = progress;
                    }
                    self.job_fraction = if self.library.selected
                        == crate::source::LibrarySize::Medium
                        && self.library.job.as_ref().is_some_and(|job| job.id == id)
                    {
                        progress.accepted as f32 / 200_000.0
                    } else {
                        progress.bytes as f32
                            / crate::source::LibrarySize::Full.download_bytes() as f32
                    }
                    .clamp(0.0, 1.0);
                    self.status = format!(
                        "{} rows scanned · {} puzzles imported · {:.1} MB",
                        widgets::grouped_count(progress.scanned),
                        widgets::grouped_count(progress.accepted),
                        progress.bytes as f64 / 1_000_000.0
                    );
                }
                Event::FetchProgress {
                    batches, inserted, ..
                } => {
                    self.library_dirty |= inserted > 0;
                    self.status =
                        format!("Fetching from Lichess · {batches} batches · {inserted} puzzles");
                }
                Event::Finished { id, result } => {
                    let library_changed = self.library_dirty
                        || matches!(
                            &result,
                            Ok(JobOutput::Imported(_) | JobOutput::Daily(_) | JobOutput::Puzzle(_))
                        )
                        || matches!(&result, Ok(JobOutput::Fetched { inserted }) if *inserted > 0);
                    self.library_dirty = false;
                    if self.library.job.as_ref().is_some_and(|job| job.id == id) {
                        self.library.job = None;
                        match &result {
                            Ok(JobOutput::Imported(progress)) => {
                                self.library.progress = *progress;
                                self.welcome_step(library::WelcomeStep::Done);
                            }
                            Err(crate::service::ServiceError::Source(
                                crate::source::SourceError::Cancelled,
                            )) => {
                                self.library.message = "Download cancelled. Committed puzzles were kept; choose a size to resume.".into();
                                self.welcome_step(library::WelcomeStep::Choose);
                            }
                            Err(error) => {
                                self.library.message = error.to_string();
                                self.welcome_step(library::WelcomeStep::Error);
                            }
                            _ => {}
                        }
                    }
                    let update_status =
                        result.is_ok() && !matches!(&result, Ok(JobOutput::SetCreated(_)));
                    match result {
                        Ok(JobOutput::SetCreated(c)) => self.created(c),
                        Ok(_) => {
                            self.status = format!(
                                "{} puzzles available offline",
                                widgets::grouped_count(self.library.count)
                            );
                        }
                        Err(e) => {
                            self.status = e.to_string();
                            self.error(e);
                        }
                    }
                    if library_changed {
                        // Source jobs can commit rows even when cancelled or failing.
                        self.refresh_library(update_status);
                    } else {
                        self.refresh();
                    }
                }
            }
        }
    }
    fn created(&mut self, c: SetCreation) {
        if let Some(id) = c.set_id {
            self.selected_set = Some(id);
            self.screen = Screen::Training;
            self.resume_selected();
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
impl Woodpeck {
    fn tick(&mut self, ctx: &egui::Context, time: Duration) {
        self.clock.0.set(time);
        let now = self.now();
        if let Some(p) = self.theme.poll() {
            style::apply(ctx, &p);
            self.palette = p;
        }
        self.poll();
        crate::sound::configure(&self.settings);
        let solving = matches!(self.screen, Screen::Training | Screen::Review);
        if solving && let Some(session) = &mut self.session {
            session.engine.update(now);
            for event in session.engine.take_sounds() {
                if self.screenshot.is_none() {
                    crate::sound::play(event, &self.settings);
                }
            }
        }
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
            self.record();
        }
        if self.settings.auto_advance
            && solving
            && self.session.as_ref().is_some_and(|s| {
                s.engine.state() == State::Complete
                    && s.recorded
                    && !self.loading
                    && s.finished_at
                        .is_some_and(|t| now.saturating_sub(t) > Duration::from_millis(350))
            })
        {
            self.advance();
        }
        if solving && let Some(s) = &self.session {
            if let Some(deadline) = s.engine.next_deadline() {
                ctx.request_repaint_after(deadline.saturating_sub(now));
            }
            if self.settings.auto_advance
                && s.engine.state() == State::Complete
                && s.recorded
                && !self.loading
                && let Some(then) = s.finished_at
            {
                ctx.request_repaint_after((then + Duration::from_millis(351)).saturating_sub(now));
            }
            if self.timer.visible() && s.engine.state() == State::PlayerTurn && conditions.running()
            {
                ctx.request_repaint_after(if self.screen == Screen::Review {
                    Duration::from_millis(100)
                } else {
                    Duration::from_secs(1)
                });
            }
        }
    }
}
impl eframe::App for Woodpeck {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame_started = Instant::now();
        self.tick(ctx, self.origin.elapsed());
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
        self.frame_stats.record(self.frame_started.elapsed());
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
    let local_shutdown = service.local_shutdown();
    let capture = screenshot.is_some();
    let (sender, receiver) = std::sync::mpsc::channel();
    let size = screenshot.as_ref().map_or([1280.0, 820.0], |s| s.size);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("woodpeck")
            .with_app_id("woodpeck")
            .with_inner_size(size)
            .with_min_inner_size([340.0, 480.0]),
        glow_options: eframe::egui_glow::GlowConfiguration {
            vsync: true,
            ..Default::default()
        },
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
    local_shutdown.finish();
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
