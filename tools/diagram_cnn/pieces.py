# Pre-renders the free-licence piece sets and chess fonts to 128 px RGBA arrays.
# usage: pieces.py SOURCES_DIR OUT.npz   (SOURCES_DIR/<set>/{w,b}{P,N,B,R,Q,K}.svg and SOURCES_DIR/fonts)
import numpy as np, io, os, sys, cairosvg
from PIL import Image, ImageDraw, ImageFont
from scipy.ndimage import binary_fill_holes
P = 'PNBRQK'
R = 128
def svgset(d):
    out = {}
    for c in 'wb':
        for p in P:
            png = cairosvg.svg2png(url=f'{d}/{c}{p}.svg', output_width=R, output_height=R)
            out[c + p] = np.array(Image.open(io.BytesIO(png)).convert('RGBA'))
    return out
def fontset(path, scale=0.92, dy=0.0):
    f = ImageFont.truetype(path, int(R * scale))
    out = {}
    def ink(ch):
        im = Image.new('L', (R, R), 0); d = ImageDraw.Draw(im)
        bb = d.textbbox((0, 0), ch, font=f)
        x = (R - (bb[2] - bb[0])) / 2 - bb[0]; y = (R - (bb[3] - bb[1])) / 2 - bb[1] + dy * R
        d.text((x, y), ch, font=f, fill=255)
        return np.array(im) > 127
    for i, p in enumerate('KQRBNP'):
        wi = ink(chr(0x2654 + i)); bi = ink(chr(0x265A + i))
        sil = binary_fill_holes(bi | wi)
        for c, k in (('w', wi), ('b', bi)):
            a = np.zeros((R, R, 4), np.uint8)
            a[..., 3] = sil * 255
            v = np.where(k, 0, 255) if c == 'w' else np.where(k, 0, 255)
            a[..., 0] = a[..., 1] = a[..., 2] = v
            out[c + p] = a
    return out
sets = {}
SRC = sys.argv[1]
for n in ['cburnett', 'celtic', 'chessnut', 'fantasy', 'merida', 'mpchess', 'spatial']:
    d = os.path.join(SRC, n)
    sets[n] = svgset(d)
F = os.path.join(SRC, 'fonts') + '/'
sets['freeserif'] = fontset(F + 'FreeSerif.otf')
sets['dejavu'] = fontset(F + 'DejaVuSans.ttf')
sets['notosym2'] = fontset(F + 'NotoSansSymbols2-Regular.ttf')
np.savez_compressed(sys.argv[2], **{f'{s}_{k}': v for s, d in sets.items() for k, v in d.items()})
print(sorted(sets))
