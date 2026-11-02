# Book evaluation

The offline generated matrix contains 35 distinct historical move records (at least
30 different games), four seeded 160-ply legal walks, and four targeted positions.
`games.py` contains historical PGN move facts, with source URLs and no downloaded
commentary. Players include Morphy, Anderssen, Steinitz, Lasker, Rubinstein and
Capablanca. Eight historical records exceed 60 moves. Both castles, en passant,
promotions, underpromotions, checks, mates, captures and file/rank/full-square
source disambiguation occur across the source set. Some historical sources contain
alternate records of the same encounter; distinct move records are not necessarily
distinct encounters. `inventory.txt` identifies each record and its length.

The Python generator uses only the standard library. Its independent Rust helper
uses shakmaty directly to replay source PGN and generate seeded random legal games;
it never imports the books parser, resolver or importer. Sorted legal UCI moves and
an explicit xorshift64 state make random walks reproducible. The helper also supplies
alternative legal branches and pre-move FENs. All expected UCI sequences are replayed
again by the separate oracle integration test. Neither legal replay nor random walks
establish historical provenance; the handwritten source records supply that.

```sh
cargo build --locked --example books_oracle --example books_diagrams
python3 tests/books/generate.py
cargo test --locked --test books_oracles --test books_eval
cargo run --locked -- book eval tests/books/fixtures
```

Every source is rendered as SAN, LAN, figurines, PT, ES, DE, FR, NL, IT and Polish, plus four
English descriptive styles. Styles include Kt/N, ch/+, Castles/O-O, compact and
spaced moves/captures, piece-target captures and old-style dotted/to/takes/ordinal
forms. Unqualified descriptive moves can be ambiguous; they intentionally remain in
the denominator, and the resolver must stop rather than choose a convenient board.
Layouts sample inline paragraphs, move numbers with/without dots, repeated black
move numbers, comma/semicolon lists, interleaved comments/NAGs and prose. Additional
pages exercise tables, one move per paragraph, line breaks, nested legal alternatives
with explicit expected parents and FEN-seeded midgame fragments. Full-game PDFs use
two columns and page breaks. `image-diagrams.epub` embeds ten diagram images rendered from oracle positions by
`examples/books_diagrams.rs` (five flat screen PNGs, five engraved-style JPEGs) plus a
picture that is not a board. The gate requires zero misread images and at least the five
screen boards read; the engraved JPEG look currently abstains. Further fixtures cover midgame text diagrams, analysis jumping back 17 plies, an
EPUB spine break within a game, old-style source qualifiers and descriptive square
notation. `continuations.epub` resumes a game after prose, jumps back into analysis,
continues from a black move, abstains where two lines reach the same move with equally
legal continuations, and resets at a new game heading; `text-grid.epub` seeds a position
from a captioned board printed as text rows; `pgn-headers.epub` pastes PGN games whose tag pairs
must not read as moves and whose `[FEN]` tag seeds the game. Legacy fixtures retain text/font diagrams, broken/scanned files and promotions. Negative controls contain
prose coordinates, page/year numbers, illegal moves and unknown starting plies.

There are 93 materialized fixtures. Bundling each notation matrix into a multi-page
EPUB keeps all fixture files below 2 MB (about 1.97 MB). Outputs use fixed ZIP timestamps.
No book downloads or network access participate in regeneration or CI. Current CI
gates require 82% overall recall, 100% sampled precision, zero wrong positions, exact
text-diagram accuracy, all deliberately broken cases detected, and a runtime below
60 seconds. Per-notation descriptive floors are 36/20/36/38%; PDF recall is at least
97% overall; other notation/layout fixtures require 99%. These thresholds document
observed gaps rather than implying general book support. Development-profile
optimization settings in Cargo.toml apply to the test.

For the separate, SHA-256-pinned Gutenberg sample:

```sh
python3 tests/books/fetch_gutenberg.py --from-dir supplied-download-directory
cargo run --locked -- book eval target/books-gutenberg
```

Omit `--from-dir` to download the pinned editions. Gutenberg EPUBs belong only in
ignored storage. The annotations contain 15 nonoverlapping excerpts each for 33870,
5614, 16377, 4913 and Polish 15201, including continuations and diagram-seeded endings.
34180 is biographical prose with isolated move descriptions (one annotated initial
move); 55278 is strategic prose without comparable move tables. Both remain in the
whole-book coverage sample. These editions cannot honestly supply 15 game excerpts
each. Fig.1–15 in 33870 were viewed and independently transcribed as diagram FENs;
`diagram_sources` maps each FEN to its JPG suffix, and the report's "Diagram images"
table scores what the recognizer read from those pixels: correct, wrong (must stay 0)
and abstained. These 15 figures are never used for training. Oracle FENs never seed the
importer under evaluation.

`diagram-labels/` holds square labels for other diagrams of 33870 and 16377, used only to
train the bundled prototypes (one line per image: file suffix and eight ranks, `x` for
squares left out). Figures whose engraving is ambiguous (for example a queen whose
drawn colour contradicts the game) are left out or marked `x` rather than guessed.
5614 uses a different engraving style with no labels, so its diagrams abstain.
Do not choose gates or thresholds from Fig.1–15. For generalization, report books never
used for training separately: 5614 (167 images, different engraving) and the generated
`image-diagrams` fixture are held out by book; Fig.1–15 are held-out figures of a training
book, so they measure in-sample style only.

`book eval <directory>` writes `report.md` and `metrics.json`. EPUB/PDF inputs without
truth JSON are evaluated for coverage only, with no invented accuracy denominator.
Whole-book coverage uses an independent conservative surface detector, counting each
move-like span once and assigning it to a resolved, partial or unresolved containing
fragment. Undetected spans count as unresolved. Years and bare page numbers are
excluded; coordinate references in prose can still be counted. Percentages measure
fragment membership, not correct-move recall. A partial fragment's validated prefix
also belongs to the partial bucket. JSON retains exact diagnostic reason counts per
book, plus mutually exclusive ranked categories and up to three source snippets.

Truth format:

- `file`, `notation`, `complete`, optional `scanned`.
- `fragments`: zero-based `page`, exact `anchor`, optional zero-based `occurrence`,
  `start_fen`, UCI `moves`, optional `unresolved` and authoring `source_note`.
- Optional `parents` gives a preceding expected index or null per move for a tree.
- `diagrams`: expected FENs, matched exactly. Optional `diagram_sources` records JPG
  identities and transcription conventions without supplying images to recognition.

Recall requires the expected excerpt, order, UCI, parent and FEN. Precision divides
correct moves by all emitted moves for complete generated fixtures, or emitted moves
in annotated real-book ranges. Overlapping oracle excerpts are rejected. Missing
anchors are failures and fail CI. Every emitted tree edge is independently replayed,
including edges outside real-book samples. Legal replay cannot prove authorial intent
for unannotated text; whole-book precision is not claimed. Empty move denominators
are vacuous success, while coverage counts remain explicit.

An unrecognized image invalidates position context until a new section or valid
explicit position. This intentionally reduces recall where the importer cannot
prove that a subsequent line is an opening rather than a diagram-seeded ending.
Use `cargo run --example books_document -- <book>` to inspect imported source text
and block boundaries; asset bytes are omitted from its JSON output.
