/* 一束光 · healing interface mockup
   Copy in four languages lives here so every frame can be rendered
   in the language of the README that shows it. */

const COPY = {
  zh: {
    brand: "一束光",
    mode: "本地",
    nav: ["我的光", "传光", "光路", "回响"],

    "hero.chip1": "开源",
    "hero.chip2": "本地优先",
    "hero.chip3": "概念系统",
    "hero.title": "一束光",
    "hero.sub": "把你用过的 AI，点成一束可以递出去的光。",

    "home.caption": "你的小小一束光",
    "home.eyebrow": "我的光",
    "home.title": "今天，也亮了。",
    "home.sub": "三种可能，已经亮着。分享不会让它们变暗。",
    "home.cta": "看看我的光",
    "home.s1": "个 AI 家族",
    "home.s2": "束光被接住",
    "home.s3": "次回响",
    "home.foot": "光不因分享而减少 · 只在这台电脑上计算",

    "scan.eyebrow": "点亮",
    "scan.title": "先听一听，你的桌面。",
    "scan.sub": "我们只读模型的名字，不读你的对话。可以不答应。",
    "scan.c1": "只读工具的元数据",
    "scan.c2": "不读取任何聊天内容",
    "scan.c3": "全程可以停下",
    "scan.heard": "已经听见",
    "scan.item1": "2 个家族",
    "scan.item2": "1 个本地模型",
    "scan.item3": "其他工具",
    "scan.item3m": "安静地待着",
    "scan.note": "扫描只在这台电脑上完成。结果留在本地，不上传。",
    "scan.cta": "开始扫描",

    "pass.eyebrow": "传光",
    "pass.title": "让另一束光，\n因你而开始。",
    "pass.sub": "把这一小块纸递给对方，扫一扫。",
    "pass.code": "HI · 2 6 2 0",
    "pass.status": "等待对方接收 · 14:52",
    "pass.cancel": "取消这次传光",
    "pass.note": "对方主动接收之后，这束光才会离开你。",

    "path.eyebrow": "光路",
    "path.title": "一条慢慢亮起来的路。",
    "path.n1": "已点亮",
    "path.n2": "等待中",
    "path.n3": "我收到的",
    "path.i1": "第一次点亮",
    "path.i2": "等待真实使用",
    "path.i3": "收到一束光",
    "path.i4": "第一次点亮",
    "path.t1": "11 月 18 日 · 21:40",
    "path.t2": "11 月 17 日 · 09:12",
    "path.t3": "11 月 15 日 · 19:03",
    "path.t4": "11 月 12 日 · 23:31",
    "path.note": "对方是匿名的 · 这里只留下发生过的光",

    "echo.eyebrow": "回响",
    "echo.line": "你传出的一束光，\n今天又亮了。",
    "echo.sub": "对方真正开始使用之后，这里会亮一次。",
    "echo.dates": "11 月 18 日\n11 月 12 日",

    "moment.badge": "进行中",
    "moment.eyebrow": "传光 · 正在发生",
    "moment.line": "让光，\n去向另一个人。",
    "moment.sub": "一束光从你这里出发，落在对方手里。",
    "moment.done": "完成",
  },

  en: {
    brand: "First Light",
    mode: "local",
    nav: ["My light", "Pass it on", "The path", "Echo"],

    "hero.chip1": "open source",
    "hero.chip2": "local first",
    "hero.chip3": "a concept",
    "hero.title": "First Light",
    "hero.sub": "Turn the AI you have used into light you can hand on.",

    "home.caption": "your own little light",
    "home.eyebrow": "my light",
    "home.title": "Today, lit again.",
    "home.sub": "Three possibilities are lit.\nSharing never dims them.",
    "home.cta": "See my light",
    "home.s1": "AI families",
    "home.s2": "lights received",
    "home.s3": "echoes",
    "home.foot": "Light is never diminished by sharing · computed here",

    "scan.eyebrow": "light up",
    "scan.title": "Listen to your desktop first.",
    "scan.sub": "We read model names. Never your conversations. You may say no.",
    "scan.c1": "Tool metadata only",
    "scan.c2": "No chat content, ever",
    "scan.c3": "You can stop anytime",
    "scan.heard": "already heard",
    "scan.item1": "2 families",
    "scan.item2": "1 local model",
    "scan.item3": "Other tools",
    "scan.item3m": "resting quietly",
    "scan.note": "The scan happens on this computer only. Nothing is uploaded.",
    "scan.cta": "Begin the scan",

    "pass.eyebrow": "pass the light",
    "pass.title": "Let another light\nbegin, because of you.",
    "pass.sub": "Hand over this small piece of paper. Let it be scanned.",
    "pass.code": "HI · 2 6 2 0",
    "pass.status": "Waiting to be received · 14:52",
    "pass.cancel": "Cancel this passing",
    "pass.note": "The light leaves you only after they accept it.",

    "path.eyebrow": "the path",
    "path.title": "A road that lights up slowly.",
    "path.n1": "lit",
    "path.n2": "waiting",
    "path.n3": "received",
    "path.i1": "First light",
    "path.i2": "Waiting for real use",
    "path.i3": "A light received",
    "path.i4": "First light",
    "path.t1": "Nov 18 · 21:40",
    "path.t2": "Nov 17 · 09:12",
    "path.t3": "Nov 15 · 19:03",
    "path.t4": "Nov 12 · 23:31",
    "path.note": "They stay anonymous · only the light is kept here",

    "echo.eyebrow": "echo",
    "echo.line": "A light you passed on\nis bright again today.",
    "echo.sub": "It lights up here once they truly begin.",
    "echo.dates": "Nov 18\nNov 12",

    "moment.badge": "in progress",
    "moment.eyebrow": "passing · in progress",
    "moment.line": "Let the light\ngo to someone else.",
    "moment.sub": "A light leaves you and rests in another hand.",
    "moment.done": "Done",
  },

  fr: {
    brand: "Première Lumière",
    mode: "local",
    nav: ["Ma lumière", "Transmettre", "Le chemin", "Écho"],

    "hero.chip1": "open source",
    "hero.chip2": "local d'abord",
    "hero.chip3": "un concept",
    "hero.title": "Première Lumière",
    "hero.sub": "Transforme l'IA que tu as utilisée en lumière à transmettre.",

    "home.caption": "ta petite lumière",
    "home.eyebrow": "ma lumière",
    "home.title": "Aujourd'hui encore,\nelle luit.",
    "home.sub": "Trois possibilités sont allumées.\nLes partager ne les éteint pas.",
    "home.cta": "Voir ma lumière",
    "home.s1": "familles d'IA",
    "home.s2": "lumières reçues",
    "home.s3": "échos",
    "home.foot": "Partager n'éteint jamais la lumière · calcul local",

    "scan.eyebrow": "s'allumer",
    "scan.title": "Écoute d'abord ton bureau.",
    "scan.sub": "Nous lisons les noms des modèles. Jamais tes conversations. Tu peux refuser.",
    "scan.c1": "Seulement les métadonnées",
    "scan.c2": "Aucun contenu de conversation",
    "scan.c3": "Tu peux arrêter à tout moment",
    "scan.heard": "déjà entendu",
    "scan.item1": "2 familles",
    "scan.item2": "1 modèle local",
    "scan.item3": "Autres outils",
    "scan.item3m": "au repos",
    "scan.note": "L'analyse se fait uniquement sur cet ordinateur. Rien n'est envoyé.",
    "scan.cta": "Commencer l'écoute",

    "pass.eyebrow": "transmettre",
    "pass.title": "Qu'une autre lumière\ncommence, grâce à toi.",
    "pass.sub": "Montre ce petit morceau de papier. Laisse-le scanner.",
    "pass.code": "HI · 2 6 2 0",
    "pass.status": "En attente d'être reçue · 14:52",
    "pass.cancel": "Annuler ce passage",
    "pass.note": "Cette lumière ne partira qu'une fois acceptée.",

    "path.eyebrow": "le chemin",
    "path.title": "Une route qui s'allume lentement.",
    "path.n1": "allumées",
    "path.n2": "en attente",
    "path.n3": "reçue",
    "path.i1": "Première lumière",
    "path.i2": "En attente d'un usage réel",
    "path.i3": "Une lumière reçue",
    "path.i4": "Première lumière",
    "path.t1": "18 nov. · 21:40",
    "path.t2": "17 nov. · 09:12",
    "path.t3": "15 nov. · 19:03",
    "path.t4": "12 nov. · 23:31",
    "path.note": "L'autre reste anonyme · seule la lumière demeure ici",

    "echo.eyebrow": "écho",
    "echo.line": "Une lumière que tu as transmise\nbrille encore aujourd'hui.",
    "echo.sub": "Elle s'allumera ici le jour où l'autre commencera vraiment.",
    "echo.dates": "18 nov.\n12 nov.",

    "moment.badge": "en cours",
    "moment.eyebrow": "transmission · en cours",
    "moment.line": "Que la lumière\nailles vers quelqu'un d'autre.",
    "moment.sub": "Une lumière part de chez toi et se pose dans une autre main.",
    "moment.done": "Terminer",
  },

  ja: {
    brand: "一筋の光",
    mode: "ローカル",
    nav: ["わたしの光", "わたす", "光の道", "こだま"],

    "hero.chip1": "オープンソース",
    "hero.chip2": "ローカル優先",
    "hero.chip3": "コンセプト",
    "hero.title": "一筋の光",
    "hero.sub": "使ったAIを、手渡せる光に。",

    "home.caption": "あなたの小さな光",
    "home.eyebrow": "わたしの光",
    "home.title": "今日も、\n灯りました。",
    "home.sub": "三つの可能性が灯っています。\n分けても、消えません。",
    "home.cta": "わたしの光を見る",
    "home.s1": "種類のAI",
    "home.s2": "受け取った光",
    "home.s3": "こだま",
    "home.foot": "光は分けても減らない · この端末の上で計算",

    "scan.eyebrow": "ともす",
    "scan.title": "まず、デスクトップに\n耳を澄ます。",
    "scan.sub": "読むのはモデルの名前だけ。会話は読みません。断れます。",
    "scan.c1": "メタデータだけを読む",
    "scan.c2": "会話の中身は読まない",
    "scan.c3": "いつでも止められる",
    "scan.heard": "すでに聞こえた",
    "scan.item1": "2 つの系列",
    "scan.item2": "1 つのローカルモデル",
    "scan.item3": "ほかの道具",
    "scan.item3m": "静かにしている",
    "scan.note": "スキャンはこのコンピューターの中だけ。何も送りません。",
    "scan.cta": "スキャンをはじめる",

    "pass.eyebrow": "わたす",
    "pass.title": "あなたから、\n光がはじまる。",
    "pass.sub": "この小さな紙を見せて、読み取ってもらいましょう。",
    "pass.code": "HI · 2 6 2 0",
    "pass.status": "相手の受け取りを待っています · 14:52",
    "pass.cancel": "この受け渡しをやめる",
    "pass.note": "相手が受け取ってから、光は離れます。",

    "path.eyebrow": "光の道",
    "path.title": "ゆっくり灯る道。",
    "path.n1": "灯った",
    "path.n2": "待っている",
    "path.n3": "受け取った",
    "path.i1": "はじめて灯った",
    "path.i2": "本当の使用を待っている",
    "path.i3": "光を受け取った",
    "path.i4": "はじめて灯った",
    "path.t1": "11 月 18 日 · 21:40",
    "path.t2": "11 月 17 日 · 09:12",
    "path.t3": "11 月 15 日 · 19:03",
    "path.t4": "11 月 12 日 · 23:31",
    "path.note": "相手は匿名のまま · ここに残るのは光だけ",

    "echo.eyebrow": "こだま",
    "echo.line": "あなたがわたした光は、\n今日も灯っています。",
    "echo.sub": "相手が本当にはじめたとき、ここが一度だけ灯ります。",
    "echo.dates": "11 月 18 日\n11 月 12 日",

    "moment.badge": "進行中",
    "moment.eyebrow": "受け渡し · 進行中",
    "moment.line": "光よ、\n別のだれかへ。",
    "moment.sub": "光はあなたから離れて、もう片方の手に宿ります。",
    "moment.done": "完了",
  },
};

const LEVELS = [
  { id: "L0", count: "0", size: 116, tone: [176, 164, 146], halo: 0.14, mist: 0.16,
    core: ["#fbf7ef", "#e4dacb", "#c8bba7"], glow: 0.18 },
  { id: "L1", count: "1", size: 130, tone: [223, 169, 106], halo: 0.22, mist: 0.3,
    core: ["#fffdf6", "#fbe6c6", "#eec08a"], glow: 0.5 },
  { id: "L2", count: "2–3", size: 146, tone: [219, 155, 96], halo: 0.28, mist: 0.38,
    core: ["#fffaf0", "#f8d8a8", "#e3a86b"], glow: 0.58 },
  { id: "L3", count: "4–6", size: 164, tone: [217, 141, 112], halo: 0.34, mist: 0.44,
    core: ["#fff4e6", "#f5cbb2", "#dd9678"], glow: 0.66 },
  { id: "L4", count: "7+", size: 186, tone: [168, 97, 74], halo: 0.4, mist: 0.5,
    core: ["#ffe9d6", "#eeb69a", "#bf6f57"], glow: 0.78 },
];

function orbMarkup(size, tone, level) {
  const style = level
    ? `--size:${size}px;--tone:${tone.join(",")};--halo:${level.halo};--mist:${level.mist};--core-1:${level.core[0]};--core-2:${level.core[1]};--core-3:${level.core[2]};--glow:${level.glow}`
    : `--size:${size}px;--tone:${tone.join(",")}`;
  return `
    <div class="orb" style="${style}">
      <span class="orb-halo"></span>
      <span class="orb-ink"></span>
      <span class="orb-mist"></span>
      <span class="orb-arc a1"></span>
      <span class="orb-arc a2"></span>
      <span class="orb-arc a3"></span>
      <span class="orb-dots"></span>
      <span class="orb-core"></span>
    </div>`;
}

function buildLevels() {
  const host = document.getElementById("levels");
  if (!host) return;
  host.innerHTML = LEVELS.map(
    (l) => `
    <div class="level">
      ${orbMarkup(l.size, l.tone, l)}
      <div class="lnum">${l.id}</div>
      <div class="lmeta">${l.count}</div>
    </div>`
  ).join("");
}

/* a quiet, deterministic QR-shaped stamp — a design prop, not a live code */
function buildQR() {
  const host = document.getElementById("qr");
  if (!host) return;
  const n = 21;
  let seed = 20261118;
  const rand = () => ((seed = (seed * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff);
  const cells = [];
  const finder = (r, c) => {
    const inBox = (top, left) => r >= top && r < top + 7 && c >= left && c < left + 7;
    const ring = (top, left) => {
      const dr = r - top;
      const dc = c - left;
      const edge = dr === 0 || dr === 6 || dc === 0 || dc === 6;
      const core = dr >= 2 && dr <= 4 && dc >= 2 && dc <= 4;
      return edge || core;
    };
    if (inBox(0, 0)) return ring(0, 0);
    if (inBox(0, 14)) return ring(0, 14);
    if (inBox(14, 0)) return ring(14, 0);
    return null;
  };
  for (let r = 0; r < n; r++) {
    for (let c = 0; c < n; c++) {
      const f = finder(r, c);
      let cls = "off";
      if (f === true) cls = rand() > 0.06 ? "on" : "off";
      else if (f === null) {
        const quiet =
          (r < 8 && c < 8) || (r < 8 && c > 12) || (r > 12 && c < 8);
        cls = quiet ? "off" : rand() > 0.52 ? (rand() > 0.72 ? "warm" : "on") : "off";
      }
      cells.push(`<i class="${cls}"></i>`);
    }
  }
  host.innerHTML = cells.join("");
}

function applyLang(lang) {
  const table = COPY[lang] || COPY.zh;
  document.documentElement.lang = { zh: "zh-CN", en: "en", fr: "fr", ja: "ja" }[lang] || "zh-CN";
  document.body.dataset.lang = lang;
  document.querySelectorAll("[data-t]").forEach((el) => {
    const parts = el.dataset.t.split(".");
    let val = table[el.dataset.t];
    if (val == null && parts.length === 2) {
      const group = table[parts[0]];
      if (Array.isArray(group)) val = group[Number(parts[1])];
      else if (group && typeof group === "object") val = group[parts[1]];
    }
    if (val == null) return;
    if (val.includes("\n")) el.innerHTML = val.split("\n").join("<br>");
    else el.textContent = val;
  });
  const toggle = document.querySelector(".gallery-item label");
  if (toggle) toggle.textContent = lang;
}

function capture(frame) {
  document.body.classList.add("capture");
  document.querySelectorAll(".frame").forEach((f) => f.classList.remove("is-target"));
  const el = document.getElementById("frame-" + frame);
  if (el) el.classList.add("is-target");
  else document.body.classList.remove("capture");
  return !!el;
}

(function init() {
  const params = new URLSearchParams(location.search);
  if (params.get("still") !== "0") document.body.classList.add("still");
  applyLang(params.get("lang") || "zh");
  buildLevels();
  buildQR();
  const frame = params.get("frame");
  if (frame) {
    if (document.fonts && document.fonts.ready) {
      document.fonts.ready.then(() => capture(frame));
    } else {
      capture(frame);
    }
  }
})();
