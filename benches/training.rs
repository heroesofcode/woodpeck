use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use std::{hint::black_box, time::Duration};
use woodpeck::{
    app::bench_support::{PieceTextures, TrainingFrame},
    puzzle::{Engine, Outcome},
    service::AppService,
    sets::{BuilderParams, generate},
    source::{CancelFlag, import_starter},
    stats::{Attempt, AttemptKind},
    store::Store,
};

fn store() -> Store {
    let mut store = Store::memory().unwrap();
    import_starter(&mut store, false, &CancelFlag::default(), |_| {}).unwrap();
    store
}
fn params() -> BuilderParams {
    BuilderParams {
        size: 20,
        ..BuilderParams::default()
    }
}
fn benchmarks(c: &mut Criterion) {
    let store = store();
    let generated = generate(&store, &params(), 42).unwrap();
    let puzzle = &generated.puzzles[0];
    let mut engine = Engine::new(puzzle, Duration::ZERO, true);
    engine.update(Duration::from_secs(1));
    let uci = puzzle.moves.split_whitespace().nth(1).unwrap();
    c.bench_function("engine/validate_move", |b| {
        b.iter_batched(
            || engine.clone(),
            |mut e| {
                black_box(e.play(black_box(uci), Duration::from_secs(2))).unwrap();
            },
            BatchSize::SmallInput,
        );
    });
    c.bench_function("sets/generate_20", |b| {
        b.iter(|| generate(black_box(&store), &params(), 42).unwrap());
    });
    let mut store = store;
    let id = store
        .create_set(&params(), &generated.puzzles, 1, 42)
        .unwrap();
    let cycle = store.start_cycle(id, 1).unwrap();
    let attempt = Attempt {
        puzzle_id: cycle.order[0].clone(),
        set_id: id,
        cycle_id: Some(cycle.id),
        kind: AttemptKind::Cycle,
        outcome: Outcome::Correct,
        time_ms: 1234,
        moves_played: vec![],
        created_at: 2,
    };
    c.bench_function("store/record_attempt", |b| {
        b.iter_batched(
            || {
                let mut s = Store::memory().unwrap();
                let id = s.create_set(&params(), &generated.puzzles, 1, 42).unwrap();
                s.start_cycle(id, 1).unwrap();
                s
            },
            |mut s| black_box(s.record_attempt(&attempt).unwrap()),
            BatchSize::SmallInput,
        );
    });
    let service = AppService::new(store).unwrap();
    c.bench_function("store/next_puzzle", |b| {
        b.iter(|| service.current_puzzle(black_box(cycle.id)).unwrap());
    });
    c.bench_function("pieces/cold_12_80px", |b| {
        b.iter_batched(
            PieceTextures::default,
            |mut p| p.rasterize(80.0),
            BatchSize::SmallInput,
        );
    });
    let mut pieces = PieceTextures::default();
    pieces.rasterize(80.0);
    c.bench_function("pieces/warm_12_80px", |b| {
        b.iter(|| pieces.rasterize(black_box(80.0)));
    });
    let mut idle = TrainingFrame::default();
    c.bench_function("frame/idle", |b| b.iter(|| idle.frame(Vec::new())));
    let mut drag = TrainingFrame::default();
    drag.drag();
    c.bench_function("frame/drag", |b| {
        b.iter(|| {
            drag.frame(vec![eframe::egui::Event::PointerMoved(eframe::egui::pos2(
                390.0, 215.0,
            ))]);
        });
    });
    training_frames(c, "frame", TrainingFrame::default);
    disk_benchmarks(c);
}

fn disk_benchmarks(c: &mut Criterion) {
    if let Some(source) = std::env::var_os("WOODPECK_BENCH_DB") {
        // Copy once per run, outside all measurements. Only the disposable copy is opened.
        let copy = tempfile::Builder::new()
            .prefix("wp-big-copy-")
            .suffix(".db")
            .tempfile()
            .unwrap();
        std::fs::copy(&source, copy.path()).unwrap();
        eprintln!(
            "Disk training: source={}, copy={}",
            std::path::Path::new(&source).display(),
            copy.path().display()
        );
        {
            let store = Store::open(copy.path()).unwrap();
            eprintln!("Disk library: {} puzzles", store.puzzle_count().unwrap());
            // Keep the supplied puzzle pool intact during automatic service startup.
            store
                .set_setting("starter_version", woodpeck::source::STARTER_VERSION)
                .unwrap();
        }
        let fixture = || TrainingFrame::from_database(copy.path());
        training_frames(c, "frame/disk", fixture);
        let (wall, maximum) = fixture().check_set_creation();
        eprintln!(
            "frame/disk/create_set: generation/load wall time={wall:?}, maximum UI frame={maximum:?}"
        );
    }
}

fn training_frames(c: &mut Criterion, prefix: &str, fixture: impl Fn() -> TrainingFrame) {
    // Fresh fixture per completion keeps persistence valid; setup is excluded from timing.
    c.bench_function(&format!("{prefix}/correct_commit"), |b| {
        b.iter_batched_ref(
            || {
                let mut f = fixture();
                f.correct();
                f
            },
            |f| f.frame(Vec::new()),
            BatchSize::PerIteration,
        );
    });
    let mut transition = fixture();
    transition.correct();
    transition.frame(Vec::new());
    transition.transition();
    c.bench_function(&format!("{prefix}/post_correct"), |b| {
        b.iter(|| transition.frame(Vec::new()));
    });
    drop(transition);
    training_transitions(c, prefix, fixture);
}

fn training_transitions(c: &mut Criterion, prefix: &str, fixture: impl Fn() -> TrainingFrame) {
    c.bench_function(&format!("{prefix}/advance"), |b| {
        b.iter_batched_ref(
            || {
                let mut f = fixture();
                f.correct();
                f.frame(Vec::new());
                f.transition();
                f
            },
            TrainingFrame::advance_frame,
            BatchSize::PerIteration,
        );
    });
    let mut observed = Duration::ZERO;
    let mut sequences = 0_u64;
    c.bench_function(&format!("{prefix}/solve_advance_max_frame"), |b| {
        // Criterion estimates the mean of per-sequence maxima, not sequence wall time.
        b.iter_custom(|iterations| {
            let mut total = Duration::ZERO;
            for _ in 0..iterations {
                let mut f = fixture();
                let maximum = f.solve_and_advance();
                total += maximum;
                observed = observed.max(maximum);
                sequences += 1;
                f.settle();
            }
            total
        });
    });
    if sequences > 0 {
        eprintln!(
            "{prefix}: maximum UI frame over {sequences} solve-and-advance sequences: {observed:?}"
        );
    }
}
criterion_group! { name = benches; config = Criterion::default().sample_size(10).warm_up_time(Duration::from_millis(300)).measurement_time(Duration::from_secs(1)); targets = benchmarks }
criterion_main!(benches);
