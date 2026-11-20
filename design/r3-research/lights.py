import numpy as np
from PIL import Image, ImageDraw, ImageFilter

def base(size,bg):
    a=np.zeros((size,size,3),np.float32)+np.array(bg,np.float32)/255
    return a

def add(a, x, y, color, amt):
    h,w,_=a.shape
    m=(x>=0)&(x<w)&(y>=0)&(y<h)
    col=np.array(color,np.float32)/255
    xs,ys=x[m].astype(int),y[m].astype(int)
    v=(amt[m])[...,None]
    a[ys,xs]=a[ys,xs]+col*v
    return a

def grid(size):
    yy,xx=np.mgrid[0:size,0:size].astype(np.float32)
    c=(size-1)/2
    return xx-c, yy-c

def pure(size, bg, core=(232,248,248), halo=(126,214,204), r=9.0):
    a=base(size,bg); x,y=grid(size)
    d=np.sqrt(x*x+y*y)
    fall=np.exp(-(d/(r*3.4))**2)*1.25 + np.exp(-(d/(r*1.05))**2)*1.5
    a=add(a,x,y,halo,np.clip(fall,0,1.2))
    core_m=np.clip(1.0-(d/(r*0.62)),0,1)**0.5
    a=add(a,x,y,core,np.clip(core_m*2.1,0,2.4))
    return a

def optical(size, bg, col=(255,176,128), r=13.0):
    a=base(size,bg); x,y=grid(size); d=np.sqrt(x*x+y*y)
    # contact shadow first
    sh=np.exp(-(((x)/ (r*1.6))**2 + ((y-r*0.9)/(r*0.9))**2))*0.35
    a=np.clip(a-np.array((0,0,0),np.float32)*0,0,1); a=a*(1-sh[...,None]*0.55)
    # rim: light entering the bead
    rim=np.exp(-((d-r*0.92)/(r*0.13))**2)*0.9
    inner=np.clip(1-d/(r*0.95),0,1)**0.45
    a=add(a,x,y,(255,238,220),rim*0.55)
    a=add(a,x,y,col,inner*1.5)
    a=add(a,x,y,(255,252,246),np.exp(-((d/(r*0.34))**2))*0.75)
    # specular
    sx,sy=-r*0.34,-r*0.40
    sp=np.exp(-((((x-sx)**2+(y-sy)**2))/((r*0.20)**2)))
    a=add(a,x,y,(255,255,255),sp*1.05)
    return a

def organic(size, bg, col=(232,132,112), r=12.0, seed=3):
    rng=np.random.default_rng(seed); x,y=grid(size); d=np.sqrt(x*x+y*y)
    th=np.arctan2(y,x)
    wob=np.zeros_like(th)
    for k in (2,3,5):
        wob+=np.sin(k*th+rng.uniform(0,6.28))*0.055/k*3
    rr=r*(1+wob)
    a=base(size,bg)
    a=add(a,x,y,(214,146,130),np.exp(-(d/(r*3.2))**2)*1.1)
    a=add(a,x,y,col,np.clip(1-d/rr,0,1)**0.72*1.55)
    mott=np.sin(x*0.5+1.3)*np.cos(y*0.42-0.7)*0.12+1.0
    a=add(a,x,y,(255,214,196),np.clip(1-d/(rr*0.55),0,1)**1.4*mott*0.55)
    return a

def to_img(a):
    return Image.fromarray((np.clip(a,0,1)*255).astype(np.uint8),"RGB")

def render(kind,size,bg):
    fn={"pure":pure,"optical":optical,"organic":organic}[kind]
    return to_img(fn(size,bg))
