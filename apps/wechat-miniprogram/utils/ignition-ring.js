(function(root){
// First Light · Ignition — a 2.4s brand opening drawn on canvas.
//
// The trails are the same geometry as the shipped B asset: an inclined ellipse per trail, each
// with its own aspect, tilt and centre offset, so they cross rather than nest. What moves is the
// energy inside the trails, not the logo itself.

const DURATION = 2400;
const CHAPTERS = [
  { name: "awakening",   from: 0,    to: 400 },
  { name: "acceleration",from: 400,  to: 1150 },
  { name: "redshift",    from: 1150, to: 1600 },
  { name: "dissolution", from: 1600, to: 2050 },
  { name: "reveal",      from: 2050, to: 2400 },
];

// aspect, tilt(deg), radius, dx, dy, thickness, brightness, phase, wobble
const TRAILS = [
  [1.06, -12, 0.965,  0.000,  0.010, 1.00, 1.00, 0.10, 0.018],
  [0.86, -44, 1.075,  0.020, -0.014, 0.90, 0.62, 1.70, 0.026],
  [1.16, -30, 0.905, -0.022,  0.012, 0.84, 0.55, 3.05, 0.014],
  [0.80, -52, 1.150,  0.026, -0.004, 0.66, 0.34, 4.40, 0.030],
  [1.10,  -6, 0.850, -0.024, -0.010, 0.60, 0.28, 5.55, 0.012],
  [0.96, -36, 1.010,  0.008,  0.018, 0.48, 0.19, 5.90, 0.022],
  [0.82,  -9, 1.310,  0.030, -0.014, 0.40, 0.085, 2.40, 0.040],
  [1.14, -37, 0.735, -0.026,  0.016, 0.36, 0.070, 4.10, 0.010],
];

const GOLD   = [255, 198, 152];
const CORAL  = [239, 142, 117];
const VERM   = [217,  74,  70];
const CRIMSON= [185,  47,  61];

const clamp = (v, a, b) => (v < a ? a : v > b ? b : v);
const ease   = (t) => t * t * (3 - 2 * t);

function mix(a, b, t) {
  t = clamp(t, 0, 1);
  return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
}

// gold -> coral -> vermilion -> crimson
function palette(shift) {
  const s = clamp(shift, 0, 1) * 3;
  if (s < 1) return mix(GOLD, CORAL, s);
  if (s < 2) return mix(CORAL, VERM, s - 1);
  return mix(VERM, CRIMSON, s - 2);
}

function chapterAt(ms) {
  for (const c of CHAPTERS) if (ms >= c.from && ms < c.to) return c;
  return CHAPTERS[CHAPTERS.length - 1];
}

// progress 0..1 inside a chapter
function local(ms, name) {
  const c = CHAPTERS.find((x) => x.name === name);
  return clamp((ms - c.from) / (c.to - c.from), 0, 1);
}

function trailPoints(trail, cx, cy, R, t, speed) {
  const [aspect, tiltDeg, radius, dx, dy, thick, bright, phase, wob] = trail;
  const tilt = (tiltDeg * Math.PI) / 180;
  const ox = dx * R, oy = dy * R;
  return { tilt, ox, oy, radius, aspect, thick, bright, phase, wob };
}

/**
 * Draw one frame. All state comes from `ms` so the timeline is reproducible and a seek is
 * enough to inspect any chapter.
 */
function draw(ctx, W, H, ms, opts) {
  const reduce = opts && opts.reduce;
  const R = Math.min(W, H) * 0.34;
  const cx = W / 2;
  const cy = H * 0.40;

  const acc = local(ms, "acceleration");
  const red = local(ms, "redshift");
  const dis = local(ms, "dissolution");
  const rev = local(ms, "reveal");

  // energy speed rises through acceleration, peaks at the red shift, then drains away
  const speed = (reduce ? 0.4 : 1) * (0.35 + 1.5 * acc + 1.1 * red) * (1 - 0.75 * dis);
  const appear = ease(local(ms, "awakening"));
  const globalAlpha = (1 - ease(rev)) * (0.15 + 0.85 * appear) * (1 - 0.35 * clamp(dis - 0.6, 0, 1));

  ctx.clearRect(0, 0, W, H);
  if (globalAlpha <= 0.002) return;

  ctx.save();
  ctx.globalAlpha = globalAlpha;
  ctx.lineCap = "round";

  // Energy climax: a short release between roughly 1250 and 1450ms, not a new colour.
  const peak = Math.exp(-Math.pow((ms - 1350) / 105, 2));

  TRAILS.forEach((trail, idx) => {
    const [aspect, tiltDeg, radius, dx, dy, thick, bright, phase, wob] = trail;
    const tilt = (tiltDeg * Math.PI) / 180;
    const ox = dx * R, oy = dy * R;

    const stagger = clamp((red * 1.25) - idx * 0.075, 0, 1);   // trails turn one after another, gently
    const col = palette(stagger);
    const rgb = `${Math.round(col[0])},${Math.round(col[1])},${Math.round(col[2])}`;

    const disLocal = clamp((dis * 1.6) - idx * 0.08, 0, 1);
    const thin = 1 - 0.78 * disLocal;
    const trailAlpha = bright * (1 - 0.92 * disLocal);
    if (trailAlpha <= 0.01) return;

    const N = 150;
    const pts = [];
    for (let i = 0; i <= N; i++) {
      const t = (i / N) * Math.PI * 2;
      const rad = R * radius * (1 + wob * Math.sin(3 * t + phase) + wob * 0.5 * Math.sin(5 * t + 1.7 * phase));
      const x = rad * Math.cos(t), y = rad * Math.sin(t) * 0.60 * aspect;
      const xr = x * Math.cos(tilt) - y * Math.sin(tilt);
      const yr = x * Math.sin(tilt) + y * Math.cos(tilt);
      pts.push([cx + ox + xr, cy + oy + yr, t]);
    }

    // --- dissolution: fade along the direction of travel instead of shattering ---
    // A slow field makes some stretches go first; the trail keeps thinning as it goes.
    // Fade, never shatter: the trail thins and dims along its whole length, with only a
    // gentle modulation so it does not look mechanical.
    const vis = (t) => {
      if (disLocal <= 0.001) return 1;
      const wave = 0.5 + 0.5 * Math.sin(t * 1.1 - disLocal * 1.6 + phase);
      return clamp(1 - disLocal * (0.92 + 0.08 * wave), 0, 1);
    };

    const layers = [
      { w: 10.0, a: 0.055 },
      { w: 4.0, a: 0.140 },
      { w: 1.5, a: 0.620 },
    ];
    const SEG = 26;                     // segments per trail: enough for smooth fade, cheap enough
    for (const L of layers) {
      ctx.strokeStyle = `rgba(${rgb},${clamp(trailAlpha * L.a, 0, 1)})`;
      for (let seg = 0; seg < SEG; seg++) {
        const a0 = Math.floor((seg / SEG) * N), a1 = Math.floor(((seg + 1) / SEG) * N);
        const mid = pts[Math.min(a1, N)][2];
        const v = vis(mid);
        if (v <= 0.004) continue;
        ctx.globalAlpha = globalAlpha * v;
        ctx.beginPath();
        ctx.moveTo(pts[a0][0], pts[a0][1]);
        for (let i = a0 + 1; i <= a1; i++) ctx.lineTo(pts[i][0], pts[i][1]);
        ctx.lineWidth = Math.max(0.4, R * 0.011 * thick * L.w * thin * (0.55 + 0.45 * v));
        ctx.stroke();
      }
    }
    ctx.globalAlpha = globalAlpha;

    // --- moving highlight: bright leading edge, tapering tail, faster with acceleration ---
    const flow = (opts && opts.flowPhase) || 0;
    for (let k = 0; k < 2; k++) {
      const head = (flow * speed * (1 + k * 0.33) + phase + k * 2.1) % (Math.PI * 2);
      const span = 0.45 + 0.95 * acc + 0.35 * peak;
      const heat = (0.55 + 0.9 * acc + 1.6 * peak) * (1 - 0.55 * disLocal);
      const HEAD = 7;                    // sub-arcs across the highlight
      for (let h = 0; h < HEAD; h++) {
        const f0 = h / HEAD, f1 = (h + 1) / HEAD;
        // leading edge bright, tail fades out
        const fall = Math.pow(1 - f0, 1.7);
        const alpha = clamp(trailAlpha * 0.9 * fall * clamp(heat, 0, 1.6), 0, 0.95) * vis(head * 0.5 + 1);
        if (alpha <= 0.012) continue;
        ctx.strokeStyle = `rgba(${rgb},${alpha})`;
        for (const L of [{ w: 3.0, a: 0.30 }, { w: 1.3, a: 1.0 }]) {
          ctx.beginPath();
          let pen = false;
          for (let i = 0; i <= N; i++) {
            const t = (i / N) * Math.PI * 2;
            let d = (t - head + Math.PI * 4) % (Math.PI * 2);
            if (d < f0 * span || d >= f1 * span) { pen = false; continue; }
            if (!pen) { ctx.moveTo(pts[i][0], pts[i][1]); pen = true; } else ctx.lineTo(pts[i][0], pts[i][1]);
          }
          ctx.lineWidth = Math.max(0.5, R * 0.010 * thick * L.w * thin * (0.5 + 0.5 * fall));
          ctx.stroke();
        }
      }
    }
  });

  // --- the two crossings take the release first ---
  if (peak > 0.05) {
    const hot = clamp(peak * (0.35 + 0.65 * red), 0, 1);
    for (const [bx, by] of [[cx - R * 0.42, cy + R * 0.10], [cx + R * 0.40, cy - R * 0.06]]) {
      const g = ctx.createRadialGradient(bx, by, 0, bx, by, R * 0.16);
      g.addColorStop(0, `rgba(255,244,226,${0.42 * hot})`);
      g.addColorStop(0.45, `rgba(255,206,172,${0.20 * hot})`);
      g.addColorStop(1, "rgba(255,190,150,0)");
      ctx.fillStyle = g;
      ctx.beginPath(); ctx.arc(bx, by, R * 0.16, 0, Math.PI * 2); ctx.fill();
    }
  }

  ctx.restore();
}

const api = { DURATION, CHAPTERS, TRAILS, draw, chapterAt, palette };
if (typeof module === 'object' && module.exports) module.exports = api;
else root.FirstLightRing = api;

})(typeof window!=='undefined'?window:(typeof globalThis!=='undefined'?globalThis:{}));
