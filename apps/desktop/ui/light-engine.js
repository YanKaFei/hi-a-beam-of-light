// Luminous Matter — the light point.
//
// One engine, two hosts (overlay + panel). Pure: it paints, it owns no time and no
// state.
//
// Why not a glow: additive light cannot exist on a light desktop, because white is
// already the ceiling. A bead survives there because it works by *occlusion* — a
// contact shadow underneath and a rim that catches light — and only then adds a
// small veil. That is the whole reason this shape was chosen over the ink wash.
window.InkLight = (function () {
  "use strict";

  function hexToRgb(h) {
    const n = parseInt(String(h).replace("#", "").slice(0, 6), 16);
    return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 };
  }
  function rgba(col, a) {
    const { r, g, b } = typeof col === "string" ? hexToRgb(col) : col;
    return `rgba(${r},${g},${b},${Math.max(0, Math.min(1, a)).toFixed(3)})`;
  }
  function mix(a, b, t) {
    const A = hexToRgb(a), B = hexToRgb(b);
    return {
      r: Math.round(A.r + (B.r - A.r) * t),
      g: Math.round(A.g + (B.g - A.g) * t),
      b: Math.round(A.b + (B.b - A.b) * t),
    };
  }

  /** Level → bead colours. `levels` stays the business binding; this is its look. */
  function specOf(tokens, level) {
    const lv = (tokens.levels && tokens.levels[level]) || tokens.levels.DORMANT;
    const core = (tokens.colors && tokens.colors[lv.coreColor]) || "#8A8F96";
    const halo = (tokens.colors && tokens.colors[lv.glowColor]) || core;
    return {
      level,
      core,
      halo,
      // the bead keeps a warm specular regardless of tier: it is one object
      specular: tokens.colors.warmLight || "#FFF3E6",
      title: lv.title,
      label: lv.label,
      breathMs: lv.breathPeriodMs || 9600,
      breathAmp: lv.breathAmplitude || 0.03,
    };
  }

  function nearestSize(sizes, size) {
    const keys = Object.keys(sizes).map(Number).sort((a, b) => a - b);
    return keys.reduce((best, k) => (Math.abs(k - size) < Math.abs(best - size) ? k : best), keys[0]);
  }

  /** Per-size parameters. Small sizes drop the veil entirely rather than scaling it. */
  function paramsFor(tokens, size) {
    const bead = tokens.bead || {};
    const key = nearestSize(bead.veilFactor || { 32: 0.34 }, size);
    const scale = size / 96;
    return {
      key,
      body: Math.max(4, size * (bead.bodyFraction || 0.4)),
      veil: size * ((bead.veilFactor || {})[key] || 0),
      rimAlpha: (bead.rimAlpha || {})[key] || 0.6,
      rimWidth: bead.rimWidth || 0.14,
      specularAlpha: bead.specularAlpha || 0.85,
      specularRadius: bead.specularRadius || 0.26,
      shadowAlpha: (bead.shadowAlpha || {})[key] || 0.28,
      shadowOffset: bead.shadowOffset || 0.3,
      shadowBlur: bead.shadowBlur || 0.34,
      limb: bead.limbDarkening || 0.55,
      scale,
    };
  }

  function prepare(canvas, size, dpr) {
    const px = Math.max(1, Math.round(size * dpr));
    canvas.width = px;
    canvas.height = px;
    canvas.style.width = size + "px";
    canvas.style.height = size + "px";
    const ctx = canvas.getContext("2d", { alpha: true });
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, size, size);
    return ctx;
  }

  /**
   * Paint the bead.
   *
   * `geom` = { size, dpr, theme } where theme is "dark" | "light". The theme only
   * changes how much the contact shadow can do: on a dark desktop a shadow is
   * invisible, so the rim carries the shape instead.
   */
  function paintBead(canvas, spec, tokens, geom) {
    const size = geom.size;
    const ctx = prepare(canvas, size, geom.dpr || 1);
    const p = paramsFor(tokens, size);
    const cx = size / 2;
    const cy = size / 2 - size * 0.02;      // a hair above centre: it is lit from above
    const r = p.body / 2;
    const dark = (geom.theme || "dark") === "dark";

    // 1. contact shadow — the thing that makes it an object rather than a sprite
    if (p.shadowAlpha > 0) {
      const sy = cy + r + size * p.shadowOffset * 0.5;
      const g = ctx.createRadialGradient(cx, sy, 0, cx, sy, r * (1 + p.shadowBlur * 2));
      const a = p.shadowAlpha * (dark ? 0.55 : 1.0);
      g.addColorStop(0, `rgba(0,0,0,${a})`);
      g.addColorStop(0.55, `rgba(0,0,0,${a * 0.35})`);
      g.addColorStop(1, "rgba(0,0,0,0)");
      ctx.save();
      ctx.translate(cx, sy);
      ctx.scale(1, 0.55);
      ctx.translate(-cx, -sy);
      ctx.fillStyle = g;
      ctx.beginPath();
      ctx.arc(cx, sy, r * (1 + p.shadowBlur * 2), 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();
    }

    // 2. veil — small, additive, and only on sizes that can hold it.
    //    Its radius is bounded by the canvas so a halo can never be clipped square.
    if (p.veil > 0) {
      const vr = Math.min(p.veil, size / 2 - 0.5);
      ctx.globalCompositeOperation = "lighter";
      const g = ctx.createRadialGradient(cx, cy, r * 0.8, cx, cy, vr);
      g.addColorStop(0, rgba(spec.halo, dark ? 0.30 : 0.16));
      g.addColorStop(0.45, rgba(spec.halo, dark ? 0.13 : 0.07));
      g.addColorStop(1, rgba(spec.halo, 0));
      ctx.fillStyle = g;
      ctx.beginPath();
      ctx.arc(cx, cy, vr, 0, Math.PI * 2);
      ctx.fill();
      ctx.globalCompositeOperation = "source-over";
    }

    // 3. body: limb darkening from a bright centre toward a darker edge
    const body = ctx.createRadialGradient(
      cx - r * 0.22, cy - r * 0.26, r * 0.1, cx, cy, r);
    body.addColorStop(0, rgba(mix(spec.core, "#FFFFFF", 0.45), 1));
    body.addColorStop(0.45, rgba(spec.core, 1));
    body.addColorStop(0.86, rgba(mix(spec.core, "#000000", p.limb * 0.55), 1));
    body.addColorStop(1, rgba(mix(spec.core, "#000000", p.limb * 0.80), 1));
    ctx.fillStyle = body;
    ctx.beginPath();
    ctx.arc(cx, cy, r, 0, Math.PI * 2);
    ctx.fill();

    // 4. rim: an arc that catches light on the side away from the specular
    ctx.save();
    ctx.strokeStyle = rgba(mix(spec.core, "#FFFFFF", 0.42), p.rimAlpha * 0.55);
    ctx.lineWidth = Math.max(0.5, r * p.rimWidth * 0.62);
    ctx.beginPath();
    ctx.arc(cx, cy, r - ctx.lineWidth * 0.5, Math.PI * 0.38, Math.PI * 0.98);
    ctx.stroke();
    ctx.restore();

    // 5. one specular, upper-left, small
    const sr = Math.max(0.8, r * p.specularRadius);
    const sx = cx - r * 0.34;
    const sy2 = cy - r * 0.40;
    const sp = ctx.createRadialGradient(sx, sy2, 0, sx, sy2, sr);
    sp.addColorStop(0, rgba(spec.specular, p.specularAlpha));
    sp.addColorStop(0.6, rgba(spec.specular, p.specularAlpha * 0.35));
    sp.addColorStop(1, rgba(spec.specular, 0));
    ctx.fillStyle = sp;
    ctx.beginPath();
    ctx.arc(sx, sy2, sr, 0, Math.PI * 2);
    ctx.fill();

    // 6. at small sizes the shape reads by silhouette alone, so give it an edge
    if (size <= 32) {
      ctx.strokeStyle = rgba(mix(spec.core, "#000000", 0.55), 0.5);
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.arc(cx, cy, r - 0.5, 0, Math.PI * 2);
      ctx.stroke();
    }
    return ctx;
  }

  return { specOf, paintBead, paramsFor, rgba, mix };
})();
