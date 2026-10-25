use super::*;
use crate::layout;
fn settle(app: &mut Woodpeck) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        app.poll();
        if app.background.is_empty() && !app.service.is_busy() {
            break;
        }
        assert!(Instant::now() < deadline, "local worker timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn skip_updates_badge_once_and_correct_review_preserves_cycle_stats() {
    let ctx = egui::Context::default();
    let service = demo::service().unwrap();
    let settings = Settings {
        completion_sound: false,
        ..Default::default()
    };
    service.set_settings(&settings).unwrap();
    let mut app = Woodpeck::new(&ctx, service, PathBuf::from("/tmp/demo.db"), None);
    settle(&mut app);
    app.navigate(Screen::Training);
    settle(&mut app);
    let count = app.pending;
    let set_id = app.selected_set.unwrap();
    app.clock.0.set(Duration::from_secs(1));
    app.session
        .as_mut()
        .unwrap()
        .engine
        .update(Duration::from_secs(1));
    app.action(shortcuts::Action::Skip);
    app.record();
    app.record();
    settle(&mut app);
    assert_eq!(app.pending, count + 1);
    assert_eq!(app.cycle_marks.len(), 8);
    assert_eq!(app.toast.as_ref().unwrap().0, "Added to Review");
    app.navigate(Screen::Review);
    let moves: Vec<_> = app
        .session
        .as_ref()
        .unwrap()
        .puzzle
        .moves
        .split_whitespace()
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect();
    let mut time = Duration::from_secs(2);
    app.clock.0.set(time);
    app.session.as_mut().unwrap().engine.update(time);
    for uci in moves {
        app.play(&uci);
        time += Duration::from_secs(1);
        app.clock.0.set(time);
        app.session.as_mut().unwrap().engine.update(time);
    }
    assert_eq!(
        app.session.as_ref().unwrap().engine.state(),
        State::Complete
    );
    app.record();
    settle(&mut app);
    assert_eq!(app.pending, count);
    assert_eq!(app.toast.as_ref().unwrap().0, "Cleared from Review");
    assert!(app.clearing_card.is_some());
    assert_eq!(
        app.service
            .progress(set_id)
            .unwrap()
            .cycles
            .last()
            .unwrap()
            .stats
            .attempted(),
        8
    );
}
#[test]
fn every_screen_keeps_content_within_tiled_widths() {
    let ctx = egui::Context::default();
    let mut app = Woodpeck::new(
        &ctx,
        demo::service().unwrap(),
        PathBuf::from("/tmp/demo.db"),
        None,
    );
    settle(&mut app);
    for (width, height) in [
        (360.0, 640.0),
        (640.0, 900.0),
        (960.0, 1080.0),
        (1920.0, 1080.0),
    ] {
        let layout = layout::calculate(width, height);
        for (screen, _) in Screen::ALL {
            app.navigate(screen);
            settle(&mut app);
            for _ in 0..2 {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width - layout.navigation_width, height),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default()
                            .frame(egui::Frame::new().inner_margin(14))
                            .show(ui, |ui| {
                                let right = ui.max_rect().right();
                                if matches!(screen, Screen::Training | Screen::Review) {
                                    app.solving(ui, layout.mode, layout.panel_width);
                                } else {
                                    egui::ScrollArea::vertical().show(ui, |ui| {
                                        match screen {
                                            Screen::Dashboard => app.dashboard(ui),
                                            Screen::Builder => app.builder_screen(ui),
                                            Screen::Progress => app.progress_screen(ui),
                                            Screen::Settings => app.settings_screen(ui),
                                            _ => {}
                                        }
                                        assert!(
                                            ui.min_rect().right() <= right + 1.0,
                                            "{screen:?} overflow at {width}: {} > {right}",
                                            ui.min_rect().right()
                                        );
                                    });
                                }
                                assert!(
                                    ui.min_rect().right() <= right + 1.0,
                                    "{screen:?} overflows at {width}"
                                );
                            });
                    },
                );
                output.drop_without_applying_deltas();
            }
        }
    }
}

fn welcome_frame(app: &mut Woodpeck, ctx: &egui::Context, width: f32, events: Vec<egui::Event>) {
    app.poll();
    let output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 900.0),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().inner_margin(14))
                .show(ui, |ui| {
                    let right = ui.max_rect().right();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        app.welcome(ui);
                        assert!(
                            ui.min_rect().right() <= right + 1.0,
                            "welcome overflow at {width}"
                        );
                    });
                });
        },
    );
    output.drop_without_applying_deltas();
}
fn key(key: egui::Key) -> Vec<egui::Event> {
    vec![
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}
fn button_is_enabled(ctx: &egui::Context, output: &egui::FullOutput, label: &str) -> bool {
    let rect = output
        .shapes
        .iter()
        .find_map(|shape| {
            if let egui::Shape::Text(text) = &shape.shape
                && text.galley.text() == label
            {
                Some(text.galley.rect.translate(text.pos.to_vec2()))
            } else {
                None
            }
        })
        .unwrap();
    ctx.viewport(|viewport| {
        viewport.prev_pass.widgets.layers().any(|(_, layer)| {
            layer.iter().any(|widget| {
                widget.enabled && widget.sense.senses_click() && widget.rect.contains_rect(rect)
            })
        })
    })
}

#[test]
fn welcome_states_fit_tiled_widths_and_screenshot_done_is_stable() {
    let ctx = egui::Context::default();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut app = Woodpeck::new(
        &ctx,
        demo::service().unwrap(),
        PathBuf::from("/tmp/demo.db"),
        Some(Screenshot {
            path: PathBuf::from("/tmp/demo.png"),
            screen: Screen::WelcomeDone,
            size: [360.0, 640.0],
        }),
    );
    app.refresh();
    settle(&mut app);
    assert_eq!(app.library.count, 200_000);
    assert_eq!(app.library.bands.iter().sum::<u64>(), 200_000);
    for width in [360.0, 640.0, 960.0, 1920.0] {
        for step in [
            library::WelcomeStep::Choose,
            library::WelcomeStep::Downloading,
            library::WelcomeStep::Done,
            library::WelcomeStep::Error,
        ] {
            app.library.step = step;
            app.library.message = "The connection was interrupted. Retry to continue.".into();
            for _ in 0..2 {
                welcome_frame(&mut app, &ctx, width, vec![]);
            }
        }
    }
}
#[test]
fn welcome_arrows_tab_enter_and_escape_persist_choice() {
    use crate::source::LibrarySize;
    let ctx = egui::Context::default();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut app = Woodpeck::new(
        &ctx,
        demo::service().unwrap(),
        PathBuf::from("/tmp/demo.db"),
        None,
    );
    app.show_welcome();
    welcome_frame(&mut app, &ctx, 960.0, vec![]);
    welcome_frame(&mut app, &ctx, 960.0, key(egui::Key::ArrowRight));
    assert_eq!(app.library.selected, LibrarySize::Full);
    welcome_frame(&mut app, &ctx, 960.0, vec![]);
    welcome_frame(&mut app, &ctx, 960.0, key(egui::Key::ArrowLeft));
    welcome_frame(&mut app, &ctx, 960.0, vec![]);
    assert_eq!(app.library.selected, LibrarySize::Medium);
    ctx.memory_mut(|m| m.request_focus(egui::Id::unique(("library-card", "starter"))));
    welcome_frame(&mut app, &ctx, 960.0, vec![]);
    assert_eq!(app.library.selected, LibrarySize::Starter);
    welcome_frame(&mut app, &ctx, 960.0, key(egui::Key::Tab));
    welcome_frame(&mut app, &ctx, 960.0, vec![]);
    assert_eq!(app.library.selected, LibrarySize::Medium);
    ctx.memory_mut(|m| m.request_focus(egui::Id::unique(("library-card", "starter"))));
    welcome_frame(&mut app, &ctx, 960.0, vec![]);
    welcome_frame(&mut app, &ctx, 960.0, key(egui::Key::Enter));
    assert_eq!(app.screen, Screen::Builder);
    settle(&mut app);
    assert_eq!(
        app.service
            .local_setting("onboarding_done")
            .unwrap()
            .as_deref(),
        Some("true")
    );
    app.show_welcome();
    welcome_frame(&mut app, &ctx, 360.0, key(egui::Key::Escape));
    assert_eq!(app.screen, Screen::Builder);
}

#[test]
fn library_download_is_available_during_startup_and_finishes_after_it() {
    let ctx = egui::Context::default();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let service = AppService::new(crate::store::Store::memory().unwrap()).unwrap();
    let mut app = Woodpeck::new(&ctx, service, PathBuf::from("/tmp/demo.db"), None);
    assert!(app.service.is_busy());
    assert!(app.library.can_start());
    let startup = app.service.background_import().unwrap();
    assert_eq!(startup.size, crate::source::LibrarySize::Starter);

    // Render before polling, as on the first frame: startup is still pending.
    let output = ctx.run_ui(egui::RawInput::default(), |ui| app.settings_screen(ui));
    let texts: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| {
            if let egui::Shape::Text(text) = &shape.shape {
                Some(text.galley.text().to_owned())
            } else {
                None
            }
        })
        .collect();
    assert!(
        texts
            .iter()
            .any(|text| text == "Preparing the starter pack…")
    );
    assert!(texts.iter().any(|text| text == "Cancel background import"));
    assert!(texts.iter().any(|text| text == "Download Medium"));
    assert!(button_is_enabled(&ctx, &output, "Download Medium"));
    assert!(button_is_enabled(&ctx, &output, "Cancel background import"));
    output.drop_without_applying_deltas();

    let output = ctx.run_ui(egui::RawInput::default(), |ui| app.welcome(ui));
    assert!(button_is_enabled(&ctx, &output, "Download Medium"));
    output.drop_without_applying_deltas();

    // Use a local download source to exercise the queued job without network access.
    let handle = app
        .service
        .submit(Job::ImportLibrary {
            location: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lichess.csv").into(),
            size: crate::source::LibrarySize::Medium,
            limit: Some(2),
        })
        .unwrap();
    app.library.job = Some(handle);
    app.welcome_step(library::WelcomeStep::Downloading);
    assert!(!app.library.can_start());
    let output = ctx.run_ui(egui::RawInput::default(), |ui| app.library_progress(ui));
    assert!(output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Preparing the starter pack…")
    }));
    output.drop_without_applying_deltas();

    let deadline = Instant::now() + Duration::from_secs(20);
    while app.library.job.is_some() {
        app.poll();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    settle(&mut app);
    assert_eq!(app.library.step, library::WelcomeStep::Done);
    assert_eq!(app.library.progress.accepted, 2);
    assert!(app.library.count >= crate::source::STARTER_COUNT);
    assert!(app.library.can_start());
    assert!(app.service.background_import().is_none());
}

#[test]
fn library_worker_finishes_after_background_navigation_and_errors_can_retry() {
    let ctx = egui::Context::default();
    let mut app = Woodpeck::new(
        &ctx,
        demo::service().unwrap(),
        PathBuf::from("/tmp/demo.db"),
        None,
    );
    for (location, expected) in [
        (
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lichess.csv"),
            library::WelcomeStep::Done,
        ),
        ("/missing/puzzles.csv", library::WelcomeStep::Error),
    ] {
        let handle = app
            .service
            .submit(Job::ImportLibrary {
                location: location.into(),
                size: crate::source::LibrarySize::Medium,
                limit: Some(2),
            })
            .unwrap();
        app.library.job = Some(handle);
        app.welcome_step(library::WelcomeStep::Downloading);
        app.navigate(Screen::Builder);
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.library.job.is_some() {
            app.poll();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        settle(&mut app);
        assert_eq!(app.screen, Screen::Builder);
        assert_eq!(app.library.step, expected);
        if expected == library::WelcomeStep::Done {
            assert_eq!(app.library.progress.accepted, 2);
        } else {
            assert_ne!(app.library.message, "");
        }
    }
}

fn training_app() -> (egui::Context, Woodpeck) {
    let ctx = egui::Context::default();
    let service = demo::service().unwrap();
    service
        .set_settings(&Settings {
            completion_sound: false,
            ..Settings::default()
        })
        .unwrap();
    let mut app = Woodpeck::new(&ctx, service, PathBuf::new(), None);
    settle(&mut app);
    app.navigate(Screen::Training);
    settle(&mut app);
    app.clock.0.set(Duration::from_secs(1));
    let now = app.now();
    app.session.as_mut().unwrap().engine.update(now);
    (ctx, app)
}

fn solve(app: &mut Woodpeck) {
    let moves = app.session.as_ref().unwrap().puzzle.moves.clone();
    let mut now = app.now() + Duration::from_secs(1);
    app.clock.0.set(now);
    app.session.as_mut().unwrap().engine.update(now);
    for uci in moves.split_whitespace().skip(1).step_by(2) {
        app.play(uci);
        now += Duration::from_secs(1);
        app.clock.0.set(now);
        app.session.as_mut().unwrap().engine.update(now);
    }
    assert_eq!(
        app.session.as_ref().unwrap().engine.state(),
        State::Complete
    );
}

#[test]
fn completion_renders_while_save_waits_and_early_next_advances_once() {
    let (ctx, mut app) = training_app();
    let (release, wait) = std::sync::mpsc::channel();
    let blocked = app
        .service
        .background(move |_| {
            wait.recv().unwrap();
            Ok(())
        })
        .unwrap();
    let old = app.session.as_ref().unwrap().puzzle.id.clone();
    let set_id = app.session.as_ref().unwrap().set_id;
    solve(&mut app);
    app.record();
    app.record();
    assert!(app.saving);
    app.advance();
    assert!(app.advance_pending);
    for _ in 0..3 {
        let output = ctx.run_ui(egui::RawInput::default(), |ui| app.draw(ui));
        output.drop_without_applying_deltas();
        app.poll();
        assert_eq!(app.session.as_ref().unwrap().puzzle.id, old);
    }
    release.send(()).unwrap();
    blocked
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    settle(&mut app);
    assert_ne!(app.session.as_ref().unwrap().puzzle.id, old);
    assert_eq!(app.session.as_ref().unwrap().position, 8);
    assert_eq!(
        app.service
            .progress(set_id)
            .unwrap()
            .cycles
            .last()
            .unwrap()
            .stats
            .attempted(),
        8
    );
}

#[test]
fn save_acknowledgment_survives_navigation_and_cannot_replace_another_session() {
    let (_, mut app) = training_app();
    let (release, wait) = std::sync::mpsc::channel();
    let blocked = app
        .service
        .background(move |_| {
            wait.recv().unwrap();
            Ok(())
        })
        .unwrap();
    solve(&mut app);
    app.record();
    app.navigate(Screen::Dashboard);
    release.send(()).unwrap();
    blocked
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    settle(&mut app);
    assert!(!app.saving);
    assert!(app.session.as_ref().unwrap().recorded);
    app.navigate(Screen::Training);
    app.advance();
    assert_eq!(app.session.as_ref().unwrap().position, 8);
    solve(&mut app);
    app.record();
    // Selecting a review board while a training write completes must preserve that board.
    app.navigate(Screen::Review);
    let review = app.session.as_ref().unwrap().puzzle.id.clone();
    settle(&mut app);
    assert_eq!(app.session.as_ref().unwrap().puzzle.id, review);
    assert!(app.session.as_ref().unwrap().cycle_id.is_none());
}

#[test]
fn hidden_timer_idle_frames_have_no_periodic_repaint() {
    let (ctx, mut app) = training_app();
    app.timer.toggle_visible();
    // Expire all input/feedback animations before checking the viewport deadline.
    for time in [10, 11, 12] {
        let output = ctx.run_ui(
            egui::RawInput {
                time: Some(f64::from(time)),
                ..Default::default()
            },
            |ui| {
                app.tick(ui.ctx(), Duration::from_secs_f64(f64::from(time)));
                app.draw(ui);
            },
        );
        if time == 12 {
            assert_eq!(
                output.viewport_output[&egui::ViewportId::ROOT].repaint_delay,
                Duration::MAX
            );
        }
        output.drop_without_applying_deltas();
    }
}

#[test]
fn failed_save_can_retry_without_advancing_or_recording_twice() {
    let (_, mut app) = training_app();
    solve(&mut app);
    let set_id = app.session.as_ref().unwrap().set_id;
    app.session.as_mut().unwrap().set_id = -1;
    app.record();
    settle(&mut app);
    assert!(app.record_error);
    assert!(!app.session.as_ref().unwrap().recorded);
    app.advance();
    assert!(app.advance_pending);
    app.session.as_mut().unwrap().set_id = set_id;
    app.record_error = false;
    app.record();
    app.record();
    settle(&mut app);
    assert!(!app.record_error);
    assert_eq!(app.session.as_ref().unwrap().position, 8);
    assert_eq!(
        app.service
            .progress(set_id)
            .unwrap()
            .cycles
            .last()
            .unwrap()
            .stats
            .attempted(),
        8
    );
}

#[test]
fn attempts_advancement_and_ordinary_refresh_preserve_library_snapshot() {
    let (_, mut app) = training_app();
    let count = app.library.count;
    let bands = app.library.bands;
    // A sentinel detects accidental inclusion of library queries in ordinary snapshots.
    app.library.count = u64::MAX;
    app.library.bands = [u64::MAX; crate::source::RATING_BANDS];
    app.refresh();
    settle(&mut app);
    solve(&mut app);
    app.record();
    settle(&mut app);
    app.advance();
    settle(&mut app);
    assert_eq!(app.library.count, u64::MAX);
    assert_eq!(app.library.bands, [u64::MAX; crate::source::RATING_BANDS]);
    app.refresh_library(false);
    settle(&mut app);
    assert_eq!(app.library.count, count);
    assert_eq!(app.library.bands, bands);
}

#[test]
fn sync_notifications_preserve_moves_and_timer_when_training_position_is_unchanged() {
    let (_, mut app) = training_app();
    app.timer.set_conditions(TimerConditions {
        player_turn: true,
        interactive: true,
        focused: true,
        ..Default::default()
    });
    app.clock.0.set(Duration::from_secs(5));
    let first_move = app
        .session
        .as_ref()
        .unwrap()
        .puzzle
        .moves
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_owned();
    app.play(&first_move);
    let generation = app.load_generation;
    let state = app.session.as_ref().unwrap().engine.state();
    let position = app.session.as_ref().unwrap().engine.position().cloned();
    // Syncing/Synced notifications and pulls with unrelated changes all use this refresh.
    for unrelated_change in [false, true, false] {
        if unrelated_change {
            let card = &app.all_reviews[0];
            app.service
                .record_attempt(&Attempt {
                    puzzle_id: card.puzzle.id.clone(),
                    set_id: card.item.set_id,
                    cycle_id: None,
                    kind: AttemptKind::Review,
                    outcome: Outcome::Correct,
                    time_ms: 1000,
                    moves_played: vec![],
                    created_at: chrono::Local::now().timestamp(),
                })
                .unwrap();
        }
        app.refresh_after_sync();
        settle(&mut app);
        let session = app.session.as_ref().unwrap();
        assert_eq!(app.load_generation, generation);
        assert_eq!(
            session.engine.moves_played(),
            std::slice::from_ref(&first_move)
        );
        assert_eq!(session.engine.state(), state);
        assert_eq!(session.engine.position(), position.as_ref());
        assert_eq!(app.timer.elapsed(), Duration::from_secs(4));
    }
}

#[test]
fn sync_refresh_advances_changed_training_position_and_clears_completed_cycle() {
    let (_, mut app) = training_app();
    let session = app.session.as_ref().unwrap();
    let cycle = session.cycle_id.unwrap();
    let old = session.puzzle.id.clone();
    let set_id = session.set_id;
    loop {
        let position = app.service.current_puzzle(cycle).unwrap();
        let Some(puzzle) = position.puzzle else {
            break;
        };
        app.service
            .record_attempt(&Attempt {
                puzzle_id: puzzle.id,
                set_id,
                cycle_id: Some(cycle),
                kind: AttemptKind::Cycle,
                outcome: Outcome::Correct,
                time_ms: 1000,
                moves_played: vec![],
                created_at: chrono::Local::now().timestamp(),
            })
            .unwrap();
        app.refresh_after_sync();
        settle(&mut app);
        if let Some(session) = &app.session {
            assert_ne!(session.puzzle.id, old);
            assert_eq!(session.position, position.position + 1);
            assert_eq!(session.engine.state(), State::Loading);
        }
    }
    assert!(app.session.is_none());
}

#[test]
fn sync_refresh_replaces_reviews_cleared_elsewhere_and_empties_the_board() {
    let (_, mut app) = training_app();
    app.navigate(Screen::Review);
    let generation = app.load_generation;
    let now = app.now() + Duration::from_secs(1);
    app.clock.0.set(now);
    app.session.as_mut().unwrap().engine.update(now);
    app.review_theme = "missing-theme".into();
    app.filter_reviews();
    app.refresh_after_sync();
    settle(&mut app);
    assert_eq!(app.load_generation, generation);
    assert_eq!(
        app.session.as_ref().unwrap().engine.state(),
        State::PlayerTurn
    );
    app.review_theme.clear();
    app.filter_reviews();
    while let Some(session) = &app.session {
        let old = session.puzzle.id.clone();
        let count = app.pending;
        app.service
            .record_attempt(&Attempt {
                puzzle_id: old.clone(),
                set_id: session.set_id,
                cycle_id: None,
                kind: AttemptKind::Review,
                outcome: Outcome::Correct,
                time_ms: 1000,
                moves_played: vec![],
                created_at: chrono::Local::now().timestamp(),
            })
            .unwrap();
        app.refresh_after_sync();
        settle(&mut app);
        assert_eq!(app.pending, count - 1);
        assert!(app.session.as_ref().is_none_or(|s| s.puzzle.id != old));
    }
    assert_eq!(app.all_reviews, []);
    assert_eq!(app.reviews, []);
}

#[test]
fn delayed_sync_refresh_preserves_newer_or_recorded_sessions_and_pending_saves() {
    let (_, mut app) = training_app();
    app.refresh_after_sync();
    app.navigate(Screen::Review);
    let generation = app.load_generation;
    let review = app.session.as_ref().unwrap().puzzle.id.clone();
    settle(&mut app);
    assert_eq!(app.load_generation, generation);
    assert_eq!(app.session.as_ref().unwrap().puzzle.id, review);

    solve(&mut app);
    app.record();
    app.refresh_after_sync();
    settle(&mut app);
    assert_eq!(app.load_generation, generation);
    assert_eq!(app.session.as_ref().unwrap().puzzle.id, review);
    assert!(app.session.as_ref().unwrap().recorded);
}

#[test]
fn successful_import_status_uses_refreshed_library_count() {
    let (_, mut app) = training_app();
    let before = app.library.count;
    let csv = include_str!("../../tests/fixtures/lichess.csv")
        .replace("00008,", "new08,")
        .replace("0000D,", "new0D,");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new-puzzles.csv");
    std::fs::write(&path, csv).unwrap();
    app.submit(Job::ImportLibrary {
        location: path.display().to_string(),
        size: crate::source::LibrarySize::Medium,
        limit: Some(2),
    });
    settle(&mut app);
    assert_eq!(app.library.count, before + 2);
    assert_eq!(
        app.status,
        format!(
            "{} puzzles available offline",
            widgets::grouped_count(before + 2)
        )
    );

    app.refresh_library(true);
    app.status_generation += 1;
    app.status = "Working in the background…".into();
    settle(&mut app);
    assert_eq!(app.status, "Working in the background…");
}

#[test]
fn set_creation_queues_once_and_preserves_library_snapshot() {
    let (ctx, mut app) = training_app();
    app.navigate(Screen::Builder);
    app.builder.size = 20;
    app.library.count = u64::MAX;
    let before = app.sets.len();
    let (release, wait) = std::sync::mpsc::channel();
    let blocked = app
        .service
        .background(move |_| {
            wait.recv().unwrap();
            Ok(())
        })
        .unwrap();
    app.create_set();
    app.create_set();
    assert!(app.creating);
    assert_eq!(app.background.len(), 1);
    let output = ctx.run_ui(egui::RawInput::default(), |ui| app.draw(ui));
    assert!(!button_is_enabled(&ctx, &output, "Create Set  >"));
    output.drop_without_applying_deltas();
    release.send(()).unwrap();
    blocked
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    settle(&mut app);
    assert!(!app.creating);
    assert_eq!(app.sets.len(), before + 1);
    assert_eq!(app.library.count, u64::MAX);
    assert_eq!(app.screen, Screen::Training);
}

fn check_visible_action_text(
    shape: &egui::epaint::Shape,
    clip: egui::Rect,
    viewport: egui::Rect,
    expected: &[&str],
    found: &mut Vec<String>,
) {
    match shape {
        egui::epaint::Shape::Vec(shapes) => {
            for shape in shapes {
                check_visible_action_text(shape, clip, viewport, expected, found);
            }
        }
        egui::epaint::Shape::Text(text) if expected.contains(&text.galley.job.text.as_str()) => {
            let rect = text.galley.rect.translate(text.pos.to_vec2());
            assert!(
                clip.expand(1.0).contains_rect(rect),
                "clipped {}: {rect:?} by {clip:?}",
                text.galley.job.text
            );
            assert!(
                viewport.contains_rect(rect),
                "offscreen {}: {rect:?}",
                text.galley.job.text
            );
            found.push(text.galley.job.text.clone());
        }
        _ => {}
    }
}

fn feedback_engine(state: State, walkthrough_complete: bool) -> Engine {
    let mut puzzle = crate::puzzle::fixture();
    if state == State::Invalid {
        puzzle.fen.clear();
    }
    let mut engine = Engine::new(&puzzle, Duration::ZERO, false);
    engine.update(Duration::from_secs(1));
    match state {
        State::Complete => demo::solve(&puzzle, &mut engine),
        State::Skipped => engine.skip().unwrap(),
        State::FailedReview => {
            engine.play("h5f6", Duration::from_secs(1)).unwrap();
            engine.update(Duration::from_secs(2));
            let walkthrough = engine.walkthrough().unwrap();
            if walkthrough_complete {
                while !walkthrough.is_complete() {
                    walkthrough.next();
                }
            }
        }
        _ => {}
    }
    engine
}

#[test]
fn solving_actions_are_visible_in_the_viewport_for_every_size_and_feedback_state() {
    let ctx = egui::Context::default();
    let mut app = Woodpeck::new(
        &ctx,
        demo::service().unwrap(),
        PathBuf::from("/tmp/demo.db"),
        None,
    );
    settle(&mut app);
    app.navigate(Screen::Training);
    settle(&mut app);
    for (width, height) in [
        (1280.0, 800.0),
        (1920.0, 1080.0),
        (1366.0, 768.0),
        (900.0, 600.0),
        (600.0, 1000.0),
        (360.0, 640.0),
    ] {
        for (state, walkthrough_complete) in [
            (State::PlayerTurn, false),
            (State::Complete, false),
            (State::FailedReview, false),
            (State::FailedReview, true),
            (State::Skipped, false),
            (State::Invalid, false),
        ] {
            for save_error in [false, true] {
                app.session.as_mut().unwrap().engine = feedback_engine(state, walkthrough_complete);
                app.record_error = save_error;
                let viewport =
                    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
                let mut expected = vec!["× Exit", "Open on Lichess"];
                expected.push(match state {
                    State::Complete | State::Skipped => "Next puzzle  >",
                    State::FailedReview => "Skip solution & next puzzle",
                    State::Invalid => "Report and continue",
                    _ => "Skip puzzle",
                });
                if save_error {
                    expected.push("Retry saving");
                }
                for _ in 0..3 {
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(viewport),
                            ..Default::default()
                        },
                        |ui| app.draw(ui),
                    );
                    let mut found = Vec::new();
                    for shape in &output.shapes {
                        check_visible_action_text(
                            &shape.shape,
                            shape.clip_rect,
                            viewport,
                            &expected,
                            &mut found,
                        );
                    }
                    for text in &expected {
                        assert!(
                            found.iter().any(|s| s == text),
                            "missing {text} at {width}x{height}, {state:?}"
                        );
                    }
                    output.drop_without_applying_deltas();
                }
            }
        }
    }
}
