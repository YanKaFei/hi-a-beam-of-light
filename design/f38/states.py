from kit import T, cjk, latin

def banner_a(p, x, y, w, title, body, tone="error"):
    col = T["accent"] if tone=="error" else T["accent2"]
    p.rect(x,y,w,52, fill="#FBEFEC" if tone=="error" else "#EDF2F0", r=8)
    p.rect(x,y,3,52, fill=col, r=1.5)
    p.text(x+16,y+11,title, cjk(12,1), T["text"])
    p.text(x+16,y+30,body, cjk(11), T["text2"])

def banner_b(p, x, y, w, title, body, tone="error"):
    col = T["accent"] if tone=="error" else T["accent2"]
    p.line(x,y,x+w,y, color=col, width=1.5)
    p.text(x,y+10,title, cjk(12,1), T["text"])
    p.text(x,y+30,body, cjk(11), T["text2"])
