"""Luminous Orbit — a floating elliptical light sculpture built from filaments.

The ring is not one shape. Several filaments sit at different depths, each with its own
radius, phase, thickness and gap pattern, so they cross, occlude and disappear. Light is
accumulated additively and then written as straight alpha, so the asset composites on any
ground without a baked background.
"""
import numpy as np
from PIL import Image

SS = 3  # supersample

# warm optical palette: nothing here is black or metallic brown
PAPER_WARM = np.array([255.0, 244.0, 228.0])
CHAMPAGNE  = np.array([255.0, 226.0, 178.0])
AMBER      = np.array([255.0, 191.0, 137.0])
PEACH      = np.array([255.0, 176.0, 138.0])
IVORY      = np.array([255.0, 246.0, 224.0])


def ramp(t):
    """intensity -> colour: peach at the edges, champagne mid, ivory at the core."""
    t = np.clip(t, 0, 1)[..., None]
    lo = PEACH + (CHAMPAGNE - PEACH) * np.clip(t * 2.4, 0, 1)
    hi = CHAMPAGNE + (IVORY - CHAMPAGNE) * np.clip((t - 0.42) / 0.58, 0, 1)
    return np.where(t < 0.42, lo, hi)


class Field:
    def __init__(self, size):
        self.n = size
        self.rgb = np.zeros((size, size, 3), np.float32)
        self.a = np.zeros((size, size), np.float32)

    def stamp(self, x, y, radius, value, colour, alpha_w):
        n = self.n
        r = max(1.0, radius)
        x0, x1 = int(max(0, x - r * 3)), int(min(n, x + r * 3 + 1))
        y0, y1 = int(max(0, y - r * 3)), int(min(n, y + r * 3 + 1))
        if x0 >= x1 or y0 >= y1:
            return
        yy, xx = np.mgrid[y0:y1, x0:x1]
        d2 = ((xx - x) ** 2 + (yy - y) ** 2) / (r * r)
        g = np.exp(-d2 * 1.35) * value
        self.rgb[y0:y1, x0:x1] += g[..., None] * colour
        self.a[y0:y1, x0:x1] = np.maximum(self.a[y0:y1, x0:x1], np.clip(g * alpha_w, 0, 1))

    def to_image(self):
        i = np.clip(self.a * 2.6, 0, 1)      # trails must read as light, not as haze
        col = np.clip(self.rgb * 0.75 + ramp(i) * 0.25, 0, 255)
        rgba = np.dstack([col, (i ** 0.78) * 255])
        return Image.fromarray(np.clip(rgba, 0, 255).astype(np.uint8), "RGBA")

    def bloom(self, strength=0.55):
        """A restrained volumetric pass: blur the light, add it back, keep the contour."""
        from PIL import ImageFilter
        base = self.to_image()
        g = Image.fromarray(np.clip(self.rgb * 255, 0, 255).astype(np.uint8), "RGB")
        g = g.filter(ImageFilter.GaussianBlur(max(1.0, self.n * 0.012)))
        halo = np.asarray(g).astype(np.float32)
        a = np.asarray(base).astype(np.float32)
        warm = halo * np.array([1.0, 0.86, 0.70], np.float32)
        out = a.copy()
        out[..., :3] = np.clip(a[..., :3] + warm * strength * (halo.max() / 255.0 + 0.15), 0, 255)
        out[..., 3] = np.clip(a[..., 3] + halo.max(axis=2) * strength * 0.55, 0, 255)
        return Image.fromarray(out.astype(np.uint8), "RGBA")


# Each filament: (radius scale, depth z, thickness, brightness, phase, gap seed, wobble)
# (radius, depth, thickness, brightness, phase, gapseed, wobble, inclination)
FILAMENTS = [
    (1.000,  0.35, 0.0125, 2.20, 0.15, 0.0, 0.020, 1.00),
    (0.940,  0.05, 0.0068, 1.30, 1.90, 1.1, 0.014, 1.16),
    (1.060, -0.30, 0.0052, 0.85, 3.20, 1.9, 0.024, 0.86),
    (0.908, -0.12, 0.0042, 0.62, 4.40, 2.6, 0.012, 1.10),
    (1.092,  0.20, 0.0034, 0.50, 5.60, 3.3, 0.020, 0.92),
    (0.876, -0.48, 0.0028, 0.34, 0.90, 4.1, 0.010, 1.02),
    (1.120, -0.22, 0.0022, 0.24, 2.70, 4.8, 0.028, 0.84),
]

TILT = np.radians(-17.0)
INCL = 0.62          # how much the ellipse is compressed: a sculpture seen at an angle


def render(size, detail=1.0):
    """detail < 1 drops the faintest filaments and thickens the rest (small-size optics)."""
    n = size * SS
    f = Field(n)
    cx = cy = n / 2
    R = n * 0.44

    fils = FILAMENTS if detail >= 1.0 else [x for x in FILAMENTS if x[3] >= (0.60 if detail > 0.5 else 0.90)]
    boost = 1.0 if detail >= 1.0 else (1.25 if detail > 0.5 else 1.6)

    for k, (rs, z, th, br, ph, gapseed, wob, incl) in enumerate(fils):
        # sampling must be finer than the brush radius, or thin trails bead up
        steps = max(1200, int(1400 * SS * detail))
        for i in range(steps):
            t = 2 * np.pi * i / steps
            # organic radius: a slow wobble so the filament is never a perfect ellipse
            rad = R * rs * (1 + wob * np.sin(3 * t + ph) + wob * 0.6 * np.sin(5 * t + 2 * ph))
            x = rad * np.cos(t)
            y = rad * np.sin(t) * INCL * incl
            # tilt in the plane
            xr = x * np.cos(TILT) - y * np.sin(TILT)
            yr = x * np.sin(TILT) + y * np.cos(TILT)
            # depth: nearer arcs are larger and brighter, far ones dimmer and shifted
            persp = 1.0 + 0.10 * z
            px = cx + xr * persp
            py = cy + yr * persp

            # gaps: a smooth mask makes parts of the trail vanish (breathing, not dashed)
            gap = 0.5 + 0.5 * np.sin(2 * t + gapseed * 2.1) * np.sin(3 * t - gapseed)
            if gap < 0.34:
                continue

            # focal concentration near one crossing, plus a soft long-form gradient
            focal = np.exp(-((np.cos(t - 2.35)) ** 2) / 0.030)
            value = br * gap * (0.55 + 0.45 * np.sin(t + ph)) * (0.55 + 2.9 * focal) * boost
            depth_w = 0.55 + 0.45 * (z + 0.5)
            value *= depth_w
            if value <= 0.004:
                continue

            col = ramp(min(1.0, value))[0]
            f.stamp(px, py, th * n * (0.85 + 0.5 * focal) * 1.15, value * 0.55, col,
                    alpha_w=1.0 if detail >= 1.0 else 1.15)

    img = f.bloom(0.40 if detail >= 1.0 else 0.26)
    if SS != 1:
        img = img.resize((size, size), Image.LANCZOS)
    return img


def on(bg, img):
    from PIL import Image as I
    base = I.new("RGBA", img.size, bg + (255,))
    return I.alpha_composite(base, img).convert("RGB")
