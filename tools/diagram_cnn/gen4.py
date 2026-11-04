# Synthetic board generator v4: printed-book styles on top of gen3 (free-licence piece sets only;
# see assets/LICENSES.md). Adds diagonal hatched or shaded dark squares with a paper-coloured halo
# around pieces, black pieces with thin white detail lines or speckled ink, thin and heavy outlines,
# and print/scan wear: yellowed or unevenly lit paper, resolution loss, ink spread or thinning, salt
# and pepper, JPEG, slight rotation. Ideas from fenshot (MIT); no code or data from it is used.
# usage: gen4.py BOARDS SEED OUT.npz [BOOK_FRACTION]   ($PIECES and $FONTS as for gen3.py)
import numpy as np, io, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from PIL import Image, ImageFilter
from scipy.ndimage import minimum_filter, maximum_filter, binary_erosion, binary_dilation, gaussian_filter
import gen3
from gen import PIECES, SETS, rand_position
from gen3 import cells_shift
def to_pil(im): return Image.fromarray((np.clip(im, 0, 1) * 255).astype(np.uint8))
def from_pil(p): return np.array(p, np.float32) / 255
def hatch(rng, H, s):
    y, x = np.mgrid[0:H, 0:H].astype(np.float32)
    per = s / rng.uniform(6.5, 14); th = rng.uniform(.3, .62)
    kind = rng.choice(['up', 'down', 'cross', 'grey'], p=[.6, .18, .07, .15])
    wob = gaussian_filter(rng.normal(0, 1, (H, H)), 2) * rng.uniform(0, .6)
    up = (((x + y) / per + wob) % 1.0) < th
    if kind == 'up': ink = up
    elif kind == 'down': ink = (((x - y) / per + wob) % 1.0) < th
    elif kind == 'cross': ink = up | ((((x - y) / per) % 1.0) < th * .7)
    else: ink = None
    if ink is None:
        return np.full((H, H), rng.uniform(.5, .85), np.float32)
    lv = rng.uniform(0, .5) if rng.random() < .75 else rng.uniform(.5, .75)
    return np.where(ink, lv, 1.0).astype(np.float32)
def piece(rng, S, c, ps, style):
    g, a = PIECES[(S, c)]
    g = g.copy()
    if c.islower() and style['engraved']:
        wg, _ = PIECES[(S, c.upper())]
        inner = binary_erosion(a > .5, iterations=style['inset'])
        lines = (wg < .5) & inner
        if style['thin']: lines = binary_erosion(lines, iterations=1) | (lines & (rng.random(lines.shape) < .3))
        g = np.where(lines, rng.uniform(.8, 1.0), g)
    if c.islower() and style['speckle']:
        sp = gaussian_filter(rng.random(g.shape), 1.5) > style['speckle']
        g = np.where((a > .5) & sp & (g < .5), 1.0, g)
    if style['line'] > 0: g = np.where(a > .5, minimum_filter(g, size=style['line']), g)
    elif style['line'] < 0: g = np.where(a > .5, maximum_filter(g, size=-style['line']), g)
    pw = max(6, int(round(ps * rng.uniform(.85, 1.06)))) if rng.random() < .3 else ps
    gi = from_pil(to_pil(g).resize((pw, ps), Image.LANCZOS)); ai = from_pil(to_pil(a).resize((pw, ps), Image.LANCZOS))
    return gi, ai
def render(rng, b, s):
    S = rng.choice(SETS); H = 8 * s
    paper = rng.uniform(.86, 1.0)
    img = np.full((H, H), paper, np.float32)
    yy, xx = np.mgrid[0:H, 0:H]
    dark = ((yy // s + xx // s) % 2) == 1
    img = np.where(dark, np.minimum(hatch(rng, H, s), paper), img)
    style = dict(engraved=rng.random() < .55, inset=int(rng.integers(3, 12)), thin=rng.random() < .5,
                 speckle=rng.uniform(.55, .7) if rng.random() < .15 else 0,
                 line=int(rng.choice([0, 0, 3, 5, 7, -3, -5])))
    halo = int(rng.integers(1, 4)) if rng.random() < .75 else 0
    ps = max(6, int(round(s * rng.uniform(.78, 1.0))))
    for i, c in enumerate(b):
        if c == '.': continue
        gi, ai = piece(rng, S, c, ps, style)
        r, f = divmod(i, 8); ph, pw = gi.shape
        oy = r * s + (s - ph) // 2 + int(rng.integers(-1, 2)); ox = f * s + (s - pw) // 2 + int(rng.integers(-1, 2))
        oy = min(max(oy, 0), H - ph); ox = min(max(ox, 0), H - pw)
        reg = img[oy:oy + ph, ox:ox + pw]
        if halo:  # printers knock the hatching out around a piece
            hm = binary_dilation(ai > .5, iterations=halo * max(1, s // 24))
            reg = np.where(hm, paper, reg)
        img[oy:oy + ph, ox:ox + pw] = reg * (1 - ai) + gi * ai
    m = int(s * rng.uniform(.3, 1.0))
    out = np.full((H + 2 * m, H + 2 * m), paper, np.float32); out[m:m + H, m:m + H] = img
    bw = int(rng.integers(1, max(3, s // 7)))
    out[m - bw:m, m - bw:m + H + bw] = 0; out[m + H:m + H + bw, m - bw:m + H + bw] = 0
    out[m - bw:m + H + bw, m - bw:m] = 0; out[m - bw:m + H + bw, m + H:m + H + bw] = 0
    return out, m
def wear(rng, im):
    h, w = im.shape
    if rng.random() < .5:  # yellowed / unevenly lit paper (luma only)
        g = gaussian_filter(rng.normal(0, 1, (h // 16 + 2, w // 16 + 2)), 1.5)
        g = from_pil(to_pil((g - g.min()) / (np.ptp(g) + 1e-6)).resize((w, h), Image.BILINEAR))
        im = im * (1 - rng.uniform(0, .18) * g)
    if rng.random() < .35:  # resolution loss: printed or scanned smaller, enlarged again
        k = rng.uniform(.35, .8); p = to_pil(im)
        im = from_pil(p.resize((max(8, int(w * k)), max(8, int(h * k))), Image.BILINEAR).resize((w, h), Image.BILINEAR))
    if rng.random() < .5: im = from_pil(to_pil(im).filter(ImageFilter.GaussianBlur(rng.uniform(.3, 1.1))))
    if rng.random() < .3:  # ink spread or thinning
        im = minimum_filter(im, 2) if rng.random() < .5 else maximum_filter(im, 2)
    if rng.random() < .3:
        bw = im < rng.uniform(.4, .65); im = np.where(bw, rng.uniform(0, .2), rng.uniform(.85, 1)).astype(np.float32)
    im = im * rng.uniform(.85, 1.05) + rng.uniform(-.04, .08)
    if rng.random() < .6: im = im + rng.normal(0, rng.uniform(.01, .07), im.shape)
    if rng.random() < .2:
        sp = rng.random(im.shape); q = rng.uniform(.001, .006); im = np.where(sp < q, 0, np.where(sp > 1 - q, 1, im))
    im = np.clip(im, 0, 1)
    if rng.random() < .5:
        bio = io.BytesIO(); to_pil(im).save(bio, 'JPEG', quality=int(rng.integers(20, 90))); im = from_pil(Image.open(bio))
    return im
def book_sample(rng):
    s = int(rng.integers(18, 64)); b = rand_position(rng)
    im, m = render(rng, b, s)
    if rng.random() < .35:
        im = from_pil(to_pil(im).rotate(rng.normal(0, .5), resample=Image.BILINEAR, fillcolor=255))
    im = wear(rng, im)
    if rng.random() < .1:  # misframed -> junk
        mag = rng.uniform(.3, .6, 2) * rng.choice([-1, 1], 2) * (rng.random(2) < [.7, .7])
        if not mag.any(): mag[0] = .45
        big = (float(mag[0]), float(mag[1])); side = 8 * s
    else: big = (0.0, 0.0); side = 8 * s * rng.uniform(.985, 1.015)
    x0 = m + rng.normal(0, .025 * s); y0 = m + rng.normal(0, .025 * s)
    return cells_shift(im, x0, y0, side, b, rng, .03, big)
if __name__ == '__main__':
    n, seed, out = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
    frac = float(sys.argv[4]) if len(sys.argv) > 4 else .6
    rng = np.random.default_rng(seed)
    X = np.zeros((n, 64, 32, 32), np.uint8); Y = np.zeros((n, 64), np.int64)
    for k in range(n):
        X[k], Y[k] = book_sample(rng) if rng.random() < frac else gen3.board_sample(rng)
    np.savez_compressed(out, X=X, Y=Y)
