"""星星之火可以疗愈 —— 朱砂底、白字、行楷。

一张给 README 用的书法条幅。不是字体截图：每个字单独落笔，
带一点飞白、纸纹和一枚白印。

    python3 calligraphy.py
"""
from __future__ import annotations

import math
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

HERE = Path(__file__).resolve().parent
OUT = HERE.parents[1] / "assets" / "readme" / "shared" / "sparks-heal.webp"

FONT_CANDIDATES = [
    "/System/Library/AssetsV2/com_apple_MobileAsset_Font8/13b8ce423f920875b28b551f9406bf1014e0a656.asset/AssetData/Xingkai.ttc",
    "/System/Library/Fonts/Supplemental/Kaiti.ttc",
    "/Library/Fonts/Xingkai.ttc",
]

TEXT = "星星之火可以疗愈"
W, H = 1600, 600
CREAM = (247, 241, 229)
RED_TOP = (172, 52, 41)
RED_BOTTOM = (140, 40, 33)
SS = 3  # supersampling factor for the brush mask


def find_font() -> str:
    for path in FONT_CANDIDATES:
        if Path(path).exists():
            return path
    raise SystemExit("没有找到行楷/楷体字体，请把 FONT_CANDIDATES 指向本机字体。")


def paper_field(w: int, h: int, seed: int = 7) -> np.ndarray:
    """Return a float field in [0,1] used for mottling and grain."""
    rng = np.random.default_rng(seed)
    small = rng.random((h // 8, w // 8)).astype(np.float32)
    mottle = np.array(
        Image.fromarray((small * 255).astype(np.uint8)).resize((w, h), Image.BICUBIC),
        dtype=np.float32,
    ) / 255.0
    mottle = np.array(
        Image.fromarray((mottle * 255).astype(np.uint8)).filter(
            ImageFilter.GaussianBlur(18)
        ),
        dtype=np.float32,
    ) / 255.0
    grain = rng.normal(0, 1, (h, w)).astype(np.float32)
    grain = np.array(
        Image.fromarray(((grain * 0.5 + 0.5) * 255).clip(0, 255).astype(np.uint8)).filter(
            ImageFilter.GaussianBlur(0.6)
        ),
        dtype=np.float32,
    ) / 255.0
    return mottle * 0.72 + grain * 0.28


def brush_mask(text: str, font_path: str, w: int, h: int) -> Image.Image:
    """Draw each glyph on its own, then roughen the edges like wet ink on paper."""
    rng = np.random.default_rng(11)

    # one cell per character: an even rhythm matters more than a big size
    margin = 78
    cell = (w - margin * 2) / len(text)
    size = int(cell * 1.02)
    ss = 3
    font_ss = ImageFont.truetype(font_path, size * ss)
    canvas = Image.new("L", (w * ss, h * ss), 0)

    for i, ch in enumerate(text):
        box = font_ss.getbbox(ch)
        gw, gh = box[2] - box[0], box[3] - box[1]
        pad = 30 * ss
        glyph = Image.new("L", (gw + pad * 2, gh + pad * 2), 0)
        ImageDraw.Draw(glyph).text((pad - box[0], pad - box[1]), ch, font=font_ss, fill=255)
        glyph = glyph.rotate(float(rng.uniform(-1.5, 1.5)), resample=Image.BICUBIC, expand=False)
        cx = (margin + cell * (i + 0.5)) * ss
        cy = h * ss * 0.46 + rng.uniform(-7, 7) * ss
        canvas.paste(glyph, (int(cx - gw / 2 - pad), int(cy - gh / 2 - pad)), glyph)

    # re-centre on the actual ink so the line sits optically in the middle
    bbox = canvas.getbbox()
    ink = canvas.crop(bbox)
    mask = ink.resize((int(ink.width / ss), int(ink.height / ss)), Image.LANCZOS)
    mask = mask.filter(ImageFilter.GaussianBlur(0.7))

    arr = np.array(mask, dtype=np.float32) / 255.0
    field = paper_field(mask.width, mask.height, seed=3)
    # dry-brush: thin the alpha where the paper texture is low, but never fully
    dry = 0.8 + 0.3 * np.clip((field - 0.42) * 2.4, 0, 1)
    arr = np.clip(arr * dry, 0, 1)
    # edge bleed: grow a hair, then lose contrast
    bleed = np.array(
        Image.fromarray((arr * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(1.5)),
        dtype=np.float32,
    ) / 255.0
    arr = np.clip(arr * 0.88 + bleed * 0.16, 0, 1)

    page = Image.new("L", (w, h), 0)
    page.paste(
        Image.fromarray((arr * 255).astype(np.uint8)),
        (int((w - mask.width) / 2), int(h * 0.46 - mask.height / 2)),
    )
    return page


def seal(size: int = 98) -> Image.Image:
    """A white seal, slightly off-square, carved with 光."""
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(canvas)
    pad = 4
    d.rectangle([pad, pad, size - pad, size - pad], outline=(247, 241, 229, 236), width=3)
    font_path = find_font()
    font = ImageFont.truetype(font_path, int(size * 0.6))
    box = font.getbbox("光")
    tw, th = box[2] - box[0], box[3] - box[1]
    d.text(
        ((size - tw) / 2 - box[0], (size - th) / 2 - box[1]),
        "光",
        font=font,
        fill=(247, 241, 229, 232),
    )
    return canvas.rotate(-4, resample=Image.BICUBIC, expand=False)


def main() -> int:
    font_path = find_font()

    # red field
    top = np.array(RED_TOP, dtype=np.float32)
    bottom = np.array(RED_BOTTOM, dtype=np.float32)
    ramp = np.linspace(0, 1, H, dtype=np.float32)[:, None, None]
    base = top * (1 - ramp) + bottom * ramp
    base = np.repeat(base, W, axis=1)

    field = paper_field(W, H, seed=5)
    mottle = (field - 0.5) * 26
    base = base + mottle[:, :, None]

    yy, xx = np.mgrid[0:H, 0:W].astype(np.float32)
    cx, cy = W * 0.5, H * 0.42
    r = np.sqrt(((xx - cx) / (W * 0.62)) ** 2 + ((yy - cy) / (H * 0.78)) ** 2)
    vignette = np.clip(1.16 - 0.42 * r**1.7, 0.62, 1.0)[:, :, None]
    base = np.clip(base * vignette, 0, 255)

    canvas = Image.fromarray(base.astype(np.uint8), "RGB").convert("RGBA")

    # the calligraphy
    mask = brush_mask(TEXT, font_path, W, H)
    ink = Image.new("RGBA", (W, H), CREAM + (0,))
    ink.putalpha(mask)
    canvas.alpha_composite(ink)

    # embers: a few quiet sparks, never on the glyphs
    rng = np.random.default_rng(23)
    sparks = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    sd = ImageDraw.Draw(sparks)
    for _ in range(26):
        x = rng.uniform(60, W - 60)
        y = rng.uniform(60, H - 60)
        if mask.getpixel((int(x), int(y))) > 12:
            continue
        rad = rng.uniform(0.8, 2.4)
        alpha = int(rng.uniform(20, 90))
        sd.ellipse([x - rad, y - rad, x + rad, y + rad], fill=CREAM + (alpha,))
    canvas.alpha_composite(sparks)

    # the seal, bottom right
    s = seal()
    canvas.alpha_composite(s, (W - 172, H - 160))

    # a small brand line, bottom left
    small = ImageFont.truetype(font_path, 26)
    sd2 = ImageDraw.Draw(canvas)
    x = 80
    for ch in "hi，一束光":
        sd2.text((x, H - 76), ch, font=small, fill=CREAM + (128,))
        x += sd2.textlength(ch, font=small) + 3

    canvas.convert("RGB").save(OUT, "WEBP", quality=93, method=6)
    print(f"{OUT}  {OUT.stat().st_size // 1024} KB  {W}x{H}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
