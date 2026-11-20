"""A · Porcelain Workspace — sidebar + content. Dense, precise, low-chrome."""
from kit import Page, T, cjk, latin, num, qr, S

SIDEBAR = 176
CW = 480          # 720 - 176 sidebar - 32*2 padding

def shell(title, kicker=None):
    p = Page()
    # sidebar
    p.rect(0,0,SIDEBAR,560, fill=T["surface"])
    p.line(SIDEBAR-0.5,0,SIDEBAR-0.5,560, color=T["divider"])
    p.text(20,22,"第一束光", cjk(13,1), T["text"], 0.4)
    nav=[("我的光","mine"),("传光","give"),("光路","paths"),("回响","echo")]
    y=64
    for i,(label,key) in enumerate(nav):
        active = key==title
        if active:
            p.rect(12,y-6,SIDEBAR-24,30, fill=T["surface2"], r=7)
        p.text(22,y,label, cjk(13,1 if active else 0), T["text"] if active else T["text2"], 0.2)
        y+=36
    # footer status
    p.line(20,470,SIDEBAR-20,470, color=T["divider"])
    p.rect(20,492,6,6, fill=T["text3"], r=3)
    p.text(34,489,"服务未连接", cjk(11.5), T["text3"])
    p.text(20,516,"设置", cjk(11.5), T["text2"])
    return p

def content_head(p, title, sub):
    x=SIDEBAR+32
    p.text(x,34,title, cjk(17,1), T["text"], 0.2)
    if sub: p.text(x,60,sub, cjk(12), T["text2"])

# ---------------------------------------------------------------- 我的光
def mine(state="data"):
    p=shell("mine"); x=SIDEBAR+32
    content_head(p,"我的光","")
    if state=="empty":
        p.rect(x,104,44,44, fill=T["surface2"], r=22)
        p.rect(x+16,120,12,12, fill=T["text3"], r=6)
        p.text(x+60,106,"尚未核验任何模型家族", cjk(19,1), T["text"], 0.2)
        p.text(x+60,134,"开启扫描后，这里会显示你的身份", cjk(12), T["text2"])
    else:
        p.rect(x,100,44,44, fill=T["surface2"], r=22)
        p.rect(x+15,119,14,14, fill=T["accent"], r=7)
        p.text(x+60,102,"启迪者", cjk(21,1), T["text"], 0.6)
        p.text(x+60,132,"3 个可信模型家族", cjk(12), T["text2"])
    # primary action — a real button, sized to its label
    bx, by = x, 168
    p.rect(bx,by,112,34, fill=T["accent"], r=8)
    p.text(bx+56,by+9,"传一束光", cjk(12.5,1), "#FFFFFF", 0.4, "center")
    p.text(bx+128,by+9,"查看模型", cjk(12), T["text2"], 0.2)
    # model card
    cy=226
    p.shadow(x,cy,CW,132,r=10,blur=8,alpha=16,dy=1)
    p.rect(x,cy,CW,132, fill=T["surface"], r=10, stroke=T["divider"])
    p.text(x+16,cy+14,"模型使用", cjk(11.5), T["text2"], 0.6)
    rows=[("gpt","8 次会话 · Codex","已核验",T["accent2"]),
          ("claude","3 次会话","已发现",T["text2"]),
          ("deepseek","未确认","仅发现",T["text3"])]
    ry=cy+38
    for i,(n,s_,tag,col) in enumerate(rows):
        if i: p.line(x+16,ry-10,x+CW-16,ry-10, color=T["divider"])
        p.text(x+16,ry,n, latin(13,2), T["text"])
        p.text(x+96,ry+1,s_, cjk(11.5), T["text2"])
        w=p.textw(tag, cjk(11.5), 0.2)
        p.rect(x+CW-16-w-10,ry-3,w+10,18, fill=T["canvas"], r=9, stroke=T["divider"])
        p.text(x+CW-16-w-5,ry+1,tag, cjk(11.5), col, 0.2)
        ry+=30
    # scan row
    p.line(x,378,x+CW,378, color=T["divider"])
    p.text(x,394,"扫描授权", cjk(11.5), T["text2"])
    p.text(x+84,394,"已关闭 · 关闭时不会读取任何数据", cjk(11.5), T["text3"], 0.1)
    p.text(x+CW-56,394,"开启扫描", cjk(11.5,1), T["accent"], 0.2)
    return p

# ---------------------------------------------------------------- 传光
def give(state="data"):
    p=shell("give"); x=SIDEBAR+32
    content_head(p,"传光","对方接受之后，这束光才会离开你。")
    if state=="offline":
        p.rect(x,120,CW,96, fill=T["surface"], r=10, stroke=T["divider"])
        p.text(x+16,138,"传光服务未连接", cjk(13,1), T["text"])
        p.text(x+16,162,"可以继续查看自己的光。连接后才能发出一束光。", cjk(11.5), T["text2"])
        p.text(x+16,186,"重试连接", cjk(11.5,1), T["accent"], 0.2)
        return p
    # QR panel
    p.shadow(x,116,208,208,r=12,blur=9,alpha=18,dy=1)
    p.rect(x,116,208,208, fill=T["surface"], r=12, stroke=T["divider"])
    qr(p,x+20,136,168)
    # right column
    rx=x+236
    p.text(rx,124,"等待对方扫描", cjk(13,1), T["text"])
    p.text(rx,150,"有效期剩余", cjk(11), T["text3"], 0.4)
    p.text(rx,166,"6:42", num(28,2), T["text"])
    p.line(rx,214,rx+244,214, color=T["divider"])
    p.text(rx,228,"对方接受后，状态会变成「已抵达」。", cjk(11.5), T["text2"])
    p.text(rx,254,"真正的点亮需要对方产生可信的 AI 使用。", cjk(11.5), T["text2"])
    p.text(rx,292,"取消这次传光", cjk(11.5), T["text2"])
    return p

# ---------------------------------------------------------------- 光路
def paths(state="data"):
    p=shell("paths"); x=SIDEBAR+32
    content_head(p,"光路","时间留下的痕迹。")
    stats=[("1","已激活"),("0","待激活"),("0","我收到的")]
    sx=x
    for i,(n,l) in enumerate(stats):
        p.text(sx,96,n, num(26,2), T["text"])
        p.text(sx,132,l, cjk(11), T["text3"], 0.4)
        if i<2: p.line(sx+52,98,sx+52,138, color=T["divider"])
        sx+=76
    p.line(x,168,x+CW,168, color=T["divider"])
    if state=="empty":
        p.text(x,188,"还没有传出过光。", cjk(12.5), T["text2"])
        p.text(x,212,"可以从一束光开始。", cjk(12.5), T["text3"])
        return p
    rows=[("首次点亮","2026/11/01  14:32","已激活",True),
          ("等待真实使用","2026/10/28  09:10","待激活",False)]
    ry=186
    for i,(t,w_,tag,on) in enumerate(rows):
        p.rect(x,ry,6,6, fill=T["accent2"] if on else "#FFFFFF", r=3,
               stroke=None if on else T["text3"])
        p.text(x+16,ry-5,t, cjk(12.5,1), T["text"], 0.2)
        p.text(x+150,ry-4,w_, num(11,0), T["text3"])
        wt=p.textw(tag, cjk(11), 0.2)
        p.text(x+CW-wt,ry-4,tag, cjk(11), T["accent2"] if on else T["text3"], 0.2)
        ry+=44
        p.line(x,ry-12,x+CW,ry-12, color=T["divider"])
    return p

# ---------------------------------------------------------------- 回响
def echo(state="data"):
    p=shell("echo"); x=SIDEBAR+32
    content_head(p,"回响","")
    if state=="empty":
        p.text(x,200,"还没有回响。", cjk(17,1), T["text2"], 0.4)
        p.text(x,232,"对方真正开始使用 AI 之后，这里会亮一次。", cjk(12), T["text3"])
        return p
    p.rect(x+2,150,26,26, fill=T["accent"], r=13)
    p.text(x,206,"你传出的一束光，", cjk(18,1), T["text"], 0.6)
    p.text(x,232,"已经在远方亮起。", cjk(18,1), T["text"], 0.6)
    p.text(x,272,"2026 / 11 / 01  14:32", num(11,0), T["text3"])
    p.line(x,330,x+CW,330, color=T["divider"])
    p.text(x,344,"历史", cjk(11), T["text3"], 0.6)
    p.text(x,368,"11/01 14:32 · 你传出的一束光，已经被点亮。", cjk(11.5), T["text2"])
    return p
