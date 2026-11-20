"""Luminous Being — three optical principles carried by three different silhouettes.

L1 Ember    light travels through a tilted mass; a warm seam splits the crust
L2 Membrane light is the surface; a lying teardrop, caustic in the bulb
L3 Cinder   light escapes; a comma-shaped husk with the light pooled in its head

No centred radial shading anywhere: the light always sits off-axis, so none of these
can be mistaken for a sphere, a button or an indicator lamp. Design study only.
"""
import numpy as np
from PIL import Image

SS = 4
HI   = np.array([255.0, 233.0, 199.0])
MID  = np.array([228.0, 152.0, 110.0])
DEEP = np.array([104.0, 52.0, 46.0])
INK  = np.array([30.0, 27.0, 30.0])


def _grid(size):
    y, x = np.mgrid[0:size, 0:size].astype(np.float32)
    return (x - size / 2 + 0.5) / (size / 2), (y - size / 2 + 0.5) / (size / 2)


def _smooth(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0, 1)
    return t * t * (3 - 2 * t)


def _shape(kind, gx, gy):
    """Signed-ish radius. 1.0 = the boundary, larger = outside."""
    if kind == "ember":
        # tilted bean: an ellipse rotated 24 deg with one side swollen
        c, s = np.cos(-0.42), np.sin(-0.42)
        px = gx * c - gy * s
        py = gx * s + gy * c
        return np.sqrt((px / 1.12) ** 2 + (py / 0.86) ** 2) * (1 + 0.10 * px)
    if kind == "membrane":
        # a drop lying on its side: round bulb left, tail drawn out to the right
        ang = np.arctan2(gy, gx)
        r = np.sqrt(gx * gx + gy * gy)
        tail = 0.34 * np.exp(-((ang - 0.10) / 0.55) ** 2)
        return r / (0.86 + tail)
    # cinder: a comma - thick head upper-left, tapering arm to the lower-right
    ang = np.arctan2(gy, gx)
    r = np.sqrt(gx * gx + gy * gy)
    swell = 0.26 * np.exp(-((ang + 2.55) / 0.80) ** 2)
    limb  = 0.34 * np.exp(-((ang - 0.62) / 0.42) ** 2)   # a drawn-out arm
    notch = -0.16 * np.exp(-((ang + 0.10) / 0.35) ** 2)
    return r / (0.68 + swell + limb + notch)


def render(kind, px, lit=1.0):
    S = px * SS
    gx, gy = _grid(S)
    r = _shape(kind, gx, gy)
    R = 0.94
    inside = _smooth(R + 0.05, R - 0.05, r)
    edge = np.exp(-((r - R) / 0.045) ** 2)

    # directional light: upper-left key, lower-right falls away
    key = np.clip(0.62 - 0.42 * gx - 0.30 * gy, 0, 1.3)

    if kind == "ember":
        # a warm seam along the swollen flank, thick enough to survive 24px
        seam = np.exp(-(((gy + 0.10) - 0.22 * gx) / 0.085) ** 2) * inside
        body = (INK[None, None, :] * (1 - key[..., None] * 0.55)
                + DEEP[None, None, :] * (key * 0.35)[..., None])
        body = body + MID[None, None, :] * (seam * 0.80)[..., None]
        body = body + HI[None, None, :] * ((seam ** 2) * 0.85)[..., None]
        body = body + DEEP[None, None, :] * (edge * 0.30)[..., None]
        alpha = inside
        glow = 0.22
    elif kind == "membrane":
        # the shell itself is the light source; it thins toward the tail
        shell = np.clip(1.05 - r * 0.55, 0.15, 1.0)
        caustic = np.exp(-(((gx + 0.26) ** 2 + (gy + 0.16) ** 2) / 0.16))
        body = (MID[None, None, :] * (0.55 + 0.40 * shell)[..., None]
                + HI[None, None, :] * (caustic * 0.85)[..., None]
                + HI[None, None, :] * (key * 0.18)[..., None])
        body = body * (1.0 - 0.42 * edge[..., None])
        alpha = inside * 0.98
        glow = 0.34
    else:
        # husk: mostly silhouette, light pooled in the head
        head = np.exp(-(((gx + 0.30) ** 2 + (gy + 0.28) ** 2) / 0.30))
        body = INK[None, None, :] * (1 - key[..., None] * 0.35) + DEEP[None, None, :] * (0.25 * key)[..., None]
        body = body + MID[None, None, :] * (head * 0.55)[..., None]
        body = body + HI[None, None, :] * ((head ** 2.2) * 0.95)[..., None]
        alpha = inside
        glow = 0.30

    cd = np.sqrt((gx + 0.18) ** 2 + (gy + 0.14) ** 2)
    bloom = np.exp(-(cd / 1.15) ** 2) * glow

    out = np.zeros((S, S, 4), np.float32)
    out[..., :3] = np.clip(body, 0, 255)
    out[..., 3] = np.clip(np.maximum(alpha * 255 * lit, bloom * 255 * lit), 0, 255)
    img = Image.fromarray(out.astype(np.uint8), "RGBA")
    return img.resize((px, px), Image.LANCZOS)


def on(bg, im):
    base = Image.new("RGBA", im.size, bg + (255,))
    return Image.alpha_composite(base, im).convert("RGB")


NAMES = {"ember": "L1 · Ember 余烬", "membrane": "L2 · Membrane 光膜", "cinder": "L3 · Cinder 光核"}
