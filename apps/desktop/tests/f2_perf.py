#!/usr/bin/env python3
"""F2 performance and stability measurement.

Baseline first, budget second: these numbers are measured, not asserted from a
wish. Run against a live app via its control channel.

Usage:
  python3 f2_perf.py --binary target/debug/firstlight-desktop \
                     --idle-seconds 20 --stability-seconds 120
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import socket
import statistics
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
PYTHON = sys.executable


class App:
    def __init__(self, binary: Path, state: Path, fixtures: Path):
        self.binary = binary
        self.state = state
        self.fixtures = fixtures
        self.sock = state / "control.sock"
        self.proc = None

    def start(self):
        self.state.mkdir(parents=True, exist_ok=True)
        if self.sock.exists():
            self.sock.unlink()
        env = dict(os.environ)
        env["FIRSTLIGHT_STATE_DIR"] = str(self.state)
        env["FIRSTLIGHT_CONTROL_SOCKET"] = str(self.sock)
        env["FIRSTLIGHT_CODEX_DIR"] = str(self.fixtures)
        # The sandbox cannot write the login keychain; use the labelled dev backend
        # so the measurement is of the light point, not of a denied keychain.
        env["FIRSTLIGHT_IDENTITY_BACKEND"] = "file"
        self.proc = subprocess.Popen([str(self.binary)], env=env,
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.time() + 15
        while time.time() < deadline:
            if self.sock.exists():
                try:
                    self.call(cmd="report", timeout=5)
                    return
                except Exception:
                    pass
            time.sleep(0.15)
        raise RuntimeError("app did not start")

    def call(self, timeout=40, **cmd):
        s = socket.socket(socket.AF_UNIX)
        s.settimeout(timeout)
        s.connect(str(self.sock))
        s.sendall((json.dumps(cmd) + "\n").encode())
        buf = b""
        while True:
            c = s.recv(1 << 16)
            if not c:
                break
            buf += c
            if buf.endswith(b"\n"):
                break
        s.close()
        return json.loads(buf.decode())

    def stop(self):
        try:
            self.call(cmd="quit", timeout=3)
        except Exception:
            pass
        try:
            self.proc.wait(timeout=6)
        except Exception:
            self.proc.kill()
            self.proc.wait(timeout=3)
        time.sleep(0.4)


def alive_after_exit() -> bool:
    r = subprocess.run(["pgrep", "-f", "firstlight-desktop"], capture_output=True, text=True)
    return bool(r.stdout.strip())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default="target/debug/firstlight-desktop")
    ap.add_argument("--idle-seconds", type=int, default=20)
    ap.add_argument("--stability-seconds", type=int, default=90)
    ap.add_argument("--out", default="target/f2-perf.json")
    args = ap.parse_args()

    binary = (REPO / args.binary).resolve()
    fixtures = REPO / "target" / "fixtures-single"
    subprocess.run([PYTHON, str(HERE / "make_fixtures.py"), str(fixtures),
                    "--profile", "single"], check=True, capture_output=True)

    state = REPO / "target" / "perf-state"
    shutil.rmtree(state, ignore_errors=True)
    app = App(binary, state, fixtures)
    app.start()

    report = {}

    # ---- 1. idle cost with the light animating ----------------------------
    app.call(cmd="scan", enabled=True)
    time.sleep(2.0)
    samples = []
    elapsed = 0
    while elapsed < args.idle_seconds:
        s = app.call(cmd="perf", window_ms=2000)["perf"]
        samples.append(s)
        elapsed += 2
    rss_mb = [s["rss_bytes"] / (1024 * 1024) for s in samples]
    report["idle_animating"] = {
        "samples": len(samples),
        "cpu_pct_mean": round(statistics.mean(s["cpu_pct"] for s in samples), 3),
        "cpu_pct_median": round(statistics.median(s["cpu_pct"] for s in samples), 3),
        "cpu_pct_max": round(max(s["cpu_pct"] for s in samples), 3),
        "rss_mb_mean": round(statistics.mean(rss_mb), 1),
        "rss_mb_max": round(max(rss_mb), 1),
        "threads": samples[-1]["threads"],
    }

    # ---- 2. idle cost in quiet mode (animation stopped) -------------------
    app.call(cmd="quiet", enabled=True)
    time.sleep(2.0)
    quiet_samples = [app.call(cmd="perf", window_ms=2000)["perf"] for _ in range(max(3, args.idle_seconds // 5))]
    report["idle_quiet"] = {
        "samples": len(quiet_samples),
        "cpu_pct_mean": round(statistics.mean(s["cpu_pct"] for s in quiet_samples), 3),
        "cpu_pct_max": round(max(s["cpu_pct"] for s in quiet_samples), 3),
        "rss_mb_mean": round(statistics.mean(s["rss_bytes"] / (1024 * 1024) for s in quiet_samples), 1),
    }
    app.call(cmd="quiet", enabled=False)

    # ---- 3. scan cost ----------------------------------------------------
    scan_times = []
    for _ in range(5):
        out = app.call(cmd="scan", enabled=True)["outcome"]
        scan_times.append(out.get("duration_ms") or 0)
    report["scan"] = {
        "runs": len(scan_times),
        "duration_ms_min": min(scan_times),
        "duration_ms_max": max(scan_times),
        "duration_ms_median": statistics.median(scan_times),
        "note": "synthetic fixture; the real client database is larger",
    }

    # ---- 4. behaviour when the database read fails -----------------------
    app.call(cmd="eval", js="1")  # channel still alive
    good = app.call(cmd="light")["light"]
    report["failure_behaviour"] = {
        "detail": "checked by the e2e suite (scenario H/I): missing client data and "
                  "revoked permission both keep the app running with a clear message",
        "level_while_healthy": good["level"],
    }

    # ---- 5. stability over time -----------------------------------------
    t0 = time.time()
    series = []
    while time.time() - t0 < args.stability_seconds:
        s = app.call(cmd="perf", window_ms=3000)["perf"]
        series.append({
            "t": round(time.time() - t0, 1),
            "rss_mb": round(s["rss_bytes"] / (1024 * 1024), 1),
            "cpu_pct": round(s["cpu_pct"], 2),
            "threads": s["threads"],
        })
    rss_series = [p["rss_mb"] for p in series]
    report["stability"] = {
        "duration_s": args.stability_seconds,
        "rss_mb_first": rss_series[0],
        "rss_mb_last": rss_series[-1],
        "rss_mb_max": max(rss_series),
        "rss_growth_mb": round(rss_series[-1] - rss_series[0], 1),
        "alive_at_end": app.proc.poll() is None,
        "series": series,
    }

    # ---- 6. clean exit, no leftover process ------------------------------
    app.stop()
    report["exit"] = {"leftover_process": alive_after_exit()}

    (REPO / args.out).write_text(json.dumps(report, indent=2, ensure_ascii=False))

    def line(k, v):
        print(f"  {k:<28}{v}")

    print("F2 performance baseline")
    line("idle (animating) CPU mean", f"{report['idle_animating']['cpu_pct_mean']}%")
    line("idle (animating) CPU max", f"{report['idle_animating']['cpu_pct_max']}%")
    line("idle (animating) RSS mean", f"{report['idle_animating']['rss_mb_mean']} MB")
    line("idle (quiet) CPU mean", f"{report['idle_quiet']['cpu_pct_mean']}%")
    line("idle (quiet) CPU max", f"{report['idle_quiet']['cpu_pct_max']}%")
    line("idle (quiet) RSS mean", f"{report['idle_quiet']['rss_mb_mean']} MB")
    line("scan median", f"{report['scan']['duration_ms_median']} ms")
    line("scan max", f"{report['scan']['duration_ms_max']} ms")
    line(f"RSS over {args.stability_seconds}s", f"{report['stability']['rss_mb_first']} → {report['stability']['rss_mb_last']} MB "
                                               f"(growth {report['stability']['rss_growth_mb']} MB)")
    line("alive at end", report["stability"]["alive_at_end"])
    line("leftover process after quit", report["exit"]["leftover_process"])
    print(f"\n→ {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
