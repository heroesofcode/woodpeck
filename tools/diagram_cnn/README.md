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
5. Train the second network the same way (`train.py 16,32,64 8 net_b.pt ...`), then
   `python export.py ../../src/books/diagram_cnn.bin net.pt 12,24,48 net_b.pt 16,32,64`

Class 13 is "junk": a square cut by a misplaced frame. The acceptance thresholds in
`src/books/cnn.rs` were chosen on a separate set of real diagrams, so a retrained network
must be checked again before its thresholds are trusted.
