# Diagram square classifier training

Rebuilds `src/books/diagram_cnn.bin`, the weights used by `books::cnn`. Training is
synthetic only: boards are drawn from the freely licensed piece sets and fonts listed in
`assets/LICENSES.md`. No book pages or crops are needed or used. Inference is plain Rust and
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

Class 13 is "junk": a square cut by a misplaced frame. The acceptance thresholds in
`src/books/cnn.rs` were chosen on a separate set of real diagrams, so a retrained network
must be checked again before its thresholds are trusted.
