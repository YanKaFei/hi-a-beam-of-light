// Database layer.
//
// Production is PostgreSQL; tests and local runs use PGlite, which is real
// Postgres compiled to WebAssembly. The same migration file is executed in both
// (ADR-003), so there is no "works in tests, breaks in prod" gap.

import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { PGlite } from "@electric-sql/pglite";

const HERE = dirname(fileURLToPath(import.meta.url));
export const MIGRATIONS = join(HERE, "..", "migrations");

/** Open a database and run every migration in order. Idempotent. */
export async function openDb({ dataDir = null } = {}) {
  const db = dataDir ? new PGlite(dataDir) : new PGlite();
  const files = ["001_init.sql", "002_credentials.sql", "003_activation.sql", "004_pairing.sql", "005_auth.sql"];
  for (const file of files) {
    const sql = await readFile(join(MIGRATIONS, file), "utf8");
    await db.exec(sql);
  }
  return wrap(db);
}

function wrap(db) {
  return {
    raw: db,
    /** Run one statement and return rows. */
    async all(sql, params = []) {
      const res = await db.query(sql, params);
      return res.rows ?? [];
    },
    async one(sql, params = []) {
      const rows = await this.all(sql, params);
      return rows[0] ?? null;
    },
    async run(sql, params = []) {
      const res = await db.query(sql, params);
      return { rowCount: res.affectedRows ?? 0, rows: res.rows ?? [] };
    },
    /** Run several statements atomically. */
    async tx(fn) {
      await db.exec("begin");
      try {
        const out = await fn(this);
        await db.exec("commit");
        return out;
      } catch (err) {
        await db.exec("rollback");
        throw err;
      }
    },
    async close() {
      await db.close();
    },
  };
}

/** RFC 4122 v4 UUID. Kept here so the schema needs no pgcrypto. */
export function uuid() {
  const b = crypto.getRandomValues(new Uint8Array(16));
  b[6] = (b[6] & 0x0f) | 0x40;
  b[8] = (b[8] & 0x3f) | 0x80;
  const h = [...b].map((x) => x.toString(16).padStart(2, "0")).join("");
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
}

/** Cryptographically secure opaque token, URL-safe. */
export function secureToken(bytes = 32) {
  const b = crypto.getRandomValues(new Uint8Array(bytes));
  return Buffer.from(b).toString("base64url");
}
