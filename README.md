# woodpeck

A local Woodpecker Method chess puzzle trainer, written in Rust. Build a fixed
set, repeat it across cycles, and solve misses in a separate review queue.
Puzzles from the Lichess puzzle database (CC0).

Milestone 3 adds the native desktop: Dashboard, Train, Review, Progress, New set,
and Settings, all backed by the persisted application service. Rust 1.97 or newer is required (edition 2024).
Native `eframe` dependencies include Glow, Wayland, X11, and default fonts.

```sh
cargo run -- --help
cargo run -- --version
cargo run                         # open the desktop; starter import runs in the background
cargo run -- import puzzles.csv.zst
cargo run -- import https://example.org/puzzles.csv.zst
cargo run -- import --lichess      # stream the full Lichess database
```

Imports accept plain CSV or zstd, including the skippable frame prefix in the
full Lichess dump. Columns are read by name; the current 11-column format,
10-column format, reordered headers, and omitted trailing fields are supported.
Quality filtering requires popularity ≥80, rating deviation ≤100, ≥500 plays,
and no `veryLong` tag. Invalid chess lines are rejected. Imports commit batches
of 1,000 accepted rows; cancellation retains committed batches and drops the
unfinished batch. Progress includes compressed bytes consumed and rows scanned,
accepted, and rejected. The full dump is downloaded as a stream, never buffered
in memory. The embedded CC0 starter pack contains 8,540 puzzles; its version
marker advances only after success, and force import restores its source rows.

Run the quality checks with:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy --all-targets --no-default-features -- -D warnings
cargo test --all-features
cargo test --no-default-features
```

Tests run offline using fixtures and loopback HTTP servers. `unsafe` is forbidden;
Clippy pedantic is enabled in Cargo.toml. SQLite is bundled and HTTPS uses rustls.
The default `sound` feature uses rodio's playback support. Linux sound builds
require ALSA development files and pkg-config (Debian: `libasound2-dev pkgconf`).
Use `--no-default-features` to omit audio entirely. Device discovery and the
short generated sine chime run on a separate thread; unavailable devices are
ignored. Tests validate the generated samples without opening an audio device.

The library modules are:

- `puzzle`: full-line validation using shakmaty, orientation, promotion, alternative
  final checkmate, injected animation deadlines, immediate failure, and walkthrough.
- `timer`: injectable monotonic clock, independent pause gates, minimum recorded
  time, visibility preference, and minute/second/tenth formatting.
- `layout`: pure responsive geometry for stacked/column layouts and navigation.
- `sets`: exact presets and labels, builder validation, estimated time, recommended
  bands, and seeded rating-bucket generation through `PuzzleQuery`.
- `stats`: resumable cycles, deterministic shuffling, extension through ten cycles,
  accuracy, time deltas, best completed cycle, theme performance, and most misses.
- `review`: one entry per puzzle and set, shared membership transition rules,
  theme filtering, sorting, card data, and lifetime accuracy.
- `theme`: the complete Omarchy alias cascade, mode detection, legacy Alacritty,
  injectable HOME discovery, polling through theme swaps, optional fontconfig,
  semantic palette values, and all eight OKLCH board themes.
- `settings`: serialized preferences with defaults for missing keys.
- `store`: versioned SQLite migrations and repositories for source puzzles,
  immutable set snapshots, cycles, attempts, review, and settings.
- `source`: streaming CSV/zstd import, versioned embedded starter pack, Lichess
  batch/daily/by-id client, PGN reconstruction, attribution, links, and PGN export.
- `service`: UI-independent application operations plus a serial background
  worker with cancellation handles and polled channel events.
- `motivation`: local calendar-day streaks, XP, levels, and ranks.
- `sound`: optional generated completion chime.
- `app`: reserved for the desktop UI milestone.

Record outcomes through `Store::record_attempt`. The attempt, review transition,
cycle completion, and streak/XP changes are committed in one transaction. Wrong
and skipped cycle answers immediately enqueue a review item. Correct cycle answers leave pending
review items alone; only a correct Review attempt clears them. Review attempts
update lifetime accuracy and never cycle statistics. Duplicate cycle attempts,
out-of-order answers, cross-set cycle references, and reviews of nonpending
items are rejected. Set snapshots remain independent of source database updates.

Builder variance is a half-width: default ±100 corresponds to the source form's
200-point full width. Difficulty captions use the inferred thresholds documented
in `sets`. Unavailable outcomes are tracked separately and excluded from accuracy;
skips are misses. General generation enforces a 30% cap on every theme tag, even
when multiple tags describe the same puzzle. A shortage is reported explicitly
if the cap cannot be met after widening. Selection is deterministic for a seed,
with 32 candidate orderings tried per band to reduce greedy-selection bias.

`WOODPECK_DATA_DIR` overrides the application data directory (the database is
`$WOODPECK_DATA_DIR/woodpeck.db`). Otherwise, the default database path is `$XDG_DATA_HOME/woodpeck/woodpeck.db`, with the usual
`$HOME/.local/share` fallback on Linux. `Store::memory`, explicit database paths,
and `database_path_for` support isolated tests and portable use. Settings live in
SQLite. Theme discovery prefers `$HOME/.local/state/omarchy/current/theme` over
the legacy `.config` location. `ThemePoller` re-discovers the directory every two
seconds on a dedicated desktop worker, including directory/symlink swaps.

`AppService::open` (or `new` with an existing `Store`) starts a serial worker
and automatically queues the starter import. Call `poll_events` each frame to
receive job start, import/fetch progress, and completion/error events. `submit`
returns a cancellation handle; `cancel_job` also accepts the startup event ID.
Dropping the service cancels pending jobs. Database locks cover only bounded
writes and local operations; downloads and rate-limit waits never hold them.

The service exposes set summaries, builder creation results with shortfall and
widening information, start/resume/current puzzle, next/extended cycles,
attempt recording, review filtering/sorting/counts, progress reports, settings,
motivation, PGN export to a path, and transactional set deletion. Source updates
and deletion never change other sets' snapshots or erase earned XP.

Set generation first uses the local database. When it reports a shortfall, the
UI can inform the user and enqueue `CreateSetWithFallback`. The worker fetches
at most five batches, filters by the requested band/theme, inserts valid rows,
and regenerates using the same selection rules. Remaining shortfalls never
save a partial set. Anonymous API difficulty is mapped relative to rating 1500;
requests are sequential across clients, use `woodpeck/<version>`, have connect,
response, and read timeouts, and wait 60 seconds after HTTP 429. Cancellation
interrupts the wait; an in-flight blocking read is bounded by its timeout.
`PuzzleFetcher` is injectable for offline application tests.

Streaks use local calendar dates and are updated once per active day. Use
`Motivation::streak_on` with today's local day to display an expired streak as
zero. Correct cycle/review solves earn 10 XP; finishing a cycle earns 50 XP.
Level L starts at `100 × (L − 1)²` XP, with the ranks from the functional spec.

The window title and native app ID/class are `woodpeck`, with Wayland and X11
support. Navigation adapts from a sidebar to an icon rail to a top bar below
640 logical pixels. Solving switches to a stacked, scrollable layout below
900 pixels. The board is bounded by both content width and height, with
click/click and drag/drop input, promotion selection, legal targets, check glow,
move animation, and feedback flashes. `S` skips, `T` toggles the timer, arrows
step the missed line, and Enter/Space advance. Shortcuts respect text entry,
menus, modifiers, and promotion. Focus loss and minimizing pause the timer.
Settings persist immediately. PGN export uses a path field; no file-dialog
dependency is required. Full imports show byte/row progress and can be cancelled.
The desktop entry is in `assets/woodpeck.desktop`.

The embedded cburnett SVG pieces are by Colin M.L. Burnett, sourced from
[lichess-org/lila](https://github.com/lichess-org/lila/tree/master/public/piece/cburnett),
dual licensed GPLv2+ / CC BY-SA 3.0. See `assets/pieces/LICENSE`. `resvg` renders
them at the physical square size and caches textures, including HiDPI sizes.
UI fonts are loaded with fontconfig when available, with egui's built-in fallback.

For a deterministic visual self-check, screenshot mode imports the starter into
an isolated in-memory SQLite database, builds a set, and records two full cycles
plus a partial third cycle, including misses and skips. It never opens your real
database. After a few frames it saves a PNG and exits; a failed save returns an
error. Egui 0.37 delivers screenshots through the viewport screenshot callback
(the former screenshot event API was replaced).

```sh
xvfb-run -a -s "-screen 0 1920x1080x24" cargo run -- --screenshot /tmp/training.png --screen training --size 1280x800
xvfb-run -a -s "-screen 0 1920x1080x24" cargo run -- --screenshot /tmp/narrow.png --screen training --size 360x640
# Other screens: dashboard, review, progress, builder, settings
# Other tiles: 640x900, 960x1080, 1920x1080; use a 3840x2160 Xvfb for 4K.
```

The visual validation matrix used for milestone 3 (after building the binary):

```sh
for screen in dashboard training review progress builder settings; do
  for size in 1280x800 360x640; do
    xvfb-run -a -s "-screen 0 1920x1080x24" target/debug/woodpeck --screenshot "/tmp/woodpeck-${screen}-${size}.png" --screen "$screen" --size "$size"
  done
done
for spec in 'training 640x900' 'review 960x1080' 'progress 1920x1080'; do
  set -- $spec
  xvfb-run -a -s "-screen 0 1920x1080x24" target/debug/woodpeck --screenshot "/tmp/woodpeck-$1-$2.png" --screen "$1" --size "$2"
done
xvfb-run -a -s "-screen 0 3840x2160x24" target/debug/woodpeck --screenshot /tmp/woodpeck-dashboard-3840x2160.png --screen dashboard --size 3840x2160
WINIT_X11_SCALE_FACTOR=2 xvfb-run -a -s "-screen 0 3840x2160x24" target/debug/woodpeck --screenshot /tmp/woodpeck-training-hidpi.png --screen training --size 1920x1080
```

Validation includes pure formatting, chart scaling, orientation/hit testing,
shortcut dispatch, SVG rasterization, tile geometry, actual egui width checks for
all screens, and a UI-to-service skip/Review flow. Run `cargo fmt --check`,
`cargo test --all-features`, `cargo test --no-default-features`, and Clippy with
`--all-targets` and `-- -D warnings -W clippy::pedantic` for both feature modes.

License: GPL-3.0-or-later, compatible with shakmaty and pgn-reader. Puzzle data is
CC0. Omarchy palette examples and the resolver fixture were supplied with the
build brief; the shell fixture verifies parity with the official cascade.
