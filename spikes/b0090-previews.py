#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0090 live window previews: private nested Hyprland (headless labwc parent), N terminal windows,
the real daemon with its Hyprland backend, and the shell on the CPU renderer. Opens the Overview,
counts thumbnail frames per window, samples CPU and unreclaimable memory, takes screenshots.

  dbus-run-session -- b0090-previews.py --bin BINDIR --shell SHELLDIR --tools TOOLSDIR --out OUTDIR
                      [--windows 6] [--cycles 3] [--capture on|off]

BINDIR holds nacre-shell and nacre-helper. Run on the desktop under the heavy-job lock; never the live session."""
import argparse, json, os, pathlib, re, shutil, signal, statistics, subprocess, sys, tempfile, time


def sh(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def wait(fn, seconds, what):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        v = fn()
        if v:
            return v
        time.sleep(0.1)
    raise RuntimeError(f"timeout waiting for {what}")


def rollup(pid):
    out = {}
    for line in pathlib.Path(f"/proc/{pid}/smaps_rollup").read_text().splitlines():
        m = re.match(r"(\w+):\s+(\d+) kB", line)
        if m:
            out[m.group(1)] = int(m.group(2)) / 1024
    return out


def unreclaimable(pid):
    r = rollup(pid)
    return r["Pss_Anon"] + r["Pss_Shmem"] + r["SwapPss"]


def ticks(pid):
    f = pathlib.Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return int(f[11]) + int(f[12])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", type=pathlib.Path, required=True)
    ap.add_argument("--shell", type=pathlib.Path, required=True)
    ap.add_argument("--tools", type=pathlib.Path, required=True)
    ap.add_argument("--out", type=pathlib.Path, required=True)
    ap.add_argument("--windows", type=int, default=6)
    ap.add_argument("--cycles", type=int, default=3)
    ap.add_argument("--capture", choices=["on", "off"], default="on")
    ap.add_argument("--open-seconds", type=float, default=10)
    a = ap.parse_args()
    os.setpriority(os.PRIO_PROCESS, 0, 10)
    a.out.mkdir(parents=True, exist_ok=True)
    root = pathlib.Path(tempfile.mkdtemp(prefix="b90-", dir="/tmp"))
    root.chmod(0o700)
    procs = []
    report = {"capture": a.capture, "windows": a.windows, "renderer": "cpu (tiny-skia)", "compositor": "Hyprland nested in headless labwc"}

    def spawn(cmd, name, env, cwd=None):
        p = subprocess.Popen(cmd, env=env, cwd=cwd, stdout=(a.out / f"{name}.log").open("w"), stderr=subprocess.STDOUT, start_new_session=True)
        procs.append(p)
        return p

    base = {k: v for k, v in os.environ.items() if k in ("PATH", "DBUS_SESSION_BUS_ADDRESS", "LANG", "HOME")}
    base.update(GIO_USE_VFS="local", GVFS_DISABLE_FUSE="1")
    base["HOME"] = str(root / "home")
    (root / "home").mkdir(mode=0o700)
    try:
        # Parent: headless labwc.
        pr = root / "p"
        pr.mkdir(mode=0o700)
        cfg = root / "labwc"
        cfg.mkdir()
        (cfg / "autostart").write_text("")
        penv = dict(base, XDG_RUNTIME_DIR=str(pr), WLR_BACKENDS="headless", WLR_HEADLESS_OUTPUTS="1", WLR_RENDERER="gles2")
        spawn(["labwc", "--config-dir", str(cfg)], "labwc", penv)
        parent = wait(lambda: next((p for p in pr.glob("wayland-*") if not str(p).endswith(".lock")), None), 20, "labwc")
        # Nested Hyprland.
        hr = root / "h"
        hr.mkdir(mode=0o700)
        (root / "hypr.lua").write_text('hl.monitor({ output = "", mode = "1920x1080@60", position = "auto", scale = 1 })\n'
                                       'hl.config({ ["animations.enabled"] = false, ["misc.disable_splash_rendering"] = true, ["misc.disable_hyprland_logo"] = true })\n')
        henv = dict(base, XDG_RUNTIME_DIR=str(hr), WAYLAND_DISPLAY=str(parent), AQ_DRM_DEVICES="/dev/null")
        spawn(["Hyprland", "--config", str(root / "hypr.lua")], "hyprland", henv)
        disp = wait(lambda: next((p.name for p in hr.glob("wayland-*") if not p.name.endswith(".lock")), None), 20, "Hyprland")
        sig = wait(lambda: next((p.name for p in (hr / "hypr").glob("*")), None), 10, "Hyprland instance")
        cenv = dict(base, XDG_RUNTIME_DIR=str(hr), WAYLAND_DISPLAY=disp, HYPRLAND_INSTANCE_SIGNATURE=sig)
        info = sh(["wayland-info"], env=cenv).stdout
        report["globals"] = sorted(set(re.findall(r"interface: '(ext_[a-z_]*(?:capture|toplevel)[a-z_0-9]*)'", info)))
        # Daemon.
        for d in ("data", "config", "state/nacre", "cache"):
            (root / d).mkdir(mode=0o700, parents=True)
        (hr / "nacre").mkdir(mode=0o700)
        sock = hr / "nacre/daemon.sock"
        denv = dict(cenv, XDG_DATA_HOME=str(root / "data"), XDG_CONFIG_HOME=str(root / "config"), XDG_STATE_HOME=str(root / "state"),
                    XDG_CACHE_HOME=str(root / "cache"), NACRE_DAEMON_SOCKET=str(sock), NACRE_COMPOSITOR="hyprland",
                    NACRE_HELPER_TOOLS=str(a.tools), DBUS_SYSTEM_BUS_ADDRESS=base["DBUS_SESSION_BUS_ADDRESS"],
                    PIPEWIRE_REMOTE="bead-private-unavailable", PULSE_SERVER="unix:" + str(root / "no-pulse"), GIO_USE_VFS="local", GVFS_DISABLE_FUSE="1")
        ctl = root / "nacrectl"
        ctl.symlink_to((a.bin / "nacre-helper").resolve())
        spawn([str((a.bin / "nacre-helper").resolve()), "daemon", "--notifications=daemon"], "daemon", denv)
        wait(lambda: sh([str(ctl), "call", "--timeout", "2000", "daemon.status", "{}"], env=denv).returncode == 0, 40, "daemon")
        # Windows: a tiny shm client (spikes/shm-window.c) with distinct colours and titles.
        build = root / "build"
        build.mkdir()
        xml = "/usr/share/wayland-protocols/stable/xdg-shell/xdg-shell.xml"
        sh(["wayland-scanner", "client-header", xml, str(build / "xdg-shell-client-protocol.h")], check=True)
        sh(["wayland-scanner", "private-code", xml, str(build / "xdg-shell-protocol.c")], check=True)
        src = pathlib.Path(__file__).with_name("shm-window.c")
        done = sh(["gcc", "-O1", "-I", str(build), "-o", str(build / "shm-window"), str(src), str(build / "xdg-shell-protocol.c"), "-lwayland-client"])
        if done.returncode:
            raise RuntimeError(done.stderr)
        winprocs = []
        colours = ["c0392b", "27ae60", "2980b9", "8e44ad", "d35400", "16a085", "7f8c8d", "f1c40f"]
        for i in range(a.windows):
            winprocs.append(spawn([str(build / "shm-window"), f"preview-{i + 1}", "bead.preview", colours[i % len(colours)]], f"win{i + 1}", cenv))
        try:
            wait(lambda: len(json.loads(sh(["hyprctl", "-j", "clients"], env=cenv).stdout or "[]")) >= a.windows, 40, "windows")
        except RuntimeError:
            raise RuntimeError("windows missing: exit codes %s; hyprctl: %s" % ([p.poll() for p in winprocs], sh(["hyprctl", "-j", "clients"], env=cenv)))
        time.sleep(2)
        report["clients"] = [(c["class"], c["title"]) for c in json.loads(sh(["hyprctl", "-j", "clients"], env=cenv).stdout)]
        # Shell.
        broot = root / "bead"
        shutil.copytree(a.shell, broot / "shell")
        cmdsock = hr / "bead-command.sock"
        senv = dict(denv, BEAD_CAPTURE="off" if a.capture == "off" else "on")
        cmd = [str(a.bin / "nacre-shell"), "--bar", "--core-views", "--ui-provider", "--notices", "--daemon-socket", str(sock),
               "--command-socket", str(cmdsock), "--palette", str(broot / "shell/palette.json"), "--renderer", "cpu", "--panel-renderer", "cpu"]
        shell = spawn(cmd, "shell", senv, cwd=broot)
        wait(lambda: "READY output=" in (a.out / "shell.log").read_text(errors="replace"), 60, "shell READY")
        time.sleep(15)
        pid = shell.pid
        ipc = lambda *x: sh([str(a.bin / "nacre-shell"), "ipc", "--socket", str(cmdsock), *x], env=dict(senv, BEAD_PRIVATE_INPUT_PROOF="1"), cwd=broot, timeout=15)
        capdir = hr / f"bead-capture-{pid}"
        hz = os.sysconf("SC_CLK_TCK")
        cycles = []
        for n in range(a.cycles):
            row = {"cycle": n + 1}
            row["closed_before_mib"] = round(unreclaimable(pid), 2)
            t0, c0 = time.monotonic(), ticks(pid)
            time.sleep(5)
            row["closed_cpu_core_pct"] = round(100 * (ticks(pid) - c0) / hz / (time.monotonic() - t0), 2)
            ipc("--json", "bead.panel", "open", "overview")
            time.sleep(3)
            seen = {}
            t0, c0 = time.monotonic(), ticks(pid)
            end = t0 + a.open_seconds
            while time.monotonic() < end:
                for f in (capdir.glob("w*-*.png") if capdir.exists() else []):
                    seen.setdefault(f.name.split("-")[0], set()).add(f.name)
                time.sleep(0.02)
            el = time.monotonic() - t0
            row["open_cpu_core_pct"] = round(100 * (ticks(pid) - c0) / hz / el, 2)
            row["frames_per_window_per_s"] = {k: round(len(v) / el, 2) for k, v in sorted(seen.items())}
            row["open_files"] = sorted(p.name for p in capdir.glob("*")) if capdir.exists() else []
            row["open_mib"] = round(unreclaimable(pid), 2)
            if n == 0:
                sh(["grim", str(a.out / f"overview-{a.capture}.png")], env=cenv)
            ipc("--json", "bead.panel", "close")
            time.sleep(12)
            row["closed_after_mib"] = round(unreclaimable(pid), 2)
            row["files_after_close"] = sorted(p.name for p in capdir.glob("*")) if capdir.exists() else []
            row["dir_exists_after_close"] = capdir.exists()
            cycles.append(row)
        report["cycles"] = cycles
        report["closed_delta_mib_first_to_last"] = round(cycles[-1]["closed_after_mib"] - cycles[0]["closed_before_mib"], 2)
        report["median_open_cpu_core_pct"] = statistics.median(c["open_cpu_core_pct"] for c in cycles)
        (a.out / f"result-{a.capture}.json").write_text(json.dumps(report, indent=1))
        print(json.dumps(report, indent=1))
    finally:
        for p in reversed(procs):
            if p.poll() is None:
                try:
                    os.killpg(p.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
        for p in procs:
            try:
                p.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(p.pid, signal.SIGKILL)
        shutil.rmtree(root, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
