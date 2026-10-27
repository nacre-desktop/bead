#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Private, nested Bead daily-use acceptance runner."""
from __future__ import annotations

import argparse
import collections
import hashlib
import importlib.util
import json
import os
import pathlib
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import threading
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
HERE = pathlib.Path(__file__).resolve().parent


def load_module(name: str, path: pathlib.Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec and spec.loader
    spec.loader.exec_module(module)
    return module


# Keep process cleanup and the established geometry oracle aligned with the
# existing private measurement harness rather than maintaining a second copy.
measure = load_module("daily_acceptance_measure", ROOT / "spikes/measure.py")


def run(argv, *, env, timeout=15, check=True, text=True):
    return subprocess.run(argv, env=env, capture_output=True, timeout=timeout,
                         check=check, text=text)


def output_pixels(env: dict[str, str], raw: pathlib.Path) -> None:
    """Set Labwc's private headless output to the pointer's declared pixel space."""
    listing = run(["wlr-randr", "--json"], env=env).stdout
    (raw / "parent-outputs-before.json").write_text(listing)
    try:
        outputs = json.loads(listing)
        names = list(outputs) if isinstance(outputs, dict) else [o["name"] for o in outputs]
    except (json.JSONDecodeError, KeyError, TypeError) as exc:
        raise RuntimeError("cannot parse wlr-randr --json output") from exc
    if len(names) != 1:
        raise RuntimeError(f"expected one Labwc headless output, got {names!r}")
    name = names[0]
    run(["wlr-randr", "--output", name, "--mode", "2880x1800"], env=env)
    deadline = time.monotonic() + 3
    after = ""
    while time.monotonic() < deadline:
        after = run(["wlr-randr", "--json"], env=env).stdout
        try:
            outputs = json.loads(after)
            row = outputs[name] if isinstance(outputs, dict) else next(o for o in outputs if o["name"] == name)
            mode = row.get("current_mode", row.get("currentMode", {}))
            if (mode.get("width", row.get("width")), mode.get("height", row.get("height"))) == (2880, 1800):
                break
        except (json.JSONDecodeError, KeyError, StopIteration, TypeError):
            pass
        time.sleep(.01)
    (raw / "parent-outputs.json").write_text(after)
    try:
        outputs = json.loads(after)
        row = outputs[name] if isinstance(outputs, dict) else next(o for o in outputs if o["name"] == name)
        mode = row.get("current_mode", row.get("currentMode", {}))
        width = mode.get("width", row.get("width"))
        height = mode.get("height", row.get("height"))
    except (json.JSONDecodeError, KeyError, StopIteration, TypeError) as exc:
        raise RuntimeError("cannot verify Labwc headless output mode") from exc
    if (width, height) != (2880, 1800):
        raise RuntimeError(f"Labwc parent output is not 2880x1800: {row!r}")
    print(f"PARENT_OUTPUT name={name} pixel_size=2880x1800 absolute_pointer_space=verified", flush=True)


def niri_config(path: pathlib.Path, ctl_path: pathlib.Path) -> None:
    ctl = f"{shlex.quote(str(ctl_path))} ui "
    routes = [("Super+Space", 'nacre launcher ""'),
              ("Super+O", "drawers toggle dashboard"),
              ("Super+I", "nacre settings"),
              ("Super+C", "nacre controls home"),
              ("Super+A", "nacre left"),
              ("Super+W", 'nacre launcher ">wallpaper "'),
              ("Super+N", "nacre menu learn.shortcuts \"\"")]
    lines = ["input { keyboard { xkb { layout \"us\"; }; }; }", "prefer-no-csd",
             "animations { off; }", "hotkey-overlay { skip-at-startup; }", "binds {"]
    for key, call in routes:
        lines.append(f"    {key} repeat=false {{ spawn-sh {json.dumps(ctl + call)}; }}")
    lines.append("}")
    pearls = ROOT / "shell/niri-pearls.kdl"
    if pearls.is_file():
        lines.append(pearls.read_text())
    path.write_text("\n".join(lines) + "\n")


def process_threads(pid: int) -> dict[int, tuple[int, int, str, int]]:
    rows = {}
    for stat in pathlib.Path(f"/proc/{pid}/task").glob("*/stat"):
        try:
            line = stat.read_text()
            name = line[line.index("(") + 1:line.rindex(")")]
            fields = line[line.rindex(")") + 2:].split()
            rows[int(stat.parent.name)] = (int(fields[19]), int(fields[11]) + int(fields[12]), name, pid)
        except (OSError, ValueError, IndexError):
            continue
    return rows


def pss_anon(pid: int) -> float:
    text = pathlib.Path(f"/proc/{pid}/smaps_rollup").read_text()
    match = re.search(r"^Pss_Anon:\s+(\d+)\s+kB", text, re.MULTILINE)
    if not match:
        raise RuntimeError("kernel does not expose Pss_Anon in smaps_rollup")
    return int(match.group(1)) / 1024


def host_swapin_pages() -> int:
    text = pathlib.Path("/proc/vmstat").read_text()
    match = re.search(r"^pswpin\s+(\d+)\s*$", text, re.MULTILINE)
    if not match:
        raise RuntimeError("kernel does not expose host pswpin in /proc/vmstat")
    return int(match.group(1))


class CpuGuard(threading.Thread):
    def __init__(self, pids: list[int]):
        super().__init__(daemon=True)
        self.pids = pids
        self.failure: str | None = None
        self.stop_event = threading.Event()
        self.clock = os.sysconf("SC_CLK_TCK")

    def run(self):
        prior = {}
        for pid in self.pids:
            prior.update(process_threads(pid))
        samples = collections.deque([(time.monotonic(), prior)])
        while not self.stop_event.wait(.2):
            now = time.monotonic()
            current = {}
            for pid in self.pids:
                try:
                    current.update(process_threads(pid))
                except (FileNotFoundError, ProcessLookupError):
                    continue
            samples.append((now, current))
            while samples and now - samples[0][0] > 5.2:
                samples.popleft()
            if now - samples[0][0] < 4.9:
                continue
            start_at, start = samples[0]
            elapsed = now - start_at
            for tid, (started, ticks, name, pid) in current.items():
                old = start.get(tid)
                if old is None or old[0] != started:
                    continue
                percent = (ticks - old[1]) / self.clock / elapsed * 100
                if percent > 80:
                    self.failure = f"PID {pid} TID {tid} ({name}) at {percent:.1f}% for {elapsed:.1f}s"
                    return


class Runner:
    def __init__(self, args):
        self.args = args
        self.results = []
        self.layer_results = []
        self.owned = []
        self.root = None
        self.env = None
        self.pointer = None
        self.shell = None
        self.guard = None
        self.baseline_mem = None
        self.log_path = None
        self.layer_sample = 0
        self.initial_layer_count = None
        self.swapin_start_pages = None
        self.page_size = os.sysconf("SC_PAGE_SIZE")
        self.frame_limit_ms = None
        self.evidence = args.evidence.resolve()
        self.evidence.mkdir(mode=0o700, parents=True, exist_ok=True)
        self.failures = self.evidence / "failures"
        self.failures.mkdir(mode=0o700, exist_ok=True)
        self.profile = json.loads(args.profile.read_text())
        self._validate_profile()

    def _validate_profile(self):
        output = self.profile.get("output", {})
        if (output.get("width"), output.get("height"), output.get("scale")) != (2880, 1800, 1.5):
            raise RuntimeError("profile must describe the 2880x1800 / scale 1.5 nested output")
        points = {**self.profile.get("bar_targets", {}),
                  "outside_target": self.profile.get("outside_target"),
                  "left_edge_hover": self.profile.get("left_edge_hover"),
                  **{f"drag_{name}": point for name, point in self.profile.get("widget_drag", {}).items()}}
        required = {"updates", "audio", "network", "bluetooth", "battery", "notifications", "calendar", "power",
                    "outside_target", "left_edge_hover", "drag_bar_start", "drag_desktop_drop", "drag_bar_drop"}
        if not required.issubset(points):
            raise RuntimeError(f"profile missing coordinate targets: {sorted(required - set(points))}")
        for label, point in points.items():
            if (not isinstance(point, list) or len(point) != 2
                    or any(not isinstance(value, int) for value in point)
                    or not (0 <= point[0] < 2880 and 0 <= point[1] < 1800)):
                raise RuntimeError(f"profile point {label} must be integer pixels within 2880x1800: {point!r}")

    def start(self):
        before_free = subprocess.run(["free", "-h"], capture_output=True, text=True, check=True).stdout
        (self.evidence / "free-before.txt").write_text(before_free)
        exact = subprocess.run(["free", "-b"], capture_output=True, text=True, check=True).stdout
        swap = next((line.split() for line in exact.splitlines() if line.startswith("Swap:")), None)
        if not swap or len(swap) < 3:
            raise RuntimeError("cannot parse `free -b` Swap used before the arm")
        swap_used = int(swap[2])
        print("FREE_BEFORE " + before_free.strip().replace("\n", " | "), flush=True)
        if swap_used >= 64 * 1024 * 1024:
            raise RuntimeError(
                "host swap used is at least 64 MiB; stop between arms, ensure no build or capture is running "
                "and MemAvailable > swap_used + 8 GiB, then use the authorized `sudo swapoff -a && sudo "
                "swapon -a` reset, log free -h before/after, preserve swap devices/capacity, and recheck <64 MiB")
        self.swapin_start_pages = host_swapin_pages()
        self.root = pathlib.Path(tempfile.mkdtemp(prefix="bead-daily-acceptance-"))
        os.chmod(self.root, 0o700)
        rt = self.root / "runtime"
        rt.mkdir(mode=0o700)
        for name in ("config", "cache", "data", "state", "home", "tools"):
            (self.root / name).mkdir(mode=0o700)
        base_env = {key: value for key, value in os.environ.items()
                    if not key.startswith(("BEAD_", "NACRE_"))
                    and (not key.startswith("DBUS_") or key == "DBUS_SESSION_BUS_ADDRESS")
                    and key not in ("WAYLAND_DISPLAY", "NIRI_SOCKET", "HYPRLAND_INSTANCE_SIGNATURE",
                                    "DISPLAY", "DBUS_SYSTEM_BUS_ADDRESS")}
        self.env = {**base_env, "HOME": str(self.root / "home"), "XDG_RUNTIME_DIR": str(rt),
                    "XDG_CONFIG_HOME": str(self.root / "config"), "XDG_CACHE_HOME": str(self.root / "cache"),
                    "XDG_DATA_HOME": str(self.root / "data"), "XDG_STATE_HOME": str(self.root / "state"),
                    "XDG_DATA_DIRS": "/usr/local/share:/usr/share", "BEAD_VERBOSE_LOG": "1",
                    "BEAD_PRIVATE_SERVICES_TEST": "1", "NACRE_COMPOSITOR": "niri",
                    "WLR_BACKENDS": "headless", "WLR_HEADLESS_OUTPUTS": "1", "WLR_RENDERER": "gles2"}
        self.env["NACRE_DAEMON_SOCKET"] = str(rt / "nacre/daemon.sock")
        pathlib.Path(self.env["NACRE_DAEMON_SOCKET"]).parent.mkdir(mode=0o700, exist_ok=True)
        for key in ("WAYLAND_DISPLAY", "NIRI_SOCKET", "HYPRLAND_INSTANCE_SIGNATURE", "DISPLAY",
                    "DBUS_SYSTEM_BUS_ADDRESS"):
            self.env.pop(key, None)
        if not self.env.get("DBUS_SESSION_BUS_ADDRESS") or "/run/user/" in self.env["DBUS_SESSION_BUS_ADDRESS"]:
            raise RuntimeError("run.sh must provide a private dbus-run-session bus")
        self.env["DBUS_SYSTEM_BUS_ADDRESS"] = self.env["DBUS_SESSION_BUS_ADDRESS"]
        self.env["PIPEWIRE_REMOTE"] = "bead-private-unavailable"
        self.env["PULSE_SERVER"] = "unix:" + str(self.root / "no-pulse")
        self._stage_data()
        config = self.root / "labwc"
        config.mkdir(mode=0o700)
        (config / "autostart").write_text("")
        (config / "rc.xml").write_text(
            '<labwc_config><windowRules><windowRule identifier="*"><decorations>no</decorations>'
            '<action name="ResizeTo" width="2880" height="1800"/>'
            '<action name="MoveTo" x="0" y="0"/></windowRule></windowRules></labwc_config>')
        parent_rt = self.root / "parent-runtime"
        parent_rt.mkdir(mode=0o700)
        parent_env = {**self.env, "XDG_RUNTIME_DIR": str(parent_rt)}
        parent_env.pop("WAYLAND_DISPLAY", None)
        self._spawn([shutil.which("labwc") or "labwc", "--config-dir", str(config)],
                    parent_env, self.evidence / "labwc.log")
        parent_sock = self._wait_wayland(parent_rt, "Labwc")
        self.env["WAYLAND_DISPLAY"] = str(parent_sock)
        output_pixels(self.env, self.evidence)
        pointer_dir = self.root / "pointer-bin"
        run([str(HERE / "build-pointer.sh"), str(pointer_dir)], env=self.env)
        # Keep the virtual pointer device alive across hover and click actions.
        pointer_env = {**self.env, "WAYLAND_DISPLAY": str(parent_sock)}
        self.pointer = subprocess.Popen([str(pointer_dir / "daily-acceptance-pointer"), "2880", "1800"],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=(self.evidence / "pointer.log").open("w"),
                                        env=pointer_env, text=True, bufsize=1, start_new_session=True)
        self.owned.append(self.pointer)
        if self.pointer.stdout.readline().strip() != "POINTER_READY":
            raise RuntimeError("Labwc did not expose zwlr_virtual_pointer_manager_v1")
        niri_path = self.root / "niri.kdl"
        niri_config(niri_path, self.args.ctl.resolve())
        self._spawn(["niri", "--config", str(niri_path)], self.env, self.evidence / "niri.log")
        child_sock = self._wait_wayland(pathlib.Path(self.env["XDG_RUNTIME_DIR"]), "nested niri")
        self.env["WAYLAND_DISPLAY"] = str(child_sock)
        self.env["BEAD_PARENT_WAYLAND"] = str(parent_sock)
        niri_sockets = list(pathlib.Path(self.env["XDG_RUNTIME_DIR"]).glob("niri.*.sock"))
        if len(niri_sockets) != 1:
            raise RuntimeError(f"expected one private niri IPC socket, found {niri_sockets}")
        self.env["NIRI_SOCKET"] = str(niri_sockets[0])
        outputs = json.loads(run(["niri", "msg", "--json", "outputs"], env=self.env).stdout)
        if len(outputs) != 1:
            raise RuntimeError(f"expected one nested niri output, got {outputs!r}")
        output_name = next(iter(outputs))
        run(["niri", "msg", "output", output_name, "scale", "1.5"], env=self.env)
        deadline = time.monotonic() + 3
        while True:
            outputs = json.loads(run(["niri", "msg", "--json", "outputs"], env=self.env).stdout)
            try:
                measure.validate_dashboard_output_geometry("niri", outputs)
                break
            except RuntimeError:
                if time.monotonic() >= deadline:
                    raise
                time.sleep(.01)
        output = outputs[output_name]
        mode = output["modes"][output["current_mode"]]
        refresh_hz = float(mode["refresh_rate"]) / 1000
        self.frame_limit_ms = 2000 / refresh_hz
        (self.evidence / "niri-outputs.json").write_text(json.dumps(outputs, indent=2) + "\n")
        if self.args.tools:
            self.env["NACRE_HELPER_TOOLS"] = str(self.args.tools.resolve())
        daemon_env = {**self.env, "WAYLAND_DISPLAY": str(child_sock), "NIRI_SOCKET": str(niri_sockets[0])}
        self._spawn([str(self.args.daemon), "daemon"], daemon_env, self.evidence / "daemon.log")
        daemon_cli = [str(self.args.ctl), "call", "--timeout", "2000", "daemon.status", "{}"]
        deadline = time.monotonic() + 30
        while True:
            try:
                run(daemon_cli, env=daemon_env, timeout=5)
                break
            except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
                if time.monotonic() >= deadline:
                    raise RuntimeError("private Nacre daemon did not become ready; see daemon.log")
                time.sleep(.2)
        shell_env = {**daemon_env, "WAYLAND_DEBUG": "1"}
        shell_cmd = [str(self.args.bead), "--bar", "--core-views", "--ui-provider", "--notices",
                     "--daemon-socket", self.env["NACRE_DAEMON_SOCKET"], "--command-socket",
                     str(pathlib.Path(self.env["XDG_RUNTIME_DIR"]) / "nacre/bead-command.sock"),
                     "--namespace", "bead-daily-acceptance"]
        if self.args.palette:
            shell_cmd += ["--palette", str(self.args.palette.resolve())]
        self.log_path = self.evidence / "shell.log"
        self.shell = self._spawn(shell_cmd, shell_env, self.log_path, cwd=self.args.shell)
        self._wait_log("READY", 60)
        text = self.log_path.read_text(errors="replace")
        if "UI_COMMIT" not in text or "UI_ERROR" in text:
            raise RuntimeError("shell failed initial UI commit; inspect shell.log")
        self.start_memory = measure.memory(self.shell.pid)
        self.baseline_mem = self.start_memory["pss_anon_mib"]
        self.initial_layer_count, _ = self._layer_count()
        if self.initial_layer_count is None:
            raise RuntimeError("niri msg layers did not report the Bead namespace before interactions")
        self.guard = CpuGuard([process.pid for process in self.owned])
        self.guard.start()
        self._shot(self.evidence / "baseline.png")
        fingerprints = {}
        for label, path in (("bead", self.args.bead.resolve()), ("daemon", self.args.daemon.resolve())):
            digest = hashlib.sha256()
            with path.open("rb") as stream:
                for block in iter(lambda: stream.read(1024 * 1024), b""):
                    digest.update(block)
            fingerprints[label] = {"artifact": path.name, "sha256": digest.hexdigest()}
        (self.evidence / "build-inputs.json").write_text(json.dumps(fingerprints, indent=2) + "\n")
        print(f"HARNESS_READY parent=2880x1800 niri=2880x1800@1.5 refresh={refresh_hz:g}Hz "
              f"baseline_Pss_Anon={self.baseline_mem:.2f}MiB mapped_layers={self.initial_layer_count}", flush=True)

    def _stage_data(self):
        source = self.args.data
        source = source.expanduser().resolve()
        needed = ("wallpapers", "applications", "icons", "state/nacre/presentation.json")
        missing = [item for item in needed if not (source / item).exists()]
        if missing:
            raise RuntimeError(f"staged owner data incomplete at {source}: missing {missing}")
        for item in needed[:3]:
            src = source / item
            dest = (self.root / "home/Pictures/Wallpapers" if item == "wallpapers"
                    else self.root / "data" / item)
            dest.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
            shutil.copytree(src, dest, dirs_exist_ok=True, symlinks=False)
        record = source / needed[3]
        state_record = self.root / "state/nacre/presentation.json"
        state_record.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        data = json.loads(record.read_text())
        poster = data.get("poster") if isinstance(data, dict) else None
        if not isinstance(poster, str):
            raise RuntimeError("staged presentation.json must contain a poster path")
        poster_path = pathlib.Path(poster)
        if not poster_path.is_absolute():
            raise RuntimeError("staged presentation.json poster path must be absolute")
        if poster_path.is_absolute() and poster_path.is_relative_to(source / "wallpapers"):
            poster = str(self.root / "home/Pictures/Wallpapers" / poster_path.relative_to(source / "wallpapers"))
        elif poster_path.is_absolute():
            staged = source / "wallpapers" / poster_path.name
            if not staged.is_file():
                raise RuntimeError("presentation poster is outside staged wallpapers and its basename is absent")
            poster = str(self.root / "home/Pictures/Wallpapers" / staged.name)
        data["poster"] = poster
        if not pathlib.Path(poster).is_file():
            raise RuntimeError(f"staged presentation poster is not readable: {poster}")
        state_record.write_text(json.dumps(data, indent=2) + "\n")
        state_record.chmod(0o600)
        print("DATA_STAGE wallpapers/applications/icons/presentation copied to private XDG paths", flush=True)

    def _spawn(self, argv, env, log, cwd=None):
        stream = log.open("w")
        process = subprocess.Popen(argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                                   stdout=stream, stderr=subprocess.STDOUT,
                                   start_new_session=True)
        process._daily_log = stream
        self.owned.append(process)
        return process

    def _wait_wayland(self, runtime, label):
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            sockets = [p for p in runtime.glob("wayland-*") if p.is_socket()]
            if sockets:
                return sockets[0]
            if any(p.poll() is not None for p in self.owned):
                raise RuntimeError(f"{label} exited before its socket appeared")
            time.sleep(.1)
        raise RuntimeError(f"{label} did not create a Wayland socket")

    def _wait_log(self, marker, seconds=10, after=0):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if self.shell.poll() is not None:
                raise RuntimeError("Bead shell exited; inspect shell.log")
            text = self.log_path.read_text(errors="replace")
            if marker in text[after:]:
                return text
            time.sleep(.025)
        raise RuntimeError(f"shell log marker timed out: {marker}")

    def _pointer(self, action, point):
        x, y = point
        self.pointer.stdin.write(f"{action} {int(x)} {int(y)}\n")
        self.pointer.stdin.flush()
        reply = self.pointer.stdout.readline().strip()
        if reply != "POINTER_ACK":
            raise RuntimeError(f"virtual pointer did not acknowledge {action}: {reply!r}")

    def _reset_closed(self):
        self._key("-k", "Escape")
        self._pointer("click", self.profile["outside_target"])
        self._wait_closed()

    def _wait_closed(self, timeout=2):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            count, _ = self._layer_count()
            if count == self.initial_layer_count:
                return True
            if self.shell.poll() is not None:
                break
            time.sleep(.01)
        return False

    def _open_latency(self, offset, expected_name=None):
        deadline = time.monotonic() + 2
        while time.monotonic() < deadline:
            latency = self._opening_latency(offset)
            if latency[1] is not None and (expected_name is None or latency[0] == expected_name):
                return latency
            if latency[1] is not None and expected_name is not None and latency[0] != expected_name:
                return latency
            if self.shell.poll() is not None:
                break
            time.sleep(.025)
        return (None, None)

    def _surface_action(self, label, opening, closing):
        before = self.evidence / "current-before.png"
        after = self.evidence / "current-after.png"
        self._reset_closed()
        self._shot(before)
        offset = self.log_path.stat().st_size
        opening()
        expected = self.profile["surface_names"].get(label)
        latency = self._open_latency(offset, expected)
        closing()
        closed = self._wait_closed()
        self._shot(after)
        empty = self._diff(before, after)
        closed_layers, _ = self._layer_count()
        unmapped = closed and closed_layers == self.initial_layer_count
        expected = self.profile["surface_names"].get(label)
        latency_ok = (latency[1] is not None and latency[0] == expected
                      and latency[1] <= self.frame_limit_ms)
        passed = latency_ok and empty and unmapped
        row = {"check": label, "result": "PASS" if passed else "FAIL",
               "surface_name": latency[0], "expected_surface_name": expected,
               "latency_ms": latency[1], "frame_limit_ms": self.frame_limit_ms,
               "pixel_diff_empty": empty, "layer_unmapped_at_close": unmapped,
               "layer_count_before_any_open": self.initial_layer_count, "layer_count_closed": closed_layers}
        self.layer_results.append(row)
        self.results.append(row)
        if not passed:
            stem = f"r{self.round_no}-{re.sub(r'[^A-Za-z0-9_.-]+', '_', label)}-{len(self.results)}"
            shutil.copy2(before, self.failures / f"{stem}-before.png")
            shutil.copy2(after, self.failures / f"{stem}-after.png")

    def _key(self, *args):
        offset = self.log_path.stat().st_size if self.log_path and self.log_path.exists() else 0
        run(["wtype", *args], env=self.env, timeout=10)
        if args and all(not arg.startswith("-") for arg in args):
            self._wait_log("UI_COMMIT", 1, offset)

    def _shot(self, path):
        run(["grim", str(path)], env=self.env, timeout=10)
        size = run(["identify", "-format", "%wx%h", str(path)], env=self.env, timeout=10).stdout
        if size != "2880x1800":
            raise RuntimeError(f"full-window screenshot has unexpected size {size!r}: {path}")

    def _wait_presentation_change(self, path, poster, timeout=5):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                current = json.loads(path.read_text()).get("poster")
            except (OSError, json.JSONDecodeError):
                current = None
            if current != poster:
                return current
            time.sleep(.01)
        return poster

    def _diff(self, before, after):
        result = subprocess.run(["compare", "-metric", "AE", str(before), str(after), "null:"],
                                env=self.env, capture_output=True, text=True, timeout=10)
        try:
            return int(result.stderr.strip().split()[0]) == 0
        except (ValueError, IndexError):
            return False

    def _opening_latency(self, offset):
        text = self.log_path.read_text(errors="replace")[offset:]
        matches = re.findall(r"PANEL_OPEN_LATENCY name=([^ ]+) ms_to_first_content_frame=([0-9.]+)", text)
        return (matches[-1][0], float(matches[-1][1])) if matches else (None, None)

    def _layer_count(self):
        """Count this namespace in niri's compositor-side mapped-layer inventory."""
        result = subprocess.run(["niri", "msg", "layers"], env=self.env,
                                capture_output=True, text=True, timeout=5)
        raw = result.stdout + result.stderr
        self.layer_sample += 1
        (self.evidence / f"layers-{self.layer_sample:03}.txt").write_text(raw)
        if result.returncode or "bead-daily-acceptance" not in raw:
            return None, raw
        return raw.count("bead-daily-acceptance"), raw

    def click_dropdown(self, name, point, rounds):
        for dismiss in ("click", "escape", "outside"):
            for _ in range(rounds):
                before = self.evidence / "current-before.png"
                after = self.evidence / "current-after.png"
                self._reset_closed()
                self._shot(before)
                open_offset = self.log_path.stat().st_size
                self._pointer("click", point)
                expected = self.profile["surface_names"].get(f"dropdown:{name}")
                latency = self._open_latency(open_offset, expected)
                if dismiss == "click":
                    self._pointer("click", point)
                elif dismiss == "escape":
                    self._key("-k", "Escape")
                else:
                    self._pointer("click", self.profile["outside_target"])
                closed = self._wait_closed()
                self._shot(after)
                empty = self._diff(before, after)
                closed_layers, _ = self._layer_count()
                unmapped = closed and closed_layers == self.initial_layer_count
                latency_value = latency[1]
                latency_ok = (latency_value is not None and latency[0] == expected
                              and latency_value <= self.frame_limit_ms)
                passed = empty and unmapped and latency_ok
                row = {"check": f"dropdown:{name}", "dismiss": dismiss,
                       "result": "PASS" if passed else "FAIL", "latency_ms": latency_value,
                       "surface_name": latency[0], "expected_surface_name": expected,
                       "frame_limit_ms": self.frame_limit_ms,
                       "pixel_diff_empty": empty, "layer_unmapped_at_close": unmapped,
                       "layer_count_before_any_open": self.initial_layer_count,
                       "layer_count_closed": closed_layers}
                self.results.append(row)
                self.layer_results.append(row)
                if not passed:
                    stem = f"r{self.round_no}-{name}-{dismiss}-{len(self.results)}"
                    shutil.copy2(before, self.failures / f"{stem}-before.png")
                    shutil.copy2(after, self.failures / f"{stem}-after.png")

    def _super(self, key):
        self._key("-M", "logo", "-k", key, "-m", "logo")

    def simple_surface(self, label, opening, closing):
        self._surface_action(label, opening, closing)

    def wallpaper_picker(self):
        before = self.evidence / "current-before.png"
        after = self.evidence / "current-after.png"
        self._reset_closed()
        state_record = pathlib.Path(self.env["XDG_STATE_HOME"]) / "nacre/presentation.json"
        start_presentation = json.loads(state_record.read_text())
        start_poster = start_presentation.get("poster")
        offset = self.log_path.stat().st_size
        self._super("w")
        self._key("nacre")
        expected = self.profile["surface_names"].get("wallpaper picker")
        latency = self._open_latency(offset, expected)
        self._key("-k", "Down")
        self._key("-k", "Return")
        # Choosing the highlighted wallpaper applies it; Escape closes the
        # picker after the daemon has acknowledged the selection.
        self._key("-k", "Return")
        applied_poster = self._wait_presentation_change(state_record, start_poster)
        applied = applied_poster != start_poster
        # The apply changes the wallpaper behind the picker. Close once, then
        # take a fresh closed-state baseline and run a second open/close cycle
        # to isolate leftover panel pixels from the intentional wallpaper change.
        self._key("-k", "Escape")
        closed = self._wait_closed()
        self._shot(before)
        second_open_offset = self.log_path.stat().st_size
        self._super("w")
        self._key("nacre")
        second_latency = self._open_latency(second_open_offset, expected)
        self._key("-k", "Escape")
        closed_again = self._wait_closed()
        self._shot(after)
        empty = self._diff(before, after)
        closed_layers, _ = self._layer_count()
        unmapped = closed and closed_again and closed_layers == self.initial_layer_count
        passed = (latency[1] is not None and latency[0] == expected
                  and latency[1] <= self.frame_limit_ms and second_latency[1] is not None
                  and second_latency[0] == expected and second_latency[1] <= self.frame_limit_ms
                  and empty and unmapped and applied)
        row = {"check": "wallpaper picker step/apply/close", "result": "PASS" if passed else "FAIL",
               "surface_name": latency[0], "expected_surface_name": expected,
               "latency_ms": latency[1], "reopen_latency_ms": second_latency[1],
               "frame_limit_ms": self.frame_limit_ms, "pixel_diff_empty_after_apply": empty,
               "layer_unmapped_at_close": unmapped, "wallpaper_apply_changed_presentation": applied,
               "layer_count_before_any_open": self.initial_layer_count, "layer_count_closed": closed_layers,
               "data_staged": True}
        self.results.append(row)
        self.layer_results.append(row)
        if not passed:
            stem = f"r{self.round_no}-wallpaper-{len(self.results)}"
            shutil.copy2(before, self.failures / f"{stem}-before.png")
            shutil.copy2(after, self.failures / f"{stem}-after.png")

    def widget_drag(self):
        drag = self.profile["widget_drag"]
        before = self.evidence / "widget-drag-before.png"
        middle = self.evidence / "widget-drag-desktop.png"
        after = self.evidence / "widget-drag-after.png"
        self._reset_closed()
        self._shot(before)
        offset = self.log_path.stat().st_size
        self._pointer("down", drag["bar_start"])
        self._pointer("move", drag["desktop_drop"])
        self._wait_log("UI_COMMIT", 2, offset)
        self._shot(middle)
        ghost_visible = not self._diff(before, middle)
        self._pointer("move", drag["bar_drop"])
        release_offset = self.log_path.stat().st_size
        self._pointer("up", drag["bar_drop"])
        self._wait_log("UI_COMMIT", 2, release_offset)
        self._shot(after)
        errors = re.findall(r"^.*UI_ERROR.*$", self.log_path.read_text(errors="replace"), re.MULTILINE)
        passed = ghost_visible and not errors
        self.results.append({"check": "widget drag bar→desktop→bar",
                             "result": "PASS" if passed else "FAIL",
                             "desktop_ghost_visible": ghost_visible,
                             "ui_errors_seen": len(errors)})
        if not passed:
            stem = f"r{self.round_no}-widget-drag"
            shutil.copy2(before, self.failures / f"{stem}-before.png")
            shutil.copy2(middle, self.failures / f"{stem}-desktop.png")
            shutil.copy2(after, self.failures / f"{stem}-after.png")

    def one_round(self, round_no):
        self.round_no = round_no
        targets = self.profile["bar_targets"]
        for name, point in targets.items():
            self.click_dropdown(name, point, 1)
        # Keyboard bindings mirror Nacre's calls in scripts/try-bead.py, with
        # Super in the nested compositor to exercise the requested chord.
        self.simple_surface("launcher open/type/Escape", lambda: (self._super("space"), self._key("nacre")),
                            lambda: self._key("-k", "Escape"))
        self.simple_surface("launcher toggle", lambda: self._super("space"), lambda: self._super("space"))
        self.simple_surface("dashboard/calendar", lambda: self._super("o"), lambda: self._key("-k", "Escape"))
        self.simple_surface("control panel", lambda: self._super("c"),
                            lambda: self._key("-k", "Escape"))
        hover = self.profile["left_edge_hover"]
        self.simple_surface("AI panel hover open", lambda: self._pointer("move", hover),
                            lambda: self._key("-k", "Escape"))
        self.simple_surface("AI panel click open", lambda: self._pointer("click", hover),
                            lambda: self._key("-k", "Escape"))
        self.simple_surface("notification list", lambda: self._pointer("click", targets["notifications"]),
                            lambda: self._key("-k", "Escape"))
        self.wallpaper_picker()
        self.widget_drag()
        self._shot(self.evidence / f"round-{round_no}-end.png")

    def run(self):
        self.start()
        if self.args.quick:
            self.one_round(1)
            self._record_memory("quick round end")
        else:
            for number in range(1, 4):
                self.one_round(number)
            print("IDLE_BEGIN seconds=120", flush=True)
            time.sleep(120)
            self._record_memory("after 120-second idle")
            idle_layers, _ = self._layer_count()
            self.results.append({"check": "closed panel layer retirement after idle",
                                 "result": "PASS" if idle_layers == self.initial_layer_count else "FAIL",
                                 "namespace_layer_count_after_idle": idle_layers,
                                 "baseline_namespace_layer_count": self.initial_layer_count,
                                 "panels_checked": len(self.layer_results),
                                 "method": "niri msg layers, matching bead-daily-acceptance namespace"})
            self.one_round(4)
            self._record_memory("after post-idle round")
        text = self.log_path.read_text(errors="replace")
        ui_errors = re.findall(r"^.*UI_ERROR.*$", text, re.MULTILINE)
        self.results.append({"check": "UI_ERROR", "result": "FAIL" if ui_errors else "PASS",
                             "count": len(ui_errors)})
        self.results.append({"check": "thread CPU <=80%/5s",
                             "result": "FAIL" if self.guard.failure else "PASS",
                             "detail": self.guard.failure or "none"})
        swapin_end = host_swapin_pages()
        swapin_bytes = (swapin_end - self.swapin_start_pages) * self.page_size
        self.results.append({"check": "host swap-in <1 MiB",
                             "result": "PASS" if 0 <= swapin_bytes < 1024**2 else "FAIL",
                             "delta_bytes": swapin_bytes})
        swap_rows = []
        for process in self.owned:
            if process.poll() is None:
                swap_rows.append({"pid": process.pid,
                                  "swap_pss_mib": measure.memory(process.pid)["swap_pss_mib"]})
        swap_ok = all(row["swap_pss_mib"] == 0 for row in swap_rows)
        self.results.append({"check": "owned process SwapPss=0",
                             "result": "PASS" if swap_ok else "FAIL", "processes": swap_rows})
        free_after = subprocess.run(["free", "-h"], capture_output=True, text=True, check=True).stdout
        (self.evidence / "free-after.txt").write_text(free_after)
        free_bytes = subprocess.run(["free", "-b"], capture_output=True, text=True, check=True).stdout
        swap_line = next((line.split() for line in free_bytes.splitlines() if line.startswith("Swap:")), None)
        swap_used_end = int(swap_line[2]) if swap_line and len(swap_line) >= 3 else None
        self.results.append({"check": "host swap remains below 64 MiB",
                             "result": "PASS" if swap_used_end is not None and swap_used_end < 64 * 1024**2 else "FAIL",
                             "swap_used_bytes_after": swap_used_end})
        if any(row["result"] != "PASS" for row in self.results):
            last_round = 1 if self.args.quick else 4
            final_shot = self.evidence / f"round-{last_round}-end.png"
            if final_shot.exists():
                shutil.copy2(final_shot, self.failures / "global-metric-failure.png")
        self.guard.stop_event.set()
        self.guard.join(timeout=2)
        self.write_report()
        return 1 if any(row["result"] != "PASS" for row in self.results) else 0

    def _record_memory(self, phase):
        sample = measure.memory(self.shell.pid)
        anon = sample["pss_anon_mib"]
        delta = anon - self.baseline_mem
        self.results.append({"check": f"Pss_Anon +20 MiB ({phase})",
                             "result": "PASS" if delta <= 20 else "FAIL",
                             "start_mib": round(self.baseline_mem, 3),
                             "sample_mib": round(anon, 3),
                             "delta_mib": round(delta, 3), "limit_mib": 20})

    def write_report(self):
        mode = "quick (one round, no idle)" if self.args.quick else "full (three rounds, 120-second idle, repeat)"
        out = [f"# Daily acceptance result — {mode}", "", "| Check | Result | Evidence |", "|---|---:|---|"]
        for row in self.results:
            detail = ", ".join(f"{k}={v}" for k, v in row.items() if k not in ("check", "result"))
            out.append(f"| {row['check']} | {row['result']} | {detail} |")
        out += ["", f"Failure screenshots: `{self.failures}`", "",
                "Runtime calibration: bar points use the standard B0152 layout defaults; use `--profile` if the staged user has a different order.",
                "Surface lifecycle: immediate close is checked with `niri msg layers`; the 120-second idle separately checks that the shell namespace returns to its pre-open mapped-layer count.", ""]
        report = self.evidence / "report.md"
        report.write_text("\n".join(out))
        print("\n".join(out), flush=True)

    def close(self):
        if self.guard:
            self.guard.stop_event.set()
            self.guard.join(timeout=2)
        if self.pointer and self.pointer.poll() is None:
            try:
                self.pointer.stdin.write("quit\n")
                self.pointer.stdin.flush()
                self.pointer.wait(timeout=3)
            except (OSError, subprocess.TimeoutExpired):
                measure.stop(self.pointer)
        for process in reversed(self.owned):
            try:
                measure.stop(process)
            except Exception as exc:
                print(f"CLEANUP_ERROR pid={process.pid}: {exc}", file=sys.stderr)
            stream = getattr(process, "_daily_log", None)
            if stream:
                stream.close()
        if self.root:
            shutil.rmtree(self.root, ignore_errors=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bead", type=pathlib.Path,
                        default=ROOT / "target/release/nacre-shell")
    parser.add_argument("--shell", type=pathlib.Path, default=ROOT / "shell")
    parser.add_argument("--daemon", type=pathlib.Path,
                        default=pathlib.Path("/usr/bin/nacre-helper"))
    parser.add_argument("--ctl", type=pathlib.Path,
                        default=pathlib.Path(shutil.which("nacrectl") or "nacrectl"))
    parser.add_argument("--tools", type=pathlib.Path,
                        help="matching Nacre nacre/shell-tools directory for daemon routes")
    parser.add_argument("--palette", type=pathlib.Path)
    parser.add_argument("--data", type=pathlib.Path,
                        default=pathlib.Path.home() / "nacre-lead/b0162-daily-acceptance/staged-data")
    parser.add_argument("--profile", type=pathlib.Path, default=HERE / "default-profile.json")
    parser.add_argument("--quick", action="store_true",
                        help="run one interaction round without the 120-second idle and final repeat")
    parser.add_argument("--evidence", type=pathlib.Path,
                        default=pathlib.Path.home() / "nacre-lead/b0162-daily-acceptance/evidence")
    parser.add_argument("--capture-guard-child", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--capture-guard-ready-file", type=pathlib.Path, help=argparse.SUPPRESS)
    parser.add_argument("--capture-guard-release-file", type=pathlib.Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    for path in (args.bead, args.shell, args.daemon, args.ctl):
        if not path.exists() or (path.is_file() and path in (args.bead, args.daemon, args.ctl)
                                 and not os.access(path, os.X_OK)):
            parser.error(f"required input does not exist: {path}")
    if args.capture_guard_child:
        if not args.capture_guard_ready_file or not args.capture_guard_release_file:
            parser.error("capture safety child requires ready and release files")
        measure.capture_safety.wait_for_release(args.capture_guard_ready_file,
                                                args.capture_guard_release_file)
    os.setpriority(os.PRIO_PROCESS, 0, max(10, os.getpriority(os.PRIO_PROCESS, 0)))
    runner = Runner(args)
    try:
        return runner.run()
    except Exception as exc:
        print(f"HARNESS_ERROR {type(exc).__name__}: {exc}", file=sys.stderr, flush=True)
        if not runner.results:
            runner.results.append({"check": "harness startup", "result": "FAIL",
                                   "detail": f"{type(exc).__name__}: {exc}"})
        runner.write_report()
        return 2
    finally:
        runner.close()


if __name__ == "__main__":
    raise SystemExit(main())
