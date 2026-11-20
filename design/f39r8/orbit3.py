"""Luminous Orbit v3 — trails that live in slightly different planes.

Each trail carries its own tilt, ellipse ratio and centre offset, so the ring is no longer one
average ellipse. Bright points are not painted on: the energy field is boosted superlinearly
above a threshold, so light gathers exactly where trails actually cross.
"""
import numpy as np
from PIL import Image

SS = 3
CREAM = np.array([250.0, 244.0, 234.0])
CORE_HOT = np.array([255.0, 248.0, 232.0])
MID      = np.array([255.0, 208.0, 166.0])
EDGE     = np.array([250.0, 176.0, 140.0])


class Field:
    def __init__(self, n):
        self.n = n
        self.L = np.zeros((n, n), np.float32)

    def _blob(self, x, y, r, w):
        n = self.n
        rr = max(0.9, r * 3.0)
        x0, x1 = int(max(0, x - rr)), int(min(n, x + rr + 1))
        y0, y1 = int(max(0, y - rr)), int(min(n, y + rr + 1))
        if x0 >= x1 or y0 >= y1:
            return
        yy, xx = np.mgrid[y0:y1, x0:x1]
        g = np.exp(-(((xx - x) ** 2 + (yy - y) ** 2) / (r * r)))
        self.L[y0:y1, x0:x1] += g * w

    def stamp(self, x, y, r, intensity):
        self._blob(x, y, r * 7.0, 0.0011 * intensity)
        self._blob(x, y, r * 2.6, 0.0075 * intensity)
        self._blob(x, y, r * 0.85, 0.1150 * intensity)

    def resolve(self, thr=0.36, gain=1.25):
        L = self.L
        hot = np.clip(L - thr, 0, None)
        Lp = L + gain * hot ** 2 * 6.0
        i = 1.0 - np.exp(-Lp * 1.35)
        t = np.clip(i, 0, 1)[..., None]
        lo = EDGE + (MID - EDGE) * np.clip(t * 2.2, 0, 1)
        hi = MID + (CORE_HOT - MID) * np.clip((t - 0.45) / 0.55, 0, 1)
        rgb = np.where(t < 0.45, lo, hi)
        a = np.clip(i ** 0.72 * 1.15, 0, 1)
        return Image.fromarray(np.dstack([np.clip(rgb, 0, 255), a * 255]).astype(np.uint8), "RGBA")


# radius, depth, thickness, brightness, phase, gapseed, wobble, incl, tilt_deg, dx, dy
# aspect now varies per trail: with one shared ratio they nest instead of crossing
A = [
    (0.965,  0.40, 0.0034, 1.00, 0.10, 0.0, 0.018, 1.06, -12,  0.000,  0.010),
    (1.075,  0.15, 0.0030, 0.62, 1.70, 1.1, 0.026, 0.86, -44,  0.020, -0.014),
    (0.905, -0.10, 0.0028, 0.55, 3.05, 2.0, 0.014, 1.16, -30, -0.022,  0.012),
    (1.150, -0.35, 0.0022, 0.34, 4.40, 2.8, 0.030, 0.80, -52,  0.026, -0.004),
    (0.850,  0.10, 0.0020, 0.28, 5.55, 3.6, 0.012, 1.10, -6,  -0.024, -0.010),
    (1.010, -0.55, 0.0016, 0.19, 5.90, 4.4, 0.022, 0.96, -36,  0.008,  0.018),
]
B_AUX = [
    (1.310, -0.70, 0.0013, 0.085, 2.40, 5.2, 0.040, 0.82,  -9,  0.030, -0.014),
    (0.735,  0.28, 0.0012, 0.070, 4.10, 6.0, 0.010, 1.14, -37, -0.026,  0.016),
]


def render(size, variant="A", detail=1.0):
    n = size * SS
    f = Field(n)
    cx = cy = n / 2
    R = n * 0.415
    trails = A if variant == "A" else A + B_AUX
    if detail < 1.0:
        trails = [t for t in trails if t[3] >= (0.30 if detail > 0.5 else 0.55)]
    for (rs, z, th, br, ph, gs, wob, incl, tilt_deg, dx, dy) in trails:
        tilt = np.radians(tilt_deg)
        ox, oy = dx * R, dy * R
        steps = max(1400, int(1500 * SS * detail * (1.0 if br > 0.1 else 0.6)))
        for i in range(steps):
            t = 2 * np.pi * i / steps
            rad = R * rs * (1 + wob * np.sin(3 * t + ph) + wob * 0.5 * np.sin(5 * t + 1.7 * ph))
            x = rad * np.cos(t)
            y = rad * np.sin(t) * 0.60 * incl
            xr = x * np.cos(tilt) - y * np.sin(tilt)
            yr = x * np.sin(tilt) + y * np.cos(tilt)
            persp = 1.0 + 0.08 * z
            px, py = cx + ox + xr * persp, cy + oy + yr * persp
            breathe = 0.45 + 0.55 * (0.5 + 0.5 * np.sin(2 * t + gs * 1.9))
            if breathe < 0.30:
                continue
            val = br * breathe * (0.55 + 0.45 * np.sin(t + ph)) * (0.60 + 0.40 * (z + 0.5))
            if val <= 0.004:
                continue
            f.stamp(px, py, th * n * 1.3, val)
    img = f.resolve()
    if SS != 1:
        img = img.resize((size, size), Image.LANCZOS)
    return img


def edge_clean(img, margin=0.05):
    a = np.asarray(img).astype(np.float32)
    n = a.shape[0]
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float32)
    c = (n - 1) / 2
    d = np.maximum(np.abs(xx - c), np.abs(yy - c)) / c
    t = np.clip((1.0 - d) / margin, 0, 1)
    a[..., 3] *= t * t * (3 - 2 * t)
    return Image.fromarray(np.clip(a, 0, 255).astype(np.uint8), "RGBA")


def on(bg, img):
    base = Image.new("RGBA", img.size, tuple(bg) + (255,))
    return Image.alpha_composite(base, img).convert("RGB")
