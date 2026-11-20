"""F3.8 design-study renderer (Porcelain Future).

Not product code. Draws high-fidelity studies at 2x with supersampled shapes and
real system typefaces so the result can be judged as a UI, not as a wireframe.
"""
from PIL import Image, ImageDraw, ImageFont, ImageFilter

CJK   = "/System/Library/Fonts/Hiragino Sans GB.ttc"   # 0=W3 1=W6
LATIN = "/System/Library/Fonts/HelveticaNeue.ttc"      # 0=Light 1=Regular 2=Medium 3=Bold
NUM   = "/System/Library/Fonts/Avenir Next.ttc"
SERIF = "/System/Library/Fonts/Supplemental/Songti.ttc"

T = {  # light theme, the brief's starting values
 "canvas":"#F6F7F4","surface":"#FFFFFF","surface2":"#ECEEEA","surface3":"#E3E7E3",
 "text":"#202B30","text2":"#717B80","text3":"#9AA3A7",
 "accent":"#A96B5B","accent2":"#6F8985","divider":"#E5E9E6",
 "dark":"#202B30","light":"#FFFFFF",
}
S = 2  # supersample / retina factor

_fc = {}
def font(path, size, idx=0):
    k=(path,size,idx)
    if k not in _fc:
        try: _fc[k]=ImageFont.truetype(path, int(size*S), index=idx)
        except Exception:
            _fc[k]=ImageFont.truetype(path, int(size*S))
    return _fc[k]

def cjk(size, weight=0):  return font(CJK, size, 1 if weight else 0)
def latin(size, weight=1): return font(LATIN, size, weight)
def num(size, weight=1):   return font(NUM, size, weight)
def serif(size, weight=0): return font(SERIF, size, weight)

class Page:
    def __init__(self, w=720, h=560, bg=None):
        self.w, self.h = w, h
        self.img = Image.new("RGB", (w*S, h*S), bg or T["canvas"])
        self.d = ImageDraw.Draw(self.img)

    def _b(self, box):
        return [int(v*S) for v in box]

    def rect(self, x, y, w, h, fill=None, r=0, stroke=None, sw=1):
        b=self._b((x,y,x+w,y+h))
        if r:
            self.d.rounded_rectangle(b, int(r*S), fill=fill,
                                     outline=stroke, width=max(1,int(sw*S)) if stroke else 0)
        else:
            self.d.rectangle(b, fill=fill,
                             outline=stroke, width=max(1,int(sw*S)) if stroke else 0)

    def shadow(self, x, y, w, h, r=10, blur=10, alpha=26, dy=2):
        sh=Image.new("L",(self.w*S,self.h*S),0)
        ImageDraw.Draw(sh).rounded_rectangle(
            self._b((x,y+dy,x+w,y+h+dy)), int(r*S), fill=alpha)
        sh=sh.filter(ImageFilter.GaussianBlur(int(blur*S)))
        dark=Image.new("RGB",self.img.size,(32,43,48))
        self.img.paste(dark,(0,0),sh)
        self.d=ImageDraw.Draw(self.img)

    def line(self, x1,y1,x2,y2, color=None, width=1):
        self.d.line(self._b((x1,y1,x2,y2)), fill=color or T["divider"], width=int(width*S))

    def text(self, x, y, s, f, fill, tracking=0.0, anchor="left"):
        """anchor: left|center|right. y is the text top."""
        d=self.d
        width=0
        for ch in s:
            width += d.textlength(ch,font=f) + tracking*S
        width -= tracking*S if s else 0
        px=x*S
        if anchor=="center": px -= width/2
        elif anchor=="right": px -= width
        for ch in s:
            d.text((px, y*S), ch, font=f, fill=fill)
            px += d.textlength(ch,font=f) + tracking*S
        return width/ S

    def textw(self, s, f, tracking=0.0):
        d=self.d
        return (sum(d.textlength(c,font=f)+tracking*S for c in s) - (tracking*S if s else 0))/S

    def save(self, path):
        out=self.img.resize((self.w, self.h), Image.LANCZOS)
        out.save(path,"PNG")
        return out

def qr(p, x, y, size, fg="#202B30", bg="#FFFFFF", modules=25):
    """A credible QR: three finder patterns plus a deterministic data field."""
    import random
    rnd=random.Random(7)
    p.rect(x,y,size,size, fill=bg)
    m=size/modules
    def cell(cx,cy,w=1,h=1):
        p.rect(x+cx*m, y+cy*m, w*m-0.6, h*m-0.6, fill=fg)
    # quiet zone already provided by bg
    def finder(ox,oy):
        for i in range(7):
            for j in range(7):
                edge = i in (0,6) or j in (0,6)
                inner = 2<=i<=4 and 2<=j<=4
                if edge or inner: cell(ox+i,oy+j)
    finder(1,1); finder(modules-8,1); finder(1,modules-8)
    for cy in range(modules):
        for cx in range(modules):
            if cx<9 and cy<9: continue
            if cx>modules-9 and cy<9: continue
            if cx<9 and cy>modules-9: continue
            if rnd.random()<0.46: cell(cx,cy)
