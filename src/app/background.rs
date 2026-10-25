//! Nonblocking view snapshots and serial local work; callbacks only touch presentation state.
use super::{Woodpeck, style};
use crate::{
    motivation::Motivation,
    puzzle::{Engine, Outcome},
    review::{ReviewCard, ReviewSort, filter_and_sort},
    service::{LocalService, Result, SetProgress, SetSummary, TrainingPosition},
};
use std::{collections::BTreeMap, sync::mpsc, time::Duration};

type Poll = Box<dyn FnMut(&mut Woodpeck) -> bool>;
pub(super) struct Pending(Poll);

pub(super) struct ViewData {
    sets: Vec<SetSummary>,
    reviews: Vec<ReviewCard>,
    motivation: Motivation,
    progress: BTreeMap<i64, SetProgress>,
    marks: Vec<Outcome>,
    cycle: Option<i64>,
    library: Option<LibraryInfo>,
}
struct LibraryInfo {
    count: u64,
    bands: [u64; crate::source::RATING_BANDS],
    size: Option<crate::source::LibrarySize>,
    imported_at: Option<i64>,
}
impl ViewData {
    pub fn read(service: &LocalService, cycle: Option<i64>, library: bool) -> Result<Self> {
        let sets = service.list_sets()?;
        let progress = sets
            .iter()
            .map(|s| Ok((s.set.id, service.progress(s.set.id)?)))
            .collect::<Result<_>>()?;
        Ok(Self {
            sets,
            progress,
            reviews: service.review_queue(None, ReviewSort::MostRecent)?,
            motivation: service.motivation()?,
            marks: cycle
                .map(|id| service.cycle_outcomes(id))
                .transpose()?
                .unwrap_or_default(),
            cycle,
            library: if library {
                Some(LibraryInfo {
                    count: service.puzzle_count()?,
                    bands: service.rating_histogram()?,
                    size: service
                        .local_setting("library_size")?
                        .and_then(|s| crate::source::LibrarySize::parse(&s)),
                    imported_at: service
                        .local_setting("library_imported_at")?
                        .and_then(|s| s.parse().ok()),
                })
            } else {
                None
            },
        })
    }
}

pub(super) struct PreparedTraining {
    pub position: TrainingPosition,
    pub engine: Option<Engine>,
}
impl PreparedTraining {
    pub fn new(position: TrainingPosition) -> Self {
        let engine = position
            .puzzle
            .as_ref()
            .map(|p| Engine::new(p, Duration::ZERO, true));
        Self { position, engine }
    }
}
impl Woodpeck {
    pub(super) fn background<T: Send + 'static>(
        &mut self,
        work: impl FnOnce(&LocalService) -> Result<T> + Send + 'static,
        done: impl FnOnce(&mut Self, Result<T>) + 'static,
    ) {
        match self.service.background(work) {
            Ok(receiver) => {
                let mut done = Some(done);
                self.background.push(Pending(Box::new(move |app| {
                    let result = match receiver.try_recv() {
                        Ok(result) => result,
                        Err(mpsc::TryRecvError::Empty) => return false,
                        Err(mpsc::TryRecvError::Disconnected) => {
                            Err(crate::service::ServiceError::WorkerUnavailable)
                        }
                    };
                    done.take().expect("completion callback")(app, result);
                    true
                })));
            }
            Err(error) => done(self, Err(error)),
        }
    }
    pub(super) fn poll_background(&mut self) {
        let mut pending = std::mem::take(&mut self.background);
        pending.retain_mut(|pending| !(pending.0)(self));
        pending.append(&mut self.background);
        self.background = pending;
    }
    pub(super) fn apply_view(&mut self, view: ViewData) {
        self.sets = view.sets;
        self.all_reviews = view.reviews;
        self.pending = u32::try_from(self.all_reviews.len()).unwrap_or(u32::MAX);
        self.filter_reviews();
        self.motivation = view.motivation;
        self.progress = view.progress;
        if view.cycle == self.session.as_ref().and_then(|s| s.cycle_id) {
            self.cycle_marks = view.marks;
        }
        if let Some(library) = view.library {
            self.library.count = library.count;
            self.library.bands = library.bands;
            self.library.size = library.size;
            self.library.imported_at = library.imported_at;
        }
        if self
            .selected_set
            .is_none_or(|id| !self.sets.iter().any(|s| s.set.id == id))
        {
            self.selected_set = self.sets.first().map(|s| s.set.id);
        }
    }
    pub(super) fn filter_reviews(&mut self) {
        self.reviews = filter_and_sort(
            self.all_reviews.clone(),
            (!self.review_theme.is_empty()).then_some(self.review_theme.as_str()),
            self.review_sort,
        );
    }
    pub(super) fn initialize(&mut self) {
        self.background(
            |service| {
                let settings = service.settings()?;
                let onboarding = service.needs_onboarding()?;
                let view = ViewData::read(service, None, true)?;
                Ok((settings, onboarding, view))
            },
            |app, result| match result {
                Ok((settings, onboarding, view)) => {
                    app.apply_view(view);
                    if app.builder
                        == crate::sets::PRESETS[0]
                            .builder(crate::settings::Settings::default().user_rating)
                    {
                        app.builder = crate::sets::PRESETS[0].builder(settings.user_rating);
                        app.builder.shuffle_each_cycle = settings.shuffle_each_cycle;
                    }
                    if settings != app.settings {
                        app.theme = style::ThemeWatcher::new(settings.color_theme, app.ctx.clone());
                        app.settings = settings;
                    }
                    if app.screenshot.is_none()
                        && app.screen == super::Screen::Dashboard
                        && onboarding
                    {
                        app.screen = super::Screen::Welcome;
                    }
                    match app.screen {
                        super::Screen::Training => app.resume_selected(),
                        super::Screen::Review => app.select_first_review(),
                        _ => {}
                    }
                    if let Some(screenshot) = &app.screenshot {
                        super::demo::welcome(app, screenshot.screen);
                    }
                }
                Err(error) => app.error(error),
            },
        );
    }
    pub(super) fn refresh(&mut self) {
        self.refresh_view(false);
    }
    pub(super) fn refresh_library(&mut self) {
        self.refresh_view(true);
    }
    fn refresh_view(&mut self, library: bool) {
        let cycle = self.session.as_ref().and_then(|s| s.cycle_id);
        self.background(
            move |service| ViewData::read(service, cycle, library),
            |app, result| {
                match result {
                    Ok(view) => app.apply_view(view),
                    Err(error) => app.error(error),
                }
                if let Some(screenshot) = &app.screenshot {
                    super::demo::welcome(app, screenshot.screen);
                }
            },
        );
    }
    pub(super) fn request_training(
        &mut self,
        work: impl FnOnce(&LocalService) -> Result<TrainingPosition> + Send + 'static,
    ) {
        self.load_generation += 1;
        let generation = self.load_generation;
        self.loading = true;
        self.background(
            move |service| {
                let prepared = PreparedTraining::new(work(service)?);
                let view = ViewData::read(service, Some(prepared.position.cycle.id), false)?;
                Ok((prepared, view))
            },
            move |app, result| {
                if generation != app.load_generation {
                    if let Ok((_, view)) = result {
                        app.apply_view(view);
                    }
                    return;
                }
                app.loading = false;
                match result {
                    Ok((prepared, view)) => {
                        app.load_prepared(prepared);
                        app.apply_view(view);
                    }
                    Err(error) => app.error(error),
                }
            },
        );
    }
    pub(super) fn load_prepared(&mut self, mut prepared: PreparedTraining) {
        let p = prepared.position;
        self.selected_set = Some(p.cycle.set_id);
        if let (Some(puzzle), Some(mut engine)) = (p.puzzle, prepared.engine.take()) {
            engine.start_at(self.now());
            self.load_engine(puzzle, engine, p.cycle.set_id, Some(p.cycle.id), p.position);
        } else {
            self.session = None;
            self.prepared = None;
            self.timer.reset();
            self.board.reset();
        }
    }
    pub(super) fn reload_settings(&mut self) {
        self.background(LocalService::settings, |app, result| match result {
            Ok(settings) if settings != app.settings => {
                app.theme = style::ThemeWatcher::new(settings.color_theme, app.ctx.clone());
                app.settings = settings;
            }
            Err(error) => app.error(error),
            _ => {}
        });
    }
}
