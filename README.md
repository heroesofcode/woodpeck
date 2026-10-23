# woodpeck

A local Woodpecker Method chess puzzle trainer, written in Rust. Build a fixed
set, repeat it across cycles, and solve misses in a separate review queue.
Puzzles from the Lichess puzzle database (CC0).

Milestone 1 implements the core library. The executable intentionally opens no
window. Rust 1.97 or newer is required; the crate uses edition 2024. Native
`eframe` dependencies already include Glow, Wayland, X11, and default fonts.

Run the quality checks with:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

There are 92 unit tests, with no network-dependent tests. `unsafe` is forbidden;
Clippy pedantic is enabled in Cargo.toml. SQLite is bundled and HTTPS uses rustls.
If the environment makes the default Cargo cache read-only, prefix Cargo commands
with `CARGO_HOME=/tmp/woodpeck-cargo`.

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
- `source`: streaming quality-filtered CSV/zstd input, fixture-tested Lichess batch
  reconstruction, attribution, links, and SAN PGN export.
- `app`: reserved for the desktop UI milestone.

Record outcomes through `Store::record_attempt`. The attempt, review transition,
and cycle completion are committed in one transaction. Wrong and skipped cycle
answers immediately enqueue a review item. Correct cycle answers leave pending
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

The default database path is `$XDG_DATA_HOME/woodpeck/woodpeck.db`, with the usual
`$HOME/.local/share` fallback on Linux. `Store::memory`, explicit database paths,
and `database_path_for` support isolated tests and portable use. Settings live in
SQLite. Theme discovery prefers `$HOME/.local/state/omarchy/current/theme` over
the legacy `.config` location. `ThemePoller` re-discovers the directory every two
seconds and should be called by a worker in the desktop shell.

Later milestones add the application window and stable app ID, screens and board
interaction, pieces and animation rendering, theme-to-egui wiring, background
import/download jobs with cancellation and rate-limit handling, CLI import,
starter-pack installation, sound, and packaging. The desktop entry is already in
`assets/woodpeck.desktop`. No starter pack or piece artwork is bundled yet.

This milestone's execution environment makes this checkout's `.git` read-only.
Focused Conventional Commits are preserved in `milestone-1.bundle`, made from a
writable temporary history copy. The working files here match that history.
When `.git` becomes writable, attach the history while retaining these files:

```sh
git fetch ./milestone-1.bundle feat/woodpeck-app
git reset --mixed FETCH_HEAD
```

License: GPL-3.0-or-later, compatible with shakmaty and pgn-reader. Puzzle data is
CC0. Omarchy palette examples and the resolver fixture were supplied with the
build brief; the shell fixture verifies parity with the official cascade.
