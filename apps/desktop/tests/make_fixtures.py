#!/usr/bin/env python3
"""Build synthetic Codex databases for end-to-end tests.

Real client data must never be used to prove a feature works, and it must never
be committed. These fixtures reproduce the *shape* observed in F0 (metadata next
to content, model set before any call) with fully synthetic values.

Usage:
  python3 make_fixtures.py <out_dir> --profile single|double
"""

from __future__ import annotations

import argparse
import os
import sqlite3
import sys


SCHEMA_STATE = """
create table threads (
    id text primary key,
    model text,
    model_provider text,
    tokens_used integer,
    first_user_message text,
    preview text,
    title text,
    cwd text
);
"""

SCHEMA_HISTORY = """
create table thread_turns (thread_id text not null, status text not null);
create table thread_items (
    thread_id text not null,
    item_type text not null,
    item_json text
);
"""

SECRET = "FIXTURE_SECRET_CONTENT_DO_NOT_READ"


def add_thread(conn, tid, model, provider="openai"):
    conn.execute(
        "insert into threads (id, model, model_provider, tokens_used,"
        " first_user_message, preview, title, cwd)"
        " values (?,?,?,?,?,?,?,?)",
        (tid, model, provider, 1000, SECRET, SECRET, SECRET, "/fixture/private"),
    )


def add_items(conn, tid, types):
    for t in types:
        conn.execute(
            "insert into thread_items (thread_id, item_type, item_json) values (?,?,?)",
            (tid, t, SECRET),
        )


def add_turn(conn, tid, status="completed"):
    conn.execute("insert into thread_turns (thread_id, status) values (?,?)", (tid, status))


def build(out_dir: str, profile: str):
    os.makedirs(out_dir, exist_ok=True)
    state_path = os.path.join(out_dir, "state_9.sqlite")
    history_path = os.path.join(out_dir, "thread_history_9.sqlite")
    for p in (state_path, history_path):
        if os.path.exists(p):
            os.remove(p)

    state = sqlite3.connect(state_path)
    state.executescript(SCHEMA_STATE)
    history = sqlite3.connect(history_path)
    history.executescript(SCHEMA_HISTORY)

    # --- the F0 sample set: nine distinct strings, one real family ----------
    gpt_variants = [
        "gpt-6-sol",
        "gpt-5.6-sol",
        "gpt-5.6-terra",
        "gpt-6.1-sol",
        "gpt-6-astra",
        "chatgpt-web/gpt-6-sol",
        "chatgpt-web/gpt-5.6-sol",
    ]
    for i, model in enumerate(gpt_variants):
        tid = f"gpt-{i}"
        add_thread(state, tid, model)
        add_items(history, tid, ["userMessage", "reasoning", "agentMessage"])
        add_turn(history, tid)

    # an internal task label that produced plenty of output: must never count
    add_thread(state, "guardian-0", "codex-auto-review")
    add_items(history, "guardian-0", ["reasoning", "agentMessage", "commandExecution"])
    add_turn(history, "guardian-0")

    # a session created with a model but the model never produced anything
    add_thread(state, "idle-0", "gpt-6-sol")
    add_items(history, "idle-0", ["userMessage"])

    # empty model string
    add_thread(state, "empty-0", "")
    add_items(history, "empty-0", ["agentMessage"])

    # a model string nobody has registered
    add_thread(state, "mystery-0", "unregistered-model-zzz")
    add_items(history, "mystery-0", ["agentMessage"])

    if profile == "no-call":
        # Only records that must never light up: remove the countable ones.
        state.execute("delete from threads where id like 'gpt-%'")
        state.execute("delete from threads where id = 'empty-0'")
        state.execute("delete from threads where id = 'mystery-0'")
        history.execute("delete from thread_turns where thread_id like 'gpt-%'")
        history.execute("delete from thread_items where thread_id like 'gpt-%'")
        state.commit()
        history.commit()

    if profile == "double":
        # A second real family, with undeniable model output.
        add_thread(state, "claude-0", "claude-sonnet-4-5", "anthropic")
        add_items(history, "claude-0", ["userMessage", "agentMessage", "reasoning"])
        add_turn(history, "claude-0")

    state.commit()
    history.commit()
    state.close()
    history.close()

    print(f"wrote {state_path}")
    print(f"wrote {history_path}")
    print(f"profile={profile}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out_dir")
    ap.add_argument("--profile", default="single", choices=["single", "double", "no-call"])
    args = ap.parse_args()
    build(args.out_dir, args.profile)
    return 0


if __name__ == "__main__":
    sys.exit(main())
