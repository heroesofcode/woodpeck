# Synthetic board generator v2: 32x32 cells like `diagram::cells` (6% inset, area resample).
# usage: gen.py BOARDS SEED OUT.npz   (piece arrays from $PIECES, default pieces.npz)
import numpy as np, io, os
from PIL import Image, ImageFilter
from scipy.ndimage import map_coordinates, binary_dilation, binary_erosion, minimum_filter
SETS = ['cburnett','celtic','chessnut','fantasy','merida','mpchess','spatial','freeserif','dejavu','notosym2']
CLS = '.PNBRQKpnbrqk'
_raw = np.load(os.environ.get('PIECES', 'pieces.npz'))
PIECES = {}
for s in SETS:
    for c in 'wb':
        for p in 'PNBRQK':
            a = _raw[f'{s}_{c}{p}'].astype(np.float32) / 255
            g = a[..., :3] @ np.array([.299, .587, .114], np.float32)
            PIECES[(s, (p if c == 'w' else p.lower()))] = (g, a[..., 3])
def rand_position(rng):
    b = ['.'] * 64
    dens = rng.uniform(0.08, 0.55)
    for i in range(64):
        if rng.random() < dens:
            b[i] = CLS[rng.integers(1, 13)]
    if rng.random() < 0.7:  # start-like structure: pawn ranks
        for i in range(8, 16):
            if rng.random() < .6: b[i] = 'p'
        for i in range(48, 56):
            if rng.random() < .6: b[i] = 'P'
    return b
def dark_texture(rng, H, W, s):
    kind = rng.choice(['flat', 'hatch', 'hatch', 'dots', 'white'], p=[.4, .25, .15, .1, .1])
    y, x = np.mgrid[0:H, 0:W].astype(np.float32)
    if kind == 'flat':
        return np.full((H, W), rng.uniform(.45, .88), np.float32), False
    if kind == 'hatch':
        per = rng.uniform(2.2, 5.5) * s / 28; th = rng.uniform(.25, .6)
        ang = rng.choice([1, -1])
        v = ((x + ang * y) / per) % 1.0
        line = rng.uniform(0, .45)
        return np.where(v < th, line, 1.0).astype(np.float32), True
    if kind == 'dots':
        per = rng.uniform(2.5, 4.5) * s / 28
        v = (np.sin(x / per * np.pi) * np.sin(y / per * np.pi)) > rng.uniform(.3, .7)
        return np.where(v, rng.uniform(0, .5), 1.0).astype(np.float32), True
    return np.ones((H, W), np.float32), True
def render(rng, b, s):
    S = rng.choice(SETS)
    H = 8 * s
    img = np.ones((H, H), np.float32) * rng.uniform(.9, 1.0)
    dt, binok = dark_texture(rng, H, H, s)
    yy, xx = np.mgrid[0:H, 0:H]
    dark = ((yy // s + xx // s) % 2) == 1
    img = np.where(dark, dt, img)
    sc = rng.uniform(.78, 1.0)
    bold = int(rng.integers(3, 13)) if rng.random() < .35 else 0  # heavy print: ink spreads into white pieces
    ps = max(6, int(round(s * sc)))
    for i, c in enumerate(b):
        if c == '.': continue
        g, a = PIECES[(S, c)]
        if bold:
            g = np.where(a > .5, minimum_filter(g, size=bold), g)
        gi = np.array(Image.fromarray((g * 255).astype(np.uint8)).resize((ps, ps), Image.LANCZOS), np.float32) / 255
        ai = np.array(Image.fromarray((a * 255).astype(np.uint8)).resize((ps, ps), Image.LANCZOS), np.float32) / 255
        r, f = divmod(i, 8)
        oy = r * s + (s - ps) // 2 + int(rng.integers(-1, 2)); ox = f * s + (s - ps) // 2 + int(rng.integers(-1, 2))
        oy = min(max(oy, 0), H - ps); ox = min(max(ox, 0), H - ps)
        reg = img[oy:oy + ps, ox:ox + ps]
        img[oy:oy + ps, ox:ox + ps] = reg * (1 - ai) + gi * ai
    # border + margin
    m = int(s * rng.uniform(.3, 1.0))
    out = np.ones((H + 2 * m, H + 2 * m), np.float32)
    out[m:m + H, m:m + H] = img
    bw = int(rng.integers(0, max(2, s // 8)))
    if bw:
        out[m - bw:m, m - bw:m + H + bw] = 0; out[m + H:m + H + bw, m - bw:m + H + bw] = 0
        out[m - bw:m + H + bw, m - bw:m] = 0; out[m - bw:m + H + bw, m + H:m + H + bw] = 0
    return out, m, binok
def degrade(rng, im, binok=True):
    s = rng.random()
    if rng.random() < .5:
        im = np.array(Image.fromarray((im * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(rng.uniform(.3, 1.2))), np.float32) / 255
    if binok and rng.random() < .45:  # JBIG2/CCITT-like binarisation, with ink bleed
        bw = im < rng.uniform(.35, .65)
        if rng.random() < .4: bw = binary_dilation(bw) if rng.random() < .6 else binary_erosion(bw)
        im = np.where(bw, 0.0, 1.0).astype(np.float32)
    im = im * rng.uniform(.8, 1.05) + rng.uniform(-.05, .12)
    if rng.random() < .6:
        im = im + rng.normal(0, rng.uniform(.01, .08), im.shape)
    im = np.clip(im, 0, 1)
    if rng.random() < .4:
        bio = io.BytesIO(); Image.fromarray((im * 255).astype(np.uint8)).save(bio, 'JPEG', quality=int(rng.integers(25, 85)))
        im = np.array(Image.open(bio), np.float32) / 255
    return im
def cells_from(im, x0, y0, side, rng=None, jit=0.0):
    out = []
    cs = side / 8
    for i in range(64):
        r, f = divmod(i, 8)
        cx0, cy0 = x0 + f * cs, y0 + r * cs
        inset = cs * .06
        jx = jy = 0
        if rng is not None and jit:
            jx, jy = rng.normal(0, jit * cs, 2)
        a, b = cx0 + inset + jx, cy0 + inset + jy
        w = cs - 2 * inset
        crop = Image.fromarray((im * 255).astype(np.uint8)).crop((int(round(a)), int(round(b)), int(round(a + w)), int(round(b + w))))
        out.append(np.array(crop.resize((32, 32), Image.BOX), np.uint8))
    return np.stack(out)
def board_sample(rng):
    s = int(rng.integers(18, 64))
    b = rand_position(rng)
    im, m, binok = render(rng, b, s)
    ang = rng.normal(0, .4) if rng.random() < .3 else 0
    if ang:
        im = np.array(Image.fromarray((im * 255).astype(np.uint8)).rotate(ang, resample=Image.BILINEAR, fillcolor=255), np.float32) / 255
    im = degrade(rng, im, binok)
    cells = cells_from(im, m + rng.normal(0, .02 * s), m + rng.normal(0, .02 * s), 8 * s * rng.uniform(.985, 1.015), rng, .03)
    y = np.array([CLS.index(c) for c in b], np.int64)
    return cells, y
if __name__ == '__main__':
    import sys
    n, seed, out = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
    rng = np.random.default_rng(seed)
    X = np.zeros((n, 64, 32, 32), np.uint8); Y = np.zeros((n, 64), np.int64)
    for k in range(n):
        X[k], Y[k] = board_sample(rng)
    np.savez_compressed(out, X=X, Y=Y)
