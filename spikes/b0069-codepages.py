#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0069 code-page release: latency after idle, faults, idle CPU and cgroup accounting, private nested session.

  b0069-codepages.py latency PAIRS CP_BIN NOCP_BIN SRC     alternating arms: launcher open / first key / dashboard open after idle
  b0069-codepages.py idle PAIRS SECONDS CP_BIN NOCP_BIN SRC  idle CPU, context switches and faults over SECONDS per arm
  b0069-codepages.py account PAIRS CP_BIN NOCP_BIN SRC     PSS split: anon, dirty file, clean text/rodata, other clean file, shmem, DRM (idle, opened)
  b0069-codepages.py pressure CP_BIN NOCP_BIN SRC          inside a memory-limited scope: file pressure, then reopen

Run `pressure` inside a limited scope: systemd-run --user --scope -p MemoryMax=400M -p MemorySwapMax=0 python3 ...
SMOKE_ROOT as for b0047-frame.py (binaries, `out/`). CP_BIN is the build with the release timer, NOCP_BIN the same
tree without it. CPU renderer, no daemon link. Never the live session."""
import importlib.util, json, os, pathlib, re, shutil, statistics, subprocess, sys, time

HERE = pathlib.Path(__file__).parent
spec = importlib.util.spec_from_file_location("b0065", HERE / "b0065-smaps.py")
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
b = m.b
TICK = os.sysconf("SC_CLK_TCK")
IDLE = float(os.environ.get("B0069_IDLE", "12"))


def stat(pid):
    """-> minflt, majflt, cpu ms (utime+stime), voluntary+nonvoluntary context switches."""
    f = open(f"/proc/{pid}/stat").read()
    t = f[f.rindex(")") + 2:].split()
    st = {k: int(v) for k, v in (l.split(":") for l in open(f"/proc/{pid}/status").read().replace("\t", "").splitlines()
                                 if l.startswith(("voluntary_ctxt", "nonvoluntary_ctxt")))}
    return {"minflt": int(t[7]), "majflt": int(t[9]), "cpu_ms": (int(t[11]) + int(t[12])) * 1000 / TICK,
            "ctxt": sum(st.values())}


def delta(a, c):
    return {k: c[k] - a[k] for k in a}


def release_calls(pid):
    """Mappings one release would madvise now (same filter as code_pages.rs) and the bytes they cover."""
    n = size = 0
    cur = None
    for line in open(f"/proc/{pid}/smaps"):
        mm = re.match(r"^([0-9a-f]+)-([0-9a-f]+) (\S+) \S+ \S+ \S+\s*(.*)$", line)
        if mm:
            if cur:
                n += 1
                size += cur
            path = mm.group(4)
            cur = int(mm.group(2), 16) - int(mm.group(1), 16) if mm.group(3) in ("r-xp", "r--p") and path.startswith("/") \
                and "(deleted)" not in path else None
        elif cur and line.split(":")[0] in ("Anonymous", "Private_Dirty", "Shared_Dirty", "Swap") and line.split()[1] != "0":
            cur = None
    if cur:
        n += 1
        size += cur
    return n, size // 1024


def fixture():
    """320 fixture apps as the B0039/B0044 typing lane (exact menu.query replies for '', 'a', 'ap'); written once under out/."""
    path = b.OUT / "menu-fixture.json"
    b.OUT.mkdir(exist_ok=True)
    mf = importlib.util.spec_from_file_location("menu_fixtures", HERE / "menu-fixtures.py")
    mod = importlib.util.module_from_spec(mf)
    mf.loader.exec_module(mod)
    icon = str(HERE.parent / "shell/sample.svg")
    rows = [{"id": f"proof-{i:04}", "name": f"Application {i:04}", "label": f"Application {i:04}", "icon": icon, "kind": "app",
             "description": "Application fixture", "breadcrumb": "All apps", "operation": "app", "disabled": False} for i in range(320)]
    path.write_text(json.dumps({"queries": {q: mod.reply(q, rows) for q in ("", "a", "ap")}}))
    return path, icon


class Run:
    def __init__(s, label, binary, src, tag):
        s.arm = b.Arm(label, binary, pathlib.Path(src))
        s.tag, s.src = tag, pathlib.Path(src)
        if os.environ.get("B0069_FIXTURE"):
            path, icon = fixture()
            orig = s.arm.spawn

            def spawn(cmd, log, env, **kw):
                if str(b.ROOT / binary) in cmd[:1]:
                    cmd = [*cmd, "--menu-fixture", str(path)]
                    env = dict(env, BEAD_MATCHED_ICONS="1", BEAD_ICON_PATH=icon)
                return orig(cmd, log, env, **kw)

            s.arm.spawn = spawn

    def ipc(s, *a):
        return subprocess.run([str(b.ROOT / s.arm.binary), "ipc", "--socket", s.arm.cmdsock, *a],
                              env=dict(s.arm.env, BEAD_PRIVATE_INPUT_PROOF="1"), capture_output=True, text=True,
                              cwd=str(s.src), timeout=10)

    def log(s):
        return (s.arm.raw / "shell.log").read_text(errors="replace")

    def presented(s, offset, t0):
        """First PANEL_PRESENTED after offset: ms from t0 (CLOCK_MONOTONIC ns), or None."""
        for mm in re.finditer(r"PANEL_PRESENTED monotonic_ns=(\d+)", s.log()[offset:]):
            if int(mm.group(1)) >= t0:
                return (int(mm.group(1)) - t0) / 1e6
        return None

    def open_panel(s, *cmd, settle=2.0):
        """Run an IPC that opens a panel; -> presented latency ms, fault/cpu delta over the interaction."""
        pid, off = s.arm.shell.pid, len(s.log())
        before = stat(pid)
        t0 = time.monotonic_ns()
        r = s.ipc(*cmd)
        end = time.monotonic() + 5
        lat = None
        while lat is None and time.monotonic() < end:
            lat = s.presented(off, t0)
            time.sleep(.01)
        time.sleep(settle)
        return {"ms": lat, "ipc_rc": r.returncode} | delta(before, stat(pid))

    def key(s, args):
        """One wtype key; -> presented latency ms from INPUT_RECEIVED and UI CPU ms, when the shell reports them."""
        pid, off = s.arm.shell.pid, len(s.log())
        before = stat(pid)
        subprocess.run(["wtype", "-s", "20", *args], env=s.arm.env, capture_output=True, timeout=6)
        time.sleep(1.0)
        seg = s.log()[off:]
        rec = re.search(r"INPUT_RECEIVED monotonic_ns=(\d+) cpu_ns=(\d+)", seg)
        pres = re.search(r"UI_PRESENTED generation=\d+ monotonic_ns=(\d+) refresh_ns=\d+ companion=\w+ update=\d+ cpu_ns=(\d+)", seg[rec.end():]) if rec else None
        out = {"received": bool(rec)} | delta(before, stat(pid))
        if rec and pres:
            out |= {"ms": (int(pres.group(1)) - int(rec.group(1))) / 1e6, "ui_cpu_ms": (int(pres.group(2)) - int(rec.group(2))) / 1e6}
        return out


def latency_arm(label, binary, src, tag):
    r = Run(label, binary, src, tag)
    env = {"BEAD_PROFILE_INPUT": "clocks"}
    os.environ.update(env)
    try:
        r.arm.start(tag)
        res = {"label": label, "binary": binary, "cycles": []}
        pid = r.arm.shell.pid
        for cycle in range(2):
            time.sleep(IDLE)
            c = {"calls_per_release": release_calls(pid)}
            c["launcher_open"] = r.open_panel("--json", "bead.panel", "open", "launcher")
            c["key_a"] = r.key(["a"])
            c["key_back"] = r.key(["-k", "BackSpace"])
            r.ipc("--json", "bead.panel", "close")
            time.sleep(IDLE)
            c["dashboard_open"] = r.open_panel("drawers", "toggle", "dashboard")
            r.ipc("drawers", "toggle", "dashboard")
            time.sleep(1)
            res["cycles"].append(c)
        (r.arm.raw / "result.json").write_text(json.dumps(res, indent=1))
        return res
    finally:
        r.arm.finish()


def med(xs):
    xs = [x for x in xs if x is not None]
    return round(statistics.median(xs), 2) if xs else None


def latency(pairs, cp, nocp, src):
    runs = []
    for i in range(pairs):
        arms = [("cp", cp), ("nocp", nocp)]
        for label, binary in arms if i % 2 == 0 else arms[::-1]:
            runs.append(latency_arm(label, binary, src, f"lat-{i}-{label}"))
    rep = {"renderer": "cpu (tiny-skia), no daemon, nested niri in headless labwc", "idle_s": IDLE}
    for what in ("launcher_open", "key_a", "key_back", "dashboard_open"):
        rep[what] = {}
        for label in ("cp", "nocp"):
            cs = [c[what] for r in runs if r["label"] == label for c in r["cycles"]]
            rep[what][label] = {k: med(c.get(k) for c in cs) for k in ("ms", "ui_cpu_ms", "minflt", "majflt", "cpu_ms") if any(k in c for c in cs)}
            rep[what][label]["n"] = len(cs)
            rep[what][label]["ms_all"] = sorted(round(c["ms"], 1) for c in cs if c.get("ms") is not None)
    (b.OUT / "latency.json").write_text(json.dumps({"summary": rep, "runs": runs}, indent=1))
    print(json.dumps(rep, indent=1))


def idle_arm(label, binary, src, tag, seconds):
    r = Run(label, binary, src, tag)
    try:
        r.arm.start(tag)
        pid = r.arm.shell.pid
        time.sleep(IDLE + 8)
        a, t = stat(pid), time.monotonic()
        mem0 = b.rollup(pid)
        time.sleep(seconds)
        d = delta(a, stat(pid))
        res = {"label": label, "seconds": time.monotonic() - t, "mem0": mem0, "mem1": b.rollup(pid),
               "calls_per_release": release_calls(pid)} | d
        (r.arm.raw / "result.json").write_text(json.dumps(res, indent=1))
        return res
    finally:
        r.arm.finish()


def idle(pairs, seconds, cp, nocp, src):
    runs = []
    for i in range(pairs):
        arms = [("cp", cp), ("nocp", nocp)]
        for label, binary in arms if i % 2 == 0 else arms[::-1]:
            runs.append(idle_arm(label, binary, src, f"idle-{i}-{label}", seconds))
    rep = {}
    for label in ("cp", "nocp"):
        rs = [r for r in runs if r["label"] == label]
        rep[label] = {"cpu_ms_per_300s": [round(r["cpu_ms"] * 300 / r["seconds"], 1) for r in rs],
                      "minflt_per_300s": [round(r["minflt"] * 300 / r["seconds"]) for r in rs],
                      "majflt": [r["majflt"] for r in rs],
                      "ctxt_per_300s": [round(r["ctxt"] * 300 / r["seconds"]) for r in rs],
                      "pss_end_mib": [round(r["mem1"]["Pss"], 2) for r in rs],
                      "madvise_calls_per_release_and_kib": rs[0]["calls_per_release"]}
    (b.OUT / "idle.json").write_text(json.dumps({"summary": rep, "runs": runs}, indent=1))
    print(json.dumps(rep, indent=1))


def split(pid):
    """PSS (MiB) of one process by kind, from smaps: anon / dirty file / clean text+rodata (r-xp, r--p) / other clean file / shmem."""
    k = dict.fromkeys(("anon", "file_dirty", "file_clean_text_rodata", "file_clean_other", "shmem"), 0.0)
    perms = path = ""
    cur = {}

    def flush():
        """Per-mapping smaps has Pss, Pss_Dirty and Anonymous (no Pss_File/Pss_Anon, those are rollup-only)."""
        if not cur:
            return
        pss, anon = cur.get("Pss", 0), min(cur.get("Anonymous", 0), cur.get("Pss", 0))
        if "memfd:" in path or path.startswith("/dev/shm") or "SYSV" in path:
            k["shmem"] += pss
            return
        k["anon"] += anon
        if path.startswith("/"):
            dirty = min(max(cur.get("Pss_Dirty", 0) - anon, 0), pss - anon)
            k["file_dirty"] += dirty
            k["file_clean_text_rodata" if perms in ("r-xp", "r--p") else "file_clean_other"] += pss - anon - dirty

    for line in open(f"/proc/{pid}/smaps"):
        mm = re.match(r"^[0-9a-f]+-[0-9a-f]+ (\S+) \S+ \S+ \S+\s*(.*)$", line)
        if mm:
            flush()
            (perms, path), cur = mm.groups(), {}
            continue
        key, _, v = line.partition(":")
        if key in ("Pss", "Pss_Dirty", "Anonymous"):
            cur[key] = int(v.split()[0]) / 1024
    flush()
    drm = 0
    for fd in pathlib.Path(f"/proc/{pid}/fdinfo").iterdir():
        try:
            drm += sum(int(l.split()[1]) for l in fd.read_text().splitlines() if l.startswith(("drm-resident-", "drm-memory-")) and l.split()[1].isdigit()) / 1024
        except OSError:
            pass
    k["drm_kib_to_mib"] = drm / 1024
    roll = b.rollup(pid)
    k["pss_total"], k["rollup_pss_anon"], k["rollup_pss_shmem"] = roll["Pss"], roll["Pss_Anon"], roll["Pss_Shmem"]
    k["anon_heap_and_unmapped_other"] = k["pss_total"] - sum(k[x] for x in ("anon", "file_dirty", "file_clean_text_rodata", "file_clean_other", "shmem"))
    return {a: round(v, 2) for a, v in k.items()}


def account_arm(label, binary, src, tag):
    r = Run(label, binary, src, tag)
    try:
        r.arm.start(tag)
        pid = r.arm.shell.pid
        res = {"label": label}
        time.sleep(20)
        res["idle"] = split(pid)
        r.open_panel("--json", "bead.panel", "open", "launcher")
        r.key(["a"])
        r.ipc("--json", "bead.panel", "close")
        r.open_panel("drawers", "toggle", "dashboard")
        r.ipc("drawers", "toggle", "dashboard")
        time.sleep(20)
        res["after_open_close"] = split(pid)
        (r.arm.raw / "result.json").write_text(json.dumps(res, indent=1))
        return res
    finally:
        r.arm.finish()


def account(pairs, cp, nocp, src):
    runs = []
    for i in range(pairs):
        for label, binary in ((("cp", cp), ("nocp", nocp)) if i % 2 == 0 else (("nocp", nocp), ("cp", cp))):
            runs.append(account_arm(label, binary, src, f"acc-{i}-{label}"))
    rep = {ph: {label: {k: med(r[ph][k] for r in runs if r["label"] == label) for k in runs[0][ph]} for label in ("cp", "nocp")}
           for ph in ("idle", "after_open_close")}
    (b.OUT / "account.json").write_text(json.dumps({"summary": rep, "runs": runs}, indent=1))
    print(json.dumps(rep, indent=1))


def cg(pid):
    """memory.stat of the pid's cgroup, MiB."""
    path = open(f"/proc/{pid}/cgroup").read().strip().split("::")[1]
    base = pathlib.Path("/sys/fs/cgroup") / path.lstrip("/")
    st = {k: int(v) for k, v in (l.split() for l in (base / "memory.stat").read_text().splitlines())}
    mi = {k: int(v.split()[0]) / 1024 for k, v in (l.split(":") for l in open("/proc/meminfo"))}
    return {"current": int((base / "memory.current").read_text()) / 2**20,
            "host_mem_available": round(mi["MemAvailable"], 1), "host_mapped": round(mi["Mapped"], 1), "host_cached": round(mi["Cached"], 1)} | {
        k: round(st[k] / 2**20, 2) for k in ("anon", "file", "file_mapped", "shmem", "file_dirty", "active_file", "inactive_file")}, base


def drop(path):
    fd = os.open(path, os.O_RDONLY)
    os.posix_fadvise(fd, 0, 0, os.POSIX_FADV_DONTNEED)
    os.close(fd)


def fincore(path):
    out = subprocess.run(["fincore", "--bytes", "--noheadings", "-o", "RES,SIZE", path], capture_output=True, text=True).stdout.split()
    return int(out[0]) / 2**20, int(out[1]) / 2**20


def pressure_arm(label, binary, src, tag):
    # A private copy of the binary, written inside this cgroup: its page cache is charged to (and reclaimable from)
    # the limited scope, which the original file's cache (charged to whoever copied it in) is not.
    copy = f"{binary}.{tag}"
    shutil.copyfile(b.ROOT / binary, b.ROOT / copy)
    os.chmod(b.ROOT / copy, 0o755)
    binary = copy
    r = Run(label, binary, src, tag)
    try:
        r.arm.start(tag)
        pid = r.arm.shell.pid
        res = {"label": label, "limit": open(cg(pid)[1] / "memory.max").read().strip()}
        os.environ["BEAD_PROFILE_INPUT"] = "clocks"
        # Warm the launcher once so its code has been used, then let the closed shell idle (the release timer runs in cp).
        r.open_panel("--json", "bead.panel", "open", "launcher")
        r.key(["a"])
        r.ipc("--json", "bead.panel", "close")
        time.sleep(IDLE + 5)
        res["idle"] = cg(pid)[0] | {"pss": b.rollup(pid)["Pss"], "binary_cached_mib": fincore(str(b.ROOT / binary))[0]}
        # File pressure inside the same (limited) cgroup: read a large file so the cgroup must reclaim to stay under its limit.
        big = b.ROOT / "pressure.bin"
        if not big.exists():
            subprocess.run(["dd", "if=/dev/urandom", f"of={big}", "bs=1M", "count=512"], capture_output=True)
        drop(big)  # this file's cache only, so its pages are charged to this scope when the hog reads them
        hog = subprocess.Popen(["python3", "-c", f"for _ in range(3):\n  f=open('{big}','rb')\n  while f.read(1<<20): pass\n"],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
        hog.wait(timeout=120)
        time.sleep(1)

        res["after_file_pressure"] = cg(pid)[0] | {"pss": b.rollup(pid)["Pss"], "binary_cached_mib": fincore(str(b.ROOT / binary))[0]}
        drop(big)
        res["reopen_launcher"] = r.open_panel("--json", "bead.panel", "open", "launcher")
        res["reopen_key"] = r.key(["a"])
        r.ipc("--json", "bead.panel", "close")
        time.sleep(IDLE + 5)
        # Anonymous pressure: a hog that leaves the cgroup only ~40 MiB of file cache, so everything reclaimable goes
        # (the cgroup has memory.high, so it is throttled and reclaimed rather than killed).
        c = cg(pid)[0]
        want = int(os.environ.get("B0069_HOG_MIB", "0")) or int(380 - c["anon"] - c["shmem"] - 40)
        hog = subprocess.Popen(["python3", "-c", f"import sys,time\nb=bytearray({want}<<20)\nfor i in range(0,len(b),4096): b[i]=1\nprint('READY',flush=True)\ntime.sleep(60)\n"],
                               stdout=subprocess.PIPE, text=True, start_new_session=True)
        try:
            assert "READY" in hog.stdout.readline()
            time.sleep(3)
            res["hog_mib"] = want
            res["during_anon_pressure"] = cg(pid)[0] | {"pss": b.rollup(pid)["Pss"], "binary_cached_mib": fincore(str(b.ROOT / binary))[0]}
            res["pressured_launcher"] = r.open_panel("--json", "bead.panel", "open", "launcher")
            res["pressured_key"] = r.key(["a"])
            res["after_reopen"] = cg(pid)[0] | {"pss": b.rollup(pid)["Pss"], "binary_cached_mib": fincore(str(b.ROOT / binary))[0]}
        finally:
            hog.terminate()
            hog.wait(timeout=10)
        (r.arm.raw / "result.json").write_text(json.dumps(res, indent=1))
        return res
    finally:
        r.arm.finish()
        (b.ROOT / copy).unlink(missing_ok=True)


def pressure(cp, nocp, src):
    runs = [pressure_arm(label, binary, src, f"pr-{i}-{label}") for i in range(int(os.environ.get("B0069_PAIRS", "3")))
            for label, binary in ((("cp", cp), ("nocp", nocp)) if i % 2 == 0 else (("nocp", nocp), ("cp", cp)))]
    (b.OUT / "pressure.json").write_text(json.dumps(runs, indent=1))
    print(json.dumps(runs, indent=1))


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "latency":
        latency(int(sys.argv[2]), *sys.argv[3:6])
    elif cmd == "idle":
        idle(int(sys.argv[2]), int(sys.argv[3]), *sys.argv[4:7])
    elif cmd == "account":
        account(int(sys.argv[2]), *sys.argv[3:6])
    elif cmd == "pressure":
        pressure(*sys.argv[2:5])
