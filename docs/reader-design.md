# Book reading layout

The reader uses a neutral paper surface, Source Serif 4 at 20 px with 30 px
leading, and a centered measure of 660 px (about 65–75 characters). A short
paragraph gap replaces the old spacing between physical PDF lines. Chapter
titles use a larger regular serif; sections use the bundled semibold face;
subtitles and captions stay quieter. Move notation retains the existing face
and clickable position links.

The analysis pane is bounded at 420 px so wide windows give the book room to
breathe. Narrow windows stack the reading page and a readable board; scrolling
reaches the board controls and variations. Diagrams and their captions sit in
the text flow. The compact toolbar groups page navigation, contents and reading
appearance. `Aa` saves text size (16–28 px) and Auto, Light, Sepia or Dark paper in
the existing preferences. Auto follows the application's light/dark mode.
Reading progress and keyboard navigation keep their existing behavior.

## PDF text and positions

New PDF imports use line geometry, indentation, spacing, sentence endings and
captions to reconstruct paragraphs. Explicit end-of-line hyphens are removed;
a missing hyphen is repaired only when the joined word occurs independently on
the page and neither substantial fragment is independently attested. Ambiguous
splits stay unchanged. EPUB paragraphs continue to follow the HTML.

Older PDF imports receive presentation-only reflow. A UTF-8 offset map preserves
links to verified moves after text is joined. Headings, images, diagrams, score
lines and unresolved move passages are boundaries. Neither the resolver nor the
diagram classifier changes. New imports carry a backward-compatible paragraph
marker so their geometry-derived paragraph boundaries remain authoritative.

The reader retains text runs for six pages and a paragraph galley with move hit
regions for each cached block. Width, font size, pixel density and paper colors
invalidate the affected layout; selecting another move only changes its painted
highlight. Evicting a page also releases its paragraph layouts.

## Review images

These captures contain only the original study notes in `src/app/demo.rs`,
covered by the repository license. No imported or private book is shown.
The before images use release 0.4.0 (`b3b71b7`).

| Window | Before | After |
| --- | --- | --- |
| 1280 × 800 | [Before](screenshots/reader-book/before-1280x800.png) | [After](screenshots/reader-book/after-1280x800.png) |
| 1920 × 1080 | [Before](screenshots/reader-book/before-1920x1080.png) | [After](screenshots/reader-book/after-1920x1080.png) |
| 360 × 640 | [Before](screenshots/reader-book/before-360x640.png) | [After](screenshots/reader-book/after-360x640.png) |

Reproduce a capture with a virtual display large enough for the requested window:

```sh
CARGO_BUILD_JOBS=2 cargo build --no-default-features
xvfb-run -a -s '-screen 0 1920x1080x24' target/debug/woodpeck \
  --screenshot reader.png --screen reader --size 1920x1080
```

`reader_frame_measurements` reports CPU p50/p95/max for idle frames, move
navigation and alternating pages at all three window sizes, including egui
tessellation and excluding GPU presentation. Run it with
`cargo test --no-default-features reader_frame_measurements -- --ignored --nocapture`.
The Criterion suite also includes `reader/idle`, `reader/moves` and `reader/pages`.
