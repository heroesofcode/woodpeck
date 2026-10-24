use super::*;
use crate::layout;
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
    app.navigate(Screen::Training);
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
    for (width, height) in [
        (360.0, 640.0),
        (640.0, 900.0),
        (960.0, 1080.0),
        (1920.0, 1080.0),
    ] {
        let layout = layout::calculate(width, height);
        for (screen, _) in Screen::ALL {
            app.navigate(screen);
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
        assert_eq!(app.screen, Screen::Builder);
        assert_eq!(app.library.step, expected);
        if expected == library::WelcomeStep::Done {
            assert_eq!(app.library.progress.accepted, 2);
        } else {
            assert_ne!(app.library.message, "");
        }
    }
}
