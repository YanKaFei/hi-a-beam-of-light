// Model name -> registered family. The registry file is the only source of truth; a
// substring guess is never used. Mirrors crates/light-core/src/registry.rs semantics:
// client prefix stripped, then exact alias, then longest matching prefix.

import { readFileSync } from "node:fs";

export function loadRegistry(path) {
  const raw = JSON.parse(readFileSync(path, "utf8"));
  const exact = new Map();
  const prefix = [];
  for (const a of raw.aliases ?? []) {
    if (a.match === "exact") exact.set(a.raw, a.disposition);
    else if (a.match === "prefix") prefix.push([a.raw, a.disposition]);
  }
  prefix.sort((x, y) => y[0].length - x[0].length);
  const families = new Set((raw.families ?? []).map((f) => f.slug));
  const clients = raw.client_prefixes ?? [];
  return { semver: raw.semver, families, exact, prefix, clients };
}

/** Returns {kind:'family',family} | {kind:'task_label'} | {kind:'unknown'}. */
export function resolve(reg, rawName) {
  if (typeof rawName !== "string" || !rawName.trim()) return { kind: "unknown", reason: "empty" };
  let name = rawName.trim().split(":")[0];              // ollama tags are name:tag
  for (const c of reg.clients) {
    if (name.startsWith(c.prefix)) { name = name.slice(c.prefix.length); break; }
  }
  const hit = reg.exact.get(name);
  if (hit) return normalize(hit);
  for (const [p, d] of reg.prefix) {
    if (name.startsWith(p)) return normalize(d);
  }
  return { kind: "unknown", reason: "not_in_registry" };
}

function normalize(d) {
  if (d.kind === "family" && d.family) return { kind: "family", family: d.family };
  if (d.kind === "task_label") return { kind: "task_label", reason: d.reason };
  return { kind: "unknown", reason: "bad_disposition" };
}
