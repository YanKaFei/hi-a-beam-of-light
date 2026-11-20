import sys, numpy as np
from PIL import Image, ImageDraw, ImageFont
from render import F, txt, tw, SERIF, SANS, GEO
import pages as P

def sheet(key):
    name, fns, tag = P.DIRS[key]
    imgs=[f() for f in fns]
    W2,H2=imgs[0].size
    gap=28; pad=40; head=96
    W=pad*2+W2*2+gap; H=head+pad+H2*2+gap+40
    out=Image.new("RGB",(W,H),(0,0,0))
    d=ImageDraw.Draw(out)
    # the three directions sit on different grounds; give each its own surround
    bg={"A":(12,13,14),"B":(10,8,7),"C":(9,13,16)}[key]
    out=Image.new("RGB",(W,H),bg); d=ImageDraw.Draw(out)
    txt(d,(pad,34),f"{key}  {name}",F(SERIF,30),(238,236,232),2)
    txt(d,(pad,74),tag,F(SANS,12),(132,140,148),1.4)
    for i,im in enumerate(imgs):
        x=pad+(i%2)*(W2+gap); y=head+pad+(i//2)*(H2+gap)
        out.paste(im,(x,y))
        d.rectangle([x-1,y-1,x+W2,y+H2], outline=(255,255,255,26))
    lab=["我的光","传光","光路","回响"]
    for i,l in enumerate(lab):
        x=pad+(i%2)*(W2+gap); y=head+pad+(i//2)*(H2+gap)
        txt(d,(x+2,y+H2+8),l,F(SANS,11),(110,118,126),1.2)
    out.save(f"out/direction-{key}.png")
    return out.size

for k in "ABC":
    print(k, sheet(k))
