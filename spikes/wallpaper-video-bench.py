#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0099 CPU/PSS bench for the video wallpaper: bead-wallpaper-video vs the QML shell's NacreDesktopVideo.

Private everything: a headless labwc parent, a nested niri (default 2880x1800 at scale 1.5), one wallpaper
process, and the same clip for both arms. Phases: playing, covered (paused, as the policy does), released (Bead only), resumed.
Prints one JSON object. Run it on a test host (never the owner's laptop), under the desktop lock:
  flock ~/nacre-lead/desktop-heavy.lock python3 wallpaper-video-bench.py --arm bead --bin ./bead-wallpaper-video \
      --video clip.mp4 --libdir rt/usr/lib
  ... --arm qml --video clip.mp4 --qml-video-file NacreDesktopVideo.qml
"""
import argparse, json, os, selectors, signal, stat, subprocess, sys, tempfile, time
from pathlib import Path

ap = argparse.ArgumentParser()
ap.add_argument("--arm", choices=("bead", "qml"), required=True)
ap.add_argument("--bin", help="bead-wallpaper-video binary")
ap.add_argument("--hwdec", default="vaapi", help="bead-wallpaper-video --hwdec value")
ap.add_argument("--extra", action="append", default=[], help="extra renderer argument (repeatable)")
ap.add_argument("--video", required=True)
ap.add_argument("--size", default="2880x1800")
ap.add_argument("--scale", default="1.5")
ap.add_argument("--libdir", help="directory with a private libmpv.so.2 (LD_LIBRARY_PATH)")
ap.add_argument("--qml-video-file", help="NacreDesktopVideo.qml from the Nacre checkout")
ap.add_argument("--va-device", help="LIBVA/renderer DRM node for decoding, e.g. /dev/dri/renderD129")
ap.add_argument("--va-driver", help="LIBVA_DRIVER_NAME for the renderer/Qt")
ap.add_argument("--render-device", help="WLR_RENDER_DRM_DEVICE for the parent compositor")
ap.add_argument("--phase", type=float, default=20.0, help="seconds measured per phase")
ap.add_argument("--env", action="append", default=[], help="KEY=VALUE for the wallpaper process")
ap.add_argument("--keep", action="store_true")
args = ap.parse_args()
W, H = (int(v) for v in args.size.split("x"))
root = Path(tempfile.mkdtemp(prefix="wpbench-", dir=os.path.expanduser("~/nacre-lead/b0099")))
procs = []  # exact processes this script started

def spawn(cmd, name, env, **kw):
    p = subprocess.Popen(cmd, env=env, cwd=root, stdout=(root / f"{name}.log").open("w"),
                         stderr=subprocess.STDOUT, **kw)
    procs.append(p)
    return p

def wait_for(fn, what, timeout=20):
    end = time.time() + timeout
    while time.time() < end:
        v = fn()
        if v:
            return v
        time.sleep(0.2)
    raise SystemExit(f"timeout waiting for {what}")

def sock(dirpath, pat):
    return next((f for f in dirpath.glob(pat) if stat.S_ISSOCK(f.stat().st_mode)), None)

def ticks(pid):
    f = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return int(f[11]) + int(f[12])

def rollup(pid):
    out = {}
    for line in Path(f"/proc/{pid}/smaps_rollup").read_text().splitlines():
        k, _, rest = line.partition(":")
        if rest.strip().endswith("kB"):
            out[k] = int(rest.split()[0])
    return out

def mib(kib):
    return round(kib / 1024, 1)

def mem(pid):
    r = rollup(pid)
    return {"pss_mib": mib(r.get("Pss", 0)), "anon_mib": mib(r.get("Pss_Anon", 0)),
            "file_mib": mib(r.get("Pss_File", 0)), "shmem_mib": mib(r.get("Pss_Shmem", 0)),
            "swap_pss_mib": mib(r.get("SwapPss", 0))}

def cpu_all(pids, seconds):
    a = {n: ticks(p) for n, p in pids.items()}
    t = time.time()
    time.sleep(seconds)
    dt = (time.time() - t) * os.sysconf("SC_CLK_TCK")
    return {n: round((ticks(p) - a[n]) / dt * 100, 2) for n, p in pids.items()}

runtime = root / "pr"
runtime.mkdir(mode=0o700)
for d in ("home", "data", "config", "state", "cache", "labwc"):
    (root / d).mkdir(mode=0o700, exist_ok=True)
base = dict(os.environ, HOME=str(root / "home"), XDG_DATA_HOME=str(root / "data"), XDG_CONFIG_HOME=str(root / "config"),
            XDG_STATE_HOME=str(root / "state"), XDG_CACHE_HOME=str(root / "cache"))
for k in ("WAYLAND_DISPLAY", "NIRI_SOCKET", "DISPLAY", "HYPRLAND_INSTANCE_SIGNATURE", "SWAYSOCK"):
    base.pop(k, None)
(root / "labwc" / "autostart").write_text("")
(root / "labwc" / "rc.xml").write_text(
    '<labwc_config><windowRules><windowRule identifier="*"><decorations>no</decorations>'
    f'<action name="ResizeTo" width="{W}" height="{H}"/><action name="MoveTo" x="0" y="0"/>'
    "</windowRule></windowRules></labwc_config>")
penv = dict(base, XDG_RUNTIME_DIR=str(runtime), WLR_BACKENDS="headless", WLR_HEADLESS_OUTPUTS="1", WLR_RENDERER="gles2")
if args.render_device:
    penv["WLR_RENDER_DRM_DEVICE"] = args.render_device
result = {"arm": args.arm, "size": args.size, "scale": args.scale, "video": Path(args.video).name,
          "hwdec_arg": args.hwdec if args.arm == "bead" else "qt-default", "extra": args.extra}
try:
    labwc = spawn(["labwc", "--config-dir", str(root / "labwc")], "labwc", penv, start_new_session=True)
    psock = wait_for(lambda: sock(runtime, "wayland-*"), "labwc socket")
    (root / "niri.kdl").write_text('input { keyboard { xkb { layout "us" } } }\nprefer-no-csd\n'
                                   "hotkey-overlay { skip-at-startup }\nanimations { off }\n")
    nrun = root / "nr"
    nrun.mkdir(mode=0o700)
    nenv = dict(base, XDG_RUNTIME_DIR=str(nrun), WAYLAND_DISPLAY=str(psock))
    # The nested niri lives in its own runtime dir but connects to labwc by absolute socket path.
    niri = spawn(["niri", "--config", str(root / "niri.kdl")], "niri", nenv, start_new_session=True)
    nsock = wait_for(lambda: sock(nrun, "wayland-*"), "niri display")
    nirisock = wait_for(lambda: sock(nrun, "niri.*.sock"), "niri socket")
    cenv = dict(base, XDG_RUNTIME_DIR=str(nrun), WAYLAND_DISPLAY=nsock.name, NIRI_SOCKET=str(nirisock))
    def nmsg(*a):
        return subprocess.run(["niri", "msg", *a], env=cenv, capture_output=True, text=True, timeout=15, check=True).stdout
    outs = json.loads(wait_for(lambda: nmsg("-j", "outputs") if nmsg("-j", "outputs").strip() != "{}" else None, "niri outputs"))
    name = next(iter(outs))
    nmsg("output", name, "scale", args.scale)
    time.sleep(1)
    o = json.loads(nmsg("-j", "outputs"))[name]
    mode = o["modes"][o["current_mode"]]
    result["output"] = {"name": name, "mode": [mode["width"], mode["height"]], "logical": [o["logical"]["width"], o["logical"]["height"]]}

    renv = dict(cenv)
    if args.libdir:
        renv["LD_LIBRARY_PATH"] = os.path.abspath(os.path.expanduser(args.libdir))
    renv.update(kv.split("=", 1) for kv in args.env)
    if args.va_driver:
        renv["LIBVA_DRIVER_NAME"] = args.va_driver
    if args.va_device:
        renv["BEAD_WALLPAPER_VAAPI_DEVICE"] = args.va_device
        renv["QT_QPA_EGLFS_KMS_DEVICE"] = args.va_device
    if args.arm == "bead":
        cmd = [os.path.abspath(os.path.expanduser(args.bin)), f"--hwdec={args.hwdec}", *args.extra]
        rp = spawn(cmd, "renderer", renv, stdin=subprocess.PIPE)
        target = rp.pid
        def send(o):
            rp.stdin.write((json.dumps(o) + "\n").encode())
            rp.stdin.flush()
        send({"cmd": "source", "source": {"kind": "video", "path": os.path.abspath(os.path.expanduser(args.video)), "fit": "crop", "loop": True}})
        send({"cmd": "state", "decoder": "play", "render": [name]})
        play = lambda: send({"cmd": "state", "decoder": "play", "render": [name]})
        cover = lambda: send({"cmd": "state", "decoder": "pause", "render": []})
    else:
        qdir = root / "qml"
        qdir.mkdir()
        (qdir / "NacreDesktopVideo.qml").write_text(Path(os.path.abspath(os.path.expanduser(args.qml_video_file))).read_text())
        (qdir / "shell.qml").write_text(f"""
import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Io
ShellRoot {{
    id: shell
    property bool run: true
    PanelWindow {{
        anchors {{ top: true; bottom: true; left: true; right: true }}
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Background
        color: "black"
        NacreDesktopVideo {{ anchors.fill: parent; path: "{os.path.abspath(os.path.expanduser(args.video))}"; running: shell.run; screenName: "bench" }}
    }}
    IpcHandler {{ target: "ctl"; function set(v: bool): void {{ shell.run = v; }} }}
}}
""")
        renv["QT_LOGGING_RULES"] = "qt.multimedia.ffmpeg*=true"
        q = spawn(["quickshell", "-p", str(qdir / "shell.qml"), "--no-duplicate"], "quickshell", renv)
        target = q.pid
        def qs(v):
            subprocess.run(["quickshell", "ipc", "-p", str(qdir / "shell.qml"), "call", "ctl", "set", v], env=renv, capture_output=True, timeout=15)
        play = lambda: qs("true")
        cover = lambda: qs("false")
        time.sleep(3)
    pids = {"renderer": target, "niri": niri.pid, "labwc": labwc.pid}
    time.sleep(8)
    result["alive"] = Path(f"/proc/{target}").exists()
    phases = [("playing", None), ("covered", cover)]
    if args.arm == "bead":  # the policy releases the decoder after a long pause; QML has no such state
        phases.append(("released", lambda: send({"cmd": "state", "decoder": "release", "render": []})))
    phases.append(("resumed", play))
    for phase, action in phases:
        if action:
            action()
            time.sleep(3)
        cpu = cpu_all(pids, args.phase)
        result[phase] = {"cpu_pct": cpu, **mem(target)}
    # Wait a beat so the log reports decoder info.
    time.sleep(1)
except BaseException as e:  # report, then clean up
    result["error"] = repr(e)
finally:
    log = root / ("renderer.log" if args.arm == "bead" else "quickshell.log")
    if log.exists():
        txt = log.read_text(errors="replace")
        result["log_tail"] = [l[:200] for l in txt.splitlines() if any(w in l.lower() for w in ("hwdec", "vaapi", "hwaccel", "hardware"))][-6:]
    for p in reversed(procs):
        if p.poll() is None:
            p.send_signal(signal.SIGTERM)
    for p in reversed(procs):
        try:
            p.wait(5)
        except subprocess.TimeoutExpired:
            p.kill()
    print(json.dumps(result))
    if not args.keep:
        subprocess.run(["rm", "-rf", str(root)])
