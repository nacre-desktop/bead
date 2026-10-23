#!/usr/bin/env python3
"""B0020/B0030 nested smoke: real daemon producer on a PRIVATE bus; the main nacre-shell (bar,
core views, UI provider) hosts notices and the OSD in-process as overlay views (`--notices`)."""
import json, os, pathlib, re, shutil, signal, socket, subprocess, sys, time

ROOT = pathlib.Path(os.environ["SMOKE_ROOT"])
SRC, BIN = ROOT / "src", ROOT / "bin"
SHELL = BIN / os.environ.get("SHELL_BIN", "nacre-shell")
STACK_X, STACK_Y = 20, 30  # surface offsets: right inset 12 px and top 50 + 12 px, minus the 32 px shadow margin
RUN = pathlib.Path("/tmp/b0030-run")  # short socket paths; private, removed at exit
shutil.rmtree(RUN, ignore_errors=True)
RAW = ROOT / "raw"; shutil.rmtree(RAW, ignore_errors=True); RAW.mkdir()
for d in ("r", "pr", "state", "config", "cache", "data"):
    (RUN / d).mkdir(parents=True, mode=0o700)
os.chmod(RUN, 0o700)
# The managed daemon wants its private history directories to exist (0700) before it starts.
for d in ("state/nacre", "state/nacre/shell", "state/nacre/shell/notifications", "state/nacre/shell/notifications/images"):
    (RUN / d).mkdir(exist_ok=True); os.chmod(RUN / d, 0o700)
procs = []
results = []
def check(name, ok, detail=""):
    results.append((name, bool(ok), detail)); print(("PASS " if ok else "FAIL ") + name, detail, flush=True)
def spawn(cmd, log, env, **kw):
    f = (RAW / log).open("w")
    p = subprocess.Popen(cmd, env=env, stdout=f, stderr=subprocess.STDOUT, start_new_session=True, **kw)
    procs.append(p); return p
def stop_all():
    for p in reversed(procs):
        if p.poll() is None:
            try: os.killpg(p.pid, signal.SIGTERM)
            except ProcessLookupError: pass
    t = time.monotonic() + 5
    for p in procs:
        while p.poll() is None and time.monotonic() < t: time.sleep(.05)
        if p.poll() is None:
            try: os.killpg(p.pid, signal.SIGKILL)
            except ProcessLookupError: pass
import atexit
atexit.register(lambda: (stop_all(), shutil.rmtree(RUN, ignore_errors=True)))
def wait(cond, secs=15, what=""):
    end = time.monotonic() + secs
    while time.monotonic() < end:
        v = cond()
        if v: return v
        time.sleep(.05)
    raise RuntimeError("timeout: " + what)

base = {k: v for k, v in os.environ.items() if k not in ("WAYLAND_DISPLAY", "NIRI_SOCKET", "DBUS_SESSION_BUS_ADDRESS")}
base.update(XDG_CONFIG_HOME=str(RUN/"config"), XDG_CACHE_HOME=str(RUN/"cache"), XDG_STATE_HOME=str(RUN/"state"),
            XDG_DATA_HOME=str(RUN/"data"), HOME=str(RUN/"home"), PIPEWIRE_REMOTE="bead-private-unavailable")
(RUN/"home").mkdir(mode=0o700)
try:
    ambient = subprocess.run(["busctl", "--user", "call", "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner", "s", "org.freedesktop.Notifications"], capture_output=True, text=True).stdout.strip()
except Exception: ambient = "?"
BUS = f"unix:path={RUN}/bus"
spawn(["dbus-daemon", "--session", f"--address={BUS}", "--nofork", "--nopidfile"], "dbus.log", base)
wait(lambda: (RUN/"bus").exists(), 5, "private bus")
env = dict(base, DBUS_SESSION_BUS_ADDRESS=BUS, XDG_RUNTIME_DIR=str(RUN/"r"))
penv = dict(base, DBUS_SESSION_BUS_ADDRESS=BUS, XDG_RUNTIME_DIR=str(RUN/"pr"), WLR_BACKENDS="headless", WLR_HEADLESS_OUTPUTS="1", WLR_RENDERER="pixman")
(RUN/"labwc").mkdir(); (RUN/"labwc/autostart").write_text("")
spawn(["labwc", "--config-dir", str(RUN/"labwc")], "labwc.log", penv)
psock = wait(lambda: next((p for p in (RUN/"pr").glob("wayland-*") if not str(p).endswith(".lock")), None), 20, "labwc socket")
env["WAYLAND_DISPLAY"] = str(psock)
spawn(["niri", "--config", str(SRC/"spikes/niri.kdl")], "niri.log", env)
nsock = wait(lambda: next((p for p in (RUN/"r").glob("wayland-*") if not str(p).endswith(".lock")), None), 20, "niri socket")
env["WAYLAND_DISPLAY"] = str(nsock)
env["NIRI_SOCKET"] = str(wait(lambda: next(iter((RUN/"r").glob("niri.*.sock")), None), 10, "niri ipc"))
outs = json.loads(subprocess.check_output(["niri", "msg", "-j", "outputs"], env=env, text=True))
name = next(iter(outs)); mode = outs[name]["logical"]
W, H = mode["width"], mode["height"]
print("OUTPUT", name, W, H, mode.get("scale"))

SOCK = str(RUN/"d.sock")
denv = dict(env, NACRE_DAEMON_SOCKET=SOCK, NACRE_HELPER_TOOLS=str(ROOT/"nacre-assets/nacre/shell-tools"), RUST_LOG=os.environ.get("SMOKE_LOG","warn"))
daemon = spawn([str(BIN/"nacre-helper"), "daemon", "--notifications=daemon"], "daemon.log", denv)
wait(lambda: os.path.exists(SOCK), 15, "daemon socket")
def owner():
    return subprocess.run(["busctl", "--user", "call", "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner", "s", "org.freedesktop.Notifications"], env=env, capture_output=True, text=True).stdout.strip()
wait(lambda: owner() == "b true", 15, "daemon owns the PRIVATE notification name")
check("private bus: daemon owns org.freedesktop.Notifications", owner() == "b true")

# ---- protocol probe client (reads fresh snapshots, issues calls)
class Probe:
    def __init__(self, topics):
        self.s = socket.socket(socket.AF_UNIX); self.s.connect(SOCK); self.buf = b""; self.id = 100; self.mirror = {}; self.rets = {}
        self.send({"t": "hello", "proto": 1, "min": 1, "client": "b0030-smoke"}); self.welcome = self.recv()
        self.send({"t": "sub", "topics": topics})
    def send(self, o): self.s.sendall((json.dumps(o) + "\n").encode())
    def recv(self, timeout=5):
        end = time.monotonic() + timeout
        while b"\n" not in self.buf:
            left = end - time.monotonic()
            if left <= 0: raise TimeoutError("recv")
            self.s.settimeout(left)
            try: chunk = self.s.recv(65536)
            except socket.timeout: raise TimeoutError("recv")
            if not chunk: raise EOFError("daemon closed")
            self.buf += chunk
        line, self.buf = self.buf.split(b"\n", 1)
        return json.loads(line)
    def merge(self, base, patch):
        if not isinstance(patch, dict) or not isinstance(base, dict): return patch
        out = dict(base)
        for k, v in patch.items():
            if v is None: out.pop(k, None)
            else: out[k] = self.merge(out.get(k), v)
        return out
    def pump(self, window=0.25):
        """Apply every frame that arrives within the window to a mirrored topic state."""
        end = time.monotonic() + window
        while time.monotonic() < end:
            try: m = self.recv(timeout=max(0.01, end - time.monotonic()))
            except (socket.timeout, TimeoutError): break
            if m.get("t") == "snap": self.mirror[m["topic"]] = m["data"]
            elif m.get("t") == "patch" and m["topic"] in self.mirror: self.mirror[m["topic"]] = self.merge(self.mirror[m["topic"]], m["data"])
            elif m.get("t") == "ret": self.rets[m["id"]] = m
    def snapshot(self, topic="notifications"):
        self.pump(0.2)
        return self.mirror[topic]
    def call(self, m, p):
        self.id += 1; self.send({"t": "call", "id": self.id, "m": m, "p": p, "timeout": 3000})
        end = time.monotonic() + 5
        while time.monotonic() < end:
            if self.id in self.rets: return self.rets.pop(self.id)
            self.pump(0.1)
        raise RuntimeError("no reply")
probe = Probe(["notifications"])
check("daemon advertises notifications topic", probe.welcome["topics"].get("notifications") == 1)

CMD = str(RUN/"cmd")
def shell_cmd(sock, cmd, trace=True):
    return [str(SHELL), "--bar", "--core-views", "--renderer", "cpu", "--panel-renderer", "cpu", "--daemon-socket", sock, "--ui-provider",
            "--notices", "--command-socket", cmd, "--namespace", "bead-b0030", "--palette", str(SRC/"shell/palette.json")] + (["--trace-state"] if trace else [])
shell = spawn(shell_cmd(SOCK, CMD), "shell.log", dict(env, BEAD_PRIVATE_INPUT_PROOF="1"), cwd=str(SRC))
def shell_log(): return (RAW/"shell.log").read_text(errors="replace")
wait(lambda: "NOTIFICATIONS_DAEMON" in shell_log() and "UI_SESSION_CREATED" in shell_log(), 20, "shell consumer start")
time.sleep(1.0)
def layers(): return subprocess.run(["niri", "msg", "layers"], env=env, capture_output=True, text=True).stdout
def grim(n): subprocess.run(["grim", str(RAW/n)], env=env, capture_output=True, timeout=10)
def notify(summary, body="", actions=("default", "Open", "a1", "Reply", "a2", "Archive"), timeout=-1, urgency=1, rid=0):
    out = subprocess.run(["busctl", "--user", "call", "org.freedesktop.Notifications", "/org/freedesktop/Notifications", "org.freedesktop.Notifications", "Notify",
                          "susssasa{sv}i", "Smoke", str(rid), "dialog-information", summary, body, str(len(actions)), *actions, "1", "urgency", "y", str(urgency), str(timeout)],
                         env=env, capture_output=True, text=True)
    return int(out.stdout.split()[-1]) if out.returncode == 0 else None
def state(): return probe.snapshot()
def panel_state():
    return ipc("bead.panel", "state") or {}
def overlay_state():
    return ipc("bead", "overlay") or {}
def ipc(target, method, *a):
    r = subprocess.run([str(SHELL), "ipc", "--socket", CMD, "--json", target, method, *a], env=dict(env, BEAD_PRIVATE_INPUT_PROOF="1"), capture_output=True, text=True, timeout=5)
    (RAW/"ipc.err").open("a").write(f"{target} {method} rc={r.returncode} {r.stdout[:300]} {r.stderr[:300]}\n")
    return json.loads(r.stdout)["result"] if r.returncode == 0 else None

# ---- 1. admission, popup, default action hidden
nid = notify("Build finished", "All checks passed\nSecond line", timeout=60000)
check("Notify accepted on the private bus", nid)
st = wait(lambda: (lambda s: s if s["active"]["order"] else None)(state()), 5, "active notice")
aid = st["active"]["order"][0]; notice = st["active"]["items"][aid]
check("producer publishes default_action and 3 actions", notice.get("default_action") and len(notice["actions"]["order"]) == 3, str(notice.get("default_action")))
time.sleep(1.0)
ps = panel_state()
ov = overlay_state()
check("popup is the 'notices' overlay view with a surface (panel slot stays idle)", ov.get("view") == "notices" and ov.get("open") is True and ov.get("configured") is True and ps.get("target_open") is False, str((ov, ps))[:200])
grim("1-popup.png")
regs = ipc("bead", "overlay-regions"); (RAW/"regions-1.json").write_text(json.dumps(regs, indent=1))
rid = "notices.k" + aid.encode().hex() + "."
(RAW/"regs-debug.json").write_text(json.dumps(regs)[:3000]); names = [r["id"] for r in regs["regions"] if rid in r["id"]] if regs else []
check("card regions present; default action not a button row", any(n.endswith(".notice") for n in names) and not any("action-" in n for n in names), str(names)[:160])

# ---- 2. pointer: hover pauses producer clock, click expands, action button invokes
ptr = subprocess.Popen([str(BIN/"private-pointer")], env=dict(env, BEAD_PRIVATE_INPUT_PROOF="1"), stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, start_new_session=True)
procs.append(ptr); ptr.stdout.readline()
def pcmd(c):
    ptr.stdin.write(c + "\n"); ptr.stdin.flush(); ptr.stdout.readline()
def center(sub):
    regs = ipc("bead", "overlay-regions"); r = next(r for r in regs["regions"] if rid in r["id"] and r["id"].endswith("." + sub))
    x, y, w, h = r["rect"]; return round(W - regs["size"][0] + STACK_X + regs["offset"][0] + x + w / 2), round(STACK_Y + regs["offset"][1] + y + h / 2)
x, y = center("notice"); pcmd(f"move {x} {y} {W} {H}")
st = wait(lambda: (lambda s: s if s["active"]["items"][aid].get("paused") else None)(state()), 4, "hover paused")
check("hover pauses the producer clock (set_hover via UI provider)", st["active"]["items"][aid]["paused"] is True)
pcmd(f"move 5 5 {W} {H}")
st = wait(lambda: (lambda s: s if not s["active"]["items"][aid].get("paused") else None)(state()), 4, "hover resumed")
check("leaving resumes the producer clock", st["active"]["items"][aid]["paused"] is False)
pcmd(f"move {x} {y} {W} {H}"); pcmd("click"); time.sleep(.5)
regs = ipc("bead", "overlay-regions"); (RAW/"regions-2.json").write_text(json.dumps(regs, indent=1))
names = [r["id"] for r in regs["regions"] if rid in r["id"]]
check("click on the card expands it (action buttons appear, QML default policy)", any("action-" in n for n in names), str(sorted(n.split('.')[-1] for n in names)))
grim("2-expanded.png")
mon = subprocess.Popen(["busctl", "--user", "monitor", "--json=short", "--match", "type='signal',interface='org.freedesktop.Notifications'"], env=env, stdout=(RAW/"signals.log").open("w"), stderr=subprocess.DEVNULL, start_new_session=True); procs.append(mon)
time.sleep(.5)
ax, ay = center("action-1"); pcmd(f"move {ax} {ay} {W} {H}"); pcmd("click"); time.sleep(1.0)
sig = (RAW/"signals.log").read_text(errors="replace")
check("action button emits ActionInvoked on the private bus", "ActionInvoked" in sig, "a1" if '"a1"' in sig or "a1" in sig else sig[:120])

# ---- 3. right-click dismisses (retained in history), second notice for forget
nid2 = notify("Second notice", "right-click me", actions=("default", "Open"), timeout=60000)
wait(lambda: len(state()["active"]["order"]) >= 1, 5, "second notice")
st = state(); ids = st["active"]["order"]
target = ids[-1]; rid = "notices.k" + target.encode().hex() + "."
time.sleep(.8)
x, y = center("notice"); pcmd(f"move {x} {y} {W} {H}"); pcmd("context"); time.sleep(.8)
st = state()
check("right-click dismisses: no longer active, retained in history (not forgotten)", target not in st["active"]["items"] and target in st["history"]["items"] or (target not in st["active"]["items"] and st.get("history_count", 0) >= 1), f"hist={list(st['history']['order'])[:3]} count={st.get('history_count')}")

# ---- 3b. another view displaces the stack: popups are suppressed (provider-only), never replayed
notify("Before calendar", actions=("default", "Open"), timeout=60000)
wait(lambda: state()["popup_order"], 5, "popup before calendar"); time.sleep(.8)
check("popups are shown as the 'notices' overlay view", overlay_state().get("view") == "notices")
ipc("bead.panel", "open", "calendar")
st = wait(lambda: (lambda s: s if s["inhibited"] else None)(state()), 5, "inhibited by the other view")
check("opening a popout suppresses popups (set_inhibited from the shell)", st["popup_order"] == [] and panel_state().get("view") == "calendar", str(panel_state())[:120])
wait(lambda: not overlay_state().get("resident"), 5, "overlay released")
check("the suppressed stack releases its overlay", overlay_state().get("resident") is False)
ipc("bead.panel", "close")
st = wait(lambda: (lambda s: s if not s["inhibited"] else None)(state()), 5, "inhibition lifted")
time.sleep(.5)
check("closing it lifts suppression without replaying popups", st["popup_order"] == [] and state()["popup_order"] == [])
# B0053: the launcher does not suppress popups; both are visible at once.
ipc("bead.panel", "open", "launcher"); time.sleep(.8)
notify("Over the launcher", actions=("default", "Open"), timeout=60000)
wait(lambda: state()["popup_order"], 5, "popup over launcher"); time.sleep(.8)
ov, ps = overlay_state(), panel_state()
check("a popup shows while the launcher is open (own overlay slot)", ov.get("view") == "notices" and ov.get("open") is True and ps.get("view") == "launcher" and ps.get("target_open") is True and state()["inhibited"] is False, str((ov, ps))[:200])
grim("3b-popup-over-launcher.png")
ipc("bead.panel", "close"); time.sleep(.5)

# ---- 4. DND: popups hide, no replay when it ends
nid3 = notify("Before DND", actions=("default", "Open"), timeout=60000)
wait(lambda: state()["popup_order"], 5, "popup before dnd")
time.sleep(.6)
r = probe.call("notifications.set_dnd", {"enabled": True}); check("set_dnd accepted", "ok" in r, str(r)[:80])
st = state(); time.sleep(.6)
check("DND empties popup_order and hides the stack surface", st["popup_order"] == [] and st["inhibited"] is True)
grim("4-dnd.png")
n4 = notify("During DND", actions=("default", "Open"), timeout=60000); time.sleep(.5)
st = state(); check("notice during DND is admitted but never a popup", n4 and st["popup_order"] == [])
probe.call("notifications.set_dnd", {"enabled": False}); time.sleep(.8)
st = state(); check("leaving DND does not replay popups", st["popup_order"] == [] and st["inhibited"] is False)
grim("4-dnd-off.png")

# ---- 5. shell restart: no replay, daemon history intact; daemon kept running
os.killpg(shell.pid, signal.SIGTERM); shell.wait(timeout=10)
time.sleep(.5)
shell2 = spawn(shell_cmd(SOCK, CMD + "2", trace=False), "shell2.log", dict(env, BEAD_PRIVATE_INPUT_PROOF="1"), cwd=str(SRC))
time.sleep(2)
st = state(); check("shell restart: daemon state intact, no popup replay", st["popup_order"] == [] and st["history_count"] >= 1, f"count={st['history_count']}")
n5 = notify("After restart", actions=("default", "Open"), timeout=60000); time.sleep(1.2)
ipc2 = lambda *a: subprocess.run([str(SHELL), "ipc", "--socket", CMD + "2", "--json", *a], env=dict(env, BEAD_PRIVATE_INPUT_PROOF="1"), capture_output=True, text=True, timeout=5)
ps2 = json.loads(ipc2("bead.panel", "state").stdout or "{}").get("result", {})
ov2 = json.loads(ipc2("bead", "overlay").stdout or "{}").get("result", {})
check("consumer shows new notices after restart", ov2.get("view") == "notices" and ov2.get("open") is True, str(ov2)[:160])
grim("5-after-restart.png")

# ---- 6. daemon loss: consumer goes stale, no local owner fallback
os.killpg(daemon.pid, signal.SIGTERM); daemon.wait(timeout=10); time.sleep(1.5)
check("no UI owner fallback: private notification name unowned after daemon exit", owner() == "b false", owner())
grim("6-daemon-gone.png")
time.sleep(.5)
check("consumer surviving daemon loss without crashing", shell2.poll() is None)
ambient_after = subprocess.run(["busctl", "--user", "call", "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner", "s", "org.freedesktop.Notifications"], capture_output=True, text=True).stdout.strip()
check("ambient (real) notification name untouched", ambient_after == ambient, f"before={ambient} after={ambient_after}")
os.killpg(shell2.pid, signal.SIGTERM); shell2.wait(timeout=10)

# ---- 7. OSD through the published osd topic from a fake producer (no daemon OSD producer exists yet)
import threading
OSOCK = str(RUN/"o.sock")
class Fake:
    """Daemon stand-in that serves only the `osd` topic. The shell opens several links
    (UI provider, wallpaper); every one gets the welcome, and the one that subscribes to
    `osd` receives the published state."""
    def __init__(self, current=None):
        self.srv = socket.socket(socket.AF_UNIX); self.srv.bind(OSOCK); os.chmod(OSOCK, 0o600); self.srv.listen(8)
        self.seq = 0; self.conn = None; self.current = current; self.ready = threading.Event()
        threading.Thread(target=self.accept, daemon=True).start()
    def accept(self):
        while True:
            try: c, _ = self.srv.accept()
            except OSError: return
            threading.Thread(target=self.serve, args=(c,), daemon=True).start()
    def serve(self, c):
        f = c.makefile("rb")
        try:
            json.loads(f.readline())
            w = json.loads(open(SRC/"crates/nacre-shell/tests/protocol-v1/notifications-loss-restart.jsonl").readline())
            w["topics"] = {"osd": 1}; w["methods"] = {}
            c.sendall((json.dumps(w) + "\n").encode())
            for line in f:
                frame = json.loads(line)
                if frame.get("t") == "sub" and "osd" in frame.get("topics", []):
                    self.conn = c; self.publish(self.current); self.ready.set()
        except (OSError, ValueError): pass
    def send(self, o): self.conn.sendall((json.dumps(o) + "\n").encode())
    def publish(self, current):
        self.seq += 1; self.current = current
        data = {"health": {"available": True, "stale": False}}
        if current: data["current"] = current
        self.send({"t": "snap", "topic": "osd", "seq": self.seq, "data": data})
def item(serial, channel, unit, value=None, muted=None, ttl=60000):
    d = {"serial": serial, "channel": channel, "unit": unit, "expires_ms": int(time.time() * 1000) + ttl}
    if value is not None: d["value"] = value
    if muted is not None: d["muted"] = muted
    return d
def ui_osd(text):
    last = None
    for line in text.splitlines():
        if line.startswith("UI_STATE "):
            try:
                state_now = json.loads(line[9:])
                if "osd.label" in state_now: last = state_now
            except ValueError: pass
    return last
fake = Fake(item(1, "volume", "perceptual", 0.3))
shell3 = spawn(shell_cmd(OSOCK, CMD + "3"), "shell3.log", dict(env, BEAD_PRIVATE_INPUT_PROOF="1"), cwd=str(SRC))
wait(lambda: "START pid" in (RAW/"shell3.log").read_text(errors="replace"), 30, "shell3 runtime start")
wait(fake.ready.is_set, 15, "shell3 subscribed to the fake osd topic"); time.sleep(1.2)
def pixel(x, y):
    out = subprocess.run(["grim", "-g", f"{x},{y} 1x1", "-t", "ppm", "-"], env=env, capture_output=True, timeout=10).stdout
    return tuple(out[-3:])
def osd_state():
    # The trace prints UI_STATE only at commits and a closed OSD session may not commit, so
    # open/closed is read from the pixels of the pill interior (dark pill on the grey nested
    # background); label/status come from the last traced state.
    st = dict(ui_osd((RAW/"shell3.log").read_text(errors="replace")) or {})
    st["osd.open"] = sum(pixel(W // 2 + 100, H - 70)) < 3 * 48
    return st
st = osd_state()
check("OSD: existing state on connect is a baseline, never replayed", st is not None and st.get("osd.open") is False, str(st and st.get("osd.open")))
fake.publish(item(2, "volume", "perceptual", 0.5, False)); time.sleep(0.6)
st = osd_state(); grim("7-osd-volume.png")
check("OSD: new volume serial shows Volume 50%", st["osd.open"] is True and st["osd.label"] == "Volume" and st["osd.status"] == "50%", f"{st['osd.label']} {st['osd.status']}")
time.sleep(2.6); st = osd_state()
check("OSD: hides after the 2000 ms delay", st["osd.open"] is False)
fake.publish(item(3, "volume", "perceptual", 0.5, False)); time.sleep(0.5)
check("OSD: repeated equal value with a new serial shows again", osd_state()["osd.open"] is True)
ptr2 = subprocess.Popen([str(BIN/"private-pointer")], env=dict(env, BEAD_PRIVATE_INPUT_PROOF="1"), stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, start_new_session=True); procs.append(ptr2); ptr2.stdout.readline()
def p2(c): ptr2.stdin.write(c + "\n"); ptr2.stdin.flush(); ptr2.stdout.readline()
p2(f"move {W // 2 + 100} {H - 70} {W} {H}"); time.sleep(3.2)
hovered = osd_state()["osd.open"]; grim("7-osd-hovered.png")
check("OSD: pointer hover holds it open past the hide delay", hovered is True)
p2(f"move 5 5 {W} {H}"); time.sleep(2.7)
check("OSD: leaving restarts the hide delay and it closes", osd_state()["osd.open"] is False)
fake.publish(item(4, "brightness", "fraction", None)); time.sleep(0.5)
check("OSD: brightness without a readable value is not shown (N0036 gate)", osd_state()["osd.open"] is False)
fake.publish(item(5, "brightness", "fraction", 0.4)); time.sleep(0.5)
st = osd_state(); grim("7-osd-brightness.png")
check("OSD: brightness with a published value shows Display brightness 40%", st["osd.open"] is True and st["osd.label"] == "Display brightness" and st["osd.status"] == "40%", f"{st['osd.label']} {st['osd.status']}")
time.sleep(2.6)
fake.publish(item(6, "volume", "perceptual", 0.9, False, ttl=-5000)); time.sleep(0.5)
check("OSD: an already-expired item never shows", osd_state()["osd.open"] is False)
fake.publish(item(7, "capslock", "boolean", 1.0)); time.sleep(0.5)
check("OSD: caps lock shows On", osd_state()["osd.open"] is True and osd_state()["osd.status"] == "On")
check("OSD: zero UI_ERROR", "UI_ERROR" not in (RAW/"shell3.log").read_text(errors="replace"))

log = shell_log() + (RAW/"shell2.log").read_text(errors="replace")
check("zero UI_ERROR in shell logs", "UI_ERROR" not in log)
stop_all(); shutil.rmtree(RUN, ignore_errors=True)
(RAW/"results.json").write_text(json.dumps(results, indent=1))
print("SUMMARY", sum(1 for _, ok, _ in results if ok), "/", len(results))
sys.exit(0 if all(ok for _, ok, _ in results) else 1)
