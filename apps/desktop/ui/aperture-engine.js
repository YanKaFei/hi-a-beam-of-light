/* A2-Signature · Organic Aperture, canvas implementation.
 *
 * Second renderer beside paintBead, selected by FIRSTLIGHT_LIGHT_STYLE=aperture.
 * paintBead is untouched and remains the default, so rollback is an env var.
 *
 * Geometry mirrors design/f39r1/aperture.py: one solid superellipse body, a
 * low-amplitude rosette aperture, and a rim notch. Structure is never carried by
 * gaps or hairlines below 32px - that is what failed twice in the design round.
 */
(function (root) {
  "use strict";

  const INK = "#202B30";
  const LIGHT = "#F6D2B0";
  const OCHRE = "#A96B5B";

  const PARAMS = {
    radius: 0.355,      // R / size
    exponent: 3.45,     // superellipse: higher = closer to a hexagon
    rotate: 15 * Math.PI / 180,
    holeK: 0.045,       // rosette amplitude
    holeMin: 0.13,      // r0/R at tier 0
    holeMax: 0.50,      // r0/R span
    notchAngle: 6.5 * Math.PI / 180,   // a slit, not a bite
    notchDepth: 0.88,   // notch tip at 0.88R - shallow, reads as a nick
    notchAt: 38 * Math.PI / 180,
    notchMinWidthPx: 2.6,   // below this the notch is eaten by anti-aliasing
    notchWidthScale: 1.0,   // correction factor, recorded per run
    seamsMinSize: 9999, // seams off by default: at 96px they read as a cracked tile,
                        // and thin structure is exactly what failed in the design round.
                        // Set to 32 to re-enable for comparison.
    seamAlpha: 0.16,
  };


  /* Material experiments. Each row states every knob, so a difference between two
     renders can be attributed to a named variable rather than "the look". */
  const MATERIALS = {
    base: { body: "#202B30", gradient: 0.00, innerRefl: 0.00, rim: 0.08,
            light: 1.00, shadow: 0.00, saturate: 1.00 },
    t1:   { body: "#39464B", gradient: 0.35, innerRefl: 0.00, rim: 0.10,
            light: 1.00, shadow: 0.10, saturate: 1.00 },   // diffuse lift
    t2:   { body: "#2C383D", gradient: 0.18, innerRefl: 0.45, rim: 0.12,
            light: 1.05, shadow: 0.12, saturate: 1.00 },   // dual edge + inner reflection
    t3:   { body: "#4B5457", gradient: 0.12, innerRefl: 0.22, rim: 0.00,
            light: 0.95, shadow: 0.18, saturate: 0.90 },   // neutral graphite / ceramic
  };

  function superellipsePath(ctx, cx, cy, R, n, rot) {
    const steps = 180;
    ctx.beginPath();
    for (let i = 0; i <= steps; i++) {
      const t = (2 * Math.PI * i) / steps;
      const c = Math.cos(t), s = Math.sin(t);
      const x = Math.sign(c) * Math.pow(Math.abs(c), 2 / n) * R;
      const y = Math.sign(s) * Math.pow(Math.abs(s), 2 / n) * R;
      const px = cx + x * Math.cos(rot) - y * Math.sin(rot);
      const py = cy + x * Math.sin(rot) + y * Math.cos(rot);
      if (i === 0) ctx.moveTo(px, py); else ctx.lineTo(px, py);
    }
    ctx.closePath();
  }

  function shade(hex, k) {
    const n = parseInt(hex.slice(1), 16);
    const r = Math.round(((n >> 16) & 255) * k);
    const g = Math.round(((n >> 8) & 255) * k);
    const b = Math.round((n & 255) * k);
    return `rgb(${r},${g},${b})`;
  }

  function rosettePath(ctx, cx, cy, r0, k, rot) {
    const steps = 200;
    ctx.beginPath();
    for (let i = 0; i <= steps; i++) {
      const t = (2 * Math.PI * i) / steps;
      const r = r0 * (1 + k * Math.cos(6 * (t + rot)));
      const px = cx + r * Math.cos(t);
      const py = cy + r * Math.sin(t);
      if (i === 0) ctx.moveTo(px, py); else ctx.lineTo(px, py);
    }
    ctx.closePath();
  }

  /* Paint one aperture.
     size   CSS pixels (the canvas is already scaled by DPR by the caller)
     tier   0..1 aperture opening, derived from verified_families only
     lit    0..1 how present the light is (visual only, never business state)
     opts   { notch, variant: "signature" | "organic" | "original" } */
  function paint(ctx, size, tier, lit, opts) {
    opts = opts || {};
    const M = MATERIALS[opts.material || "base"] || MATERIALS.base;
    const variant = opts.variant || "signature";
    const notch = opts.notch !== undefined ? opts.notch : (variant === "signature");
    const organic = variant === "original" ? 0 : 1;

    const P = PARAMS;
    const cx = size / 2, cy = size / 2;
    const R = size * P.radius;
    const rot = P.rotate + (variant === "signature" ? 6 * Math.PI / 180 : 0);
    const n = organic ? P.exponent : 6.0;
    const r0 = R * (P.holeMin + P.holeMax * tier);

    ctx.save();
    ctx.clearRect(0, 0, size, size);

    // outer bloom - radial gradient, one fill, no filters
    const g = ctx.createRadialGradient(cx, cy, r0 * 0.4, cx, cy, R * 1.5);
    g.addColorStop(0, `rgba(169,107,91,${0.22 * lit})`);
    g.addColorStop(1, "rgba(169,107,91,0)");
    ctx.fillStyle = g;
    ctx.beginPath();
    ctx.arc(cx, cy, R * 1.5, 0, Math.PI * 2);
    ctx.fill();

    // contact shadow: a soft dark ellipse under the body, only for materials that ask
    if (M.shadow > 0) {
      const sh = ctx.createRadialGradient(cx, cy + R * 0.55, R * 0.2, cx, cy + R * 0.55, R * 1.25);
      sh.addColorStop(0, `rgba(0,0,0,${0.30 * M.shadow})`);
      sh.addColorStop(1, "rgba(0,0,0,0)");
      ctx.fillStyle = sh;
      ctx.beginPath();
      ctx.ellipse(cx, cy + R * 0.5, R * 1.15, R * 0.7, 0, 0, Math.PI * 2);
      ctx.fill();
    }

    // body, with an optional top-lit gradient so it reads as a volume, not a plate
    superellipsePath(ctx, cx, cy, R, n, rot);
    if (M.gradient > 0) {
      const bg = ctx.createLinearGradient(cx, cy - R, cx, cy + R);
      bg.addColorStop(0, M.body);
      bg.addColorStop(1, shade(M.body, 1 - 0.28 * M.gradient));
      ctx.fillStyle = bg;
    } else {
      ctx.fillStyle = M.body;
    }
    ctx.fill();

    // inner reflection: a bright arc hugging the inside of the upper rim
    if (M.innerRefl > 0) {
      ctx.save();
      superellipsePath(ctx, cx, cy, R, n, rot);
      ctx.clip();
      ctx.strokeStyle = `rgba(246,247,244,${0.22 * M.innerRefl})`;
      ctx.lineWidth = Math.max(1, size * 0.022);
      ctx.beginPath();
      ctx.ellipse(cx, cy + R * 0.10, R * 0.90, R * 0.80, 0, Math.PI * 1.08, Math.PI * 1.92);
      ctx.stroke();
      ctx.restore();
    }

    // rim notch: a real cut, so it survives downscaling
    if (notch) {
      const na = rot + P.notchAt;
      // At 24px the fixed 6.5 degrees covers under 2px of rim and vanishes.
      // Widen the angle only enough to keep a fixed pixel width; the depth and the
      // position are untouched, so the symbol stays the same family.
      const minHalf = Math.asin(Math.min(0.95, (P.notchMinWidthPx / 2) / (R * 1.02)));
      const w = Math.max(P.notchAngle, minHalf) * P.notchWidthScale;
      ctx.globalCompositeOperation = "destination-out";
      ctx.beginPath();
      ctx.moveTo(cx + R * 1.02 * Math.cos(na - w), cy + R * 1.02 * Math.sin(na - w));
      ctx.lineTo(cx + R * 1.02 * Math.cos(na + w), cy + R * 1.02 * Math.sin(na + w));
      ctx.lineTo(cx + R * P.notchDepth * Math.cos(na), cy + R * P.notchDepth * Math.sin(na));
      ctx.closePath();
      ctx.fill();
      ctx.globalCompositeOperation = "source-over";
    }

    // aperture
    const hx = cx + (variant === "signature" ? R * 0.04 : 0);
    rosettePath(ctx, hx, cy, r0, P.holeK, rot * 0.5);
    ctx.fillStyle = lit > 0.45 ? LIGHT : OCHRE;
    ctx.globalAlpha = Math.min(1, M.light);
    ctx.fill();
    ctx.globalAlpha = 1;

    // seams: only where they can be seen, and only as a value step inside the body
    if (size >= P.seamsMinSize) {
      ctx.strokeStyle = `rgba(232,226,216,${P.seamAlpha * lit})`;
      ctx.lineWidth = Math.max(1, size * 0.011);
      for (let k = 0; k < 6; k++) {
        const a = rot + (k * Math.PI) / 3;
        ctx.beginPath();
        ctx.moveTo(hx + r0 * 1.06 * Math.cos(a), cy + r0 * 1.06 * Math.sin(a));
        ctx.lineTo(cx + R * 0.90 * Math.cos(a), cy + R * 0.90 * Math.sin(a));
        ctx.stroke();
      }
    }

    // inner glow of the aperture
    const g2 = ctx.createRadialGradient(hx, cy, 0, hx, cy, r0 * 1.7);
    g2.addColorStop(0, `rgba(246,210,176,${0.55 * lit})`);
    g2.addColorStop(1, "rgba(246,210,176,0)");
    ctx.fillStyle = g2;
    ctx.beginPath();
    ctx.arc(hx, cy, r0 * 1.7, 0, Math.PI * 2);
    ctx.fill();

    ctx.restore();
  }

  /* Dark grounds: the ink body disappears. The compensation is a stroke whose
     alpha is stated explicitly - it is not a brightness multiplier. */
  const RIM = {
    none: 0,
    "8%": 0.08,          // alpha of a 1px light stroke, ~8% coverage of the edge
    "16%": 0.16,
    "24%": 0.24,
  };

  function paintCompensated(ctx, size, tier, lit, rim, opts) {
    paint(ctx, size, tier, lit, opts);
    const M = MATERIALS[(opts && opts.material) || "base"] || MATERIALS.base;
    const alpha = M.rim * ((RIM[rim] || 0) / 0.08);
    if (!alpha) return;
    const P = PARAMS;
    const R = size * P.radius;
    const rot = P.rotate + 6 * Math.PI / 180;
    ctx.save();
    superellipsePath(ctx, size / 2, size / 2, R, P.exponent, rot);
    ctx.strokeStyle = `rgba(246,247,244,${alpha * 1.5})`;
    ctx.lineWidth = Math.max(1, size * 0.012);
    ctx.stroke();
    ctx.restore();
  }

  root.Aperture = { paint, paintCompensated, PARAMS, RIM, MATERIALS };
})(window);
