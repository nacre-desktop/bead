#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0085 dashboard-close anonymous transient: 1 s time series of unreclaimable memory after a close, alternating arms.

  b0085-transient.py run RUNS BIN_A SRC_A [BIN_B SRC_B]

Per run (private nested session as b0081-gate.py: headless labwc > niri > shell, CPU renderer, no daemon): 20 s idle, dashboard open for 3 s, close,
then Pss_Anon + Pss_Shmem + SwapPss every second for 60 s together with the close-path log lines (PANEL_IDLE_RELEASED, PANEL_IDLE_TRIM,
SHM_POOL_COMPACT, ...). smaps of the largest anonymous mappings are kept at idle, +5 s, +12 s and +60 s. Output: SMOKE_ROOT/out/b0085-transient.json.
Arms alternate A,B,B,A,...; `idle BIN SRC` watches the shell alone for 150 s. Never the live session."""
import importlib.util, json, os, pathlib, re, statistics, subprocess, sys, time

HERE = pathlib.Path(__file__).parent
spec = importlib.util.spec_from_file_location("b0065", HERE / "b0065-smaps.py")
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
b = m.b
EVENTS = ("PANEL_IDLE_RELEASED", "PANEL_IDLE_TRIM", "SHM_POOL_COMPACT", "WALLPAPER_BUFFER_RELEASED")


if os.environ.get("B0085_PIPEWIRE") == "1":
    # A private PipeWire server in the session's runtime dir, so the audio service connects instead of retrying every 1.6 s.
    _spawn = b.Arm.spawn

    def _with_pipewire(self, cmd, log, env, **kw):
        if log == "shell.log":
            self.spawn(["pipewire"], "pipewire.log", dict(env, PIPEWIRE_REMOTE="pipewire-0"))
            self.wait(lambda: (b.RUN / "r/pipewire-0").exists(), 10, "pipewire")
            env = dict(env, PIPEWIRE_REMOTE="pipewire-0")
        return _spawn(self, cmd, log, env, **kw)
    b.Arm.spawn = _with_pipewire


def unreclaimable(pid):
    r = b.rollup(pid)
    return r["Pss_Anon"] + r["Pss_Shmem"] + r["SwapPss"], r["Pss_Anon"]


def threads(pid):
    """{tid: (comm, utime + stime ticks)}: which thread was running when memory moved."""
    out = {}
    for t in pathlib.Path(f"/proc/{pid}/task").iterdir():
        try:
            f = (t / "stat").read_text()
            name, rest = f[f.index("(") + 1:f.rindex(")")], f[f.rindex(")") + 2:].split()
            out[t.name] = (name, int(rest[11]) + int(rest[12]))
        except (OSError, ValueError):
            pass
    return out


def anon_maps(pid, top=6):
    """Largest anonymous mappings by Anonymous KiB: [(start, name, kib)]."""
    rows, cur = [], None
    for line in open(f"/proc/{pid}/smaps"):
        h = re.match(r"^([0-9a-f]+)-[0-9a-f]+ \S+ \S+ \S+ \d+\s*(.*)$", line)
        if h:
            cur = [h.group(1), h.group(2) or "[anon]", 0]
            rows.append(cur)
        elif line.startswith("Anonymous:") and cur:
            cur[2] = int(line.split()[1])
    return sorted((r for r in rows if r[2]), key=lambda r: -r[2])[:top]


def one(label, binary, src, tag):
    arm = b.Arm(label, binary, pathlib.Path(src))

    def ipc(*a):
        return subprocess.run([str(b.ROOT / binary), "ipc", "--socket", arm.cmdsock, *a], env=dict(arm.env, BEAD_PRIVATE_INPUT_PROOF="1"),
                              capture_output=True, text=True, cwd=str(src), timeout=10)

    try:
        arm.start(tag)
        pid = arm.shell.pid
        log = arm.raw / "shell.log"
        time.sleep(20)
        base = statistics.median(unreclaimable(pid)[0] for _ in range(3) if not time.sleep(1))
        maps = {"idle": anon_maps(pid)}
        ipc("drawers", "toggle", "dashboard")
        time.sleep(3)
        peak = unreclaimable(pid)[0]
        off = len(log.read_text(errors="replace"))
        ipc("drawers", "toggle", "dashboard")
        t0 = time.monotonic()
        series, events = [], []
        for s in range(1, 61):
            time.sleep(max(0, t0 + s - time.monotonic()))
            u, a = unreclaimable(pid)
            series.append({"t": s, "unreclaimable": round(u, 2), "anon": round(a, 2), "sec": int(time.time() % 60)})
            text = log.read_text(errors="replace")
            for line in text[off:].splitlines():
                if line.startswith(EVENTS):
                    events.append({"t": s, "line": line[:60]})
            off = len(text)
            if s in (5, 12, 60):
                maps[f"+{s}"] = anon_maps(pid)
        return {"label": label, "base": round(base, 2), "open_peak": round(peak, 2), "series": series, "events": events, "maps": maps}
    finally:
        arm.finish()


def idle(binary, src):
    """No panel at all: the shell alone for 150 s, to see which steps of memory are not caused by a close."""
    arm = b.Arm("idle", binary, pathlib.Path(src))
    try:
        arm.start("b0085-idle")
        pid, last, prev = arm.shell.pid, None, {}
        for s in range(150):
            u, a = unreclaimable(pid)
            now = threads(pid)
            busy = sorted({f"{n}:{now[k][1] - prev.get(k, (n, 0))[1]}" for k, (n, _) in now.items() if now[k][1] != prev.get(k, (n, 0))[1]})
            if last is not None and abs(u - last) > 0.3:
                print(f"t={s} wall_second={int(time.time() % 60)} {last:.2f} -> {u:.2f} MiB; threads with CPU this second: {busy}", flush=True)
            elif busy and s > 5:
                print(f"  t={s} {busy}")
            prev, last = now, u
            time.sleep(1)
        print("end", round(last, 2), "maps", anon_maps(pid, 3))
    finally:
        arm.finish()


def run(runs, arms):
    out = []
    for i in range(runs):
        for label, binary, src in arms if i % 2 == 0 else arms[::-1]:
            r = one(label, binary, src, f"b0085-{i}-{label}")
            at = lambda t: r["series"][t - 1]["unreclaimable"] - r["base"]
            print(label, i, "base", r["base"], "d+5/12/30/60", round(at(5), 2), round(at(12), 2), round(at(30), 2), round(at(60), 2),
                  "max", round(max(x["unreclaimable"] for x in r["series"]) - r["base"], 2), flush=True)
            out.append(r)
    (b.OUT / "b0085-transient.json").write_text(json.dumps(out, indent=1))
    for label in dict.fromkeys(a[0] for a in arms):
        rs = [r for r in out if r["label"] == label]
        row = {t: [round(r["series"][t - 1]["unreclaimable"] - r["base"], 2) for r in rs] for t in (3, 5, 12, 30, 60)}
        print(label, json.dumps(row))


if __name__ == "__main__" and sys.argv[1] == "idle":
    idle(sys.argv[2], sys.argv[3])
if __name__ == "__main__" and sys.argv[1] == "run":
    a = sys.argv[3:]
    run(int(sys.argv[2]), [("A", a[0], a[1])] + ([("B", a[2], a[3])] if len(a) >= 4 else []))
