#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0065 file-backed PSS attribution: per-mapping smaps, candidate versus control, private nested session.

  b0065-smaps.py dump LABEL BIN SRC      start one arm (b0047-frame.py Arm), save smaps idle and after a panel open/close
  b0065-smaps.py diff CTL_DIR CAND_DIR   table mapping -> PSS (KiB) per phase and delta
  b0065-smaps.py memory PAIRS CTL CAND CTL_SRC CAND_SRC   alternating arms; never/once/several/combined, medians, CPU, no daemon

SMOKE_ROOT as for b0047-frame.py. Never the live session."""
import importlib.util, json, os, pathlib, re, statistics, subprocess, sys, time

HERE = pathlib.Path(__file__).parent
spec = importlib.util.spec_from_file_location("b0047", HERE / "b0047-frame.py")
b = importlib.util.module_from_spec(spec)
spec.loader.exec_module(b)


def parse(path):
    """-> {(name, perms): {Pss, Pss_File, Pss_Anon, Rss}} summed per file name (anon by label)."""
    out, cur = {}, None
    for line in pathlib.Path(path).read_text().splitlines():
        m = re.match(r"^[0-9a-f]+-[0-9a-f]+ (\S+) \S+ \S+ \d+\s*(.*)$", line)
        if m:
            name = m.group(2) or "[anon]"
            cur = out.setdefault((name, m.group(1)), {"Pss": 0, "Pss_File": 0, "Pss_Anon": 0, "Rss": 0, "Size": 0})
        elif cur is not None:
            k, _, v = line.partition(":")
            if k in cur:
                cur[k] += int(v.split()[0])
    return out


_wait = b.Arm.wait


def _wait_start(self, cond, secs, what):
    """No daemon link here: the control prints no UI_SESSION_CREATED, so wait for the shell's START line."""
    if what == "bar session":
        cond = lambda: "START pid=" in (self.raw / "shell.log").read_text(errors="replace")
    return _wait(self, cond, secs, what)


b.Arm.wait = _wait_start


def dump(label, binary, src):
    arm = b.Arm(label, binary, pathlib.Path(src))
    try:
        arm.start(f"smaps-{label}")
        pid = arm.shell.pid
        time.sleep(15)
        for ph, op in (("idle", False), ("opened", True)):
            if op:
                for _ in range(2):
                    subprocess.run([str(b.ROOT / binary), "ipc", "--socket", arm.cmdsock, "drawers", "toggle", "sound"],
                                   env=arm.env, capture_output=True, text=True, cwd=str(src))
                    time.sleep(2)
                time.sleep(12)
            (arm.raw / f"{ph}.smaps").write_text(open(f"/proc/{pid}/smaps").read())
            (arm.raw / f"{ph}.rollup").write_text(open(f"/proc/{pid}/smaps_rollup").read())
        print(label, "dumped", arm.raw)
    finally:
        arm.finish()


def diff(ctl, cand):
    for ph in ("idle", "opened"):
        a, c = parse(pathlib.Path(ctl) / f"{ph}.smaps"), parse(pathlib.Path(cand) / f"{ph}.smaps")
        # collapse by name (all perms), but keep .text separate via perms
        rows = []
        for k in set(a) | set(c):
            x, y = a.get(k, {}), c.get(k, {})
            d = y.get("Pss", 0) - x.get("Pss", 0)
            rows.append((d, k, x.get("Pss", 0), y.get("Pss", 0), y.get("Pss_File", 0) - x.get("Pss_File", 0)))
        rows.sort(reverse=True)
        print(f"== {ph}: total ctl {sum(r[2] for r in rows)} KiB cand {sum(r[3] for r in rows)} KiB")
        for d, k, xa, yc, df in rows[:14] + rows[-4:]:
            print(f"{d:+8d} KiB  ctl {xa:7d} cand {yc:7d}  file {df:+7d}  {k[1]} {k[0][-70:]}")


def memory_arm(label, binary, src, tag):
    arm = b.Arm(label, binary, pathlib.Path(src))

    def ipc(*a):
        return subprocess.run([str(b.ROOT / binary), "ipc", "--socket", arm.cmdsock, *a], env=dict(arm.env, BEAD_PRIVATE_INPUT_PROOF="1"),
                              capture_output=True, text=True, cwd=str(src), timeout=10)

    def cycle(kind):
        if kind == "launcher":
            ipc("--json", "bead.panel", "open", "launcher")
            time.sleep(3)
            ipc("--json", "bead.panel", "close")
        else:
            for _ in range(2):
                ipc("drawers", "toggle", "sound")
                time.sleep(2)
        time.sleep(1)

    def phase(settle):
        time.sleep(settle)
        smp = []
        for _ in range(7):
            smp.append(b.rollup(arm.shell.pid))
            time.sleep(2)
        return {k: statistics.median(x[k] for x in smp) for k in b.KEYS} | {"max_pss": max(x["Pss"] for x in smp)}

    try:
        arm.start(tag)
        r = {"label": label, "binary": binary}
        r["never"] = phase(20)
        cycle("launcher")
        r["once"] = phase(20)
        for kind in ("sound", "launcher", "sound", "launcher"):
            cycle(kind)
        r["several"] = phase(20)
        for kind in ("launcher", "sound"):
            cycle(kind)
        r["combined"] = phase(20)
        (arm.raw / "result.json").write_text(json.dumps(r, indent=1))
        return r
    finally:
        arm.finish()


def memory(pairs, ctl, cand, ctl_src, cand_src):
    runs = []
    for i in range(pairs):
        arms = [("control", ctl, ctl_src), ("candidate", cand, cand_src)]
        for label, binary, src in arms if i % 2 == 0 else arms[::-1]:
            runs.append(memory_arm(label, binary, src, f"mem-{i}-{label}"))
    rep = {"renderer": "cpu (tiny-skia), no daemon, launcher without fixture"}
    for ph in ("never", "once", "several", "combined"):
        med = lambda label, key: statistics.median(r[ph][key] for r in runs if r["label"] == label)
        rep[ph] = {"control": round(med("control", "Pss"), 2), "candidate": round(med("candidate", "Pss"), 2),
                   "delta": round(med("candidate", "Pss") - med("control", "Pss"), 2),
                   "anon_delta": round(med("candidate", "Pss_Anon") - med("control", "Pss_Anon"), 2),
                   "cand_max_pss": round(max(r[ph]["max_pss"] for r in runs if r["label"] == "candidate"), 2)}
    (b.OUT / "memory.json").write_text(json.dumps({"summary": rep, "runs": runs}, indent=1))
    print(json.dumps(rep, indent=1))


if __name__ == "__main__":
    if sys.argv[1] == "memory":
        memory(int(sys.argv[2]), *sys.argv[3:7])
    elif sys.argv[1] == "dump":
        dump(sys.argv[2], sys.argv[3], sys.argv[4])
    else:
        diff(sys.argv[2], sys.argv[3])
