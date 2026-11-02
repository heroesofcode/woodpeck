# usage: export.py OUT.bin NET.pt WIDTHS [NET.pt WIDTHS ...]
# WPCN v2: magic, version 2, network count, then per network: classes, three widths, f32 LE weights
import sys, torch
args = sys.argv[2:]; nets = list(zip(args[::2], args[1::2]))
out = bytearray(b'WPCN' + bytes([2, len(nets)]))
for path, widths in nets:
    sd = torch.load(path); keys = [k for k in sd if k.endswith('weight') or k.endswith('bias')]
    out += bytes([sd[keys[-2]].shape[0]] + [int(v) for v in widths.split(',')])
    for k in keys: out += sd[k].numpy().astype('<f4').tobytes()
open(sys.argv[1], 'wb').write(out); print(len(out), 'bytes')
