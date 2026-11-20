// FamilyView (Rust) -> FamilyVM (UI). The one place that decides what a model row means.
// Unknown shapes become an error state; they never become a fabricated name.
(function (root) {
  "use strict";
  const INVALID = /undefined|NaN|\[object Object\]|^null$/;

  function esc(s) {
    return String(s).replace(/[&<>"']/g, (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  }

  function normalize(raw) {
    const out = { verified: [], discovered: [], error: false };
    if (raw == null) return out;                       // absent = nothing yet, not an error
    if (!Array.isArray(raw)) { out.error = true; return out; }
    for (const item of raw) {
      if (!item || typeof item !== "object" || Array.isArray(item)) { out.error = true; continue; }
      const id = typeof item.family === "string" ? item.family.trim() : "";
      if (!id || INVALID.test(id)) { out.error = true; continue; }
      const countable = item.countable === true;
      const vm = {
        familyId: id,
        displayName: id,
        evidenceLevel: typeof item.level === "string" ? item.level : null,
        verificationStatus: countable ? "verified" : "discovered",
        countable,
        sessions: Number.isFinite(item.sessions) ? item.sessions : null,
      };
      (countable ? out.verified : out.discovered).push(vm);
    }
    return out;
  }

  const api = { normalize, esc, INVALID };
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  root.Families = api;
})(typeof window !== "undefined" ? window : globalThis);
