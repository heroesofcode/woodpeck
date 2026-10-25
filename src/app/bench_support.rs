//! Headless training fixtures shared by benchmarks. No window or audio device is required.
use super::{Screen, Woodpeck, demo, pieces::Pieces};
use crate::{
    puzzle::{Engine, State},
    service::AppService,
    sets::BuilderParams,
    settings::Settings,
    store::Store,
};
use eframe::egui;
use shakmaty::{Color, Piece, Role};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn settle(app: &mut Woodpeck) {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        app.poll();
        if app.background.is_empty() && !app.service.is_busy() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "background fixture timeout"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
pub struct TrainingFrame {
    app: Woodpeck,
    ctx: egui::Context,
    time: Duration,
}
impl Drop for TrainingFrame {
    fn drop(&mut self) {
        // A completion benchmark must commit before another fixture opens the copy.
        settle(&mut self.app);
    }
}
impl Default for TrainingFrame {
    fn default() -> Self {
        Self::with_service(demo::service().unwrap(), None)
    }
}
impl TrainingFrame {
    /// The caller owns a disposable copy; this fixture never opens the source library.
    pub fn from_database(path: &Path) -> Self {
        let service = AppService::new(Store::open(path).unwrap()).unwrap();
        let selected = service
            .list_sets()
            .unwrap()
            .into_iter()
            .find(|s| {
                !s.completed
                    && s.latest_cycle.as_ref().is_none_or(|c| {
                        c.completed_at.is_some() || s.stats.attempted() + 1 < s.set.params.size
                    })
            })
            .map_or_else(
                || {
                    service
                        .create_set(
                            &BuilderParams {
                                size: 20,
                                ..BuilderParams::default()
                            },
                            42,
                            1,
                        )
                        .unwrap()
                        .set_id
                        .expect("benchmark library must support a 20-puzzle set")
                },
                |s| s.set.id,
            );
        Self::with_service(service, Some(selected))
    }
    fn with_service(service: AppService, selected: Option<i64>) -> Self {
        let ctx = egui::Context::default();
        service
            .set_settings(&Settings {
                completion_sound: false,
                auto_advance: false,
                ..Settings::default()
            })
            .unwrap();
        let mut app = Woodpeck::new(&ctx, service, PathBuf::new(), None);
        settle(&mut app);
        if let Some(selected) = selected {
            app.selected_set = Some(selected);
        }
        app.navigate(Screen::Training);
        settle(&mut app);
        let mut frame = Self {
            app,
            ctx,
            time: Duration::from_secs(10),
        };
        for _ in 0..4 {
            frame.frame(Vec::new());
            frame.time += Duration::from_secs(1);
        }
        frame
    }
}
impl TrainingFrame {
    pub fn frame(&mut self, events: Vec<egui::Event>) {
        self.run_frame(events, |_| {});
    }
    fn run_frame(&mut self, events: Vec<egui::Event>, action: impl FnOnce(&mut Woodpeck)) {
        let mut action = Some(action);
        let mut output = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 820.0),
                )),
                time: Some(self.time.as_secs_f64()),
                events,
                ..Default::default()
            },
            |ui| {
                self.app.tick(ui.ctx(), self.time);
                if let Some(action) = action.take() {
                    action(&mut self.app);
                }
                self.app.draw(ui);
            },
        );
        let _ = self
            .ctx
            .tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);
        output.drop_without_applying_deltas();
    }
    pub fn drag(&mut self) {
        let p = self
            .app
            .board
            .drag_origin(&self.app.session.as_ref().unwrap().engine);
        self.frame(vec![
            egui::Event::PointerMoved(p),
            egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        self.frame(vec![egui::Event::PointerMoved(p + egui::vec2(30.0, 20.0))]);
        assert!(self.app.board.is_selected());
    }
    pub fn correct(&mut self) {
        let s = self.app.session.as_mut().unwrap();
        s.engine = Engine::new(&s.puzzle, self.time, true);
        let mut time = self.time + Duration::from_secs(1);
        s.engine.update(time);
        for uci in s.puzzle.moves.split_whitespace().skip(1).step_by(2) {
            s.engine.play(uci, time).unwrap();
            time += Duration::from_secs(1);
            s.engine.update(time);
        }
        self.time = time;
        s.recorded = false;
        s.finished_at = None;
    }
    pub fn settle(&mut self) {
        settle(&mut self.app);
    }
    /// Exercise the builder's submission, generation, reply, and training load while rendering.
    pub fn check_set_creation(&mut self) -> (Duration, Duration) {
        self.app.navigate(Screen::Builder);
        self.app.builder = BuilderParams {
            size: 20,
            ..BuilderParams::default()
        };
        for _ in 0..4 {
            self.frame(Vec::new());
        }
        let mut maximum = Duration::ZERO;
        let started = Instant::now();
        self.timed_frame(&mut maximum, Woodpeck::create_set);
        while self.app.creating || self.app.loading || !self.app.background.is_empty() {
            assert!(
                started.elapsed() < Duration::from_secs(60),
                "set creation timeout"
            );
            std::thread::sleep(Duration::from_millis(1));
            self.time += Duration::from_millis(16);
            self.timed_frame(&mut maximum, |_| {});
        }
        assert!(self.app.creation.as_ref().unwrap().set_id.is_some());
        assert_eq!(self.app.screen, Screen::Training);
        (started.elapsed(), maximum)
    }
    pub fn advance_frame(&mut self) {
        let old = self.app.session.as_ref().unwrap().puzzle.id.clone();
        self.frame(vec![egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert_ne!(
            self.app.session.as_ref().unwrap().puzzle.id,
            old,
            "advance must load the next puzzle in this frame"
        );
    }
    /// Maximum CPU frame time, including moves, completion, worker acknowledgment, and next.
    /// Waiting between frames is excluded, but polling/applying replies happens inside frames.
    pub fn solve_and_advance(&mut self) -> Duration {
        let moves = self.app.session.as_ref().unwrap().puzzle.moves.clone();
        let mut maximum = Duration::ZERO;
        self.time += Duration::from_secs(1);
        self.timed_frame(&mut maximum, |_| {});
        for uci in moves.split_whitespace().skip(1).step_by(2) {
            self.timed_frame(&mut maximum, |app| {
                assert_eq!(
                    app.session.as_ref().unwrap().engine.state(),
                    State::PlayerTurn
                );
                app.play(uci);
            });
            for _ in 0..16 {
                self.time += Duration::from_millis(16);
                self.timed_frame(&mut maximum, |_| {});
            }
        }
        assert_eq!(
            self.app.session.as_ref().unwrap().engine.state(),
            State::Complete
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        while !self.app.session.as_ref().unwrap().recorded {
            assert!(Instant::now() < deadline, "attempt acknowledgment timeout");
            std::thread::sleep(Duration::from_millis(1));
            self.time += Duration::from_millis(16);
            self.timed_frame(&mut maximum, |_| {});
        }
        let started = Instant::now();
        self.advance_frame();
        maximum = maximum.max(started.elapsed());
        for _ in 0..16 {
            self.time += Duration::from_millis(16);
            self.timed_frame(&mut maximum, |_| {});
        }
        maximum
    }
    fn timed_frame(&mut self, maximum: &mut Duration, action: impl FnOnce(&mut Woodpeck)) {
        let started = Instant::now();
        self.run_frame(Vec::new(), action);
        *maximum = (*maximum).max(started.elapsed());
    }
    pub fn transition(&mut self) {
        // Hold the fade at its midpoint to measure a representative transition frame.
        settle(&mut self.app);
        self.time += Duration::from_millis(160);
        self.frame(Vec::new());
    }
}

#[derive(Default)]
pub struct PieceTextures {
    ctx: egui::Context,
    pieces: Pieces,
}
impl PieceTextures {
    pub fn rasterize(&mut self, size: f32) {
        for color in [Color::White, Color::Black] {
            for role in [
                Role::Pawn,
                Role::Knight,
                Role::Bishop,
                Role::Rook,
                Role::Queen,
                Role::King,
            ] {
                assert!(
                    self.pieces
                        .texture(&self.ctx, Piece { color, role }, size, true)
                        .is_some()
                );
            }
        }
    }
}
