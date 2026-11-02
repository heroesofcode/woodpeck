# usage: train.py WIDTHS EPOCHS OUT.pt VAL.npz TRAIN.npz...   e.g. train.py 12,24,48 8 net.pt val.npz t*.npz
import numpy as np, torch, torch.nn as nn, time, sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from model import Net, norm
torch.set_num_threads(6); torch.manual_seed(0); np.random.seed(0)
W = [int(v) for v in sys.argv[1].split(',')]; E = int(sys.argv[2]); out = sys.argv[3]; val = sys.argv[4]; files = sys.argv[5:]
X = []; Y = []
for f in files:
    d = np.load(f); X.append(norm(d['X']).astype(np.float16)); Y.append(d['Y'])
X = torch.from_numpy(np.concatenate(X).reshape(-1, 1, 32, 32)); Y = torch.from_numpy(np.concatenate(Y).reshape(-1))
v = np.load(val); VX = torch.tensor(norm(v['X']).reshape(-1, 1, 32, 32)); VY = torch.tensor(v['Y'].reshape(-1))
print('cells', len(X), 'junk', int((Y == 13).sum()), flush=True)
net = Net(W, 14); print('params', sum(p.numel() for p in net.parameters()), flush=True)
opt = torch.optim.AdamW(net.parameters(), 2e-3, weight_decay=1e-4); bs = 512
sched = torch.optim.lr_scheduler.OneCycleLR(opt, 3e-3, total_steps=E * (len(X) // bs + 1))
lossf = nn.CrossEntropyLoss(ignore_index=-1)
for e in range(E):
    t = time.time(); net.train(); perm = torch.randperm(len(X)); tl = 0
    for k in range(0, len(X), bs):
        idx = perm[k:k + bs]; xb = X[idx].float()
        # brightness/contrast jitter in normalised space
        xb = (xb * torch.empty(len(xb), 1, 1, 1).uniform_(.85, 1.1) + torch.empty(len(xb), 1, 1, 1).uniform_(-.1, .1)).clamp(-1, 1)
        opt.zero_grad(); l = lossf(net(xb), Y[idx]); l.backward(); opt.step(); sched.step(); tl += l.item()
    net.eval()
    with torch.no_grad():
        p = torch.cat([net(VX[i:i + 8192]).argmax(1) for i in range(0, len(VX), 8192)]); m = VY >= 0
        acc = (p[m] == VY[m]).float().mean().item()
    print(f'epoch {e} loss {tl / (len(X) / bs):.4f} val {acc:.4f} {time.time() - t:.0f}s', flush=True)
    torch.save(net.state_dict(), out)
