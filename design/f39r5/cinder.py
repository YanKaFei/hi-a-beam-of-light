"""L3 · Cinder 光核 — final form, with the 24px correction folded in.

The only size-dependent correction is the notch: below 44px the concave side deepens so
the comma silhouette survives anti-aliasing in two or three pixels. The light pool and
the bloom are size-independent (C1, which grew the pool, turned the being into a lamp and
was rejected).
"""
import numpy as np
from PIL import Image

SS = 4
HI   = np.array([255.0, 233.0, 199.0])
MID  = np.array([228.0, 152.0, 110.0])
DEEP = np.array([104.0, 52.0, 46.0])
INK  = np.array([30.0, 27.0, 30.0])

PARAMS = dict(
    body="#1E1B1E", core="#E4986E", bloom=0.30, notch_base=0.16,
    notch_small=0.55,     # extra depth below 44px, scaled by (1 - px/44)
    pool=0.30, pool_at=(-0.30, -0.28),
    limb=0.34, swell=0.26,
)


def _grid(size):
    y, x = np.mgrid[0:size, 0:size].astype(np.float32)
    return (x - size / 2 + 0.5) / (size / 2), (y - size / 2 + 0.5) / (size / 2)


def _smooth(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0, 1)
    return t * t * (3 - 2 * t)


def render(px, lit=1.0):
    S = px * SS
    gx, gy = _grid(S)
    ang = np.arctan2(gy, gx)
    r = np.sqrt(gx * gx + gy * gy)
    small = max(0.0, 1.0 - px / 44.0)
    deepen = PARAMS["notch_small"] * small

    swell = PARAMS["swell"] * np.exp(-((ang + 2.55) / 0.80) ** 2)
    limb  = PARAMS["limb"] * np.exp(-((ang - 0.62) / 0.42) ** 2)
    notch = -(PARAMS["notch_base"] * np.exp(-((ang + 0.10) / 0.35) ** 2)
              + deepen * np.exp(-((ang + 0.10) / 0.30) ** 2))
    rr = r / (0.68 + swell + limb + notch)

    inside = _smooth(0.99, 0.89, rr)
    key = np.clip(0.62 - 0.42 * gx - 0.30 * gy, 0, 1.3)
    cx, cy = PARAMS["pool_at"]
    head = np.exp(-(((gx - cx) ** 2 + (gy - cy) ** 2) / PARAMS["pool"]))

    body = INK[None, None, :] * (1 - key[..., None] * 0.35) + DEEP[None, None, :] * (0.25 * key)[..., None]
    body = body + MID[None, None, :] * (head * 0.55)[..., None]
    body = body + HI[None, None, :] * ((head ** 2.2) * 0.95)[..., None]

    cd = np.sqrt((gx + 0.18) ** 2 + (gy + 0.14) ** 2)
    bloom = np.exp(-(cd / 1.15) ** 2) * PARAMS["bloom"]
    # The bloom must reach zero before the tile edge, otherwise pasting the RGBA over a
    # dark desktop leaves a visible square. Fade over the outer 18% of the canvas.
    rimfade = np.clip((1.0 - np.maximum(np.abs(gx), np.abs(gy))) / 0.18, 0, 1)
    rimfade = rimfade * rimfade * (3 - 2 * rimfade)
    bloom = bloom * rimfade

    out = np.zeros((S, S, 4), np.float32)
    out[..., :3] = np.clip(body, 0, 255)
    out[..., 3] = np.clip(np.maximum(inside * 255 * lit, bloom * 255 * lit), 0, 255)
    return Image.fromarray(out.astype(np.uint8), "RGBA").resize((px, px), Image.LANCZOS)


def on(bg, im):
    base = Image.new("RGBA", im.size, bg + (255,))
    return Image.alpha_composite(base, im).convert("RGB")
