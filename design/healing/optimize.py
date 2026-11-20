"""Convert the rendered PNG boards into WebP for the READMEs.

Gradients compress extremely well in WebP; a 390x844@2 board drops from
~900 KB to ~30 KB with no visible loss. Run after `node render.mjs`.
"""
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[2] / "assets" / "readme"

def main() -> int:
    total = 0
    for png in sorted(ROOT.rglob("*.png")):
        img = Image.open(png).convert("RGB")
        out = png.with_suffix(".webp")
        img.save(out, "WEBP", quality=91, method=6)
        total += out.stat().st_size
        png.unlink()
        print(f"{out.relative_to(ROOT)}  {out.stat().st_size // 1024} KB")
    print(f"total {total // 1024} KB")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
