"""Luminous Orbit v2 — light bleeding into warm paper.

Every trail is two things at once: a wide soft veil and a thin bright core. That pairing is
what makes it read as light on paper rather than as a drawn line. Colour stays in the warm
gold-peach range; nothing is white-white except the tightest crossing points.
"""
import numpy as np
from PIL import Image

SS = 3
CREAM = np.array([250.0, 244.0, 234.0])

CORE_HOT = np.array([255.0, 240.0, 214.0])
MID      = np.array([255.0, 205.0, 162.0])
EDGE     = np.array([255.0, 183.0, 146.0])


class Canvas:
    """A single scalar light field. Colour and alpha are derived from it at the end, which
    keeps the hue warm instead of saturating to white when trails overlap."""

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

    def stamp(self, x, y, r, intensity, focal):
        # wide veil, mid bloom, tight core — the three together read as light in paper
        self._blob(x, y, r * 7.0, 0.0011 * intensity)
        self._blob(x, y, r * 2.6, 0.0075 * intensity)
        self._blob(x, y, r * 0.85, 0.1150 * intensity * (1.0 + 0.9 * focal))

    def image(self):
        L = self.L
        i = 1.0 - np.exp(-L * 1.35)                  # saturating, never blows out
        t = np.clip(i, 0, 1)[..., None]
        # peach -> gold -> warm cream; the top end stops short of white on purpose
        lo = EDGE + (MID - EDGE) * np.clip(t * 2.2, 0, 1)
        hi = MID + (CORE_HOT - MID) * np.clip((t - 0.45) / 0.55, 0, 1)
        rgb = np.where(t < 0.45, lo, hi)
        a = np.clip(i ** 0.72 * 1.15, 0, 1)
        return Image.fromarray(np.dstack([np.clip(rgb, 0, 255), a * 255]).astype(np.uint8), "RGBA")


# (radius, depth, thickness, brightness, phase, gapseed, wobble, inclination)
TRAILS = [
    (1.000,  0.40, 0.0034, 1.00, 0.10, 0.0, 0.018, 1.00),
    (1.115,  0.15, 0.0030, 0.66, 1.70, 1.1, 0.030, 0.93),
    (0.880, -0.10, 0.0028, 0.58, 3.05, 2.0, 0.016, 1.10),
    (1.200, -0.35, 0.0022, 0.36, 4.40, 2.8, 0.034, 0.88),
    (0.800,  0.10, 0.0020, 0.30, 5.55, 3.6, 0.012, 1.06),
    (1.060, -0.55, 0.0016, 0.20, 5.90, 4.4, 0.024, 0.97),
]
TILT = np.radians(-26.0)
INCL = 0.60


def render(size, detail=1.0):
    n = size * SS
    c = Canvas(n)
    cx = cy = n / 2
    R = n * 0.415
    trails = TRAILS if detail >= 1.0 else [t for t in TRAILS if t[3] >= (0.30 if detail > 0.5 else 0.55)]
    boost = 1.0 if detail >= 1.0 else 1.35

    for (rs, z, th, br, ph, gs, wob, incl) in trails:
        steps = max(1400, int(1500 * SS * detail))
        for i in range(steps):
            t = 2 * np.pi * i / steps
            rad = R * rs * (1 + wob * np.sin(3 * t + ph) + wob * 0.5 * np.sin(5 * t + 1.7 * ph))
            x = rad * np.cos(t)
            y = rad * np.sin(t) * INCL * incl
            xr = x * np.cos(TILT) - y * np.sin(TILT)
            yr = x * np.sin(TILT) + y * np.cos(TILT)
            persp = 1.0 + 0.08 * z
            px, py = cx + xr * persp, cy + yr * persp

            # trails fade in and out along their length instead of stopping abruptly
            breathe = 0.45 + 0.55 * (0.5 + 0.5 * np.sin(2 * t + gs * 1.9))
            if breathe < 0.28:
                continue
            # two crossings where light gathers
            focal = (np.exp(-((np.cos(t - 2.15)) ** 2) / 0.022)
                     + 0.55 * np.exp(-((np.cos(t + 0.75)) ** 2) / 0.05))
            val = br * breathe * (0.55 + 0.45 * np.sin(t + ph)) * (0.55 + 1.5 * focal) * boost
            val *= 0.60 + 0.40 * (z + 0.5)
            if val <= 0.006:
                continue
            c.stamp(px, py, th * n * (0.9 + 0.55 * focal) * 1.4, val, focal)

    img = c.image()
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
