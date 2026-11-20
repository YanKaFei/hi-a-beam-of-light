"""Three directions x four pages, drawn at 720x900 then tiled 2x2."""
import math, numpy as np
from PIL import Image, ImageDraw, ImageFilter
from render import F, txt, tw, canvas, glow, SERIF, SANS, THIN, GEO

W,H = 720,900

# ---------------------------------------------------------------- helpers

def hairline(img, a, b, rgba=(255,255,255,22)):
    ov=Image.new("RGBA", img.size, (0,0,0,0)); d=ImageDraw.Draw(ov)
    d.rectangle([a[0],a[1],b[0],b[1]], fill=rgba)
    return Image.alpha_composite(img.convert("RGBA"), ov).convert("RGB")

def panel(w,h,rgb):
    return canvas(w,h,rgb[0],rgb[1])

def card(img, box, fill, radius=10, border=None, shadow=False):
    if shadow:
        sh=Image.new("RGBA",img.size,(0,0,0,0)); d=ImageDraw.Draw(sh)
        d.rounded_rectangle([box[0],box[1]+6,box[2]+2,box[3]+8],radius,fill=(0,0,0,110))
        sh=sh.filter(ImageFilter.GaussianBlur(12))
        img=Image.alpha_composite(img.convert("RGBA"),sh).convert("RGB")
    ov=Image.new("RGBA",img.size,(0,0,0,0)); d=ImageDraw.Draw(ov)
    d.rounded_rectangle(box,radius,fill=fill)
    if border: d.rounded_rectangle(box,radius,outline=border,width=1)
    return Image.alpha_composite(img.convert("RGBA"),ov).convert("RGB")

def qr(img, box, fg=(236,238,241), bg=(24,26,30), cell=6, seed=7):
    rng=np.random.default_rng(seed); d=ImageDraw.Draw(img)
    d.rectangle(box, fill=bg)
    n=(box[2]-box[0])//cell
    for gy in range(n):
        for gx in range(n):
            if rng.random()<0.45:
                x=box[0]+gx*cell; y=box[1]+gy*cell
                d.rectangle([x,y,x+cell-1,y+cell-1], fill=fg)
    return img

def light_dot(img, cx, cy, r, core, halo, kind="optical"):
    if kind=="pure":
        img=glow(img,cx,cy,r*5.5,halo,1.0)
        img=glow(img,cx,cy,r*1.9,halo,1.2)
        d=ImageDraw.Draw(img); d.ellipse([cx-r*.55,cy-r*.55,cx+r*.55,cy+r*.55],fill=core)
    elif kind=="optical":
        img=glow(img,cx,cy,r*4.2,halo,0.9)
        d=ImageDraw.Draw(img)
        d.ellipse([cx-r,cy-r,cx+r,cy+r], fill=core)
        d.ellipse([cx-r*.45,cy-r*.62,cx+r*.05,cy-r*.05], fill=(255,255,255))
    else:
        img=glow(img,cx,cy,r*6.0,halo,1.1)
        img=glow(img,cx,cy,r*2.4,core,1.0)
    return img

# ============================================================ A Silent Future
A_BG   = ((16,17,19),(10,11,12))
A_PANEL= (22,24,27)
A_LINE = (255,255,255,20)
A_TXT  = (236,238,241)
A_DIM  = (138,144,152)
A_ACC  = (126,214,192)      # a single cool signal, not vermilion
A_WARM = (240,214,190)

def a_header(d, sub):
    txt(d,(56,54),"FIRST LIGHT",F(GEO,11),A_DIM,2.6)
    txt(d,(56,78),sub,F(GEO,11),A_ACC,2.6)

def a_footer(d, nav, active=0):
    y=H-64
    x=56
    for i,n in enumerate(nav):
        col=A_TXT if i==active else A_DIM
        txt(d,(x,y),n,F(SANS,13),col,1.2)
        if i==active:
            d.rectangle([x,y+26,x+18,y+27], fill=A_ACC)
        x+=112

NAV=["我的光","传光","光路","回响"]

def page_A_mine():
    img=panel(W,H,A_BG); d=ImageDraw.Draw(img)
    a_header(d,"PERSONAL LIGHT")
    img=light_dot(img,590,150,7,(214,240,240),(126,214,192),"pure"); d=ImageDraw.Draw(img)
    txt(d,(56,180),"01",F(GEO,11),A_DIM,2.0)
    txt(d,(56,204),"启迪者",F(SERIF,52),A_TXT,3)
    txt(d,(56,282),"3 个可信模型家族 · 已验证",F(SANS,13),A_DIM)
    img=hairline(img,(56,336),(656,337),A_LINE); d=ImageDraw.Draw(img)
    rows=[("gpt","8 次会话 · codex","已核验",A_ACC),
          ("claude","3 次会话","已发现",A_DIM),
          ("deepseek","未确认","仅发现",(96,100,108))]
    y=372
    for name,sub,tag,col in rows:
        txt(d,(56,y),name,F(GEO,19),A_TXT)
        txt(d,(160,y+4),sub,F(SANS,11.5),A_DIM)
        txt(d,(656,y+3),tag,F(SANS,11.5),col,align="right")
        y+=54
    img=hairline(img,(56,540),(656,541),A_LINE); d=ImageDraw.Draw(img)
    txt(d,(56,566),"扫描",F(SANS,11.5),A_DIM)
    txt(d,(656,566),"自动扫描已关闭",F(SANS,11.5),(214,178,132),align="right")
    img=card(img,(56,606,320,664),(28,30,34),radius=8,border=(255,255,255,26))
    d=ImageDraw.Draw(img); txt(d,(80,626),"扫描一次",F(SANS,13),A_TXT)
    txt(d,(196,626),"开启自动扫描",F(SANS,13),A_DIM)
    a_footer(d,NAV,0)
    return img

def page_A_give():
    img=panel(W,H,A_BG); d=ImageDraw.Draw(img)
    a_header(d,"GIVE LIGHT")
    txt(d,(56,180),"02",F(GEO,11),A_DIM,2.0)
    txt(d,(56,204),"传一束光",F(SERIF,40),A_TXT,2)
    txt(d,(56,262),"对方接受之后，这束光才会离开你。",F(SANS,13),A_DIM)
    img=card(img,(56,320,360,624),(24,26,30),radius=10,border=(255,255,255,22))
    img=qr(img,(84,348,332,596),fg=(236,238,241),bg=(24,26,30),cell=6)
    txt(d,(392,336),"等待对方扫描",F(SANS,13),A_TXT)
    txt(d,(392,364),"有效期 7:42",F(GEO,11),A_DIM,1.4)
    img=hairline(img,(392,396),(664,397),A_LINE); d=ImageDraw.Draw(img)
    txt(d,(392,416),"这束光正在等待相遇。",F(SANS,12),A_DIM)
    txt(d,(392,470),"取消",F(SANS,12.5),A_DIM)
    d.line([(392,492),(420,492)],fill=(255,255,255,40))
    a_footer(d,NAV,1)
    return img

def page_A_paths():
    img=panel(W,H,A_BG); d=ImageDraw.Draw(img)
    a_header(d,"PATHS")
    txt(d,(56,180),"03",F(GEO,11),A_DIM,2.0)
    txt(d,(56,204),"光路",F(SERIF,44),A_TXT,2)
    stats=[("1","已点亮"),("0","等待中"),("0","我收到的")]
    x=56
    for n,l in stats:
        txt(d,(x,286),n,F(GEO,40),A_TXT)
        txt(d,(x,340),l,F(SANS,11.5),A_DIM,1.0)
        x+=176
    img=hairline(img,(56,398),(656,399),A_LINE); d=ImageDraw.Draw(img)
    y=430
    txt(d,(56,y),"2026 / 11",F(GEO,10.5),A_DIM,2.0)
    y+=34
    img=card(img,(56,y,656,y+86),(23,25,29),radius=8)
    d=ImageDraw.Draw(img)
    d.ellipse([78,y+34,90,y+46],fill=A_ACC)
    txt(d,(106,y+28),"首次点亮",F(SANS,14),A_TXT)
    txt(d,(106,y+52),"11/01 14:32 · 等待真实使用",F(SANS,11),A_DIM)
    txt(d,(636,y+34),"已激活",F(SANS,11),A_ACC,align="right")
    a_footer(d,NAV,2)
    return img

def page_A_echo():
    img=panel(W,H,A_BG); d=ImageDraw.Draw(img)
    a_header(d,"ECHO")
    img=light_dot(img,W//2,352,9,(220,244,244),(126,214,192),"pure"); d=ImageDraw.Draw(img)
    txt(d,(W//2,470),"你传出的一束光，",F(SERIF,26),(226,228,232),2,align="center")
    txt(d,(W//2,510),"已经在远方亮起。",F(SERIF,26),(226,228,232),2,align="center")
    txt(d,(W//2,568),"11/01 14:32",F(GEO,11),A_DIM,1.8,align="center")
    a_footer(d,NAV,3)
    return img

# ========================================================== B Luminous Matter
B_BG   = ((20,16,14),(9,7,6))
B_CARD = (30,25,22)
B_TXT  = (243,236,226)
B_DIM  = (156,144,132)
B_ACC  = (255,138,76)

def b_shell(title, sub):
    img=panel(W,H,B_BG); d=ImageDraw.Draw(img)
    txt(d,(W//2,52),title,F(SERIF,15),(198,186,172),6,align="center")
    txt(d,(W//2,80),sub,F(SANS,10.5),B_DIM,2.4,align="center")
    img=hairline(img,(300,112),(420,113),(255,255,255,34)); d=ImageDraw.Draw(img)
    return img,d

def b_footer(img, idx):
    ov=Image.new("RGBA",img.size,(0,0,0,0)); d=ImageDraw.Draw(ov)
    d.rectangle([0,H-96,W,H], fill=(14,11,9,235))
    x=W//2-150
    for i,n in enumerate(NAV):
        col=B_TXT if i==idx else (120,110,100)
        txt(d,(x,H-58),n,F(SANS,11.5),col,3.2,align="center")
        if i==idx: d.ellipse([x-2,H-74,x+2,H-70], fill=B_ACC)
        x+=100
    return Image.alpha_composite(img.convert("RGBA"),ov).convert("RGB")

def page_B_mine():
    img,d=b_shell("第一束光","THE FIRST LIGHT")
    img=glow(img,W//2,300,120,(255,150,96),2.4)
    img=glow(img,W//2,300,46,(255,176,120),2.2)
    d=ImageDraw.Draw(img)
    d.ellipse([W//2-30,300-34,W//2+30,300+34],fill=(255,196,150))
    d.ellipse([W//2-16,300-26,W//2+4,300-6],fill=(255,244,232))
    txt(d,(W//2,432),"启迪者",F(SERIF,46),B_TXT,8,align="center")
    txt(d,(W//2,506),"3 个可信模型家族",F(SANS,12.5),B_DIM,1.5,align="center")
    img=img
    y=576
    for name,sub,tag in [("gpt","8 次会话","已核验"),("claude","3 次会话","已发现"),("deepseek","未确认","仅发现")]:
        ov=Image.new("RGBA",img.size,(0,0,0,0)); dd=ImageDraw.Draw(ov)
        dd.rounded_rectangle([120,y,600,y+62],12,fill=(28,23,20,255))
        img=Image.alpha_composite(img.convert("RGBA"),ov).convert("RGB"); d=ImageDraw.Draw(img)
        txt(d,(148,y+18),name,F(GEO,16),B_TXT)
        txt(d,(148,y+40),sub,F(SANS,10.5),(130,118,108))
        col=B_ACC if tag=="已核验" else (120,110,100)
        txt(d,(572,y+30),tag,F(SANS,11),col,align="right")
        y+=74
    return b_footer(img,0)

def page_B_give():
    img,d=b_shell("传光","GIVE LIGHT")
    img=glow(img,210,290,80,(255,150,96),1.8); img=glow(img,510,290,80,(120,120,255),1.2)
    d=ImageDraw.Draw(img)
    d.ellipse([190,270,230,310],fill=(255,196,150)); d.ellipse([490,270,530,310],fill=(150,150,230))
    d.line([(250,290),(470,290)],fill=(255,160,110,120),width=1)
    txt(d,(W//2,352),"把一束光传给别人",F(SERIF,30),B_TXT,4,align="center")
    txt(d,(W//2,398),"对方主动接收之后，这束光才会离开你",F(SANS,11),B_DIM,1,align="center")
    ov=Image.new("RGBA",img.size,(0,0,0,0)); dd=ImageDraw.Draw(ov)
    dd.rounded_rectangle([230,438,490,700],18,fill=(32,26,22,255))
    img=Image.alpha_composite(img.convert("RGBA"),ov).convert("RGB")
    img=qr(img,(262,470,458,666),fg=(243,236,226),bg=(32,26,22),cell=6)
    d=ImageDraw.Draw(img)
    txt(d,(W//2,730),"等待对方扫描 · 7:42",F(SANS,11.5),B_DIM,1.2,align="center")
    txt(d,(W//2,772),"取消这次传光",F(SANS,11.5),(150,138,126),1.2,align="center")
    return b_footer(img,1)

def page_B_paths():
    img,d=b_shell("光路","PATHS")
    txt(d,(W//2,166),"1",F(SERIF,64),B_TXT,2,align="center")
    txt(d,(W//2,244),"段已点亮的关系",F(SANS,11.5),B_DIM,1.6,align="center")
    img=hairline(img,(190,286),(530,287),(255,255,255,26)); d=ImageDraw.Draw(img)
    y=326
    for t,s,on in [("首次点亮","11/01 14:32",True),("等待真实使用","10/28 09:10",False)]:
        ov=Image.new("RGBA",img.size,(0,0,0,0)); dd=ImageDraw.Draw(ov)
        dd.rounded_rectangle([140,y,580,y+78],14,fill=(27,22,19,255))
        img=Image.alpha_composite(img.convert("RGBA"),ov).convert("RGB"); d=ImageDraw.Draw(img)
        if on: d.ellipse([168,y+34,182,y+48],fill=B_ACC)
        else:  d.ellipse([168,y+34,182,y+48],outline=(150,138,126),width=1)
        txt(d,(200,y+26),t,F(SANS,13),B_TXT)
        txt(d,(200,y+48),s,F(SANS,10.5),(128,116,106))
        y+=92
    return b_footer(img,2)

def page_B_echo():
    img,d=b_shell("回响","ECHO")
    img=glow(img,W//2,392,150,(255,150,96),2.6); d=ImageDraw.Draw(img)
    d.ellipse([W//2-34,392-38,W//2+34,392+38],fill=(255,204,164))
    d.ellipse([W//2-18,392-30,W//2+2,392-8],fill=(255,248,238))
    txt(d,(W//2,516),"你传出的一束光，",F(SERIF,24),B_TXT,3,align="center")
    txt(d,(W//2,552),"已经在远方亮起。",F(SERIF,24),B_TXT,3,align="center")
    txt(d,(W//2,616),"11 / 01",F(GEO,10.5),B_DIM,3,align="center")
    return b_footer(img,3)

# ====================================================== C Atmospheric Interface
C_BG   = ((15,22,26),(8,12,15))
C_TXT  = (226,234,236)
C_DIM  = (124,142,148)
C_ACC  = (150,226,214)
C_GLASS= (255,255,255,16)

def c_atmosphere(img, cy=None):
    cy = cy or H*0.34
    img=glow(img,180,cy,320,(60,120,130),1.6)
    img=glow(img,600,cy+140,260,(40,80,120),1.2)
    return img

def c_shell(title, sub, idx):
    img=panel(W,H,C_BG); img=c_atmosphere(img); d=ImageDraw.Draw(img)
    txt(d,(48,44),title,F(SANS,12),C_TXT,4.4)
    txt(d,(48,66),sub,F(SANS,10),C_DIM,2.2)
    return img,d

def c_rail(d, items, y=120):
    for k,v in items:
        txt(d,(W-56,y),v,F(GEO,11),C_DIM,1.2,align="right")
        txt(d,(W-56,y+18),k,F(SANS,10), (86,102,108), 1.4, align="right")
        y+=54
    return y

def c_footer(img, idx):
    ov=Image.new("RGBA",img.size,(0,0,0,0)); d=ImageDraw.Draw(ov)
    d.rounded_rectangle([40,H-92,W-40,H-30],30,fill=(255,255,255,14))
    return Image.alpha_composite(img.convert("RGBA"),ov).convert("RGB")

def page_C_mine():
    img,d=c_shell("我的光","PERSONAL LIGHT",0)
    txt(d,(48,150),"启迪者",F(SERIF,44),C_TXT,5)
    txt(d,(48,214),"3 个可信模型家族 · 微明",F(SANS,11.5),C_DIM,1.4)
    c_rail(d,[("已点亮","1"),("等待中","0"),("我收到的","0")],150)
    img=glow(img,300,430,150,(140,220,210),1.5)
    img=glow(img,300,430,54,(190,240,232),1.8); d=ImageDraw.Draw(img)
    d.ellipse([292,422,308,438],fill=(236,252,248))
    ov=Image.new("RGBA",img.size,(0,0,0,0)); dd=ImageDraw.Draw(ov)
    dd.rounded_rectangle([48,560,W-48,668],20,fill=(255,255,255,16))
    img=Image.alpha_composite(img.convert("RGBA"),ov).convert("RGB"); d=ImageDraw.Draw(img)
    txt(d,(76,586),"光在这台机器上，不离开它",F(SANS,12),C_TXT,1.2)
    txt(d,(76,614),"扫描已关闭 · 关闭时不会读取任何数据",F(SANS,10.5),C_DIM,0.6)
    img=c_footer(img,0); d=ImageDraw.Draw(img)
    x=76
    for i,n in enumerate(NAV):
        txt(d,(x,H-72),n,F(SANS,11.5),C_TXT if i==0 else C_DIM,2.6)
        x+=110
    return img

def page_C_give():
    img,d=c_shell("传光","GIVE LIGHT",1)
    txt(d,(48,150),"传一束光",F(SERIF,36),C_TXT,4)
    txt(d,(48,206),"对方接收之后，光才离开你。",F(SANS,11),C_DIM,1)
    ov=Image.new("RGBA",img.size,(0,0,0,0)); dd=ImageDraw.Draw(ov)
    dd.rounded_rectangle([48,262,404,616],22,fill=(255,255,255,18))
    img=Image.alpha_composite(img.convert("RGBA"),ov).convert("RGB")
    img=qr(img,(76,290,376,590),fg=(226,234,236),bg=(20,27,31),cell=6)
    d=ImageDraw.Draw(img)
    txt(d,(440,290),"等待中",F(SANS,11),C_ACC,2.6)
    txt(d,(440,318),"7:42",F(GEO,26),C_TXT)
    txt(d,(440,358),"对方扫描后，",F(SANS,10.5),C_DIM,0.8)
    txt(d,(440,376),"这里会变成「已抵达」。",F(SANS,10.5),C_DIM,0.8)
    txt(d,(440,440),"取消",F(SANS,11),C_DIM,1.6)
    img=c_footer(img,1); d=ImageDraw.Draw(img)
    x=76
    for i,n in enumerate(NAV):
        txt(d,(x,H-72),n,F(SANS,11.5),C_TXT if i==1 else C_DIM,2.6)
        x+=110
    return img

def page_C_paths():
    img,d=c_shell("光路","PATHS",2)
    txt(d,(48,150),"光路",F(SERIF,36),C_TXT,4)
    c_rail(d,[("已点亮","1"),("等待中","1"),("我收到的","0")],150)
    ov=Image.new("RGBA",img.size,(0,0,0,0)); dd=ImageDraw.Draw(ov)
    dd.rounded_rectangle([48,250,W-48,700],22,fill=(255,255,255,14))
    img=Image.alpha_composite(img.convert("RGBA"),ov).convert("RGB"); d=ImageDraw.Draw(img)
    y=286
    for t,s,col in [("首次点亮","11/01 14:32",C_ACC),("等待真实使用","10/28 09:10",(150,170,176))]:
        d.ellipse([78,y+8,90,y+20],fill=col)
        txt(d,(110,y),t,F(SANS,12.5),C_TXT,1)
        txt(d,(110,y+22),s,F(SANS,10),(110,126,132))
        y+=96
    img=c_footer(img,2); d=ImageDraw.Draw(img)
    x=76
    for i,n in enumerate(NAV):
        txt(d,(x,H-72),n,F(SANS,11.5),C_TXT if i==2 else C_DIM,2.6)
        x+=110
    return img

def page_C_echo():
    img,d=c_shell("回响","ECHO",3)
    img=glow(img,W//2,420,220,(120,210,205),2.0)
    img=glow(img,W//2,420,60,(200,244,238),1.6)
    d=ImageDraw.Draw(img)
    d.ellipse([W//2-8,412,W//2+8,428],fill=(240,252,250))
    txt(d,(W//2,556),"你传出的一束光，",F(SANS,17),C_TXT,2.4,align="center")
    txt(d,(W//2,588),"已经在远方亮起。",F(SANS,17),C_TXT,2.4,align="center")
    txt(d,(W//2,640),"11 / 01 · 14:32",F(SANS,10),C_DIM,2.0,align="center")
    img=c_footer(img,3); d=ImageDraw.Draw(img)
    x=76
    for i,n in enumerate(NAV):
        txt(d,(x,H-72),n,F(SANS,11.5),C_TXT if i==3 else C_DIM,2.6)
        x+=110
    return img

DIRS = {
 "A": ("Silent Future", [page_A_mine,page_A_give,page_A_paths,page_A_echo], "精确 · 网格 · 单一信号色"),
 "B": ("Luminous Matter", [page_B_mine,page_B_give,page_B_paths,page_B_echo], "物质 · 中心 · 暖光"),
 "C": ("Atmospheric Interface", [page_C_mine,page_C_give,page_C_paths,page_C_echo], "气候 · 层 · 玻璃"),
}
