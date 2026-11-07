# Diagram classifier training sources

`src/books/diagram_cnn.bin` holds the weights of the two small square classifiers in
`src/books/cnn.rs`. They were trained mostly on synthetic boards rendered from the freely licensed
piece sets and fonts below. The latest round also fine-tuned them on square crops of diagrams
from chess books that the maintainer bought and holds privately. Those books, their pages and
the crops are not part of this repository and are not distributed; only the learned weights are.
The piece sets come from the lichess-org/lila repository (`public/piece`, see its `COPYING.md`).

## Piece sets

| Set | Author | Licence |
|---|---|---|
| cburnett | Colin M.L. Burnett | GPL-2.0-or-later |
| merida | Armando Hernandez Marroquin | GPL-2.0-or-later |
| mpchess | Maxime Chupin | GPL-3.0-or-later |
| chessnut | Alexis Luengas | Apache-2.0 |
| fantasy, spatial, celtic | Maurizio Monge | MIT |

## Fonts (Unicode chess glyphs U+2654 to U+265F and coordinates)

| Font | Author | Licence |
|---|---|---|
| FreeSerif | GNU FreeFont | GPL-3.0-or-later with font exception |
| DejaVu Sans | DejaVu fonts (Bitstream Vera) | Bitstream Vera licence, public-domain changes |
| Noto Sans Symbols 2 | Google | OFL-1.1 |

Sets under non-free or non-commercial licences were excluded, as were AGPL sets.
The printed-book styles of `tools/diagram_cnn/gen4.py` (hatched squares, print and scan wear) are
drawn procedurally from the same sets and fonts; no new piece set or font was added.

# Bundled assets

## Source Serif 4

`fonts/SourceSerif4-Regular.ttf` and `fonts/SourceSerif4-Semibold.ttf` are
unmodified static fonts from [Adobe Source Serif 4.005R](https://github.com/adobe-fonts/source-serif/tree/4.005R/TTF).
Copyright 2014–2023 Adobe, with Reserved Font Name “Source”. Licensed under
[SIL Open Font License 1.1](fonts/SourceSerif4-LICENSE.md), which permits embedding
and redistribution with this application. The complete copyright and license
notice accompanies the fonts. These assets are independent of Cargo dependencies;
`cargo deny check` audits the Rust dependency licenses.

## Chess pieces

See [pieces/LICENSE](pieces/LICENSE) for the cburnett set.

## OCR models

`ocr/text-detection.onnx` (2,499,479 bytes) and `ocr/text-recognition.onnx`
(9,713,177 bytes) are the unmodified ocrs text detection and recognition models
by Robert Knight, published at <https://huggingface.co/robertknight/ocrs> and
<https://ocrs-models.s3-accelerate.amazonaws.com/>. They were trained on HierText
and synthetic data. The model author treats the weights as
[CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/). The weights stay
under that license; they are not relicensed as GPL-3.0-or-later with the
application code. No model file was modified.

SHA-256:

- `text-detection.onnx`: `a917b23dbd9524b465df7e922641b3ff2981623df4ded5a0234004ef2fee7cfe`
- `text-recognition.onnx`: `86c145c2edb96c8caed5b1ebb8f44d706408922211451c309c625157dd6061c5`
