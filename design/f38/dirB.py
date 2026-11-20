"""B · Editorial Light — no chrome. Open column, rules instead of cards,
   display type doing the work. Structurally different from A, not a recolour."""
from kit import Page, T, cjk, latin, num, qr

M = 72          # generous outer margin: the page breathes

def shell(active):
    p = Page()
    # a single hairline runs the full width under the masthead
    p.line(M,74,720-M,74, color=T["divider"])
    p.text(M,30,"第一束光", cjk(12), T["text2"], 1.6)
    nav=[("我的光","mine"),("传光","give"),("光路","paths"),("回响","echo")]
    x=720-M
    for label,key in reversed(nav):
        w=p.textw(label, cjk(12), 0.8)
        x-=w
        col = T["text"] if key==active else T["text3"]
        p.text(x,30,label, cjk(12, 1 if key==active else 0), col, 0.8)
        if key==active: p.line(x,52,x+w,52, color=T["text"])
        x-=26
    return p

def kicker(p, s):
    p.text(M,104,s, cjk(10.5), T["text3"], 2.2)

# ---------------------------------------------------------------- 我的光
def mine(state="data"):
    p=shell("mine")
    kicker(p,"我的光")
    if state=="empty":
        p.text(M,138,"尚未核验", cjk(34,1), T["text"], 1.2)
        p.text(M,182,"任何模型家族", cjk(34,1), T["text3"], 1.2)
        p.line(M,250,720-M,250, color=T["divider"])
        p.text(M,268,"开启扫描后，这里会显示你已经确认使用过的模型。", cjk(11.5), T["text2"])
    else:
        p.text(M,134,"启迪者", cjk(38,1), T["text"], 1.6)
        # the count is the second voice, not a badge
        p.text(M,206,"3", num(58,2), T["text"])
        p.text(M+52,232,"个可信模型家族", cjk(11.5), T["text2"], 0.4)
        p.line(M,296,720-M,296, color=T["divider"])
        rows=[("gpt","8 次会话 · Codex","已核验",T["accent2"]),
              ("claude","3 次会话","已发现",T["text2"]),
              ("deepseek","未确认","仅发现",T["text3"])]
        ry=314
        for i,(n,s_,tag,col) in enumerate(rows):
            p.text(M,ry,n, latin(12,2), T["text"])
            p.text(M+72,ry+1,s_, cjk(11), T["text2"])
            w=p.textw(tag, cjk(11), 0.6)
            p.text(720-M-w,ry+1,tag, cjk(11), col, 0.6)
            ry+=26
            if i<2: p.line(M,ry-8,720-M,ry-8, color=T["divider"])
    # action is a line of text with an arrow, not a filled button
    ay = 430 if state!="empty" else 300
    p.line(M,ay,720-M,ay, color=T["divider"])
    p.text(M,ay+14,"传一束光", cjk(13,1), T["accent"], 0.6)
    # the arrow is drawn, not typed: Helvetica has no arrow glyph
    ax=M+68; p.line(ax,ay+22,ax+14,ay+22, color=T["accent"], width=1.4)
    p.line(ax+9,ay+18,ax+14,ay+22, color=T["accent"], width=1.4)
    p.line(ax+9,ay+26,ax+14,ay+22, color=T["accent"], width=1.4)
    p.text(720-M-p.textw("查看模型",cjk(11.5),0.6), ay+15,"查看模型", cjk(11.5), T["text3"], 0.6)
    return p

# ---------------------------------------------------------------- 传光
def give(state="data"):
    p=shell("give")
    kicker(p,"传光")
    p.text(M,132,"把一束光", cjk(26,1), T["text"], 1.2)
    p.text(M,166,"交给另一个人", cjk(26,1), T["text"], 1.2)
    if state=="offline":
        p.line(M,232,720-M,232, color=T["divider"])
        p.text(M,250,"传光服务未连接", cjk(13,1), T["text"])
        p.text(M,274,"你自己的光不受影响。连接后才能发出一束光。", cjk(11.5), T["text2"])
        p.text(M,304,"重试连接", cjk(11.5,1), T["accent"], 0.6)
        return p
    # QR sits on a quiet plate, centred in the right half
    qx, qy, qs = 452, 118, 168
    p.rect(qx-16,qy-16,qs+32,qs+32, fill=T["surface"], r=6, stroke=T["divider"])
    qr(p,qx,qy,qs)
    p.text(M,232,"等待对方扫描", cjk(12.5,1), T["text"], 0.4)
    p.text(M,256,"6:42", num(22,2), T["text"])
    p.text(M,290,"对方接受后，状态会变成「已抵达」。", cjk(11), T["text2"])
    p.line(M,340,720-M,340, color=T["divider"])
    p.text(M,356,"取消这次传光", cjk(11.5), T["text3"], 0.6)
    return p

# ---------------------------------------------------------------- 光路
def paths(state="data"):
    p=shell("paths")
    kicker(p,"光路")
    if state=="empty":
        p.text(M,140,"还没有开始。", cjk(30,1), T["text2"], 1.0)
        p.text(M,190,"你传出的每一束光，会在真实使用发生后留在这里。", cjk(11.5), T["text3"])
        p.line(M,250,720-M,250, color=T["divider"])
        p.text(M,266,"传一束光  →", cjk(12,1), T["accent"], 0.6)
        return p
    p.text(M,132,"1", num(64,2), T["text"])
    p.text(M+44,176,"段关系已经点亮", cjk(11.5), T["text2"], 0.4)
    p.text(M+220,150,"0 待激活", cjk(11), T["text3"], 0.4)
    p.text(M+300,150,"0 我收到的", cjk(11), T["text3"], 0.4)
    p.line(M,232,720-M,232, color=T["divider"])
    rows=[("首次点亮","2026 / 11 / 01  14:32","已激活"),
          ("等待真实使用","2026 / 10 / 28  09:10","待激活")]
    ry=252
    for t,w_,tag in rows:
        p.text(M,ry,t, cjk(13,1), T["text"], 0.4)
        p.text(M+140,ry+2,w_, num(11,0), T["text3"])
        wt=p.textw(tag, cjk(11), 0.6)
        p.text(720-M-wt,ry+2,tag, cjk(11), T["accent2"] if tag=="已激活" else T["text3"], 0.6)
        ry+=44
        p.line(M,ry-14,720-M,ry-14, color=T["divider"])
    return p

# ---------------------------------------------------------------- 回响
def echo(state="data"):
    p=shell("echo")
    kicker(p,"回响")
    if state=="empty":
        p.text(M,190,"还没有回响。", cjk(24,1), T["text3"], 1.0)
        p.text(M,232,"对方真正开始使用 AI 之后，这里会亮一次。", cjk(11.5), T["text3"])
        return p
    p.text(M,168,"你传出的一束光，", cjk(24,1), T["text"], 1.2)
    p.text(M,206,"已经在远方亮起。", cjk(24,1), T["text"], 1.2)
    p.text(M,262,"2026 / 11 / 01  14:32", num(11,0), T["text3"], 1.0)
    p.line(M,330,720-M,330, color=T["divider"])
    p.text(M,346,"历史", cjk(10.5), T["text3"], 2.2)
    p.text(M,370,"11/01 14:32  ·  你传出的一束光，已经被点亮。", cjk(11.5), T["text2"])
    return p
