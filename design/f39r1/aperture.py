"""Organic Aperture.

Lesson from the first attempt: separating the six blades with gaps turned into
noise at 24px and destroyed the silhouette. The silhouette is the only thing that
survives small sizes, so it is now a single solid body and the "organic" quality
lives in the aperture and the rim:

  body    superellipse (rotated squircle) - soft, not a machined hexagon
  hole    rosette aperture, r(t) = r0 * (1 + k*cos(6t)) - petal-edged, not a circle
  seams   thin low-contrast radial lines, only above 32px (dropped when they'd be noise)

Nothing here ships; paintBead is untouched.
"""
import math
from PIL import Image, ImageDraw

SS = 4
INK   = (32, 43, 48)
WARM  = (138, 79, 65)
LIGHT = (246, 210, 176)
OCHRE = (169, 107, 91)
PAPER = (246, 247, 244)


def _glow(im, cx, cy, r, color, alpha, steps=14):
    g = Image.new("RGBA", im.size, (0, 0, 0, 0)); d = ImageDraw.Draw(g)
    for i in range(steps, 0, -1):
        rr = r * i / steps
        a = int(alpha * 255 * (1 - i / steps) ** 1.6)
        if a > 0:
            d.ellipse([cx - rr, cy - rr, cx + rr, cy + rr], fill=color + (a,))
    return Image.alpha_composite(im, g)


def superellipse(cx, cy, R, n=2.55, rot=math.radians(15), steps=180):
    """A squircle: |x|^n + |y|^n = 1, rotated. Softer than a hexagon, still solid."""
    pts = []
    for i in range(steps):
        t = 2 * math.pi * i / steps
        c, s = math.cos(t), math.sin(t)
        x = math.copysign(abs(c) ** (2 / n), c) * R
        y = math.copysign(abs(s) ** (2 / n), s) * R
        pts.append((cx + x * math.cos(rot) - y * math.sin(rot),
                    cy + x * math.sin(rot) + y * math.cos(rot)))
    return pts


def rosette(cx, cy, r0, lobes=6, k=0.10, rot=0.0, steps=200):
    pts = []
    for i in range(steps):
        t = 2 * math.pi * i / steps
        r = r0 * (1 + k * math.cos(lobes * (t + rot)))
        pts.append((cx + r * math.cos(t), cy + r * math.sin(t)))
    return pts


def draw(px, tier=0.0, organic=1.0, sig=0.0, lit=1.0, notch=0.0):
    """tier 0..1 -> aperture opening. organic 0 = the old hard hexagon."""
    S = px * SS
    im = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    cx = cy = S / 2
    R = S * 0.355
    rot = math.radians(15 + 7 * sig)
    # the hole never closes to nothing and never swallows the rim
    r0 = R * (0.13 + 0.50 * tier)
    n = (2.55 * organic + 6.0 * (1 - organic)) if organic < 1 else 3.45

    im = _glow(im, cx, cy, R * 1.45, OCHRE, 0.18 * lit)
    d = ImageDraw.Draw(im)
    body = superellipse(cx, cy, R, n=n, rot=rot)
    d.polygon(body, fill=INK)

    # Signature: a permanent notch in the rim. It is the only cue that still
    # reads at 16px, where the aperture itself is one bright pixel.
    if notch > 0:
        na = rot + math.radians(38)
        nr = R * (0.72 + 0.26 * (1 - notch) * 0)  # depth is fixed, see below
        w = math.radians(13 * notch)
        for side in (-1, 1):
            pass
        p1 = (cx + R * 1.02 * math.cos(na - w), cy + R * 1.02 * math.sin(na - w))
        p2 = (cx + R * 1.02 * math.cos(na + w), cy + R * 1.02 * math.sin(na + w))
        p3 = (cx + R * (0.80 - 0.06 * notch) * math.cos(na),
              cy + R * (0.80 - 0.06 * notch) * math.sin(na))
        d.polygon([p1, p2, p3], fill=(0, 0, 0, 0))

    # punch the aperture: draw the rosette in the light colour, then glow it
    hole_cx = cx + R * 0.035 * sig        # signature: slightly off-centre
    hole_cy = cy - R * 0.02 * sig
    hole = rosette(hole_cx, hole_cy, r0, k=0.045 + 0.02 * sig, rot=rot * 0.5)
    d.polygon(hole, fill=LIGHT if lit > 0.45 else OCHRE)

    # seams: only where they can actually be seen
    if px >= 32:
        a = int(46 * organic * (1 if lit > 0.4 else 0.6))
        for k in range(6):
            ang = rot + k * math.pi / 3
            x1 = hole_cx + r0 * 1.12 * math.cos(ang)
            y1 = hole_cy + r0 * 1.12 * math.sin(ang)
            x2 = cx + R * 0.93 * math.cos(ang)
            y2 = cy + R * 0.93 * math.sin(ang)
            d.line([x1, y1, x2, y2], fill=INK + (0,) if False else (232, 226, 216, a),
                   width=max(1, int(S * 0.011)))

    im = _glow(im, hole_cx, hole_cy, r0 * 1.6, LIGHT, 0.30 * lit)
    return im.resize((px, px), Image.LANCZOS)


def seed(px, lit=1.0):
    S = px * SS
    im = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    cx, cy = S / 2, S * 0.55
    im = _glow(im, cx, cy - S * 0.16, S * 0.34, OCHRE, 0.22 * lit)
    d = ImageDraw.Draw(im); r = S * 0.255
    d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=INK)
    d.polygon([(cx, cy - r * 1.95), (cx - r * 0.70, cy - r * 0.26),
               (cx + r * 0.70, cy - r * 0.26)], fill=INK)
    d.ellipse([cx - r * 0.32, cy - r * 1.32, cx + r * 0.32, cy - r * 0.62],
              fill=LIGHT if lit > 0.4 else OCHRE)
    im = _glow(im, cx, cy - r * 0.97, S * 0.09, LIGHT, 0.45 * lit)
    return im.resize((px, px), Image.LANCZOS)


def on(bg, im):
    base = Image.new("RGBA", im.size, bg + (255,))
    return Image.alpha_composite(base, im).convert("RGB")
