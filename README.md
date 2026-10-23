# woodpeck

A native **Woodpecker Method chess puzzle trainer**, inspired by [Peck Chess](https://peckchess.com). Build a fixed set of positions, repeat it over several cycles, and watch your solving time and accuracy improve. No login; your sets, results, and settings stay in a local SQLite database.

## Features

- **A review queue that catches every miss.** Wrong answers and skips enter Review immediately, with a pending badge in navigation and a dashboard callout. Filter by theme or sort by recency and accuracy. Only a correct solve in Review clears an item; review attempts update lifetime accuracy without changing cycle results. The puzzle stays in its set for future cycles.
- **Sets and presets.** General, tactical weakness, and rating presets; multi-theme filters that include and exclude Lichess themes; 20–500 positions; an inclusive rating range (400–3200, at least 100 points wide) set with a two-handle slider, number fields or quick ranges around your rating; and 1–10 cycles. Fixed snapshots preserve your positions when the puzzle library changes. Optional seeded shuffling changes the order each cycle.
- **Cycles and a count-up timer.** Resume across restarts, compare cycles, and step through the solution after a mistake. The timer pauses during animations, feedback, walkthroughs, promotion, and loss of focus. Optional completion sound and auto-advance.
- **Progress.** Time and accuracy charts, comparisons with cycle one, performance by theme, and most-missed positions. Small local streak, XP, and rank widgets.
- **Omarchy theme sync with live reload.** Desktop colors and fontconfig fonts, with two-second theme polling that follows directory and symlink swaps. Built-in dark and light themes and eight board palettes.
- **Responsive tiling layout.** Native Wayland and X11, a sidebar that collapses into a rail or top bar, and a board/panel layout that stacks below 900 px. Supports narrow 360 px tiles, wide displays, and HiDPI. SVG pieces, click/click and drag/drop, legal targets, and promotion selection.
- **Puzzles ready offline.** An embedded Lichess starter pack of 8,540 puzzles imports on first launch. Stream the full database from Settings or the CLI. If a requested set has too few local matches, the builder offers a Lichess API fallback. Export your sets as PGN from Settings.

Puzzles from the Lichess puzzle database (CC0).

## Screenshots

Screenshots use an isolated demo database, with real training and review state.

| Dashboard | Training |
| --- | --- |
| ![Dashboard with review callout and saved set](docs/screenshots/dashboard.png) | ![Training board and cycle panel](docs/screenshots/training-wide.png) |
| Review queue | Progress |
| ![Review board, filters, and pending cards](docs/screenshots/review-wide.png) | ![Cycle comparisons and progress charts](docs/screenshots/progress.png) |
| Set builder | Narrow training, 360×640 |
| ![Preset picker and set builder](docs/screenshots/builder.png) | ![Training in a narrow tile](docs/screenshots/training-narrow.png) |

## Build and install

Requires Rust **1.97 or newer** (edition 2024). `rust-toolchain.toml` selects stable with rustfmt and Clippy. SQLite is bundled and HTTPS uses rustls.

Arch / Omarchy:

```sh
sudo pacman -S --needed rust wayland libxkbcommon alsa-lib fontconfig
```

Debian / Ubuntu:

```sh
sudo apt-get update
sudo apt-get install --no-install-recommends -y \
  build-essential pkg-config libxkbcommon-dev libwayland-dev libx11-dev \
  libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev libegl1-mesa-dev \
  libasound2-dev libfontconfig1-dev
```

Build from source:

```sh
git clone https://github.com/heroesofcode/woodpeck.git
cd woodpeck
cargo build --release --locked
./target/release/woodpeck
```

Install the binary, desktop entry, and icon for your user:

```sh
install -Dm755 target/release/woodpeck "$HOME/.local/bin/woodpeck"
install -Dm644 assets/woodpeck.desktop "$HOME/.local/share/applications/woodpeck.desktop"
install -Dm644 assets/woodpeck.svg "$HOME/.local/share/icons/hicolor/scalable/apps/woodpeck.svg"
```

Ensure `$HOME/.local/bin` is in your `PATH`. The starter pack and chess pieces are embedded in the binary, so no asset directory is needed at runtime.

The default `sound` feature uses ALSA on Linux; playback happens in the background and quietly skips unavailable audio devices. To omit audio, build with `cargo build --release --locked --no-default-features`.

## Usage

Start with **New set**, choose a preset, optionally adjust the themes, then create a set. **Continue** resumes its current cycle. After a wrong move, walk through the missed line or advance; the item is already in **Review**. Use Review regularly and solve each queued puzzle correctly to clear it. **Progress** compares completed cycles. **Settings** controls themes, auto-advance, sound, tactical labels, imports, and PGN export.

### Choosing themes

The builder's **Themes** picker lists Lichess puzzle themes; type in the search box to narrow it. Click a theme once to **include** it (green), again to **exclude** it (red, struck through), and a third time to clear it. Selected themes appear as chips above the search box; click a chip to remove it, or **Clear all**.

- No included themes: any theme qualifies.
- With two or more included themes, choose **Match any** (a puzzle needs at least one) or **Match all** (it needs every one).
- Excluded themes always win: a puzzle with any excluded theme is never picked.

Weakness presets start with their theme included, and you can add more from there. If the local library and the Lichess fallback can't fill the set, woodpeck creates nothing and tells you how many matching puzzles it found. It never saves a partial set. The fallback asks Lichess for one included theme at a time, rotating through them, and applies the full filter locally. Dashboard cards show each set's themes.

| Key | Action |
| --- | --- |
| `S` | Skip on the player's turn; add to Review |
| `T` | Toggle timer visibility |
| `←` | Previous move in the missed-line walkthrough |
| `→` / `Space` | Next move in the walkthrough |
| `Enter` / `Space` | Next puzzle after completion |

Shortcuts pause while typing, while a menu/dialog or promotion picker is open, or while modifiers are held. Promote using the on-screen Q/R/B/N picker.

### Import puzzles

```sh
woodpeck import /path/to/lichess_db_puzzle.csv.zst
woodpeck import /path/to/puzzles.csv
woodpeck import https://database.lichess.org/lichess_db_puzzle.csv.zst
woodpeck import --lichess
woodpeck --help
```

The Settings download button uses the same streaming importer. Imports accept plain or zstd-compressed Lichess CSV, filter for popularity ≥80, rating deviation ≤100, ≥500 plays, and exclude `veryLong` puzzles. Invalid chess lines are rejected. Progress reports bytes and rows. Cancelling in Settings retains committed batches of 1,000 accepted rows and drops the unfinished batch.

The API fallback is offered explicitly when local generation is short; it makes sequential requests and waits at least 60 seconds after HTTP 429. Requests use the Lichess difficulty levels that overlap your range, and the results are filtered locally to the exact range. The range is never widened silently. If too few puzzles match, the builder shows the shortfall so you can widen the range, change the themes, or fetch more. An incomplete set is never saved.

### Desktop integration and local data

The window title, X11 class, and Wayland app ID are `woodpeck`. An optional Hyprland rule for configurations using the classic rule syntax:

```ini
windowrule = float, class:^(woodpeck)$
```

Keep normal tiling to use the responsive layout. Rule syntax depends on your Hyprland version.

| Data | Linux location |
| --- | --- |
| Database, sets, attempts, review queue, settings | `$XDG_DATA_HOME/woodpeck/woodpeck.db` |
| Default database when XDG is unset | `$HOME/.local/share/woodpeck/woodpeck.db` |
| Optional database directory override | `$WOODPECK_DATA_DIR/woodpeck.db` |
| Current Omarchy theme | `$HOME/.local/state/omarchy/current/theme/` |
| Legacy Omarchy theme fallback | `$HOME/.config/omarchy/current/theme/` |

Settings are stored in SQLite. Theme discovery reads `colors.toml`, falls back to legacy `alacritty.toml`, and supports Omarchy's color aliases and mode detection. System follows Omarchy when available and otherwise uses the built-in dark palette. Choose Light or Dark to override it.

## Sync across computers

Sync is opt-in. Use a **PRIVATE repository you own**: the repository contains
training history, review mistakes, preferences, and the complete puzzles in
your sets. Anyone with repository access can read that history. Git sync is not
end-to-end encryption. The full puzzle library, local paths, window settings,
audio devices, and sync credentials/configuration are not synced.

With the GitHub CLI installed and authenticated, create or reuse your private
`woodpeck-data` repository:

```sh
gh auth login
gh auth setup-git
woodpeck sync init --create
```

Alternatively, create an empty private repository manually and initialize it:

```sh
woodpeck sync init 'https://github.com/<user>/woodpeck-data.git'
# Or use your configured SSH key:
woodpeck sync init 'git@github.com:<user>/woodpeck-data.git'
```

Run the same `sync init <remote-url>` on each computer. Imported sets include
all positions and solutions, so they work without downloading the Lichess
library. Sync includes sets, cycles, every attempt, review state, preferences,
and reconstructed XP/streaks. Git must be on PATH. Authentication uses your
system Git credential helpers (`gh auth setup-git` for GitHub HTTPS), or SSH.
woodpeck never stores tokens; do not put credentials in the remote URL. Commits
use the fixed identity `woodpeck <woodpeck@localhost>` inside the data repository.
Your global Git identity and source checkout are unchanged.

Use Settings → Sync to edit the URL, enable/disable sync, sync now, and inspect
the last sync and error. The navigation indicator reports pending, syncing,
synced, offline, conflict resolved, or error. CLI equivalents:

```sh
woodpeck sync now
woodpeck sync status
woodpeck sync disable
```

Enabled startup pulls in the background. Changes sync after a 10-second pause;
closing the app makes a bounded best-effort attempt. Offline training continues
normally, with pending changes retried automatically and on next startup. A
manual sync reports connection or authentication errors instead of prompting
for a password.

Two computers' progress is merged semantically: attempts are retained by UID,
metadata and preferences use timestamps per field, and review items clear only
when a correct review solve is later than their latest miss. Keep system clocks
accurate. Equal-time review conflicts remain pending. Git text conflicts need
no manual editing. Sets are merged as a union: deleting a local set can restore
it from remote history on the next sync. A newer remote format stops sync until
you upgrade; for example, multi-theme sets use format 2, which older releases refuse rather than misread. Current releases upgrade format 1 data automatically. Disabling leaves local progress and existing private Git history
in place; it does not delete the remote repository.

Local sync files live in `$XDG_DATA_HOME/woodpeck/sync/` (or the platform data
directory / `$WOODPECK_DATA_DIR` override), beside SQLite. Keep this directory
outside the woodpeck source tree. See [the versioned format](docs/sync-format.md)
for the portable schema and merge rules.

## Development

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo clippy --all-targets --no-default-features --locked -- -D warnings
cargo test --all-features --locked
cargo test --no-default-features --locked
cargo build --release --locked
cargo install cargo-deny --locked
cargo deny check
```

Clippy pedantic is enabled; unsafe code is forbidden. Tests use fixtures, isolated SQLite databases, and loopback HTTP servers. The CI aggregate check is named `ci` and requires formatting, both Clippy configurations, tests, the release build, and supply-chain checks to succeed. PR titles use Conventional Commits.

Generate a screenshot without opening your real database (requires Xvfb and `xauth`; on Debian/Ubuntu install `xvfb xauth`):

```sh
xvfb-run -a -s '-screen 0 1920x1080x24' \
  target/release/woodpeck --screenshot /tmp/training.png --screen training --size 1920x1080
```

Validate every screen at common tiling sizes:

```sh
mkdir -p /tmp/woodpeck-screenshots
for screen in dashboard training review progress builder settings; do
  for size in 360x640 960x1080 1920x1080; do
    xvfb-run -a -s '-screen 0 1920x1080x24' \
      target/release/woodpeck --screenshot "/tmp/woodpeck-screenshots/$screen-$size.png" \
      --screen "$screen" --size "$size"
  done
done
```

The code separates the puzzle engine, injected timer, set generation, review rules, progress calculations, streaming sources, SQLite repositories, and background application service from the native UI in `src/app/`.

## Credits and licenses

- Inspired by [Peck Chess](https://peckchess.com) and the Woodpecker Method.
- Puzzles from the [Lichess puzzle database](https://database.lichess.org/#puzzles), released under [CC0](https://creativecommons.org/publicdomain/zero/1.0/). This includes the bundled starter pack.
- **cburnett chess pieces** by Colin M.L. Burnett, from [lichess-org/lila](https://github.com/lichess-org/lila/tree/master/public/piece/cburnett), dual licensed **GPLv2+ / CC BY-SA 3.0**. See [assets/pieces/LICENSE](assets/pieces/LICENSE). Pieces are rasterized locally with resvg.
- Built with Rust, eframe/egui, shakmaty, pgn-reader, rusqlite, and other open-source libraries listed in `Cargo.toml` and `Cargo.lock`.
- woodpeck is licensed under **GPL-3.0-or-later**. See [LICENSE](LICENSE).
