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
same document without changing the resolver or storage API. OCR, PDF rasterization
and DRM handling are outside this implementation.

`diagram` reads diagram images on the CPU. It locates the board from the square
alternation, resamples each square to 32x32, and compares compact features with
256 bundled prototypes (`diagram_prototypes.bin`, 82 KB, nearest prototype per square
colour). A board is accepted only when every square is close to a prototype and clearly
nearer to it than to any other label, the whole image looks like a known style, and the
material is plausible (one king each, at most eight pawns, none on the first or last
rank). Engraved kings and queens differ mostly in the crown, so when a side shows two
kings the one clearly closer to that side's queen prototypes becomes the queen; the
separation is gated like any other square margin. Anything else abstains and stays an
image. The resolver then needs the side to move from the text ("Black to play", "It is
Black's move", "12. ..."), looking at the following paragraphs and the next page. The
first move number followed by a move legal for the side it names decides; other numbers
(ranks, example numbers) are skipped, a move legal only for the other side abstains, and
conflicting statements abstain. Without any evidence the image is kept. A recognized image becomes
a `RecognizedDiagram` block whose position has no castling or en passant rights, and its
move numbers are taken from the first following move. `DiagramRecognizer` is the seam
for an OCR or neural recognizer. Prototypes are rebuilt with
`cargo run --release --example train_diagrams -- <dir with Gutenberg EPUBs>` from the
labels in `tests/books/diagram-labels/` plus synthetic boards.

`notation` provides token spans, piece-alphabet tables, algebraic normalization and
legal-candidate descriptive matching. `resolve` scores the book under each alphabet
by recovered legal moves. An unresolved ambiguity remains text. A section starts at
a heading; validated FENs and recognized text diagrams seed positions, numbered
openings start normally, and known plies allow continuation and analysis branches.
Every position a game reached is remembered by ply. A fragment that starts at move N
(white or black) resumes from the position before that ply: the current one inside a
line, otherwise the single known position there, so a game resumes after prose and an
analysis that jumps back becomes a variation of the node it branches from. When several
lines reach that ply, the one whose next moves stay legal longest (up to eight plies) is
taken, and a tie abstains with "Ambiguous continuation". Labels reset the context:
"Game 12" or a Roman numeral before move one starts from the opening position, "Example
3" or "Position 4" waits for its own diagram, and "Diagram 5" or "page 40" are only
references. Text boards printed as rows ("8 | #R | #Kt| ... |", `#` black, `^` white,
`Kt` or the book's letters, a bare mark a pawn) seed a position like a recognized image;
their caption heading ("DIAGRAM 1") keeps it, and moves numbered from one that are legal
only from the opening position are taken as the game that led to the board. A board that
cannot be placed still blocks inference, and its rank digits are never move numbers.
PGN pasted into a book keeps its tag pairs (`[PlyCount "39"]`) out of the moves: a header
starts a new game from the opening position, or from its `[FEN]` tag when one is given.
A move that closes a sentence ("2 P-g4.") keeps its full stop out of the notation.
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
or diagram FENs, and show fallback reasons for unresolved fragments. Positions read from
images are labelled as such in the reader.

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
