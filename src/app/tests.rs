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
