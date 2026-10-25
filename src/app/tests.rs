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
    app.refresh_library();
    settle(&mut app);
    assert_eq!(app.library.count, count);
    assert_eq!(app.library.bands, bands);
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
