#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0081 unreclaimable-memory gate, per phase, control versus candidate, private nested session.

  b0081-gate.py run PAIRS CTL_BIN CAND_BIN CTL_SRC CAND_SRC

Alternating arms (control first on even pairs). Phases: idle, launcher open, launcher closed, dashboard open,
dashboard closed (12 s after) and dashboard settled (about a minute after). Per phase: median of five smaps_rollup samples of the always-on shell, reported as
unreclaimable (Pss_Anon + Pss_Shmem + SwapPss; DRM is 0 on the CPU renderer), PSS and clean file-backed (Pss_File).
Also the open-to-first-PANEL_PRESENTED latency. Gate: candidate - control <= +0.5 MiB unreclaimable per phase.
SMOKE_ROOT as for b0047-frame.py. CPU renderer, no daemon link. Never the live session."""
import importlib.util, json, pathlib, re, statistics, subprocess, sys, time

HERE = pathlib.Path(__file__).parent
spec = importlib.util.spec_from_file_location("b0065", HERE / "b0065-smaps.py")
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
b = m.b
PHASES = ("idle", "launcher_open", "launcher_closed", "dashboard_open", "dashboard_closed", "dashboard_settled")


def sample(pid):
    rows = []
    for _ in range(5):
        r = b.rollup(pid)
        rows.append({"unreclaimable": r["Pss_Anon"] + r["Pss_Shmem"] + r["SwapPss"], "pss": r["Pss"],
                     "clean_file": r["Pss_File"], "anon": r["Pss_Anon"]})
        time.sleep(1)
    return {k: statistics.median(x[k] for x in rows) for k in rows[0]}


def arm_run(label, binary, src, tag):
    arm = b.Arm(label, binary, pathlib.Path(src))

    def ipc(*a):
        return subprocess.run([str(b.ROOT / binary), "ipc", "--socket", arm.cmdsock, *a], env=dict(arm.env, BEAD_PRIVATE_INPUT_PROOF="1"),
                              capture_output=True, text=True, cwd=str(src), timeout=10)

    def open_ms(*cmd):
        log = lambda: (arm.raw / "shell.log").read_text(errors="replace")
        off, t0 = len(log()), time.monotonic_ns()
        ipc(*cmd)
        end = time.monotonic() + 5
        while time.monotonic() < end:
            for mm in re.finditer(r"PANEL_PRESENTED monotonic_ns=(\d+)", log()[off:]):
                if int(mm.group(1)) >= t0:
                    return (int(mm.group(1)) - t0) / 1e6
            time.sleep(.01)
        return None

    try:
        arm.start(tag)
        pid = arm.shell.pid
        res = {"label": label, "binary": binary}
        time.sleep(20)
        res["idle"] = sample(pid)
        res["launcher_open_ms"] = open_ms("--json", "bead.panel", "open", "launcher")
        time.sleep(3)
        res["launcher_open"] = sample(pid)
        ipc("--json", "bead.panel", "close")
        time.sleep(12)
        res["launcher_closed"] = sample(pid)
        res["dashboard_open_ms"] = open_ms("drawers", "toggle", "dashboard")
        time.sleep(3)
        res["dashboard_open"] = sample(pid)
        ipc("drawers", "toggle", "dashboard")
        time.sleep(12)
        res["dashboard_closed"] = sample(pid)
        time.sleep(45)  # the laptop gate settles 60 s after a close; this is the same state a minute later
        res["dashboard_settled"] = sample(pid)
        (arm.raw / "result.json").write_text(json.dumps(res, indent=1))
        return res
    finally:
        arm.finish()


def run(pairs, ctl, cand, ctl_src, cand_src):
    runs = []
    for i in range(pairs):
        arms = [("control", ctl, ctl_src), ("candidate", cand, cand_src)]
        for label, binary, src in arms if i % 2 == 0 else arms[::-1]:
            runs.append(arm_run(label, binary, src, f"b0081-{i}-{label}"))
    med = lambda label, f: statistics.median(f(r) for r in runs if r["label"] == label)
    rep = {"renderer": "cpu (tiny-skia), no daemon, nested niri in headless labwc", "pairs": pairs}
    for ph in PHASES:
        row = {}
        for key in ("unreclaimable", "pss", "clean_file", "anon"):
            c, d = med("control", lambda r: r[ph][key]), med("candidate", lambda r: r[ph][key])
            row[key] = {"control": round(c, 2), "candidate": round(d, 2), "delta": round(d - c, 2)}
        row["gate"] = "PASS" if row["unreclaimable"]["delta"] <= 0.5 else "FAIL"
        rep[ph] = row
    for ph in ("launcher_open", "dashboard_open"):
        v = {l: [r[ph + "_ms"] for r in runs if r["label"] == l and r[ph + "_ms"] is not None] for l in ("control", "candidate")}
        rep[ph]["open_ms"] = {l: {"median": round(statistics.median(x), 2), "all": [round(y, 1) for y in x]} for l, x in v.items() if x}
    (b.OUT / "b0081.json").write_text(json.dumps({"summary": rep, "runs": runs}, indent=1))
    print(json.dumps(rep, indent=1))


if __name__ == "__main__" and sys.argv[1] == "run":
    run(int(sys.argv[2]), *sys.argv[3:7])
