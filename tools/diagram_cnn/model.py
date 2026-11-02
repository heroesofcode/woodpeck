# Square classifier and the per-board 2nd..98th percentile normalisation used by `books::cnn`.
import numpy as np, torch.nn as nn
def norm(x):
    x = x.astype(np.float32) / 255
    lo = np.percentile(x.reshape(len(x), -1), 2, axis=1)[:, None, None, None]
    hi = np.percentile(x.reshape(len(x), -1), 98, axis=1)[:, None, None, None]
    return np.clip((x - lo) / np.maximum(hi - lo, .2), 0, 1) * 2 - 1
class Net(nn.Module):
    def __init__(s, w, nc=13):
        super().__init__()
        a, b, c = w
        s.f = nn.Sequential(nn.Conv2d(1, a, 3, padding=1), nn.ReLU(), nn.MaxPool2d(2),
                            nn.Conv2d(a, b, 3, padding=1), nn.ReLU(), nn.MaxPool2d(2),
                            nn.Conv2d(b, c, 3, padding=1), nn.ReLU(), nn.MaxPool2d(2),
                            nn.Flatten(), nn.Linear(c * 16, 64), nn.ReLU(), nn.Linear(64, nc))
    def forward(s, x): return s.f(x)
