// Independent TypeScript reference implementation of the FirstLight core rules.
//
// This file is a *second implementation*, written from the product spec and the
// registry data — not generated from the Rust source. Its only job is to make
// protocol drift loud: `test/conformance.test.mjs` compares it against the
// shared fixtures, and the Rust `tests/contract.rs` compares Rust against the
// same fixtures. If the two implementations ever disagree, CI fails on one side.
//
// No dependencies, no I/O beyond reading the registry data file.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const HERE = dirname(fileURLToPath(import.meta.url));
export const REPO_ROOT = join(HERE, "..", "..", "..");

// ---------------------------------------------------------------------------
// Title ladders (product spec §6.1 / §6.2)
// A tier applies when value >= threshold; the highest matching threshold wins.
// ---------------------------------------------------------------------------

export const KNOWLEDGE_LADDER = [
  [0, "seeker", "寻光者"],
  [1, "first_light", "初明者"],
  [2, "enlightener", "启迪者"],
  [4, "explorer", "探索者"],
  [7, "frontier", "拓界者"],
];

export const TRANSMISSION_LADDER = [
  [0, "holder", "持光者"],
  [1, "kindler", "启蒙者"],
  [3, "guide", "引光者"],
  [10, "sower", "播光者"],
  [30, "lamp_bearer", "传灯者"],
  [100, "wildfire", "燎原者"],
];

function ladder(ladderRows, value) {
  let chosen = ladderRows[0];
  for (const row of ladderRows) {
    if (value >= row[0]) chosen = row;
    else break;
  }
  return { slug: chosen[1], zh: chosen[2] };
}

export const knowledgeTitle = (n) => ladder(KNOWLEDGE_LADDER, n);
export const transmissionTitle = (n) => ladder(TRANSMISSION_LADDER, n);

// ---------------------------------------------------------------------------
// Light level (product spec §4.3)
// ---------------------------------------------------------------------------

export function lightLevel(families) {
  if (families === 0) return "DORMANT";
  if (families === 1) return "FAINT";
  if (families <= 3) return "GROWING";
  if (families <= 6) return "BLAZING";
  return "DEEP";
}

// ---------------------------------------------------------------------------
// Registry normalization
// ---------------------------------------------------------------------------

export function loadRegistry(path = join(REPO_ROOT, "data", "model-registry.v1.json")) {
  const raw = JSON.parse(readFileSync(path, "utf8"));
  const exact = new Map();
  const prefixes = [];
  for (const alias of raw.aliases) {
    const key = alias.raw.trim().toLowerCase();
    if (!key) throw new Error("registry alias with empty raw");
    if (alias.disposition.kind === "family" && !raw.families.some((f) => f.slug === alias.disposition.family)) {
      throw new Error(`alias ${key} points at unknown family ${alias.disposition.family}`);
    }
    if (alias.match === "exact") {
      if (exact.has(key)) throw new Error(`duplicate exact alias ${key}`);
      exact.set(key, alias.disposition);
    } else {
      prefixes.push([key, alias.disposition]);
    }
  }
  prefixes.sort((a, b) => b[0].length - a[0].length || (a[0] < b[0] ? -1 : 1));
  const clientPrefixes = (raw.client_prefixes ?? [])
    .map((c) => [c.prefix.toLowerCase(), c.client])
    .sort((a, b) => b[0].length - a[0].length);
  return { semver: raw.semver, families: raw.families, exact, prefixes, clientPrefixes };
}

function lookup(registry, lowered) {
  if (registry.exact.has(lowered)) return registry.exact.get(lowered);
  for (const [pattern, disposition] of registry.prefixes) {
    if (lowered.startsWith(pattern)) return disposition;
  }
  return null;
}

export function normalize(registry, rawModel) {
  const raw = String(rawModel ?? "").trim();
  if (!raw) return { raw, disposition: { kind: "unknown" }, family: null, client: null };

  const lowered = raw.toLowerCase();
  for (const [prefix, client] of registry.clientPrefixes) {
    if (!lowered.startsWith(prefix)) continue;
    const rest = lowered.slice(prefix.length).trim();
    if (!rest) break;
    const found = lookup(registry, rest);
    if (found) {
      return {
        raw,
        disposition: found,
        family: found.kind === "family" ? found.family : null,
        client,
      };
    }
    break;
  }

  const found = lookup(registry, lowered);
  if (!found) return { raw, disposition: { kind: "unknown" }, family: null, client: null };
  return {
    raw,
    disposition: found,
    family: found.kind === "family" ? found.family : null,
    client: null,
  };
}

// ---------------------------------------------------------------------------
// Evidence: two gates (see docs/adr/ADR-011-evidence-gates.md)
// ---------------------------------------------------------------------------

export const MODEL_OUTPUT_ITEM_TYPES = [
  "agentMessage",
  "reasoning",
  "commandExecution",
  "mcpToolCall",
  "webSearch",
  "fileChange",
  "imageGeneration",
  "collabAgentToolCall",
  "subAgentActivity",
];

export const NON_PROOF_ITEM_TYPES = ["userMessage", "imageView", "sleep", "contextCompaction"];

export function provesCall({ output_items = 0, completed_turns = 0 } = {}) {
  // Only model output proves a call. Completed turns are corroboration: a turn
  // can be marked completed with no model output at all.
  return output_items > 0 && completed_turns >= 0;
}

export function evaluateEvidence(registry, rawModel, proof = {}) {
  const normalization = normalize(registry, rawModel);
  const raw = String(rawModel ?? "").trim();

  if (!raw) return { decision: "rejected", reason: "EmptyModel", normalization };
  if (normalization.disposition.kind === "unknown") {
    return { decision: "rejected", reason: "UnknownFamily", normalization };
  }
  if (normalization.disposition.kind === "task_label") {
    return { decision: "rejected", reason: "TaskLabelNotAModel", normalization };
  }
  if (!provesCall(proof)) {
    return {
      decision: "observable",
      level: "E1",
      reason: "SessionCreatedButNoModelOutput",
      normalization,
    };
  }
  return {
    decision: "countable",
    family: normalization.family,
    level: "E3",
    normalization,
  };
}

// ---------------------------------------------------------------------------
// Light state
// ---------------------------------------------------------------------------

export function computeLightState({ verified, attested, received, nodes }) {
  return {
    level: lightLevel(verified),
    knowledge_title: knowledgeTitle(verified).slug,
    transmission_title: transmissionTitle(nodes).slug,
    attested_families: attested,
    received_glow: received,
  };
}
