# Synthetic board generator v3 (free-licence piece sets only; see assets/LICENSES.md).
# Adds: coordinates around the board, engraved/old-style black pieces, per-piece aspect
# and ink-fill variation, misaligned frames with a JUNK class (13) for cells that straddle squares.
# usage: gen3.py BOARDS SEED OUT.npz   ($PIECES as for gen.py, $FONTS holds DejaVuSans.ttf and FreeSerif.otf)
import numpy as np, io, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from PIL import Image, ImageDraw, ImageFont, ImageFilter
from scipy.ndimage import minimum_filter, maximum_filter, binary_erosion
import gen
from gen import PIECES, SETS, CLS, rand_position, dark_texture, degrade
JUNK = 13
FONTS = [os.path.join(os.environ.get('FONTS', 'fonts'), f) for f in ('DejaVuSans.ttf', 'FreeSerif.otf')]
def piece_img(rng, S, c, ps, bold, style):
    g, a = PIECES[(S, c)]
    if bold: g = np.where(a > .5, minimum_filter(g, size=bold), g)
    if c.isupper() and rng.random() < .08:  # heavy ink fill of a white piece (outline kept)
        k = int(rng.integers(12, 30)); g = np.where(a > .5, minimum_filter(g, size=k), g)
    if c.islower() and style == 'engraved':  # black piece with white detail lines (old books)
        wg, _ = PIECES[(S, c.upper())]
        inner = binary_erosion(a > .5, iterations=int(rng.integers(6, 14)))
        lines = (wg < .5) & inner
        g = np.where(lines, rng.uniform(.75, 1.0), g)
    pw = max(6, int(round(ps * rng.uniform(.82, 1.08)))) if rng.random() < .3 else ps
    gi = np.array(Image.fromarray((g * 255).astype(np.uint8)).resize((pw, ps), Image.LANCZOS), np.float32) / 255
    ai = np.array(Image.fromarray((a * 255).astype(np.uint8)).resize((pw, ps), Image.LANCZOS), np.float32) / 255
    return gi, ai
def render(rng, b, s):
    S = rng.choice(SETS); H = 8 * s
    img = np.ones((H, H), np.float32) * rng.uniform(.88, 1.0)
    dt, binok = dark_texture(rng, H, H, s)
    if rng.random() < .05:  # light squares also lightly textured
        lt, _ = dark_texture(rng, H, H, s); img = np.maximum(img * .9, lt)
    yy, xx = np.mgrid[0:H, 0:H]
    dark = ((yy // s + xx // s) % 2) == 1
    if rng.random() < .03: dark = ~dark  # wrong-parity print (rare)
    img = np.where(dark, dt, img)
    sc = rng.uniform(.75, 1.0)
    bold = int(rng.integers(3, 13)) if rng.random() < .35 else 0
    style = 'engraved' if rng.random() < .2 else 'plain'
    ps = max(6, int(round(s * sc)))
    for i, c in enumerate(b):
        if c == '.': continue
        gi, ai = piece_img(rng, S, c, ps, bold, style)
        r, f = divmod(i, 8); ph, pw = gi.shape
        oy = r * s + (s - ph) // 2 + int(rng.integers(-1, 2)); ox = f * s + (s - pw) // 2 + int(rng.integers(-1, 2))
        oy = min(max(oy, 0), H - ph); ox = min(max(ox, 0), H - pw)
        reg = img[oy:oy + ph, ox:ox + pw]
        img[oy:oy + ph, ox:ox + pw] = reg * (1 - ai) + gi * ai
    m = int(s * rng.uniform(.3, 1.2))
    out = np.ones((H + 2 * m, H + 2 * m), np.float32)
    out[m:m + H, m:m + H] = img
    bw = int(rng.integers(0, max(2, s // 8)))
    if bw:
        out[m - bw:m, m - bw:m + H + bw] = 0; out[m + H:m + H + bw, m - bw:m + H + bw] = 0
        out[m - bw:m + H + bw, m - bw:m] = 0; out[m - bw:m + H + bw, m + H:m + H + bw] = 0
    if rng.random() < .4 and m > 6:  # coordinates
        pil = Image.fromarray((out * 255).astype(np.uint8)); d = ImageDraw.Draw(pil)
        f = ImageFont.truetype(rng.choice(FONTS), max(6, int(min(m - bw - 1, s * .45))))
        inside = rng.random() < .2
        for k in range(8):
            if inside:
                d.text((m + 2, m + k * s + 1), str(8 - k), font=f, fill=int(rng.uniform(0, 90)))
                d.text((m + k * s + s - f.size, m + H - f.size - 2), 'abcdefgh'[k], font=f, fill=int(rng.uniform(0, 90)))
            else:
                d.text((max(0, m - bw - f.size), m + k * s + s // 3), str(8 - k), font=f, fill=0)
                d.text((m + k * s + s // 3, m + H + bw), 'abcdefgh'[k], font=f, fill=0)
        out = np.array(pil, np.float32) / 255
    return out, m, binok
def cells_shift(im, x0, y0, side, b, rng, jit, big):
    """Cells with frame error. big=(dx,dy) in cell units applied to the whole frame -> junk labels."""
    cs = side / 8; out = []; ys = []
    pil = Image.fromarray((im * 255).astype(np.uint8))
    dx, dy = big
    for i in range(64):
        r, f = divmod(i, 8)
        inset = cs * .06
        jx, jy = rng.normal(0, jit * cs, 2)
        a = x0 + (f + dx) * cs + inset + jx; bb = y0 + (r + dy) * cs + inset + jy
        w = cs - 2 * inset
        out.append(np.array(pil.crop((int(round(a)), int(round(bb)), int(round(a + w)), int(round(bb + w)))).resize((32, 32), Image.BOX), np.uint8))
        if dx == 0 and dy == 0: ys.append(CLS.index(b[i])); continue
        # squares overlapped by the shifted window
        fx, fy = f + dx, r + dy
        cov = []
        for rr in (int(np.floor(fy)), int(np.floor(fy)) + 1):
            for ff in (int(np.floor(fx)), int(np.floor(fx)) + 1):
                ox = max(0, min(fx + 1, ff + 1) - max(fx, ff)); oy = max(0, min(fy + 1, rr + 1) - max(fy, rr))
                if ox * oy > .04:
                    cov.append(b[rr * 8 + ff] if 0 <= rr < 8 and 0 <= ff < 8 else '#')
        if all(c == '.' for c in cov): ys.append(0)
        else: ys.append(JUNK)
    return np.stack(out), np.array(ys, np.int64)
def board_sample(rng):
    s = int(rng.integers(18, 64))
    b = rand_position(rng)
    im, m, binok = render(rng, b, s)
    ang = rng.normal(0, .4) if rng.random() < .3 else 0
    if ang:
        im = np.array(Image.fromarray((im * 255).astype(np.uint8)).rotate(ang, resample=Image.BILINEAR, fillcolor=255), np.float32) / 255
    im = degrade(rng, im, binok)
    u = rng.random()
    if u < .12:  # misframed: whole-frame offset 0.3..0.6 cell on one or both axes
        mag = rng.uniform(.3, .6, 2) * rng.choice([-1, 1], 2) * (rng.random(2) < [.7, .7])
        if not mag.any(): mag[0] = .45
        big = (float(mag[0]), float(mag[1])); side = 8 * s
    elif u < .17:  # wrong scale: frame too big/small by ~half a cell overall
        big = (0.0, 0.0); side = 8 * s * rng.choice([.9, 1.1])
        cells, _ = cells_shift(im, m - (side - 8 * s) / 2, m - (side - 8 * s) / 2, side, b, rng, .02, (0, 0))
        y = np.full(64, JUNK); y[[0, 7, 56, 63]] = -1  # corners undefined
        cs = side / 8
        for i in range(64):  # centre cells barely move: keep only true outer ring as junk
            r, f = divmod(i, 8)
            if 2 <= r <= 5 and 2 <= f <= 5: y[i] = -1
        return cells, y
    else:
        big = (0.0, 0.0); side = 8 * s * rng.uniform(.985, 1.015)
    x0 = m + rng.normal(0, .025 * s); y0 = m + rng.normal(0, .025 * s)
    cells, y = cells_shift(im, x0, y0, side, b, rng, .03, big)
    return cells, y
if __name__ == '__main__':
    n, seed, out = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
    rng = np.random.default_rng(seed)
    X = np.zeros((n, 64, 32, 32), np.uint8); Y = np.zeros((n, 64), np.int64)
    for k in range(n):
        X[k], Y[k] = board_sample(rng)
    np.savez_compressed(out, X=X, Y=Y)
