#!/usr/bin/env python3
"""F3.6 P0: prove the scan permission cannot be bypassed.

F3.5 shipped `settings.scan_enabled || true` in the transfer activation path, so a
transfer read the user's local client database with scanning switched off. Unit
tests assert the decision; this test asserts the stronger property — **zero reads
actually happened**, measured by a counter incremented at the point where the
process first touches client data.

Run: python3 apps/desktop/tests/f3_6_scan_auth.py
"""

from __future__ import annotations

import json
import os
import shutil
import socket
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
PYTHON = sys.executable


class App:
    def __init__(self, binary: Path, tag: str, fixtures: Path):
        self.binary = binary
        self.dir = REPO / "target" / f"f36-{tag}"
        self.sock = self.dir / "control.sock"
        self.fixtures = fixtures
        self.proc = None

    def start(self):
        self.dir.mkdir(parents=True, exist_ok=True)
        if self.sock.exists():
            self.sock.unlink()
        env = dict(os.environ)
        env["FIRSTLIGHT_STATE_DIR"] = str(self.dir)
        env["FIRSTLIGHT_CONTROL_SOCKET"] = str(self.sock)
        env["FIRSTLIGHT_CODEX_DIR"] = str(self.fixtures)
        env["FIRSTLIGHT_IDENTITY_BACKEND"] = "file"
        self.proc = subprocess.Popen([str(self.binary)], env=env,
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.time() + 20
        while time.time() < deadline:
            if self.sock.exists():
                try:
                    self.call(cmd="report", timeout=5)
                    return
                except Exception:
                    pass
            if self.proc.poll() is not None:
                raise RuntimeError(f"app exited {self.proc.returncode}")
            time.sleep(0.15)
        raise RuntimeError("app did not start")

    def stop(self):
        if not self.proc:
            return
        try:
            self.call(cmd="quit", timeout=2)
        except Exception:
            pass
        try:
            self.proc.wait(timeout=8)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait(timeout=3)
        self.proc = None

    def call(self, timeout=30, **cmd):
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

    def executions(self) -> int:
        return self.call(cmd="scan_stats")["executions"]

    def set_scan(self, enabled: bool):
        self.call(cmd="scan", enabled=enabled, dry_run=True)


def main() -> int:
    binary = REPO / "target" / "debug" / "firstlight-desktop"
    if not binary.exists():
        print(f"missing binary: {binary}", file=sys.stderr)
        return 2

    fixtures = REPO / "target" / "f36-fixtures"
    shutil.rmtree(fixtures, ignore_errors=True)
    subprocess.run([PYTHON, str(HERE / "make_fixtures.py"), str(fixtures), "--profile", "single"],
                   check=True, capture_output=True)
    shutil.rmtree(REPO / "target" / "f36-auth", ignore_errors=True)

    app = App(binary, "auth", fixtures)
    results = []
    try:
        app.start()

        # -- 1. permission off: a transfer must not read anything ----------
        base = app.executions()
        r = app.call(cmd="transfer_evidence", id="00000000-0000-4000-8000-000000000000")
        after = app.executions()
        ok = (after == base) and r.get("ok") is False
        results.append(("scan_disabled_blocks_transfer_scan", ok,
                        f"executions {base}→{after}; refused with: "
                        f"{(r.get('error') or '')[:48]}"))

        # -- 2. switch on, but no explicit confirmation --------------------
        app.call(cmd="scan", enabled=True, dry_run=True)
        base = app.executions()
        r = app.call(cmd="scan_now", confirmed=False)["result"]
        after = app.executions()
        ok = (after == base) and r.get("ok") is False and r.get("scanned") is False
        results.append(("scan_requires_explicit_consent", ok,
                        f"executions {base}→{after}; reason: {(r.get('reason') or '')[:40]}"))

        # -- 3. explicit confirmation runs exactly one read ----------------
        base = app.executions()
        r = app.call(cmd="scan_now", confirmed=True)["result"]
        after = app.executions()
        ok = (after == base + 1) and r.get("ok") is True
        results.append(("confirmed_scan_runs_exactly_once", ok,
                        f"executions {base}→{after}; status={r.get('status')}"))

        # -- 4. revoking permission stops the very next scan ---------------
        app.call(cmd="scan", enabled=False, dry_run=True)
        base = app.executions()
        r = app.call(cmd="scan_now", confirmed=True)["result"]
        after = app.executions()
        ok = (after == base) and r.get("ok") is False
        results.append(("permission_revoked_blocks_next_scan", ok,
                        f"executions {base}→{after}; reason: {(r.get('reason') or '')[:40]}"))

        # -- 5. activation with no stored evidence never scans -------------
        base = app.executions()
        r = app.call(cmd="transfer_evidence", id="00000000-0000-4000-8000-000000000001")
        after = app.executions()
        ok = (after == base) and r.get("ok") is False
        results.append(("activation_never_scans", ok,
                        f"executions {base}→{after}"))
    finally:
        app.stop()

    print(f"{'CASE':<40}{'RESULT':<8}DETAIL")
    failed = 0
    for name, ok, detail in results:
        print(f"{name:<40}{'PASS' if ok else 'FAIL':<8}{detail}")
        if not ok:
            failed += 1
    print(f"\n{len(results) - failed}/{len(results)} passed")
    out = REPO / "target" / "f36-scan-auth.json"
    out.write_text(json.dumps(
        {"generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
         "results": [{"case": n, "status": "PASS" if o else "FAIL", "detail": d}
                     for n, o, d in results]}, indent=2, ensure_ascii=False))
    print(f"→ {out}")
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
