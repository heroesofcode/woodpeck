# Diagram square classifier training

Rebuilds `src/books/diagram_cnn.bin`, the weights used by `books::cnn`. Steps 1 to 6
are synthetic only: boards are drawn from the freely licensed piece sets and fonts listed in
`assets/LICENSES.md`. Step 7 also used square crops from privately held, purchased chess books;
those are not distributed, so that step can only be repeated with your own labelled diagrams. Inference is plain Rust and
needs none of this; these scripts need Python 3 with numpy, scipy, Pillow, cairosvg and PyTorch.

1. Put the SVG sets (`cburnett`, `celtic`, `chessnut`, `fantasy`, `merida`, `mpchess`,
   `spatial`, from lichess-org/lila `public/piece`) under `SRC/<set>/`, and `DejaVuSans.ttf`,
   `FreeSerif.otf` and `NotoSansSymbols2-Regular.ttf` under `SRC/fonts/`.
2. `python pieces.py SRC pieces.npz`
3. Generate 2500-board sets. The bundled networks used `gen3.py` seeds 3001 to 3006 for
   training and 3999 for validation, plus three sets from the earlier `gen.py` (their seeds
   were not recorded, so a rebuild is close to but not bit-identical with the bundled file):

   ```sh
   export PIECES=pieces.npz FONTS=SRC/fonts
   for s in 3001 3002 3003 3004 3005 3006; do python gen3.py 2500 $s t$s.npz; done
   python gen3.py 500 3999 val.npz
   for s in 1 2 3; do python gen.py 2500 $s v2-$s.npz; done
   ```

4. `python train.py 12,24,48 8 net.pt val.npz t*.npz v2-*.npz` (about 15 minutes on 6 CPU threads)
5. Round 3 (printed-book styles) fine-tunes those two networks on `gen4.py` sets, which add
   diagonal hatched or shaded dark squares with a knocked-out halo around pieces, black pieces with
   thin white detail lines or speckled ink, and print/scan wear (uneven paper, resolution loss, ink
   spread, salt and pepper, JPEG, slight rotation). The bundled file used seeds 4001 to 4006
   (70% book style), validation seed 4999, and three epochs at a lower learning rate, rehearsing
   `t3005`, `t3006` and two `gen.py` sets (training stores raw cells, so it fits in about 2.5 GB):

   ```sh
   for s in 4001 4002 4003 4004 4005 4006; do python gen4.py 2500 $s t$s.npz .7; done
   python gen4.py 500 4999 val4.npz .7
   INIT=net.pt MAXLR=1e-3 THREADS=4 python train.py 12,24,48 3 net3.pt val4.npz t400?.npz t3005.npz t3006.npz v2-2.npz v2-3.npz
   INIT=net_b.pt MAXLR=1e-3 THREADS=4 python train.py 16,32,64 3 net3_b.pt val4.npz t400?.npz t3005.npz t3006.npz v2-2.npz v2-3.npz
   ```

6. Train the second network the same way (`train.py 16,32,64 8 net_b.pt ...`), then
   `python export.py ../../src/books/diagram_cnn.bin net.pt 12,24,48 net_b.pt 16,32,64`

7. Round 4 (real diagrams) fine-tunes the round 3 networks for four epochs at `MAXLR=1e-3` on
   `t4001` to `t4004` plus a `real.npz` of labelled board crops from purchased books, listed twice.
   `real.npz` has the same layout as the generated sets (`X` uint8 `N x 64 x 32 x 32` cells as cut
   by the Rust locator, `Y` int64 `N x 64`); a square that was not checked by hand is `-1` and is
   ignored by the loss. Whole books were held out for testing, and the thresholds were left as they
   were because the held-out books and the Gutenberg suite gave no wrong position with them.

   ```sh
   INIT=net3.pt MAXLR=1e-3 THREADS=3 python train.py 12,24,48 4 net4.pt val4.npz t400[1-4].npz real.npz real.npz
   INIT=net3_b.pt MAXLR=1e-3 THREADS=3 python train.py 16,32,64 4 net4_b.pt val4.npz t400[1-4].npz real.npz real.npz
   ```

8. Round 5 (scanned and drawn pages) fine-tunes the round 4 networks the same way, adding a
   `real6.npz` of board crops cut from rendered scanned, vector-drawn and diagram-font PDF pages,
   also listed twice. Boards set in a diagram font were labelled from their decoded glyphs. Whole
   scanned books were again held out for testing, and the thresholds are unchanged.

   ```sh
   INIT=net4.pt MAXLR=1e-3 THREADS=3 python train.py 12,24,48 4 net5.pt val4.npz t400[1-4].npz real.npz real.npz real6.npz real6.npz
   INIT=net4_b.pt MAXLR=1e-3 THREADS=3 python train.py 16,32,64 4 net5_b.pt val4.npz t400[1-4].npz real.npz real.npz real6.npz real6.npz
   ```


9. Round 6 (newer book styles) fine-tunes the round 5 networks the same way, adding a
   `real7.npz` of board crops from further privately held purchased books (scanned PDFs and
   EPUB diagram images). Whole books stay held out for testing. Labels that disagreed with an
   earlier gate were checked square-by-square against the cell images before being kept. The
   acceptance thresholds are unchanged. The previous round's `diagram_cnn.bin` is kept beside
   the new weights as `diagram_cnn_prior.bin` so inference can fall back to it when the new
   nets abstain, which keeps per-book locate/read from dropping.

   ```sh
   INIT=net5.pt MAXLR=1e-3 THREADS=3 python train.py 12,24,48 4 net6.pt val4.npz t400[1-4].npz real.npz real.npz real6.npz real6.npz real7.npz real7.npz
   INIT=net5_b.pt MAXLR=1e-3 THREADS=3 python train.py 16,32,64 4 net6_b.pt val4.npz t400[1-4].npz real.npz real.npz real6.npz real6.npz real7.npz real7.npz
   ```

Class 13 is "junk": a square cut by a misplaced frame. The acceptance thresholds in
`src/books/cnn.rs` were chosen on a separate set of real diagrams, so a retrained network
must be checked again before its thresholds are trusted.
