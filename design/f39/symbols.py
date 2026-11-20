"""F3.9 brand symbol studies.

Nine candidates in three families. Drawn at 4x and downsampled, so the shape
survives 24px. Nothing here ships; production UI is untouched.

Palette is the B+ one: ink structure, warm ochre light.
"""
from PIL import Image, ImageDraw, ImageFilter
import math

INK    = (32, 43, 48)
WARM   = (138, 79, 65)
OCHRE  = (169, 107, 91)
LIGHT  = (246, 210, 176)
PALE   = (252, 240, 228)

SS = 4  # supersample


def _canvas(px):
    im = Image.new("RGBA", (px * SS, px * SS), (0, 0, 0, 0))
    return im, ImageDraw.Draw(im)


def _glow(im, cx, cy, r, color, alpha):
    """soft falloff, drawn as concentric alpha steps (no blur cost per frame)"""
    g = Image.new("RGBA", im.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(g)
    steps = 12
    for i in range(steps, 0, -1):
        rr = r * i / steps
        a = int(alpha * 255 * (1 - i / steps) ** 1.6)
        if a <= 0:
            continue
        d.ellipse([cx - rr, cy - rr, cx + rr, cy + rr], fill=color + (a,))
    return Image.alpha_composite(im, g)


def _finish(im, px):
    return im.resize((px, px), Image.LANCZOS)


# ---------------------------------------------------------------- A · light aperture
def a1_slit(px, lit=1.0):
    """a narrow aperture; light leaks through the seam"""
    im, d = _canvas(px)
    S = px * SS
    w = S * 0.16            # aperture width
    h = S * 0.62            # aperture height
    cx, cy = S / 2, S / 2
    im = _glow(im, cx, cy, S * 0.42, OCHRE, 0.30 * lit)
    d = ImageDraw.Draw(im)
    # two arcs form a vertical almond
    d.chord([cx - w, cy - h / 2, cx + w, cy + h / 2], 0, 360, fill=INK)
    # the seam
    d.rectangle([cx - w * 0.16, cy - h * 0.42, cx + w * 0.16, cy + h * 0.42], fill=LIGHT)
    im = _glow(im, cx, cy, S * 0.12, LIGHT, 0.55 * lit)
    return im


def a2_iris(px, lit=1.0):
    """a six-blade iris; the opening is where light comes from"""
    im, d = _canvas(px)
    S = px * SS
    cx, cy = S / 2, S / 2
    r = S * 0.34
    im = _glow(im, cx, cy, r * 1.5, OCHRE, 0.26 * lit)
    d = ImageDraw.Draw(im)
    for k in range(6):
        a = math.radians(k * 60)
        a2 = math.radians(k * 60 + 60)
        pts = [(cx, cy),
               (cx + r * math.cos(a), cy + r * math.sin(a)),
               (cx + r * math.cos(a2), cy + r * math.sin(a2))]
        d.polygon(pts, fill=INK)
    d.ellipse([cx - S * 0.10, cy - S * 0.10, cx + S * 0.10, cy + S * 0.10],
              fill=LIGHT if lit > 0.5 else OCHRE)
    im = _glow(im, cx, cy, S * 0.13, LIGHT, 0.5 * lit)
    return im


def a3_louvre(px, lit=1.0):
    """three offset slits, light escaping diagonally"""
    im, d = _canvas(px)
    S = px * SS
    im = _glow(im, S * 0.52, S * 0.5, S * 0.4, OCHRE, 0.24 * lit)
    d = ImageDraw.Draw(im)
    for i, off in enumerate((-1, 0, 1)):
        y = S * 0.5 + off * S * 0.16
        x0 = S * (0.22 + (0.06 if off > 0 else 0.0))
        x1 = S * (0.74 + (0.04 if off > 0 else 0.0))
        d.rounded_rectangle([x0, y - S * 0.045, x1, y + S * 0.045],
                            radius=S * 0.045, fill=INK)
        d.rounded_rectangle([x0 + S * 0.05, y - S * 0.014, x1 - S * 0.05, y + S * 0.014],
                            radius=S * 0.014, fill=LIGHT)
    return im


# ---------------------------------------------------------------- B · luminous seed
def b1_pip(px, lit=1.0):
    """a teardrop with the light at its tip"""
    im, d = _canvas(px)
    S = px * SS
    cx, cy = S / 2, S * 0.54
    im = _glow(im, cx, cy - S * 0.16, S * 0.34, OCHRE, 0.28 * lit)
    d = ImageDraw.Draw(im)
    r = S * 0.26
    # body: circle + cone to the tip
    d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=INK)
    d.polygon([(cx, cy - r * 1.9), (cx - r * 0.72, cy - r * 0.28),
               (cx + r * 0.72, cy - r * 0.28)], fill=INK)
    d.ellipse([cx - r * 0.34, cy - r * 1.30, cx + r * 0.34, cy - r * 0.62],
              fill=LIGHT if lit > 0.4 else OCHRE)
    im = _glow(im, cx, cy - r * 0.96, S * 0.1, LIGHT, 0.5 * lit)
    return im


def b2_cotyledon(px, lit=1.0):
    """a bean: light runs along the inner curve"""
    im, d = _canvas(px)
    S = px * SS
    im = _glow(im, S * 0.5, S * 0.52, S * 0.36, OCHRE, 0.24 * lit)
    d = ImageDraw.Draw(im)
    d.ellipse([S * 0.24, S * 0.30, S * 0.76, S * 0.74], fill=INK)
    d.ellipse([S * 0.38, S * 0.22, S * 0.66, S * 0.44], fill=(0, 0, 0, 0))
    d.arc([S * 0.26, S * 0.30, S * 0.72, S * 0.72], 200, 340,
          fill=LIGHT if lit > 0.4 else OCHRE, width=int(S * 0.045))
    return im


def b3_calyx(px, lit=1.0):
    """a seed held in an open cup"""
    im, d = _canvas(px)
    S = px * SS
    cx, cy = S / 2, S * 0.56
    im = _glow(im, cx, cy - S * 0.10, S * 0.32, OCHRE, 0.26 * lit)
    d = ImageDraw.Draw(im)
    r = S * 0.15
    d.ellipse([cx - r, cy - r * 1.5, cx + r, cy + r * 0.5], fill=LIGHT if lit > 0.4 else OCHRE)
    d.arc([cx - S * 0.30, cy - S * 0.10, cx + S * 0.30, cy + S * 0.34],
          20, 160, fill=INK, width=int(S * 0.075))
    im = _glow(im, cx, cy - S * 0.10, S * 0.11, LIGHT, 0.45 * lit)
    return im


# ---------------------------------------------------------------- C · fractured halo
def c1_crack(px, lit=1.0):
    """a ring with one wedge missing; the break is the brightest part"""
    im, d = _canvas(px)
    S = px * SS
    cx, cy = S / 2, S / 2
    r = S * 0.32
    im = _glow(im, cx, cy, r * 1.5, OCHRE, 0.26 * lit)
    d = ImageDraw.Draw(im)
    d.arc([cx - r, cy - r, cx + r, cy + r], -60, 240, fill=INK, width=int(S * 0.085))
    a = math.radians(-60)
    im = _glow(im, cx + r * math.cos(a), cy + r * math.sin(a), S * 0.13, LIGHT, 0.6 * lit)
    return im


def c2_shard(px, lit=1.0):
    """three arcs of unequal length at slightly different radii"""
    im, d = _canvas(px)
    S = px * SS
    cx, cy = S / 2, S / 2
    im = _glow(im, cx, cy, S * 0.34, OCHRE, 0.22 * lit)
    d = ImageDraw.Draw(im)
    for start, end, rr in ((10, 95, 0.34), (125, 190, 0.30), (215, 330, 0.36)):
        R = S * rr
        d.arc([cx - R, cy - R, cx + R, cy + R], start, end, fill=INK, width=int(S * 0.07))
    a = math.radians(100)
    im = _glow(im, cx + S * 0.34 * math.cos(a), cy + S * 0.34 * math.sin(a),
               S * 0.11, LIGHT, 0.55 * lit)
    return im


def c3_split(px, lit=1.0):
    """two concentric half-arcs drifting apart; the gap is the light"""
    im, d = _canvas(px)
    S = px * SS
    cx, cy = S / 2, S / 2
    im = _glow(im, cx, cy, S * 0.36, OCHRE, 0.24 * lit)
    d = ImageDraw.Draw(im)
    r1, r2 = S * 0.22, S * 0.32
    d.arc([cx - r2, cy - r2, cx + r2, cy + r2], 200, 20, fill=INK, width=int(S * 0.07))
    d.arc([cx - r1, cy - r1, cx + r1, cy + r1], 20, 200, fill=INK, width=int(S * 0.07))
    im = _glow(im, cx + r2 * 0.75, cy, S * 0.09, LIGHT, 0.5 * lit)
    im = _glow(im, cx - r2 * 0.75, cy, S * 0.07, LIGHT, 0.35 * lit)
    return im


ALL = {
    "A1": ("光隙 · 竖缝", a1_slit), "A2": ("光隙 · 六叶光圈", a2_iris), "A3": ("光隙 · 三层百叶", a3_louvre),
    "B1": ("光种 · 水滴", b1_pip), "B2": ("光种 · 子叶", b2_cotyledon), "B3": ("光种 · 种托", b3_calyx),
    "C1": ("裂环 · 缺口", c1_crack), "C2": ("裂环 · 三片", c2_shard), "C3": ("裂环 · 双弧", c3_split),
}


def draw(code, px, lit=1.0):
    return _finish(ALL[code][1](px, lit), px)


def on(bg, im):
    """composite onto a solid background"""
    base = Image.new("RGBA", im.size, bg + (255,))
    return Image.alpha_composite(base, im).convert("RGB")
