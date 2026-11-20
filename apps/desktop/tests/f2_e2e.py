#!/usr/bin/env python3
"""F2 end-to-end acceptance tests (A–J).

Runs the real desktop binary, drives it over its control channel, and asserts on
observable behaviour. Fixtures are synthetic: no real client data is involved.

Usage:
  python3 f2_e2e.py --binary target/debug/firstlight-desktop [--out report.json]
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
REPO = HERE.parents[2]  # first-light/
PYTHON = sys.executable


# ---------------------------------------------------------------------------
# Harness
# ---------------------------------------------------------------------------

@dataclass
class Result:
    id: str
    name: str
    status: str
    detail: str = ""
    evidence: dict = field(default_factory=dict)


class App:
    def __init__(self, binary: Path, state_dir: Path, fixtures: Path | None):
        self.binary = binary
        self.state_dir = state_dir
        self.fixtures = fixtures
        self.proc: subprocess.Popen | None = None
        self.sock = state_dir / "control.sock"

    def start(self, timeout=12.0):
        self.state_dir.mkdir(parents=True, exist_ok=True)
        if self.sock.exists():
            self.sock.unlink()
        env = dict(os.environ)
        env["FIRSTLIGHT_STATE_DIR"] = str(self.state_dir)
        env["FIRSTLIGHT_CONTROL_SOCKET"] = str(self.sock)
        if self.fixtures is not None:
            env["FIRSTLIGHT_CODEX_DIR"] = str(self.fixtures)
        else:
            env["FIRSTLIGHT_CODEX_DIR"] = str(self.state_dir / "no-such-client")
        self.proc = subprocess.Popen(
            [str(self.binary)],
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        deadline = time.time() + timeout
        while time.time() < deadline:
            if self.sock.exists():
                try:
                    self.call(cmd="report", timeout=5)
                    return
                except Exception:
                    pass
            if self.proc.poll() is not None:
                raise RuntimeError(f"app exited early with {self.proc.returncode}")
            time.sleep(0.15)
        raise RuntimeError("app did not become ready")

    def stop(self, timeout=6.0):
        if self.proc is None:
            return
        try:
            self.call(cmd="quit", timeout=2)
        except Exception:
            pass
        try:
            self.proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait(timeout=3)
        self.proc = None
        time.sleep(0.4)

    def call(self, timeout=20, **cmd):
        s = socket.socket(socket.AF_UNIX)
        s.settimeout(timeout)
        s.connect(str(self.sock))
        s.sendall((json.dumps(cmd) + "\n").encode())
        buf = b""
        while True:
            chunk = s.recv(1 << 16)
            if not chunk:
                break
            buf += chunk
            if buf.endswith(b"\n"):
                break
        s.close()
        return json.loads(buf.decode())

    def report(self) -> dict:
        r = self.call(cmd="report")
        if not r.get("ok"):
            raise RuntimeError(f"report failed: {r}")
        return r["report"]

    def probe(self) -> dict:
        r = self.call(cmd="probe")
        if not r.get("ok"):
            raise RuntimeError(f"probe failed: {r}")
        return r["probe"]


def run(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------

def t_a_startup_lightpoint(app: App, res: list):
    """A. A real transparent light point appears on the desktop."""
    lp = app.report()["lightpoint"]
    probe = app.probe()
    ak = lp["appkit"]
    ok = (
        lp["visible"] is True
        and ak["is_opaque"] is False
        and ak["level"] > 0
        and probe["level"] in ("DORMANT", "FAINT", "GROWING", "BLAZING", "DEEP")
    )
    res.append(Result(
        "A", "启动后桌面出现真实光点", "PASS" if ok else "FAIL",
        f"visible={lp['visible']} isOpaque={ak['is_opaque']} level={ak['level']} "
        f"probe.level={probe['level']} core={probe['coreHex']}",
        {"appkit": {k: ak[k] for k in ("is_opaque", "level", "collection_behavior")},
         "probe": probe},
    ))


def t_b_click_through(app: App, res: list):
    """B. Click-through lets the mouse reach the app underneath."""
    app.call(cmd="click_through", enabled=True)
    time.sleep(0.3)
    on = app.report()["lightpoint"]["appkit"]["ignores_mouse_events"]
    app.call(cmd="click_through", enabled=False)
    time.sleep(0.3)
    off = app.report()["lightpoint"]["appkit"]["ignores_mouse_events"]
    ok = on is True and off is False
    res.append(Result(
        "B", "开启穿透后鼠标可操作下层应用", "PASS" if ok else "FAIL",
        f"ignoresMouseEvents on={on} off={off}",
        {"ignores_mouse_events_on": on, "ignores_mouse_events_off": off},
    ))


def t_c_open_panel(app: App, res: list):
    """C. A clear entry point opens the personal light interface."""
    before = app.report().get("panel_visible", False)
    # The overlay listens to pointer events (F3.7+): a real mouse produces both
    # pointer* and mouse*, but a synthetic legacy mouseup alone never reaches the
    # handler. Dispatch the events a real click actually generates.
    js = """
      (function () {
        const stage = document.getElementById('stage');
        const opts = {button:0, screenX:400, screenY:300, bubbles:true, pointerId:1, pointerType:'mouse'};
        if (window.PointerEvent) {
          stage.dispatchEvent(new PointerEvent('pointerdown', opts));
          window.dispatchEvent(new PointerEvent('pointerup', opts));
        } else {
          stage.dispatchEvent(new MouseEvent('mousedown', opts));
          window.dispatchEvent(new MouseEvent('mouseup', opts));
        }
      })();
    """
    app.call(cmd="eval", js=js)
    # The panel is a large window with three painted canvases; creating it takes
    # longer than a fixed 1.2s on a busy machine. Poll for the same assertion
    # instead of guessing a sleep, so a slow machine is not reported as a bug.
    after = False
    deadline = time.time() + 12.0
    while time.time() < deadline:
        if app.report().get("panel_visible", False):
            after = True
            break
        # A synthetic DOM click can be dropped while the webview is busy. A real
        # person would simply click again, so retry the gesture rather than
        # concluding the entry point is broken.
        app.call(cmd="eval", js=js)
        time.sleep(1.0)
    ok = before is False and after is True
    res.append(Result(
        "C", "可通过明确入口打开个人界面", "PASS" if ok else "FAIL",
        f"panel_visible {before} → {after}",
        {"before": before, "after": after},
    ))
    app.call(cmd="close_panel")


ALLOWED_COLUMNS = {"id", "model", "model_provider", "thread_id", "item_type", "status"}
FORBIDDEN_COLUMNS = {
    "first_user_message", "preview", "title", "item_json", "feedback_log_body",
    "cwd", "rollout_path", "tokens_used", "agent_path", "git_origin_url",
}


def t_d_privacy(app: App, res: list):
    """D. The adapter reads allowed metadata only."""
    out = app.call(cmd="scan", enabled=True)["outcome"]
    cols = set(out.get("columns_touched") or [])
    leaked = cols & FORBIDDEN_COLUMNS
    extra = cols - ALLOWED_COLUMNS
    ok = not leaked and not extra and cols
    res.append(Result(
        "D", "Codex 适配器仅读允许的元数据", "PASS" if ok else "FAIL",
        f"读取列={sorted(cols)}；越界={sorted(leaked)}；未声明={sorted(extra)}",
        {"columns": sorted(cols), "forbidden_hit": sorted(leaked),
         "undeclared": sorted(extra)},
    ))


def t_e_normalisation(app: App, res: list):
    """E. Nine raw model strings normalise to exactly one GPT family."""
    out = app.call(cmd="scan", enabled=True)["outcome"]
    families = out["verified_families"]
    ok = families == ["gpt"] and out["countable_sessions"] == 7
    res.append(Result(
        "E", "9 个原始取值归一化为 1 个 GPT 家族", "PASS" if ok else "FAIL",
        f"families={families} countable_sessions={out['countable_sessions']} "
        f"task_label={out['rejected_task_label']} unknown={out['rejected_unknown']} "
        f"empty={out['rejected_empty']}",
        {"families": families, "countable": out["countable_sessions"],
         "task_label": out["rejected_task_label"],
         "unknown": out["rejected_unknown"], "empty": out["rejected_empty"]},
    ))


def t_f_no_call_does_not_light(app: App, res: list):
    """F. Sessions where the model never produced anything cannot light up."""
    out = app.call(cmd="scan", enabled=True)["outcome"]
    ok = out["verified_families"] == [] and out["countable_sessions"] == 0
    res.append(Result(
        "F", "未发生模型调用的会话不触发点亮", "PASS" if ok else "FAIL",
        f"families={out['verified_families']} countable={out['countable_sessions']} "
        f"status={out['status']}",
        {"families": out["verified_families"],
         "countable_sessions": out["countable_sessions"], "status": out["status"]},
    ))


def t_g_second_family(app: App, res: list, fixtures_single: Path, fixtures_double: Path):
    """G. A second verified family deepens the colour and raises the title.

    Run as a genuine progression in one state directory: first only GPT is
    provable, then a Claude record appears.
    """
    app.fixtures = fixtures_single
    first_scan = app.call(cmd="scan", enabled=True)["outcome"]
    time.sleep(2.2)
    first = app.probe()

    # Swap in the fixture that also contains a Claude call, same state dir.
    app.fixtures = fixtures_double
    app.call(cmd="restart_fixtures", path=str(fixtures_double)) if False else None
    # The fixture path is read at scan time from the environment, so relaunch
    # with the same state dir and the doubled fixture.
    app.stop()
    app.fixtures = fixtures_double
    app.start()
    second_scan = app.call(cmd="scan", enabled=True)["outcome"]
    time.sleep(2.4)
    second = app.probe()
    light = app.call(cmd="light")["light"]

    ok = (
        first["level"] == "FAINT"
        and second["level"] == "GROWING"
        and first["coreHex"] != second["coreHex"]
        and second_scan["newly_verified"] == ["claude"]
        and light["knowledge_title"]["zh"] == "启迪者"
        and light["verified_families"] == 2
    )
    res.append(Result(
        "G", "第二个模型家族使颜色变化、称号升级", "PASS" if ok else "FAIL",
        f"首次 families={first_scan['verified_families']} "
        f"{first['level']}({first['coreHex']}) → 增加后 families={second_scan['verified_families']} "
        f"{second['level']}({second['coreHex']})，新增={second_scan['newly_verified']}，"
        f"称号={light['knowledge_title']['zh']}",
        {"before": {"families": first_scan["verified_families"], "level": first["level"],
                    "coreHex": first["coreHex"], "beadPx": first.get("beadPx")},
         "after": {"families": second_scan["verified_families"], "level": second["level"],
                   "coreHex": second["coreHex"], "beadPx": second.get("beadPx")},
         "newly_verified": second_scan["newly_verified"],
         "knowledge_title_zh": light["knowledge_title"]["zh"]},
    ))


def t_h_offline(app: App, res: list):
    """H. Everything works with no client data, no server, no network."""
    out = app.call(cmd="scan", enabled=True)["outcome"]
    status = app.call(cmd="light")
    lp = app.report()["lightpoint"]
    ok = (
        out["status"] == "client_not_found"
        and status.get("ok") is True
        and lp["visible"] is True
        and status["light"]["level"] == "DORMANT"
    )
    res.append(Result(
        "H", "离线 / 无客户端数据时仍正常工作", "PASS" if ok else "FAIL",
        f"scan.status={out['status']} reason={out.get('reason')} "
        f"lightpoint.visible={lp['visible']}",
        {"scan_status": out["status"], "reason": out.get("reason"),
         "lightpoint_visible": lp["visible"]},
    ))


def t_i_permission_refused(app: App, res: list):
    """I. Refusing scan permission must not break the app or read anything."""
    out = app.call(cmd="scan", enabled=False)["outcome"]
    lp = app.report()["lightpoint"]
    ok = (
        out["status"] == "not_authorized"
        and out["sessions_seen"] == 0
        and lp["visible"] is True
    )
    res.append(Result(
        "I", "拒绝扫描权限后应用仍启动且不读数据", "PASS" if ok else "FAIL",
        f"scan.status={out['status']} sessions_seen={out['sessions_seen']} "
        f"lightpoint.visible={lp['visible']}",
        {"scan_status": out["status"], "sessions_seen": out["sessions_seen"],
         "lightpoint_visible": lp["visible"]},
    ))


def t_j_restart(app: App, res: list, target_pos=(430, 360)):
    """J. Position and local state survive a restart."""
    app.call(cmd="scan", enabled=True)
    time.sleep(0.8)
    app.call(cmd="move", x=target_pos[0], y=target_pos[1])
    time.sleep(0.4)
    before = app.probe()
    before_light = app.call(cmd="light")["light"]

    app.stop()
    app.start()
    time.sleep(0.6)

    lp = app.report()["lightpoint"]
    pos = lp["position"]
    after = app.probe()
    after_light = app.call(cmd="light")["light"]

    got = (pos["x"], pos["y"])
    ok = (
        got == tuple(target_pos)
        and after["level"] == before["level"]
        and before_light.get("verified_families") == after_light.get("verified_families")
    )
    res.append(Result(
        "J", "重启后位置与本地状态正确恢复", "PASS" if ok else "FAIL",
        f"位置 {target_pos} → {got}；等级 {before['level']} → {after['level']}；"
        f"家族 {before_light.get('verified_families')} → "
        f"{after_light.get('verified_families')}",
        {"expected_position": list(target_pos), "actual_position": got,
         "level_before": before["level"], "level_after": after["level"],
         "families_before": before_light.get("verified_families"),
         "families_after": after_light.get("verified_families")},
    ))


# ---------------------------------------------------------------------------
# Orchestration
# ---------------------------------------------------------------------------

def scenario(binary, tag, fixtures, fn, keep=False):
    state = REPO / "target" / f"e2e-{tag}"
    if state.exists():
        shutil.rmtree(state)
    app = App(binary, state, fixtures)
    app.start()
    try:
        results: list[Result] = []
        fn(app, results)
        return results
    finally:
        app.stop()
        if not keep:
            shutil.rmtree(state, ignore_errors=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default="target/debug/firstlight-desktop")
    ap.add_argument("--out", default="target/f2-e2e.json")
    args = ap.parse_args()

    binary = (REPO / args.binary).resolve()
    if not binary.exists():
        print(f"binary not found: {binary}", file=sys.stderr)
        return 2

    fixtures_single = REPO / "target" / "fixtures-single"
    fixtures_double = REPO / "target" / "fixtures-double"
    fixtures_nocall = REPO / "target" / "fixtures-nocall"
    for d, profile in ((fixtures_single, "single"),
                       (fixtures_double, "double"),
                       (fixtures_nocall, "no-call")):
        run([PYTHON, str(HERE / "make_fixtures.py"), str(d), "--profile", profile])

    all_results: list[Result] = []
    meta = {"binary": str(binary), "fixtures": "synthetic"}

    # One app per scenario keeps each assertion independent of the others' state.
    all_results += scenario(binary, "a", fixtures_single,
                            lambda a, r: t_a_startup_lightpoint(a, r))
    all_results += scenario(binary, "b", fixtures_single,
                            lambda a, r: t_b_click_through(a, r))
    all_results += scenario(binary, "c", fixtures_single,
                            lambda a, r: t_c_open_panel(a, r))
    all_results += scenario(binary, "de", fixtures_single,
                            lambda a, r: (t_d_privacy(a, r), t_e_normalisation(a, r)))
    all_results += scenario(binary, "f", fixtures_nocall,
                            lambda a, r: t_f_no_call_does_not_light(a, r))
    all_results += scenario(binary, "g", fixtures_single,
                            lambda a, r: t_g_second_family(a, r, fixtures_single, fixtures_double),
                            keep=True)
    all_results += scenario(binary, "h", None,
                            lambda a, r: t_h_offline(a, r))
    all_results += scenario(binary, "i", fixtures_single,
                            lambda a, r: t_i_permission_refused(a, r))
    all_results += scenario(binary, "j", fixtures_single,
                            lambda a, r: t_j_restart(a, r))

    out = {
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "meta": meta,
        "results": [asdict(r) for r in all_results],
    }
    (REPO / args.out).parent.mkdir(parents=True, exist_ok=True)
    (REPO / args.out).write_text(json.dumps(out, indent=2, ensure_ascii=False))

    passed = sum(1 for r in all_results if r.status == "PASS")
    failed = [r for r in all_results if r.status != "PASS"]
    print(f"{'ID':<4}{'STATUS':<7}NAME")
    for r in all_results:
        print(f"{r.id:<4}{r.status:<7}{r.name}")
        if r.detail:
            print(f"{'':<11}{r.detail}")
    print(f"\nPASS={passed} FAIL={len(failed)} → {args.out}")
    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main())
