// LightScene — parameter groups, focus envelopes, and crossings that come from the geometry.
//
// FIELD    the ambient volume the light sits in
// FILAMENT three graded layers of trail
// ENERGY   where light concentrates, always along a trail, never a free-standing blob
// STATE    temperature per trust level; a level may change warmth, never verification
// MOTION   slow breathing only

const FIELD = {
  coreScale: 0.92,        // inner field radius, in R
  coreOffset: [0.05, -0.035],  // slight asymmetry: the volume has a side it leans toward
  haloScale: 0.50,        // outer halo, fraction of the canvas short side
  coreAlpha: 0.88,
  haloAlpha: 0.34,
};

const FILAMENT = {
  widths: { veil: 0.075, body: 0.0190, core: 0.0070 },
  alphas: { veil: 0.200, body: 0.92, core: 1.00 },
  // how far each grade sits back from the lead
  grade: [1.00, 0.86, 0.55],
};

const ENERGY = {
  peak: 1.05,             // added alpha at the focus of a filament
  width: 0.16,            // envelope width in path parameter
  crossBoost: 1.15,       // extra along a trail at a real crossing
  crossSpan: 0.045,       // how far the crossing light travels along the trail
};

const STATE = {
  UNPAIRED:            { field: [250, 246, 240], core: [248, 243, 236], fil: [206, 196, 182], hot: [250, 244, 232], lift: 0.86, speed: 0.10 },
  PAIRED_IDLE:         { field: [252, 244, 230], core: [250, 240, 222], fil: [226, 166, 98], hot: [255, 236, 198], lift: 1.00, speed: 0.15 },
  LOCAL_OBSERVED:      { field: [253, 238, 214], core: [252, 232, 200], fil: [222, 156, 84], hot: [255, 232, 188], lift: 1.30, speed: 0.34 },
  SIGNATURE_VALID:     { field: [212, 158, 140], core: [232, 176, 156], fil: [255, 244, 234], hot: [255, 250, 242], lift: 1.42, speed: 0.46 },
  SOURCE_APPROVED:     { field: [204, 146, 136], core: [226, 164, 148], fil: [255, 242, 232], hot: [255, 248, 240], lift: 1.48, speed: 0.52 },
  ACTIVATION_ELIGIBLE:{ field: [194, 132, 128], core: [218, 150, 138], fil: [255, 240, 230], hot: [255, 246, 238], lift: 1.54, speed: 0.58 },
  ACTIVATED:           { field: [184, 120, 120], core: [208, 138, 130], fil: [255, 238, 228], hot: [255, 244, 236], lift: 1.60, speed: 0.62 },
};

const MOTION = { breathMs: 7000, breathe: 0.14, highlightSpeed: 0.15 };

// lead carries the silhouette, second creates the crossings, third supports and stays back
const MAIN = [
  [1.06, -12, 0.965,  0.000,  0.010, 1.00, 1.00, 0.10, 0.018],
  [0.86, -44, 1.075,  0.020, -0.014, 0.90, 0.72, 1.70, 0.026],
  [1.16, -30, 0.905, -0.022,  0.012, 0.84, 0.42, 3.05, 0.014],
];
const SECOND = [
  [0.80, -52, 1.150,  0.026, -0.004, 0.66, 0.22, 4.40, 0.030],
  [1.10,  -6, 0.850, -0.024, -0.010, 0.60, 0.16, 5.55, 0.012],
];
const WISP = [
  [0.96, -36, 1.010,  0.008,  0.018, 0.48, 0.08, 5.90, 0.022],
  [0.82,  -9, 1.310,  0.030, -0.014, 0.40, 0.05, 2.40, 0.040],
  [1.14, -37, 0.735, -0.026,  0.016, 0.36, 0.04, 4.10, 0.010],
];

const clamp = (v, a, b) => (v < a ? a : v > b ? b : v);
const rgba = (c, a) => `rgba(${Math.round(c[0])},${Math.round(c[1])},${Math.round(c[2])},${clamp(a, 0, 1)})`;
const stateOf = (level, fixture) => (fixture && STATE[fixture]) || STATE[level] || STATE.UNPAIRED;

function pathOf(trail, R, cx, cy, N) {
  const [aspect, tiltDeg, radius, dx, dy, , , phase, wob] = trail;
  const tilt = (tiltDeg * Math.PI) / 180;
  const pts = [];
  for (let i = 0; i <= N; i++) {
    const a = (i / N) * Math.PI * 2;
    const rad = R * radius * (1 + wob * Math.sin(3 * a + phase) + wob * 0.5 * Math.sin(5 * a + 1.7 * phase));
    const x = rad * Math.cos(a), y = rad * Math.sin(a) * 0.60 * aspect;
    const xr = x * Math.cos(tilt) - y * Math.sin(tilt);
    const yr = x * Math.sin(tilt) + y * Math.cos(tilt);
    pts.push([cx + dx * R + xr, cy + dy * R + yr]);
  }
  return pts;
}

/** Gaussian envelope around a point on the path, with soft shoulders. */
function envelope(u, centre, width) {
  const d = ((u - centre + 1.5) % 1) - 0.5;      // wrap so the seam is not a hard edge
  return Math.exp(-(d * d) / (width * width));
}

/** Real intersections between two polylines, as path parameters on each. */
function crossings(a, b, thresh) {
  const out = [];
  for (let i = 0; i < a.length; i += 2) {
    for (let j = 0; j < b.length; j += 2) {
      const dx = a[i][0] - b[j][0], dy = a[i][1] - b[j][1];
      if (dx * dx + dy * dy < thresh * thresh) {
        out.push([i / a.length, j / b.length]);
      }
    }
  }
  // keep the strongest, well separated
  const kept = [];
  for (const [ua, ub] of out) {
    if (kept.every(([ka, kb]) => Math.abs(ua - ka) > 0.12 || Math.abs(ub - kb) > 0.12)) kept.push([ua, ub]);
  }
  return kept.slice(0, 2);
}

function draw(ctx, W, H, level, t, opts) {
  const o = opts || {};
  const frame = o.frame;
  const st = stateOf(level, o.fixture);
  const scan = clamp(o.scan || 0, 0, 1);
  const lift = clamp(o.lift || 0, 0, 1);
  // hybrid: the material base already carries field + fibres, the canvas adds motion and state
  const hybrid = o.variant === "HYBRID" || o.hybrid === true;

  const R = (frame && frame.R) || Math.min(W * 0.30, H * 0.20);
  const cx = (frame && frame.cx) || W / 2, cy = (frame && frame.cy) || H * 0.34;
  const breath = 0.5 + 0.5 * Math.sin((t / MOTION.breathMs) * Math.PI * 2);

  ctx.clearRect(0, 0, W, H);
  ctx.save();
  ctx.lineCap = "round";
  ctx.lineJoin = "round";

  if (!hybrid) {
    // ---------- FIELD ----------
    const haloR = Math.min(W, H) * FIELD.haloScale;
    const a = 0.42 * st.lift * (0.86 + 0.14 * breath);   // on white the field only softens the edge
    const hx = cx + R * FIELD.coreOffset[0], hy = cy + R * FIELD.coreOffset[1];

    const halo = ctx.createRadialGradient(cx, cy, R * 0.7, cx, cy, haloR);
    halo.addColorStop(0.00, rgba(st.field, a * FIELD.haloAlpha));
    halo.addColorStop(0.45, rgba(st.field, a * FIELD.haloAlpha * 0.42));
    halo.addColorStop(0.78, rgba(st.field, a * FIELD.haloAlpha * 0.12));
    halo.addColorStop(1.00, rgba(st.field, 0));
    ctx.fillStyle = halo;
    ctx.beginPath(); ctx.arc(cx, cy, haloR, 0, Math.PI * 2); ctx.fill();

    const coreR = R * FIELD.coreScale;
    const core = ctx.createRadialGradient(hx, hy, R * 0.04, hx, hy, coreR);
    core.addColorStop(0.00, rgba(st.core, a * FIELD.coreAlpha));
    core.addColorStop(0.38, rgba(st.field, a * 0.66));
    core.addColorStop(0.72, rgba(st.field, a * 0.28));
    core.addColorStop(1.00, rgba(st.field, 0));
    ctx.fillStyle = core;
    ctx.beginPath(); ctx.arc(hx, hy, coreR, 0, Math.PI * 2); ctx.fill();
  }

  // ---------- FILAMENTS ----------
  const mainPts = MAIN.map((tr) => pathOf(tr, R, cx, cy, 180));
  const secondPts = SECOND.map((tr) => pathOf(tr, R, cx, cy, 120));
  const wispPts = WISP.map((tr) => pathOf(tr, R, cx, cy, 110));

  const base = (0.56 + 0.12 * breath + 0.16 * lift) * st.lift * (1 + 0.2 * scan);

  // the two real crossings, found from the geometry, not chosen by hand
  const cross = crossings(mainPts[0], mainPts[1], R * 0.06);
  const crossU = { 0: [], 1: [] };
  cross.forEach(([ua, ub], k) => { crossU[0].push([ua, k]); crossU[1].push([ub, k]); });

  function strokeSegs(pts, width, colour, alphaFn, segs) {
    for (let s = 0; s < segs; s++) {
      const i0 = Math.floor((s / segs) * (pts.length - 1));
      const i1 = Math.floor(((s + 1) / segs) * (pts.length - 1));
      const u = (i0 + i1) / 2 / (pts.length - 1);
      const al = alphaFn(u);
      if (al <= 0.004) continue;
      ctx.strokeStyle = rgba(colour, al);
      ctx.lineWidth = width;
      ctx.beginPath();
      ctx.moveTo(pts[i0][0], pts[i0][1]);
      for (let i = i0 + 1; i <= i1; i++) ctx.lineTo(pts[i][0], pts[i][1]);
      ctx.stroke();
    }
  }

  MAIN.forEach((tr, i) => {
    const [ , , , , , thick, bright] = tr;
    const grade = (FILAMENT.grade[i] || 0.6) * (hybrid ? 0.28 : 1);

    // veil: wide, soft, always present
    strokeSegs(mainPts[i], Math.max(2.0, R * FILAMENT.widths.veil * thick), st.fil,
      () => FILAMENT.alphas.veil * bright * base, 1);
    // body: the structure
    strokeSegs(mainPts[i], Math.max(1.0, R * FILAMENT.widths.body * thick), st.fil,
      () => FILAMENT.alphas.body * bright * base * grade, 1);

    // core: not a full outline. A focus envelope around one point, plus crossings along this trail.
    const focuses = [0.18 + i * 0.27];
    (crossU[i] || []).forEach(([u, k]) => focuses.push(u));
    strokeSegs(mainPts[i], Math.max(0.55, R * FILAMENT.widths.core * thick), st.hot, (u) => {
      let env = 0;
      focuses.forEach((f, idx) => {
        const w = idx === 0 ? ENERGY.width * 1.35 : ENERGY.crossSpan;
        const amp = idx === 0 ? 1 : (k_amp(idx));
        env = Math.max(env, amp * envelope(u, f, w));
      });
      return (0.18 + ENERGY.peak * env) * bright * base * grade;
    }, 36);

    // energy at the crossing: a short, thicker run along the same trail
    (crossU[i] || []).forEach(([u, k]) => {
      const amp = k === 0 ? 1 : 0.55;                 // the main crossing is the climax
      strokeSegs(mainPts[i], Math.max(1.4, R * 0.012 * thick), st.hot, (uu) =>
        ENERGY.crossBoost * amp * envelope(uu, u, ENERGY.crossSpan * 1.6) * bright * base, 24);
    });
  });

  function k_amp(idx) { return idx === 0 ? 1 : 0.6; }

  SECOND.forEach((tr, i) => {
    const [ , , , , , thick, bright] = tr;
    strokeSegs(secondPts[i], Math.max(1.1, R * 0.018 * thick), st.hot, () => 0.030 * bright * base, 1);
    strokeSegs(secondPts[i], Math.max(0.7, R * 0.006 * thick), st.fil, () => 0.26 * bright * base, 1);
  });
  WISP.forEach((tr, i) => {
    const [ , , , , , thick, bright] = tr;
    strokeSegs(wispPts[i], Math.max(0.6, R * 0.005 * thick), st.fil, () => 0.085 * bright * base, 1);
  });

  // ---------- travelling highlight, lead filament only ----------
  {
    const tr = MAIN[0];
    const [ , , , , , thick, bright, phase] = tr;
    const head = ((t / 1000) * MOTION.highlightSpeed * st.speed * (1 + 1.4 * scan) + phase) % 1;
    const pts = mainPts[0];
    const span = 0.06 + 0.03 * lift;
    strokeSegs(pts, Math.max(0.7, R * 0.006 * thick), st.hot, (u) => {
      const d = Math.abs(((u - head + 1.5) % 1) - 0.5);
      return d < span ? 0.50 * bright * base : 0;
    }, 40);
  }

  ctx.restore();
}

module.exports = { FIELD, FILAMENT, ENERGY, STATE, MOTION, MAIN, SECOND, WISP, draw, stateOf };
