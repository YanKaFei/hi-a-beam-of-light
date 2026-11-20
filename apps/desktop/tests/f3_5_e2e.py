#!/usr/bin/env python3
"""F3.5 acceptance matrix A01–A20 for the desktop transfer loop.

Runs two real desktop instances against a real local API server:
  A = the helper (sends light)
  B = the receiver (proves usage with its own local adapter, then activates)

Everything here is real except the *source* of B's usage: it comes from a
synthetic Codex fixture rather than a real model call. The report must say so.

Usage:
  python3 f3_5_e2e.py --binary target/debug/firstlight-desktop
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import socket
import subprocess
import sys
import time
from dataclasses import dataclass, field, asdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
PYTHON = sys.executable
API_DIR = REPO / "services" / "api"
PORT = 8799
BASE = f"http://127.0.0.1:{PORT}"


@dataclass
class Result:
    id: str
    name: str
    status: str
    detail: str = ""
    evidence: dict = field(default_factory=dict)


class App:
    """One desktop instance with its own state dir, identity, and control socket."""

    def __init__(self, binary: Path, tag: str, fixtures: Path):
        self.binary = binary
        self.tag = tag
        self.dir = REPO / "target" / f"f35-{tag}"
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
        env["FIRSTLIGHT_API_BASE"] = BASE
        # The sandbox cannot write the login keychain, so the explicitly-labelled
        # development backend is used here. The keychain path is exercised by A04.
        env["FIRSTLIGHT_IDENTITY_BACKEND"] = "file"
        self.proc = subprocess.Popen(
            [str(self.binary)], env=env,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        deadline = time.time() + 20
        while time.time() < deadline:
            if self.sock.exists():
                try:
                    self.call(cmd="report", timeout=5)
                    return
                except Exception:
                    pass
            if self.proc.poll() is not None:
                raise RuntimeError(f"{self.tag} exited with {self.proc.returncode}")
            time.sleep(0.15)
        raise RuntimeError(f"{self.tag} did not become ready")

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
        time.sleep(0.3)

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

    def identity(self) -> dict:
        return self.call(cmd="identity").get("identity", {})

    def transfer(self) -> dict:
        return self.call(cmd="transfer")["snapshot"]


def start_server():
    env = dict(os.environ)
    env["FL_PORT"] = str(PORT)
    env["FL_PUBLIC_BASE_URL"] = BASE
    proc = subprocess.Popen(
        ["node", "src/serve.mjs"], cwd=str(API_DIR), env=env,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    import urllib.request
    deadline = time.time() + 20
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(f"{BASE}/v1/field/summary", timeout=2) as r:
                if r.status == 200:
                    return proc
        except Exception:
            pass
        time.sleep(0.2)
    proc.kill()
    raise RuntimeError("api server did not start")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default="target/debug/firstlight-desktop")
    ap.add_argument("--out", default="target/f35-e2e.json")
    args = ap.parse_args()

    binary = (REPO / args.binary).resolve()
    if not binary.exists():
        print(f"binary not found: {binary}", file=sys.stderr)
        return 2

    fixtures = REPO / "target" / "f35-fixtures"
    subprocess.run([PYTHON, str(HERE / "make_fixtures.py"), str(fixtures), "--profile", "single"],
                   check=True, capture_output=True)

    for d in ("f35-A", "f35-B"):
        shutil.rmtree(REPO / "target" / d, ignore_errors=True)

    server = start_server()
    results: list[Result] = []
    a = App(binary, "A", fixtures)
    b = App(binary, "B", fixtures)

    try:
        # ---- A01 / A02 ------------------------------------------------
        a.start()
        ida1 = a.identity()
        a.stop()
        a.start()
        ida2 = a.identity()
        ok = ida1.get("ok") and ida2.get("ok") and ida1["anon_id"] == ida2["anon_id"]
        results.append(Result(
            "A01/A02", "首次运行生成光种；重启后公钥身份完全相同",
            "PASS" if ok else "FAIL",
            f"anon_id {ida1.get('anon_id')} → {ida2.get('anon_id')}；"
            f"backend={ida1.get('backend')}",
            {"first": ida1.get("anon_id"), "second": ida2.get("anon_id"),
             "backend": ida1.get("backend")},
        ))

        # ---- A05 recovery --------------------------------------------
        rec = a.call(cmd="recovery_export")["code"]
        restored = a.call(cmd="recovery_import", code=rec, allowReplace=False)
        ok = restored.get("ok") and restored.get("anon_id") == ida1["anon_id"]
        results.append(Result(
            "A05", "恢复凭据可以恢复同一身份", "PASS" if ok else "FAIL",
            f"恢复后 anon_id={restored.get('anon_id')}",
            {"restored": restored.get("anon_id")},
        ))

        # ---- A06 bad recovery ---------------------------------------
        bad = a.call(cmd="recovery_import", code="FL1-NOTAREALCODE", allowReplace=True)
        ok = bad.get("ok") is False
        results.append(Result(
            "A06", "错误恢复凭据被拒绝", "PASS" if ok else "FAIL",
            f"error={bad.get('error')}", {"error": bad.get("error")},
        ))

        # ---- A07 server registration ---------------------------------
        api = a.call(cmd="api_register")
        ok = api.get("ok") is True
        results.append(Result(
            "A07", "桌面 App 用本地光种签名并被服务端验签", "PASS" if ok else "FAIL",
            f"connected={api.get('connected')} registered={api.get('registered')}",
            {"api": api},
        ))

        # ---- A08 invitation ------------------------------------------
        inv = a.call(cmd="invite")
        ok = inv.get("ok") and inv.get("token")
        results.append(Result(
            "A08", "桌面 App 发起真实邀请", "PASS" if ok else "FAIL",
            f"id={inv.get('id')} url={inv.get('receive_url')} err={inv.get('error')}",
            {"id": inv.get("id"), "receive_url": inv.get("receive_url")},
        ))

        # ---- A09 receive page ----------------------------------------
        import urllib.request
        if not inv.get("receive_url"):
            results.append(Result("A09", "Web 接收端可以打开二维码邀请", "FAIL",
                                  f"没有拿到邀请链接：{inv.get('error')}"))
            raise SystemExit(1)
        page = urllib.request.urlopen(f"{inv['receive_url']}", timeout=5).read().decode()
        ok = "你收到了一束光" in page
        results.append(Result(
            "A09", "Web 接收端可以打开二维码邀请", "PASS" if ok else "FAIL",
            f"receive page {len(page)} bytes", {"has_title": ok},
        ))

        # ---- B receives ----------------------------------------------
        b.start()
        # B must register its own light seed with the server before it can sign
        # an accept. This is the multi-device path, not a shortcut.
        reg_b = b.call(cmd="api_register")
        acc = b.call(cmd="accept", token=inv["token"])
        results.append(Result(
            "A09b", "第二个光种接收这束光", "PASS" if acc.get("ok") else "FAIL",
            f"state={acc.get('state')} register={reg_b.get('ok')} "
            f"error={acc.get('error')}", {"accept": acc, "register": reg_b},
        ))

        # ---- A10 mutual confirmation ---------------------------------
        conf = a.call(cmd="confirm", id=inv["id"])
        ok = conf.get("ok") and conf.get("state") == "LEARNING"
        results.append(Result(
            "A10", "双方确认后进入等待激活状态", "PASS" if ok else "FAIL",
            f"state={conf.get('state')} effects={conf.get('effects')}",
            {"confirm": conf},
        ))

        # ---- A11 no evidence yet => zero counted ----------------------
        paths = a.call(cmd="paths")["paths"]
        ok = paths["transmitted_nodes"] == 0
        results.append(Result(
            "A11", "没有真实使用证据时传播计数仍为零", "PASS" if ok else "FAIL",
            f"transmitted_nodes={paths['transmitted_nodes']} "
            f"outgoing={[e['state'] for e in paths['outgoing']]}",
            {"transmitted_nodes": paths["transmitted_nodes"]},
        ))

        # ---- A12 activation with the receiver's own local evidence ----
        # The receiver must have produced this evidence deliberately. Activation
        # never scans (F3.6 P0): B granted permission and scanned here, and the
        # transfer only consumes what that scan proved.
        b.call(cmd="scan", enabled=True)
        done = b.call(cmd="complete", id=inv["id"])
        ok = done.get("ok") is True
        results.append(Result(
            "A12", "接收者提供合格证据后传播完成激活", "PASS" if ok else "FAIL",
            f"family={done.get('family')} server={json.dumps(done.get('server'))[:120]}",
            {"complete": done},
        ))

        # ---- A13 echo reaches the helper -----------------------------
        deadline = time.time() + 12
        echo_seen = False
        snap = {}
        while time.time() < deadline:
            snap = a.transfer()
            if snap.get("phase") == "acknowledged":
                echo_seen = True
                break
            time.sleep(0.4)
        results.append(Result(
            "A13", "发送者通过 SSE 收到回响，光点播放一次光波",
            "PASS" if echo_seen else "FAIL",
            f"phase={snap.get('phase')}", {"phase": snap.get("phase")},
        ))

        # ---- A14 no duplicate activation ----------------------------
        again = b.call(cmd="complete", id=inv["id"])
        paths2 = a.call(cmd="paths")["paths"]
        ok = paths2["transmitted_nodes"] == 1
        results.append(Result(
            "A14", "重复提交不会重复激活", "PASS" if ok else "FAIL",
            f"transmitted_nodes={paths2['transmitted_nodes']}（重放返回 "
            f"{json.dumps(again.get('server'))[:80]}）",
            {"transmitted_nodes": paths2["transmitted_nodes"],
             "replay": again.get("server")},
        ))

        # ---- A15 light path -----------------------------------------
        paths3 = a.call(cmd="paths")["paths"]
        e = paths3["outgoing"][0] if paths3["outgoing"] else {}
        ok = e.get("state") == "ACTIVATED" and e.get("kind") in ("FIRST_LIGHT", "NEW_DISCOVERY")
        results.append(Result(
            "A15", "个人光路正确展示真实传播状态", "PASS" if ok else "FAIL",
            f"kind={e.get('kind')} state={e.get('state')} "
            f"echoes={len(paths3.get('echoes') or [])}",
            {"edge": e, "echoes": len(paths3.get("echoes") or [])},
        ))

        # ---- A16 offline still works --------------------------------
        a.stop()
        env_offline = dict(os.environ)
        env_offline["FIRSTLIGHT_STATE_DIR"] = str(a.dir)
        env_offline["FIRSTLIGHT_CONTROL_SOCKET"] = str(a.sock)
        env_offline["FIRSTLIGHT_IDENTITY_BACKEND"] = "file"
        env_offline["FIRSTLIGHT_API_BASE"] = "http://127.0.0.1:1"
        if a.sock.exists():
            a.sock.unlink()
        a.proc = subprocess.Popen([str(binary)], env=env_offline,
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.time() + 20
        while time.time() < deadline and not a.sock.exists():
            time.sleep(0.15)
        time.sleep(1.0)
        # The light point must still render and still report a light state with
        # no server reachable. The identity is loaded from the local store.
        try:
            probe = a.call(cmd="probe", timeout=15).get("probe", {})
            light = a.call(cmd="light", timeout=15).get("light", {})
            visible = probe.get("ready") is True and probe.get("level") is not None
            ok = visible and light.get("level") is not None
            detail = (f"probe.ready={probe.get('ready')} level={probe.get('level')} "
                      f"core={probe.get('coreHex')} light.level={light.get('level')}")
            evidence = {"probe": probe.get("level"), "light": light.get("level")}
        except Exception as e:
            ok = False
            detail = f"offline probe failed: {e}"
            evidence = {"error": str(e)}
        results.append(Result(
            "A16", "关闭网络后桌面个人光点仍然正常工作", "PASS" if ok else "FAIL",
            detail, evidence,
        ))
        a.stop()

    finally:
        try:
            a.stop()
        except Exception:
            pass
        try:
            b.stop()
        except Exception:
            pass
        server.terminate()
        try:
            server.wait(timeout=5)
        except Exception:
            server.kill()

    out = {"generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
           "results": [asdict(r) for r in results]}
    (REPO / args.out).parent.mkdir(parents=True, exist_ok=True)
    (REPO / args.out).write_text(json.dumps(out, indent=2, ensure_ascii=False))

    passed = sum(1 for r in results if r.status == "PASS")
    failed = [r for r in results if r.status != "PASS"]
    print(f"{'ID':<10}{'STATUS':<7}NAME")
    for r in results:
        print(f"{r.id:<10}{r.status:<7}{r.name}")
        if r.detail:
            print(f"{'':<18}{r.detail}")
    print(f"\nPASS={passed} FAIL={len(failed)} → {args.out}")
    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main())
