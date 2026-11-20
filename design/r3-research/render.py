"""Design-studies renderer. NOT the product UI.

Draws high-fidelity page studies with PIL so three directions can be compared on
the same business state. Nothing here ships: the production UI is untouched.
"""
import math, numpy as np
from PIL import Image, ImageDraw, ImageFont, ImageFilter

SERIF = "/System/Library/Fonts/Supplemental/Songti.ttc"
SANS  = "/System/Library/Fonts/Hiragino Sans GB.ttc"
THIN  = "/System/Library/Fonts/STHeiti Light.ttc"
GEO   = "/System/Library/Fonts/Avenir Next.ttc"
_f = {}
def F(path, size, idx=0):
    k=(path,size,idx)
    if k not in _f:
        try: _f[k]=ImageFont.truetype(path,size,index=idx)
        except Exception: _f[k]=ImageFont.truetype(SANS,size)
    return _f[k]

def txt(d, xy, s, font, fill, tracking=0.0, align="left"):
    x,y = xy
    if align!="left":
        w=sum(d.textlength(c,font=font)+tracking for c in s)-tracking
        x = x-w if align=="right" else x-w/2
    for c in s:
        d.text((x,y), c, font=font, fill=fill)
        x += d.textlength(c, font=font) + tracking
    return x

def tw(d, s, font, tracking=0.0):
    return sum(d.textlength(c,font=font)+tracking for c in s)-tracking if s else 0

def canvas(w,h,top,bottom):
    a=np.zeros((h,w,3),np.float32); tb=np.array(top,float)/255; bb=np.array(bottom,float)/255
    for y in range(h):
        t=y/max(1,h-1); a[y,:,:]=tb+(bb-tb)*t
    return Image.fromarray(a.astype(np.uint8),"RGB")

def hairline(img, box, rgba=(255,255,255,22)):
    ov=Image.new("RGBA", img.size, (0,0,0,0)); d=ImageDraw.Draw(ov)
    d.rectangle(box, fill=rgba)
    return Image.alpha_composite(img.convert("RGBA"), ov).convert("RGB")

def glow(img, cx, cy, r, color, alpha=1.0, steps=26):
    ov=Image.new("RGBA", img.size, (0,0,0,0)); d=ImageDraw.Draw(ov)
    for i in range(steps,0,-1):
        rr=r*i/steps; a=int(255*alpha*0.05*(1-i/steps)**0.8)+1
        d.ellipse([cx-rr,cy-rr,cx+rr,cy+rr], fill=color+(a,))
    return Image.alpha_composite(img.convert("RGBA"), ov).convert("RGB")
