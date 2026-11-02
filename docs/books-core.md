# Book core

Books use `crate::books` and independent SQLite tables. The puzzle library and its
sync format are unchanged. The desktop Library and reader are implemented in `src/app/books.rs` and
`src/app/reader.rs`. Imports extract and resolve on a dedicated thread; storage
operations use the serial local worker.

`document` retains page text, block kinds, source spine paths, and embedded image
assets. Spans are half-open UTF-8 byte ranges into the page's stored text. EPUB
paragraph text is whitespace-normalized; preformatted diagrams retain line breaks.
Spine items paginate at major headings and a 6,000-byte block budget. A single large
paragraph/table remains intact. This is text pagination, not the publisher's physical
page numbering. The original book file is not needed to access retained EPUB images.

`import::PageSource` returns the format-independent document. `TextImporter` detects
formats by magic bytes, reads EPUB container/OPF spine order with a lenient HTML parser,
and extracts PDF glyphs through pdf-extract/lopdf. Coordinates determine line order;
a repeated wide gutter separates two text columns while short White/Black tables
remain row-major. More complicated layouts, rotated text and marginal notes still
need evaluation. PDFs with fewer than eight alphanumeric characters on a majority of
pages return the typed `NoTextLayer` error. A future OCR page source can produce the
same document without changing the resolver or storage API. OCR, image diagram
recognition, PDF rasterization and DRM handling are outside this implementation.

`notation` provides token spans, piece-alphabet tables, algebraic normalization and
legal-candidate descriptive matching. `resolve` scores the book under each alphabet
by recovered legal moves. An unresolved ambiguity remains text. A section starts at
a heading; validated FENs and recognized text diagrams seed positions, numbered
openings start normally, and known plies allow continuation and analysis branches.
Nested delimiters save and restore line contexts. Failed moves stop their branch,
retain a reason and preserve the successfully validated prefix. Invalid explicit
FENs block inference until a new section or valid explicit position. Descriptive
source-piece qualifiers are conservative once pieces have moved: without enough
identity information, ambiguity falls back. Prose and malformed text are not PGN;
the evaluation suite measures how much is actually recovered.

`model::Book` is a versioned serde value containing sections, fragment status, and
nodes. Each node contains a parent, English SAN, UCI, resulting FEN, page/span, NAGs,
and comment. Children can be found by parent ID; their source order preserves the
mainline and alternatives. Node IDs are indices stable within the persisted model.
`text_runs(page)` yields the complete page text, splitting clickable moves from
ordinary text. Diagram blocks directly carry FENs; image blocks refer to retained
asset paths and alt text. A reader should display boards only for validated nodes
or diagram FENs, and show fallback reasons for unresolved fragments.

Schema 6 adds `books` (SHA-256 deduplication, metadata and zstd-compressed JSON) and
`book_progress`. Imports of an existing hash preserve the model and progress, so
stored node IDs stay valid. Progress verifies the page and node before saving;
older updates do not overwrite newer ones. Books remain local and are not synced.
The CLI supports `book import <path>` and `book eval <directory>`.

ZIP reads enable only the pure-Rust deflate backend; HTML, PDF and hash dependencies
use permissive licenses. EPUB decompression is bounded per entry and per document.
The dependency policy documents the sole maintenance-status exception for the font
metrics dependency of the current lopdf release. Known vulnerability checks remain
active. See `tests/books/README.md` for measurement scope and reproduction.

The expanded evaluation is documented in `tests/books/README.md`. `coverage` detects
surface move-like spans independently of notation resolution, including undetected
spans in its denominator. Reports separate excerpt accuracy from whole-book fragment
membership and retain exact fallback reasons and source snippets. A directory needs
no annotations to produce coverage metrics. Unrecognized image blocks invalidate
position context: an apparently legal `1 P-K3` must not silently become an initial
opening when the source supplies an unread diagram. Parenthesized move numbers and
older algebraic piece/pawn prefixes are accepted, and a failed spaced descriptive
phrase cannot skip its piece name and reinterpret a later word as another move.

The reader uses source-order children for mainline and variation choices and follows
node page references when stepping. Reading locations are debounced for 450 ms,
flushed when leaving the reader and queued on shutdown. The puzzle database
onboarding remains in `app/library.rs`; the book collection is separate. Screenshots
`library` and `reader` use original in-memory study notes in the isolated demo store.

Native file selection was evaluated with `rfd` 0.15.4, default features disabled,
and `xdg-portal` plus `async-std` (the pure Rust Linux portal backend). Its
`ashpd` / `zbus` dependency chain fails the advisory policy on
RUSTSEC-2026-0370, RUSTSEC-2026-0371, and RUSTSEC-2026-0372. The path input
remains available; no dependency or advisory exception was added. Revisit when
a pure Rust portal release removes those deprecated dependencies.
