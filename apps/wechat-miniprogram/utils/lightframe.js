// One coordinate system for every layer of the light.
//
// The baked optical base and the live canvas must agree on where the ring is, or they drift apart
// exactly like the B2-2 attempt did. So both read their geometry from here and nowhere else.

/** Device pixel ratio, clamped: a 3x canvas is not worth 2.25x the fill cost. */
function dprOf(sysDpr) { return Math.min(3, Math.max(1, sysDpr || 2)); }

/**
 * The frame every light layer draws into.
 *  width, height  CSS pixels of the container
 *  scale          ring radius as a fraction of the container's short side
 */
function frameOf(width, height, opts) {
  const o = opts || {};
  const scale = o.scale || 0.30;
  const cx = o.cx !== undefined ? width * o.cx : width / 2;
  const cy = o.cy !== undefined ? height * o.cy : height * 0.34;
  const R = Math.min(width, height) * scale;
  return {
    width, height, cx, cy, R,
    halo: Math.min(width, height) * 0.50,   // outer field radius
    core: R * 0.92,                          // inner field radius
    offset: [R * 0.05, R * -0.035],          // the volume leans, it is not concentric
    // where the baked base must be placed, in CSS pixels, for its ring to land on ours
    base: { x: cx - R / scale * scale, y: cy - R, size: R * 2 },
  };
}

/** Canvas backing store size for a frame at a given device pixel ratio. */
function backing(frame, dpr) {
  const d = dprOf(dpr);
  return { width: Math.round(frame.width * d), height: Math.round(frame.height * d), dpr: d };
}

module.exports = { frameOf, backing, dprOf };
