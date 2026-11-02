# Diagram classifier training sources

`src/books/diagram_cnn.bin` holds the weights of the two small square classifiers in
`src/books/cnn.rs`. They were trained only on synthetic boards rendered from the freely licensed
piece sets and fonts below; no scanned or copyrighted book pages were used for training.
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
