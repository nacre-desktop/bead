#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0065 file-backed PSS attribution: per-mapping smaps, candidate versus control, private nested session.

  b0065-smaps.py dump LABEL BIN SRC      start one arm (b0047-frame.py Arm), save smaps idle and after a panel open/close
  b0065-smaps.py diff CTL_DIR CAND_DIR   table mapping -> PSS (KiB) per phase and delta

SMOKE_ROOT as for b0047-frame.py. Never the live session."""
import importlib.util, json, os, pathlib, re, subprocess, sys, time

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


if __name__ == "__main__":
    if sys.argv[1] == "dump":
        dump(sys.argv[2], sys.argv[3], sys.argv[4])
    else:
        diff(sys.argv[2], sys.argv[3])
