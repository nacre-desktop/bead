#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Private, nested Bead daily-use acceptance runner."""
from __future__ import annotations

import argparse
import collections
import hashlib
import importlib.util
import json
import math
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

ROOT = pathlib.Path(__file__).resolve().parents[2]
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


def process_cpu_ticks(pid: int) -> tuple[int, int]:
    """Return user and system CPU ticks from /proc, excluding comm spaces."""
    stat = pathlib.Path(f"/proc/{pid}/stat").read_text()
    fields = stat[stat.rfind(")") + 2:].split()
    return int(fields[11]), int(fields[12])


def process_thread_cpu_ticks(pid: int) -> dict[int, tuple[int, int, str]]:
    """Return each live thread's user/system ticks and comm name."""
    result = {}
    for stat_path in pathlib.Path(f"/proc/{pid}/task").glob("*/stat"):
        try:
            stat = stat_path.read_text()
            name = stat[stat.index("(") + 1:stat.rindex(")")]
            fields = stat[stat.rindex(")") + 2:].split()
            result[int(stat_path.parent.name)] = (int(fields[11]), int(fields[12]), name)
        except (OSError, ValueError, IndexError):
            continue
    return result


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
    run(["wlr-randr", "--output", name, "--custom-mode", "2880x1800@60Hz"], env=env)
    deadline = time.monotonic() + 3
    after = ""
    while time.monotonic() < deadline:
        after = run(["wlr-randr", "--json"], env=env).stdout
        try:
            outputs = json.loads(after)
            row = outputs[name] if isinstance(outputs, dict) else next(o for o in outputs if o["name"] == name)
            mode = row.get("current_mode", row.get("currentMode", {}))
            if not mode and isinstance(row.get("modes"), list):
                mode = next((candidate for candidate in row["modes"] if candidate.get("current")), {})
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
        if not mode and isinstance(row.get("modes"), list):
            mode = next((candidate for candidate in row["modes"] if candidate.get("current")), {})
        width = mode.get("width", row.get("width"))
        height = mode.get("height", row.get("height"))
    except (json.JSONDecodeError, KeyError, StopIteration, TypeError) as exc:
        raise RuntimeError("cannot verify Labwc headless output mode") from exc
    if (width, height) != (2880, 1800):
        raise RuntimeError(f"Labwc parent output is not 2880x1800: {row!r}")
    print(f"PARENT_OUTPUT name={name} pixel_size=2880x1800 absolute_pointer_space=verified", flush=True)


def niri_config(path: pathlib.Path, ctl_path: pathlib.Path, route_log: pathlib.Path) -> None:
    ctl = f'NACRE_DAEMON_SOCKET="$XDG_RUNTIME_DIR/nacre/daemon.sock" {shlex.quote(str(ctl_path))} ui '
    routes = [("Mod+Space", 'nacre launcher ""'),
              ("Mod+O", "drawers toggle dashboard"),
              ("Mod+I", "nacre settings"),
              ("Mod+C", "nacre controls home"),
              ("Mod+A", "nacre left"),
              ("Mod+W", 'nacre launcher ">wallpaper "'),
              ("Mod+N", 'nacre menu learn.shortcuts ""')]
    lines = ["input { keyboard { xkb { layout \"us\"; }; }; }", "prefer-no-csd",
             "animations { off; }", "hotkey-overlay { skip-at-startup; }", "binds {"]
    for key, arguments in routes:
        command = ctl + arguments + f" >> {shlex.quote(str(route_log))} 2>&1"
        lines.append(f"    {key} repeat=false {{ spawn-sh {json.dumps(command)}; }}")
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
        self.persistent_root = None
        self.runtime_root = None
        cache_snapshot = os.environ.get("BEAD_DAILY_CACHE_SNAPSHOT")
        self.cache_snapshot = pathlib.Path(cache_snapshot).expanduser() if cache_snapshot else None
        self.env = None
        self.key_env = None
        self.pointer = None
        self.shell = None
        self.daemon = None
        self.guard = None
        self.baseline_mem = None
        self.log_path = None
        self.initial_layer_count = None
        self.closed_layer_inventory = None
        self.waived_closed_layer_identities = []
        self.swapin_start_pages = None
        self.page_size = os.sysconf("SC_PAGE_SIZE")
        self.frame_limit_ms = None
        self.last_latency_source = None
        self.last_panel_latency_ms = None
        self.last_first_feedback_ns = None
        self.wallpaper_route_count = 0
        self.last_wallpaper_route_log = None
        self.last_wallpaper_route_process = None
        self.last_wallpaper_route_end_written = False
        self.layer_sample = 0
        self.last_pointer_location = None
        self.render_backend = args.panel_renderer or "CPU"
        self.evidence = args.evidence.resolve()
        self.evidence.mkdir(mode=0o700, parents=True, exist_ok=True)
        self.failures = self.evidence / "failures"
        self.failures.mkdir(mode=0o700, exist_ok=True)
        self.profile = json.loads(args.profile.read_text())
        self.diff_before = None
        self.diff_after = None
        self._validate_profile()

    def _validate_profile(self):
        output = self.profile.get("output", {})
        if (output.get("width"), output.get("height"), output.get("scale")) != (2880, 1800, 1.5):
            raise RuntimeError("profile must describe the 2880x1800 / scale 1.5 nested output")
        points = {**self.profile.get("bar_targets", {}),
                  "outside_target": self.profile.get("outside_target"),
                  "title_target": self.profile.get("title_target"),
                  "left_edge_hover": self.profile.get("left_edge_hover"),
                  **{f"battery_gap_{index}": point for index, point in
                     enumerate(self.profile.get("battery_hover_gap_path", []))},
                  **{f"drag_{name}": point for name, point in self.profile.get("widget_drag", {}).items()}}
        required = {"updates", "audio", "network", "bluetooth", "battery", "notifications", "calendar", "power",
                    "outside_target", "title_target", "left_edge_hover", "drag_bar_start", "drag_desktop_drop", "drag_bar_drop"}
        if not self.profile.get("battery_hover_gap_path"):
            required.add("battery_gap_path")
        if not required.issubset(points):
            raise RuntimeError(f"profile missing coordinate targets: {sorted(required - set(points))}")
        for label, point in points.items():
            if (not isinstance(point, list) or len(point) != 2
                    or any(not isinstance(value, int) for value in point)
                    or not (0 <= point[0] < 2880 and 0 <= point[1] < 1800)):
                raise RuntimeError(f"profile point {label} must be integer pixels within 2880x1800: {point!r}")
        for rect in self.profile.get("volatile_rectangles", []):
            if (not isinstance(rect, list) or len(rect) != 4
                    or any(type(value) is not int for value in rect)
                    or rect[0] < 0 or rect[1] < 0 or rect[2] <= 0 or rect[3] <= 0
                    or rect[0] + rect[2] > 2880 or rect[1] + rect[3] > 1800):
                raise RuntimeError(f"profile volatile rectangle must fit the output: {rect!r}")

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
        persistent_root = os.environ.get("BEAD_DAILY_HOME_ROOT")
        if persistent_root:
            self.root = pathlib.Path(persistent_root).expanduser().resolve()
            self.root.mkdir(mode=0o700, parents=True, exist_ok=True)
            os.chmod(self.root, 0o700)
            self.persistent_root = self.root
            # Reuse only private HOME/XDG roots. Every compositor, runtime
            # socket, pointer helper, and shell/daemon process is new per arm.
            for name in ("pointer-bin", "labwc"):
                shutil.rmtree(self.root / name, ignore_errors=True)
            for name in ("niri.kdl", "closed-before.png", "closed-after.png", "no-pulse"):
                path = self.root / name
                if path.is_dir():
                    shutil.rmtree(path, ignore_errors=True)
                else:
                    path.unlink(missing_ok=True)
        else:
            self.root = pathlib.Path(tempfile.mkdtemp(prefix="bead-daily-acceptance-"))
            os.chmod(self.root, 0o700)
        runtime_root = os.environ.get("BEAD_DAILY_RUNTIME_ROOT")
        if runtime_root:
            self.runtime_root = pathlib.Path(runtime_root).expanduser().resolve()
            shutil.rmtree(self.runtime_root, ignore_errors=True)
            self.runtime_root.mkdir(mode=0o700, parents=True)
            os.chmod(self.runtime_root, 0o700)
        else:
            self.runtime_root = self.root
        self.diff_before = self.root / "closed-before.png"
        self.diff_after = self.root / "closed-after.png"
        rt = self.runtime_root / "runtime"
        rt.mkdir(mode=0o700, parents=True)
        for name in ("config", "cache", "data", "state", "home", "tools"):
            (self.root / name).mkdir(mode=0o700, exist_ok=True)
        if self.cache_snapshot and self.cache_snapshot.is_dir():
            shutil.copytree(self.cache_snapshot, self.root / "cache", dirs_exist_ok=True)
        base_env = {key: value for key, value in os.environ.items()
                    if not key.startswith(("BEAD_", "NACRE_"))
                    and (not key.startswith("DBUS_") or key == "DBUS_SESSION_BUS_ADDRESS")
                    and key not in ("WAYLAND_DISPLAY", "NIRI_SOCKET", "HYPRLAND_INSTANCE_SIGNATURE",
                                    "DISPLAY", "DBUS_SYSTEM_BUS_ADDRESS",
                                    "GNOME_KEYRING_CONTROL", "GNOME_KEYRING_PID")}
        self.env = {**base_env, "HOME": str(self.root / "home"), "XDG_RUNTIME_DIR": str(rt),
                    "XDG_CONFIG_HOME": str(self.root / "config"), "XDG_CACHE_HOME": str(self.root / "cache"),
                    "XDG_DATA_HOME": str(self.root / "data"), "XDG_STATE_HOME": str(self.root / "state"),
                    "XDG_DATA_DIRS": "/usr/local/share:/usr/share", "BEAD_VERBOSE_LOG": "1",
                    "BEAD_ACCEPTANCE_PANEL_LATENCY": "1",
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
        parent_rt = self.runtime_root / "parent-runtime"
        parent_rt.mkdir(mode=0o700)
        parent_env = {**self.env, "XDG_RUNTIME_DIR": str(parent_rt)}
        parent_env.pop("WAYLAND_DISPLAY", None)
        self._spawn([shutil.which("labwc") or "labwc", "--config-dir", str(config)],
                    parent_env, self.evidence / "labwc.log")
        parent_sock = self._wait_wayland(parent_rt, "Labwc")
        self.key_env = {**self.env, "XDG_RUNTIME_DIR": str(parent_rt),
                        "WAYLAND_DISPLAY": str(parent_sock)}
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
        niri_config(niri_path, self.args.ctl.resolve(), self.evidence / "routes.log")
        shutil.copy2(niri_path, self.evidence / "niri.kdl")
        self.niri = self._spawn(["niri", "--config", str(niri_path)], self.env, self.evidence / "niri.log")
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
        self.frame_limit_ms = self.args.latency_limit_ms or 65.0
        (self.evidence / "niri-outputs.json").write_text(json.dumps(outputs, indent=2) + "\n")
        if self.args.tools:
            self.env["NACRE_HELPER_TOOLS"] = str(self.args.tools.resolve())
            self.env["NACRE_HELPER_BINARY"] = str(self.args.daemon.resolve())
            # The Nacre daemon launches wallpaper-media through `nice` by name.
            # Keep that lookup deterministic inside a private acceptance HOME and
            # ensure the task-local wrappers (when present) can observe the child.
            self.env["PATH"] = os.pathsep.join((
                str(self.args.daemon.resolve().parent), "/usr/bin", "/bin"))
            (self.evidence / "acceptance-environment.json").write_text(json.dumps({
                "HOME": self.env["HOME"],
                "XDG_CONFIG_HOME": self.env["XDG_CONFIG_HOME"],
                "XDG_CACHE_HOME": self.env["XDG_CACHE_HOME"],
                "XDG_DATA_HOME": self.env["XDG_DATA_HOME"],
                "XDG_STATE_HOME": self.env["XDG_STATE_HOME"],
                "PATH": self.env["PATH"],
                "NACRE_HELPER_BINARY": self.env["NACRE_HELPER_BINARY"],
                "NACRE_HELPER_TOOLS": self.env["NACRE_HELPER_TOOLS"],
                "persistent_private_home_root": str(self.persistent_root) if self.persistent_root else None,
                "ephemeral_runtime_root": str(self.runtime_root),
                "fresh_shell_and_daemon": True,
                "WALLPAPER_LIBRARY": str(self.args.data.expanduser().resolve() / "wallpapers"),
            }, indent=2) + "\n")
        daemon_env = {**self.env, "WAYLAND_DISPLAY": str(child_sock), "NIRI_SOCKET": str(niri_sockets[0])}
        self.daemon = self._spawn([str(self.args.daemon), "daemon"], daemon_env,
                                  self.evidence / "daemon.log")
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
        shell_env = dict(daemon_env)
        if self.args.wallpaper_live or self.args.wallpaper_open_only:
            shell_env["BEAD_ACCEPTANCE_OPEN_TRACE"] = "1"
        if self.args.wayland_debug:
            shell_env["WAYLAND_DEBUG"] = "client"
        if self.args.slide_trace:
            shell_env["BEAD_VERBOSE_LOG"] = "1"
        shell_cmd = [str(self.args.bead), "--bar", "--core-views", "--ui-provider", "--notices",
                     "--daemon-socket", self.env["NACRE_DAEMON_SOCKET"], "--command-socket",
                     str(pathlib.Path(self.env["XDG_RUNTIME_DIR"]) / "nacre/bead-command.sock"),
                     "--namespace", "bead-daily-acceptance"]
        if self.args.panel_renderer:
            shell_cmd += ["--panel-renderer", self.args.panel_renderer]
        if self.args.renderer:
            shell_cmd += ["--renderer", self.args.renderer]
        if self.args.palette:
            shell_cmd += ["--palette", str(self.args.palette.resolve())]
        if self.args.slide_trace:
            shell_cmd += ["--trace-state"]
        self.log_path = self.evidence / "shell.log"
        self.shell = self._spawn(shell_cmd, shell_env, self.log_path, cwd=self.args.shell)
        self._wait_log("READY", 60)
        text = self.log_path.read_text(errors="replace")
        if "UI_COMMIT" not in text or "UI_ERROR" in text:
            raise RuntimeError("shell failed initial UI commit; inspect shell.log")
        self.start_memory = measure.memory(self.shell.pid)
        self.baseline_mem = self.start_memory["pss_anon_mib"]
        self.initial_layer_count, self.initial_layer_inventory = self._layer_count("startup")
        self.closed_layer_inventory = self.initial_layer_inventory
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
        fingerprints["panel_renderer_override"] = self.args.panel_renderer or "default"
        fingerprints["renderer_override"] = self.args.renderer or "default-auto"
        fingerprints["render_backend"] = self.render_backend
        fingerprints["latency_boundary"] = "Super+W nacrectl UI route invocation to first wp_presentation_feedback"
        (self.evidence / "build-inputs.json").write_text(json.dumps(fingerprints, indent=2) + "\n")
        self.results.append({"check": "render backend", "result": "PASS",
                             "backend": self.render_backend,
                             "detail": "bar auto-fallback and status popout specs default to CPU"})
        print(f"HARNESS_READY parent=2880x1800 niri=2880x1800@1.5 refresh={refresh_hz:g}Hz "
              f"baseline_Pss_Anon={self.baseline_mem:.2f}MiB mapped_layers={self.initial_layer_count}", flush=True)

    def _stage_data(self):
        source = self.args.data
        source = source.expanduser().resolve()
        needed = ("wallpapers", "applications", "icons", "state/nacre/presentation.json")
        missing = [item for item in needed if not (source / item).exists()]
        if missing:
            raise RuntimeError(f"staged owner data incomplete at {source}: missing {missing}")
        # Keep the private wallpaper library at its stable task-private path.
        # Display-raster cache keys include source path and file identity, so
        # copying owner files into each random HOME invalidates every raster.
        # HOME and every XDG root remain fresh for this arm.
        wallpaper_library = source / "wallpapers"
        wallpaper_files = sorted(
            path for path in wallpaper_library.rglob("*")
            if path.is_file() and not path.is_symlink()
        )
        identities = []
        for path in wallpaper_files:
            stat = path.stat()
            identities.append({"path": str(path), "size": stat.st_size,
                               "mtime_ns": stat.st_mtime_ns, "inode": stat.st_ino,
                               "device": stat.st_dev})
        (self.evidence / "wallpaper-library-identities.json").write_text(
            json.dumps(identities, indent=2) + "\n")
        cli_config = self.root / "config/nacre/cli.json"
        cli_config.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        cli_config.write_text(json.dumps({"wallpaper": {"directory": str(wallpaper_library)}}) + "\n")
        cli_config.chmod(0o600)
        for item in needed[1:3]:
            src = source / item
            dest = self.root / "data" / item
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
            poster = str(wallpaper_library / poster_path.relative_to(source / "wallpapers"))
        elif poster_path.is_absolute():
            candidates = list((source / "wallpapers").rglob(poster_path.name))
            if len(candidates) != 1:
                raise RuntimeError("presentation poster is outside staged wallpapers and its basename is absent")
            poster = str(candidates[0])
        data["poster"] = poster
        if not pathlib.Path(poster).is_file():
            raise RuntimeError(f"staged presentation poster is not readable: {poster}")
        state_record.write_text(json.dumps(data, indent=2) + "\n")
        state_record.chmod(0o600)
        print(f"DATA_STAGE wallpapers={wallpaper_library} (stable task-private path); "
              "applications/icons/presentation copied to private XDG paths", flush=True)

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
        self.last_pointer_location = (int(x), int(y))
        self.pointer.stdin.write(f"{action} {int(x)} {int(y)}\n")
        self.pointer.stdin.flush()
        reply = self.pointer.stdout.readline().strip()
        if reply != "POINTER_ACK":
            raise RuntimeError(f"virtual pointer did not acknowledge {action}: {reply!r}")

    def _reset_closed(self):
        self._key("-k", "Escape")
        self._pointer("click", self.profile["outside_target"])
        self._wait_closed()

    def _wait_closed(self, timeout=.5):
        deadline = time.monotonic() + timeout
        last_inventory = None
        stable_polls = 0
        while time.monotonic() < deadline:
            count, inventory = self._layer_count("close-poll")
            if inventory in (self.initial_layer_inventory, self.closed_layer_inventory):
                stable_polls = stable_polls + 1 if inventory == last_inventory else 1
                last_inventory = inventory
                if stable_polls >= 3:
                    return True
            else:
                stable_polls = 0
                last_inventory = None
            if self.shell.poll() is not None:
                break
            time.sleep(.01)
        return False

    def _closed_layers_match(self, inventory, pixel_diff_empty, label):
        if inventory in (self.initial_layer_inventory, self.closed_layer_inventory):
            return True
        if inventory is None or not pixel_diff_empty:
            return False
        before = collections.Counter(json.dumps(row, sort_keys=True)
                                     for row in self.initial_layer_inventory)
        after = collections.Counter(json.dumps(row, sort_keys=True) for row in inventory)
        added = after - before
        removed = before - after
        identities = (
            {"namespace": "bead-daily-acceptance", "output": "winit",
             "layer": "Top", "keyboard_interactivity": "None"},
            {"namespace": "bead-daily-acceptance", "output": "winit",
             "layer": "Background", "keyboard_interactivity": "None"},
            {"namespace": "bead-daily-acceptance", "output": "winit",
             "layer": "Overlay", "keyboard_interactivity": "None"},
        )
        if removed or not added or any(count != 1 for count in added.values()):
            return False
        known = {json.dumps(item, sort_keys=True): item for item in identities}
        if any(encoded not in known for encoded in added):
            return False
        added_identities = [known[encoded] for encoded in added]
        for identity in added_identities:
            if identity not in self.waived_closed_layer_identities:
                self.waived_closed_layer_identities.append(identity)
        self.closed_layer_inventory = inventory
        self.results.append({"check": "stable blank baseline layer retained", "result": "PASS",
                             "action": label, "identities": added_identities,
                             "reason": "only known blank widget/bar layers remain while the closed screenshot equals clean state"})
        return True

    def _idle_layers_match(self, inventory):
        if inventory in (self.initial_layer_inventory, self.closed_layer_inventory):
            return True
        if inventory is None:
            return False
        before = collections.Counter(json.dumps(row, sort_keys=True)
                                     for row in self.initial_layer_inventory)
        after = collections.Counter(json.dumps(row, sort_keys=True) for row in inventory)
        added = after - before
        removed = before - after
        if removed:
            return False
        known = {json.dumps(identity, sort_keys=True)
                 for identity in self.waived_closed_layer_identities}
        return bool(added) and all(encoded in known and count == 1
                                   for encoded, count in added.items())

    def _opening_latency(self, offset, expected_name=None, started_ns=None):
        text = self.log_path.read_text(errors="replace")[offset:]
        matches = re.findall(
            r"PANEL_OPEN_LATENCY name=([^ ]+) ms_to_first_content_frame=([0-9.]+)"
            r"(?: monotonic_ns=(\d+))?",
            text,
        )
        if not matches:
            self.last_panel_latency_ms = None
            self.last_first_feedback_ns = None
            return (None, None)
        name, panel_value, presented_ns = matches[0]
        self.last_panel_latency_ms = float(panel_value)
        self.last_first_feedback_ns = int(presented_ns) if presented_ns else None
        if started_ns is not None and self.last_first_feedback_ns is not None:
            self.last_latency_source = "nacrectl UI route invocation to wp_presentation_feedback"
            return (name, max(0., (self.last_first_feedback_ns - started_ns) / 1_000_000.))
        self.last_latency_source = "PanelVisibility to wp_presentation_feedback (legacy marker)"
        return (name, float(panel_value))

    def _open_route_phases(self, shell_offset, route_offset):
        shell_text = self.log_path.read_text(errors="replace")[shell_offset:]
        route_path = self.last_wallpaper_route_log or (self.evidence / "routes.log")
        route_text = route_path.read_text(errors="replace")[route_offset:]
        phases = {}
        for key, pattern, source in (
            ("remote_received_ns", r"WALLPAPER_OPEN_REMOTE_RECEIVED monotonic_ns=(\d+)", shell_text),
            ("remote_enqueued_ns", r"WALLPAPER_OPEN_REMOTE_ENQUEUED monotonic_ns=(\d+)", shell_text),
            ("companion_select_ns", r"WALLPAPER_OPEN_COMPANION_SELECT monotonic_ns=(\d+)", shell_text),
            ("panel_start_ns", r"WALLPAPER_OPEN_PANEL_START monotonic_ns=(\d+)", shell_text),
            ("route_start_ns", r"NACRE_ROUTE_START monotonic_ns=(\d+)", route_text),
            ("route_end_ns", r"NACRE_ROUTE_END monotonic_ns=(\d+)", route_text),
        ):
            matches = re.findall(pattern, source)
            phases[key] = int(matches[-1]) if matches else None
        phases["first_feedback_ns"] = self.last_first_feedback_ns
        return phases

    def _wallpaper_process_tree(self):
        roots = [process.pid for process in
                 (self.daemon, self.shell, self.last_wallpaper_route_process)
                 if process is not None and process.poll() is None]
        pending = list(roots)
        seen = set()
        rows = []
        while pending:
            pid = pending.pop()
            if pid in seen:
                continue
            seen.add(pid)
            proc = pathlib.Path(f"/proc/{pid}")
            try:
                stat = (proc / "stat").read_text()
                end = stat.rindex(")")
                comm = stat[stat.index("(") + 1:end]
                fields = stat[end + 2:].split()
                nice = int(fields[16])
                cmdline = (proc / "cmdline").read_bytes().replace(b"\0", b" ").decode(errors="replace")
                argv = shlex.split(cmdline)
                safe_command = pathlib.Path(argv[0]).name if argv else comm
                safe_args = ["<path>" if "/" in arg or "\\" in arg else arg
                             for arg in argv[1:4]]
                rows.append({"nice": nice, "comm": comm,
                             "command": " ".join([safe_command, *safe_args])})
                for children in (proc / "task").glob("*/children"):
                    pending.extend(int(value) for value in children.read_text().split())
            except (OSError, ValueError, IndexError):
                continue
        return rows

    def _finish_wallpaper_route_if_exited(self):
        process = self.last_wallpaper_route_process
        if (
            process is None
            or process.poll() is None
            or self.last_wallpaper_route_end_written
        ):
            return process.poll() if process is not None else None
        stream = getattr(process, "_daily_log", None)
        if stream and not stream.closed:
            stream.write(
                f"NACRE_ROUTE_END monotonic_ns={time.monotonic_ns()} "
                f"exit_code={process.returncode}\n"
            )
            stream.flush()
            stream.close()
        self.last_wallpaper_route_end_written = True
        return process.returncode

    def _open_latency(self, offset, expected_name=None, started_ns=None, before=None, label="open"):
        self.last_latency_source = None
        timeout = 10 if "wallpaper" in label or label.startswith("wl") else 2
        deadline = time.monotonic() + timeout
        trace_processes = "wallpaper" in label or label.startswith("wl")
        next_process_sample = time.monotonic()
        self.last_open_process_samples = []
        while time.monotonic() < deadline:
            self._finish_wallpaper_route_if_exited()
            latency = self._opening_latency(offset, expected_name, started_ns)
            if latency[1] is not None:
                self._layer_count(f"{label}-opened")
                return latency
            if trace_processes and time.monotonic() >= next_process_sample:
                self.last_open_process_samples.append({
                    "sample_monotonic_ns": time.monotonic_ns(),
                    "processes": self._wallpaper_process_tree(),
                })
                next_process_sample = time.monotonic() + .1
            if self.shell.poll() is not None:
                break
            time.sleep(.025)
        self._layer_count(f"{label}-open-timeout")
        return (None, None)

    @staticmethod
    def _surface_name_matches(actual, expected):
        return actual == expected or (
            actual == "panel" and expected in ("panel", "control", "ai-chat", "picker", "dashboard")
        )

    def _surface_action(self, label, opening, closing):
        before = self.evidence / "current-before.png"
        after = self.evidence / "current-after.png"
        self._reset_closed()
        self._shot(before)
        offset = self.log_path.stat().st_size
        started_ns = time.monotonic_ns()
        opening()
        expected = self.profile["surface_names"].get(label)
        latency = self._open_latency(offset, expected, started_ns, before, label)
        latency_source = self.last_latency_source
        slide = self._slide_visible(latency)
        closing()
        self._pointer("move", self.profile["outside_target"])
        closed = self._wait_closed()
        empty = self._settled_empty(before, after)
        closed_layers, closed_inventory = self._layer_count(f"{label}-closed")
        unmapped = self._closed_layers_match(closed_inventory, empty, label)
        expected = self.profile["surface_names"].get(label)
        latency_ok = (latency[1] is not None and self._surface_name_matches(latency[0], expected)
                      and latency[1] <= self.frame_limit_ms)
        passed = latency_ok and empty and unmapped
        row = {"check": label, "result": "PASS" if passed else "FAIL",
               "surface_name": latency[0], "expected_surface_name": expected,
               "latency_ms": latency[1], "latency_source": latency_source,
               "frame_limit_ms": self.frame_limit_ms, "slide_visible": slide,
               "pixel_diff_empty": empty, "layer_unmapped_at_close": unmapped,
               "layer_count_before_any_open": self.initial_layer_count, "layer_count_closed": closed_layers}
        self.layer_results.append(row)
        self.results.append(row)
        if not passed:
            stem = f"r{self.round_no}-{re.sub(r'[^A-Za-z0-9_.-]+', '_', label)}-{len(self.results)}"
            shutil.copy2(before, self.failures / f"{stem}-before.png")
            shutil.copy2(after, self.failures / f"{stem}-after.png")

    def _slide_visible(self, latency):
        """A shot right after the first presented frame must differ from the
        settled open state: the panel is still sliding. None when it never opened."""
        if latency[1] is None:
            return None
        mid = self.evidence / "slide-mid.png"
        settled = self.evidence / "slide-settled.png"
        self._shot(mid)
        time.sleep(.6)
        self._shot(settled)
        return not self._diff(mid, settled)

    def _key(self, *args):
        offset = self.log_path.stat().st_size if self.log_path and self.log_path.exists() else 0
        run(["wtype", "-s", "20", *args], env=self.env, timeout=10)
        if args and all(not arg.startswith("-") for arg in args):
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                if "UI_COMMIT" in self.log_path.read_text(errors="replace")[offset:]:
                    return True
                if self.shell.poll() is not None:
                    break
                time.sleep(.025)
            return False
        return True

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

    def _diff(self, before, after, extra_rectangles=None):
        left, right = before, after
        rectangles = [*self.profile.get("volatile_rectangles", []), *(extra_rectangles or [])]
        if rectangles:
            for source, target in ((left, self.diff_before), (right, self.diff_after)):
                args = ["convert", str(source)]
                for x, y, width, height in rectangles:
                    args += ["-fill", "black", "-draw",
                             f"rectangle {x},{y} {x + width - 1},{y + height - 1}"]
                run([*args, str(target)], env=self.env, timeout=10)
            left, right = self.diff_before, self.diff_after
        result = subprocess.run(["compare", "-metric", "AE", str(left), str(right), "null:"],
                                env=self.env, capture_output=True, text=True, timeout=10)
        try:
            return int(result.stderr.strip().split()[0]) == 0
        except (ValueError, IndexError):
            return False

    def _settled_empty(self, before, after, timeout=.5):
        self._shot(after)
        deadline = time.monotonic() + timeout
        while not self._diff(before, after) and time.monotonic() < deadline:
            time.sleep(.025)
            self._shot(after)
        return self._diff(before, after)

    def _layer_count(self, label="sample"):
        """Return the Bead layer identities from Niri's compositor inventory."""
        result = subprocess.run(["niri", "msg", "--json", "layers"], env=self.env,
                                capture_output=True, text=True, timeout=5)
        raw = result.stdout
        self.layer_sample += 1
        try:
            payload = json.loads(raw)
        except json.JSONDecodeError:
            payload = None
        layers = []

        def collect(value):
            if isinstance(value, dict):
                if "namespace" in value:
                    layers.append(value)
                else:
                    for child in value.values():
                        collect(child)
            elif isinstance(value, list):
                for child in value:
                    collect(child)
        if payload is not None:
            collect(payload)
        bead_layers = [layer for layer in layers
                       if "bead-daily-acceptance" in str(layer.get("namespace", ""))]
        inventory = sorted(bead_layers, key=lambda item: json.dumps(item, sort_keys=True))
        with (self.evidence / "layers.jsonl").open("a") as stream:
            stream.write(json.dumps({"sample": self.layer_sample, "label": label,
                                     "layers": inventory, "raw": raw}, sort_keys=True) + "\n")
        if result.returncode or not layers or not bead_layers:
            return None, None
        return len(inventory), inventory

    def click_dropdown(self, name, point, rounds):
        # Warm each lazy view with keyboard dismissal before measuring the
        # repeated click-to-close path; all three close methods still run once.
        for dismiss in ("escape", "outside", "click"):
            for _ in range(rounds):
                before = self.evidence / "current-before.png"
                after = self.evidence / "current-after.png"
                self._reset_closed()
                self._shot(before)
                open_offset = self.log_path.stat().st_size
                started_ns = time.monotonic_ns()
                self._pointer("click", point)
                expected = self.profile["surface_names"].get(f"dropdown:{name}")
                latency = self._open_latency(open_offset, expected, started_ns, before,
                                             f"r{self.round_no}-{name}-{dismiss}")
                latency_source = self.last_latency_source
                slide = self._slide_visible(latency) if dismiss == "escape" else None
                if dismiss == "click":
                    self._pointer("click", point)
                elif dismiss == "escape":
                    self._key("-k", "Escape")
                else:
                    self._pointer("click", self.profile["outside_target"])
                if dismiss != "outside":
                    self._pointer("move", self.profile["outside_target"])
                closed = self._wait_closed()
                empty = self._settled_empty(before, after)
                closed_layers, closed_inventory = self._layer_count(f"{name}-{dismiss}-closed")
                unmapped = self._closed_layers_match(closed_inventory, empty, f"{name}-{dismiss}")
                latency_value = latency[1]
                latency_ok = (latency_value is not None and self._surface_name_matches(latency[0], expected)
                              and latency_value <= self.frame_limit_ms)
                passed = empty and unmapped and latency_ok
                row = {"check": f"dropdown:{name}", "dismiss": dismiss,
                       "result": "PASS" if passed else "FAIL", "latency_ms": latency_value,
                       "surface_name": latency[0], "expected_surface_name": expected,
                       "frame_limit_ms": self.frame_limit_ms,
                       "latency_source": latency_source, "slide_visible": slide,
                       "pixel_diff_empty": empty, "layer_unmapped_at_close": unmapped,
                       "layer_count_before_any_open": self.initial_layer_count,
                       "layer_count_closed": closed_layers}
                self.results.append(row)
                self.layer_results.append(row)
                if not passed:
                    stem = f"r{self.round_no}-{name}-{dismiss}-{len(self.results)}"
                    shutil.copy2(before, self.failures / f"{stem}-before.png")
                    shutil.copy2(after, self.failures / f"{stem}-after.png")

    def battery_gap_rearm(self):
        self._reset_closed()
        before = self.evidence / "current-before.png"
        after = self.evidence / "current-after.png"
        self._shot(before)
        offset = self.log_path.stat().st_size
        started_ns = time.monotonic_ns()
        self._pointer("click", self.profile["bar_targets"]["battery"])
        opened = self._open_latency(offset, self.profile["surface_names"].get("dropdown:battery"),
                                    started_ns, before, "battery-gap-open")
        latency_source = self.last_latency_source
        for target in self.profile["battery_hover_gap_path"]:
            self._pointer("move", target)
        self._shot(after)
        x, y = self.last_pointer_location
        still_open = not self._diff(before, after, [[max(0, x - 24), max(0, y - 24), 48, 48]])
        self._layer_count("battery-gap-still-open")
        self._key("-k", "Escape")
        self._pointer("move", self.profile["outside_target"])
        self._wait_closed()
        row = {"check": "battery dropdown stays open across gap/re-arm path",
               "result": "PASS" if opened[1] is not None and still_open else "FAIL",
               "latency_ms": opened[1], "latency_source": latency_source,
               "surface_name": opened[0], "still_open_after_path": still_open}
        self.results.append(row)
        if row["result"] != "PASS":
            self._shot(self.failures / "battery-gap-rearm.png")

    def dropdown_switch(self, first, second):
        """With `first` click-open, one click on `second` must open its dropdown at once."""
        targets = self.profile["bar_targets"]
        label = f"dropdown switch {first}->{second}"
        self._reset_closed()
        before = self.evidence / "current-before.png"
        self._shot(before)
        offset = self.log_path.stat().st_size
        started_ns = time.monotonic_ns()
        self._pointer("click", targets[first])
        opened = self._open_latency(offset, self.profile["surface_names"].get(f"dropdown:{first}"),
                                    started_ns, before, f"{label}-first")
        time.sleep(.6)
        self._shot(self.evidence / f"{first}-open.png")
        offset = self.log_path.stat().st_size
        started_ns = time.monotonic_ns()
        self._pointer("click", targets[second])
        switched = self._open_latency(offset, self.profile["surface_names"].get(f"dropdown:{second}"),
                                      started_ns, before, f"{label}-second")
        time.sleep(.8)
        reference = self.evidence / f"{first}-to-{second}.png"
        self._shot(reference)
        self._reset_closed()
        # The same two clicks 80 ms apart, as a fast user would: the second dropdown must still win.
        self._pointer("click", targets[first])
        time.sleep(.08)
        self._pointer("click", targets[second])
        time.sleep(1.2)
        rapid = self.evidence / f"{first}-to-{second}-rapid.png"
        self._shot(rapid)
        rapid_same = self._diff(reference, rapid)
        self._reset_closed()
        passed = opened[1] is not None and switched[1] is not None and rapid_same
        self.results.append({"check": label, "result": "PASS" if passed else "FAIL",
                             "first_ms": opened[1], "second_ms": switched[1],
                             "second_surface": switched[0], "rapid_switch_shows_second": rapid_same})
        if not passed:
            shutil.copy2(rapid, self.failures / f"{first}-to-{second}-rapid.png")

    def title_hover_opens_dashboard(self):
        """Hovering the bar's centre title (not only the screen edge) opens the dashboard."""
        self._reset_closed()
        offset = self.log_path.stat().st_size
        self._pointer("move", self.profile["title_target"])
        opened = False
        deadline = time.monotonic() + 2.5
        while time.monotonic() < deadline and not opened:
            _, inventory = self._layer_count("title-hover")
            opened = (self._opening_latency(offset)[1] is not None
                      or (inventory is not None and inventory != self.closed_layer_inventory))
            time.sleep(.05)
        time.sleep(2.5)  # a cold dashboard needs its first content frame before the evidence shot
        self._shot(self.evidence / "title-hover.png")
        self._key("-k", "Escape")
        self._pointer("move", self.profile["outside_target"])
        self._wait_closed()
        self.results.append({"check": "title hover opens dashboard",
                             "result": "PASS" if opened else "FAIL"})

    def pill_has_no_gaps(self):
        """Sweeping the pointer along the pill must show a hover background at every step."""
        x0, x1, y, step = self.profile["pill_sweep"]
        height = 30
        origin_x, origin_y = 1, 27  # parent pointer space to the nested screenshot
        region = self.evidence / "pill-region.png"
        base = self.evidence / "pill-base.png"
        def crop(path):
            self._shot(self.evidence / "pill-full.png")
            run(["convert", str(self.evidence / "pill-full.png"), "-crop",
                 f"{x1 - x0}x{height}+{x0 - origin_x}+{y - origin_y - height // 2}", "+repage", str(path)],
                env=self.env, timeout=10)
        self._reset_closed()
        crop(base)
        misses = []
        self._pointer("move", (x0, y))  # the first hover after a reset needs a moment to settle
        time.sleep(.3)
        for x in range(x0 + step, x1, step):
            self._pointer("move", (x, y))
            time.sleep(.08)
            crop(region)
            same = subprocess.run(["compare", "-metric", "AE", str(base), str(region), "null:"],
                                  env=self.env, capture_output=True, text=True, timeout=10)
            if same.stderr.strip().split()[0] == "0":
                misses.append(x)
        self._pointer("move", self.profile["outside_target"])
        self._key("-k", "Escape")
        self._wait_closed()
        self.results.append({"check": "status pill hit areas have no gaps",
                             "result": "PASS" if not misses else "FAIL", "unhovered_x": misses})

    # Super chords from the parent seat do not reach nested niri's bindings, so
    # invoke the exact nacrectl UI route installed by the Super+W binding.
    SUPER_ROUTES = {"space": ["nacre", "launcher", ""], "o": ["drawers", "toggle", "dashboard"],
                    "c": ["nacre", "controls", "home"], "w": ["nacre", "launcher", ">wallpaper "]}

    def _super(self, key):
        if key == "w":
            route_log = self.evidence / f"wallpaper-route-{self.wallpaper_route_count}.log"
            self.wallpaper_route_count += 1
            self.last_wallpaper_route_log = route_log
            route_log.write_text(f"NACRE_ROUTE_START monotonic_ns={time.monotonic_ns()}\n")
            stream = route_log.open("a")
            process = subprocess.Popen(
                [str(self.args.ctl.resolve()), "ui", "nacre", "launcher", ">wallpaper "],
                env=self.env,
                stdin=subprocess.DEVNULL,
                stdout=stream,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            process._daily_log = stream
            self.owned.append(process)
            self.last_wallpaper_route_process = process
            self.last_wallpaper_route_end_written = False
            return
        socket = pathlib.Path(self.env["XDG_RUNTIME_DIR"]) / "nacre/bead-command.sock"
        deadline = time.monotonic() + 5
        while not socket.is_socket():
            if self.shell.poll() is not None:
                raise RuntimeError(f"Bead command server exited before becoming ready: {socket}")
            if time.monotonic() >= deadline:
                raise RuntimeError(f"Bead command server did not create its private socket: {socket}")
            time.sleep(.01)
        try:
            run([str(self.args.bead), "ipc", "--socket", str(socket), *self.SUPER_ROUTES[key]],
                env=self.env, timeout=10)
        except subprocess.CalledProcessError as exc:
            detail = exc.stderr or exc.stdout or ""
            if isinstance(detail, bytes):
                detail = detail.decode(errors="replace")
            detail = detail.strip()
            raise RuntimeError(f"Bead IPC route {key!r} failed: {detail or exc}") from exc

    def simple_surface(self, label, opening, closing):
        self._surface_action(label, opening, closing)

    def wallpaper_picker(self):
        before = self.evidence / "current-before.png"
        after = self.evidence / "current-after.png"
        self._reset_closed()
        self._shot(before)
        state_record = pathlib.Path(self.env["XDG_STATE_HOME"]) / "nacre/presentation.json"
        start_presentation = json.loads(state_record.read_text())
        start_poster = start_presentation.get("poster")
        offset = self.log_path.stat().st_size
        expected = self.profile["surface_names"].get("wallpaper picker")
        started_ns = time.monotonic_ns()
        self._super("w")
        latency = self._open_latency(offset, expected, started_ns, before, f"r{self.round_no}-wallpaper-first")
        self._key("nacre")
        latency_source = self.last_latency_source
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
        second_started_ns = time.monotonic_ns()
        self._super("w")
        second_latency = self._open_latency(second_open_offset, expected, second_started_ns, before,
                                            f"r{self.round_no}-wallpaper-second")
        self._key("nacre")
        reopen_latency_source = self.last_latency_source
        self._key("-k", "Escape")
        closed_again = self._wait_closed()
        self._shot(after)
        empty = self._diff(before, after)
        closed_layers, closed_inventory = self._layer_count("wallpaper-closed")
        unmapped = self._closed_layers_match(closed_inventory, empty, "wallpaper")
        passed = (latency[1] is not None and self._surface_name_matches(latency[0], expected)
                  and latency[1] <= self.frame_limit_ms and second_latency[1] is not None
                  and self._surface_name_matches(second_latency[0], expected)
                  and second_latency[1] <= self.frame_limit_ms
                  and empty and unmapped and applied)
        row = {"check": "wallpaper picker step/apply/close", "result": "PASS" if passed else "FAIL",
               "surface_name": latency[0], "expected_surface_name": expected,
               "latency_ms": latency[1], "latency_source": latency_source,
               "reopen_latency_ms": second_latency[1], "reopen_latency_source": reopen_latency_source,
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

    def wallpaper_open_only(self):
        """Measure repeated wallpaper picker opens in one shell/daemon session."""
        self._reset_closed()
        expected = self.profile["surface_names"].get("wallpaper picker")
        limit = self.args.latency_limit_ms or 300.0
        rows = []
        for index in range(self.args.wallpaper_opens):
            before = self.evidence / f"wallpaper-open-{index}-before.png"
            first_frame = self.evidence / f"wallpaper-open-{index}-first-frame.png"
            mid = self.evidence / f"wallpaper-open-{index}-mid.png"
            settled = self.evidence / f"wallpaper-open-{index}-settled.png"
            self._shot(before)
            shell_offset = self.log_path.stat().st_size
            started_ns = time.monotonic_ns()
            self._super("w")
            name, latency = self._open_latency(
                shell_offset, expected, started_ns, before, f"wallpaper-open-{index}"
            )
            latency_source = self.last_latency_source
            panel_latency = self.last_panel_latency_ms
            route_process = self.last_wallpaper_route_process
            route_cancelled = False
            if route_process is not None:
                route_deadline = time.monotonic() + 2
                while route_process.poll() is None and time.monotonic() < route_deadline:
                    time.sleep(.01)
                route_cancelled = route_process.poll() is None
                if route_cancelled:
                    route_process.terminate()
                    try:
                        route_process.wait(timeout=2)
                    except subprocess.TimeoutExpired:
                        pass
                    self._finish_wallpaper_route_if_exited()
            phases = self._open_route_phases(shell_offset, 0)
            if latency is not None:
                self._shot(first_frame)
                time.sleep(.08)
                self._shot(mid)
                time.sleep(.28)
                self._shot(settled)
                slide_visible = not self._diff(mid, settled)
            else:
                slide_visible = False
            self._key("-k", "Escape")
            closed = self._wait_closed()
            count, inventory = self._layer_count(f"wallpaper-open-{index}-closed")
            layers_closed = closed and inventory in (
                self.initial_layer_inventory, self.closed_layer_inventory
            )
            route_start = phases["route_start_ns"]
            route_end = phases["route_end_ns"]
            route_ms = ((route_end - route_start) / 1_000_000.
                        if route_start is not None and route_end is not None else None)
            rows.append({
                "open": index + 1,
                "surface_name": name,
                "first_content_frame_ms_from_route_invocation": latency,
                "panel_start_to_frame_ms": panel_latency,
                "nacrectl_route_ms": route_ms,
                "nacrectl_exit_code": route_process.poll() if route_process else None,
                "nacrectl_cancelled_after_timeout": route_cancelled,
                "nacrectl_route_log": (
                    self.last_wallpaper_route_log.name
                    if self.last_wallpaper_route_log else None
                ),
                "latency_source": latency_source,
                "panel_phases_monotonic_ns": phases,
                "process_samples": self.last_open_process_samples,
                "slide_visible": slide_visible,
                "closed_after_escape": closed,
                "closed_layer_count": count,
                "baseline_layer_count": self.initial_layer_count,
                "first_frame_screenshot": first_frame.name if first_frame.exists() else None,
                "same_shell_instance": self.shell.poll() is None,
                "same_daemon_instance": self.daemon is not None and self.daemon.poll() is None,
            })
            if route_cancelled:
                break
        passed = (len(rows) == self.args.wallpaper_opens and all(
            self._surface_name_matches(row["surface_name"], expected)
            and row["first_content_frame_ms_from_route_invocation"] is not None
            and row["first_content_frame_ms_from_route_invocation"] < limit
            and row["nacrectl_exit_code"] == 0
            and not row["nacrectl_cancelled_after_timeout"]
            and row["closed_after_escape"]
            and row["slide_visible"]
            for row in rows
        ))
        self.results.append({
            "check": (
                f"wallpaper repeated open; {self.args.wallpaper_opens} nacrectl "
                "Super+W routes in one shell/daemon session"
            ),
            "result": "PASS" if passed else "FAIL",
            "expected_surface_name": expected,
            "open_limit_ms_exclusive": limit,
            "selection_count": 0,
            "same_shell_instance": self.shell.poll() is None,
            "same_daemon_instance": self.daemon is not None and self.daemon.poll() is None,
            "opens": rows,
        })

    def wallpaper_stress(self):
        """Step the picker both ways, apply, close, let the closed panel's GPU
        state suspend, then reopen: the paths that once broke the layer role."""
        self._reset_closed()
        self._super("w")
        time.sleep(.5)
        for key in ("Down", "Up"):
            for _ in range(10):
                self._key("-k", key)
                time.sleep(.05)
        self._key("-k", "Return")
        time.sleep(.5)
        self._key("-k", "Escape")
        closed = self._wait_closed()
        time.sleep(12)
        alive_after_suspend = self.shell.poll() is None
        self._super("w")
        time.sleep(.5)
        self._key("-k", "Escape")
        closed_again = self._wait_closed()
        alive = alive_after_suspend and self.shell.poll() is None
        self.results.append({"check": "wallpaper picker step x10/apply/suspend/reopen",
                             "result": "PASS" if alive and closed and closed_again else "FAIL",
                             "shell_alive": alive, "closed": closed, "closed_after_reopen": closed_again})

    def wallpaper_live(self):
        """B0171: the owner's real wallpaper set. Open the picker, step and select five
        wallpapers quickly in a row, close, reopen; report open latency, per-selection
        latency and whether the last selection won."""
        rounds = []
        for cycle in range(self.args.wallpaper_cycles):
            self._reset_closed()
            baseline_started = time.monotonic()
            baseline_threads = [(0.0, process_thread_cpu_ticks(self.shell.pid))]
            for target in (.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0):
                remaining = target - (time.monotonic() - baseline_started)
                if remaining > 0:
                    time.sleep(remaining)
                baseline_threads.append((time.monotonic() - baseline_started,
                                         process_thread_cpu_ticks(self.shell.pid)))
            baseline_deltas = []
            for index in range(1, len(baseline_threads)):
                before = baseline_threads[index - 1][1]
                after = baseline_threads[index][1]
                deltas = []
                for tid, (after_user, after_system, name) in after.items():
                    before_user, before_system, _ = before.get(tid, (0, 0, name))
                    user_delta = max(0, after_user - before_user)
                    system_delta = max(0, after_system - before_system)
                    if user_delta or system_delta:
                        deltas.append({"tid": tid, "comm": name,
                                       "user_ticks": user_delta,
                                       "system_ticks": system_delta})
                baseline_deltas.append({"start_s": round(baseline_threads[index - 1][0], 3),
                                        "end_s": round(baseline_threads[index][0], 3),
                                        "thread_deltas": deltas})
            (self.evidence / f"wallpaper-live-{cycle}-idle-baseline.json").write_text(
                json.dumps({"window_s": round(baseline_threads[-1][0], 3),
                            "thread_deltas": baseline_deltas}, indent=2) + "\n")
            offset = self.log_path.stat().st_size
            started_ns = time.monotonic_ns()
            self._super("w")
            if self.args.wallpaper_stacks and cycle == 0:
                for sample in range(12):
                    time.sleep(.2)
                    threads = subprocess.run(
                        f"ps -L -p {self.shell.pid} -o tid,comm,pcpu,stat,wchan:22 --sort=-pcpu | head -8; "
                        "ps -eo pid,pcpu,stat,args --sort=-pcpu | head -6 | cut -c1-160",
                        shell=True, capture_output=True, text=True).stdout
                    (self.evidence / f"open-stack-{sample}.txt").write_text(threads)
            latency = self._open_latency(offset, None, started_ns, None, f"wl{cycle}")
            time.sleep(1.5 if cycle == 0 else .8)
            sel_offset = self.log_path.stat().st_size
            for _ in range(5):
                self._key("-k", "Down")
                time.sleep(.12)
                self._key("-k", "Return")
                time.sleep(self.args.wallpaper_gap)
            time.sleep(3)
            generation_path = (pathlib.Path(self.env["XDG_STATE_HOME"])
                               / "nacre/wallpaper/current-selection-generation")
            generation = generation_path.read_text().strip() if generation_path.is_file() else None
            (self.evidence / f"wallpaper-live-{cycle}-generation.json").write_text(
                json.dumps({"path": str(generation_path), "value": generation}, indent=2) + "\n")
            snapshot = subprocess.run(
                "ps -eo pid,ppid,etime,stat,wchan:22,args --sort=start_time | grep -E 'nacre-helper|orient|nacre-wallpaper' | grep -v grep",
                shell=True, capture_output=True, text=True).stdout
            (self.evidence / f"wallpaper-live-{cycle}.ps").write_text(snapshot)
            text = self.log_path.read_text(errors="replace")[sel_offset:]
            switch_feedback = [
                (output, int(revision), float(value))
                for output, revision, value in re.findall(
                    r"WALLPAPER_SWITCH_LATENCY output=(\S+) revision=(\d+) [^\n]*ms=([0-9.]+)",
                    text,
                )
            ]
            # A wallpaper revision can repaint more than once (for example when
            # predecode finishes after an optimistic commit). Count only its
            # first presented feedback as the selection's latency sample.
            first_feedback = {}
            for output, revision, value in switch_feedback:
                first_feedback.setdefault((output, revision), value)
            switches = list(first_feedback.values())
            switch_p95 = None
            if switches:
                ordered_switches = sorted(switches)
                switch_p95 = ordered_switches[max(0, math.ceil(.95 * len(ordered_switches)) - 1)]
            phases = re.findall(r"WALLPAPER_SWITCH_PHASE [^\n]*", text)
            phase_names = re.findall(r"WALLPAPER_SWITCH_PHASE revision=\d+ phase=([a-z_]+)", text)
            (self.evidence / f"wallpaper-live-{cycle}.log").write_text(text)
            rounds.append({"cycle": cycle, "open_ms": latency[1], "switch_ms": switches,
                           "switch_count": len(switches), "switch_p95_ms": switch_p95,
                           "switch_max_ms": max(switches) if switches else None,
                           "raw_switch_feedback_count": len(switch_feedback),
                           "duplicate_revision_feedback_count": len(switch_feedback) - len(switches),
                           "selection_attempt_count": phase_names.count("selection"),
                           "request_sent_count": phase_names.count("request_sent"),
                           "daemon_ack_count": phase_names.count("daemon_ack"),
                           "phases": len(phases)})
            self._key("-k", "Escape")
            self._wait_closed()
            close_log_offset = self.log_path.stat().st_size
            cpu_start_monotonic_ns = time.monotonic_ns()
            cpu_start = time.monotonic()
            try:
                sample_targets = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.7, 1.8,
                                  1.85, 1.9, 1.95, 1.98, 2.0, 2.02, 2.05,
                                  2.1, 2.2, 2.3, 2.5, 2.75, 3.0]
                initial_threads = process_thread_cpu_ticks(self.shell.pid)
                cpu_initial = process_cpu_ticks(self.shell.pid)
                thread_baselines = {tid: (row[0], row[1])
                                    for tid, row in initial_threads.items()}
                thread_latest = dict(initial_threads)
                initial_group = (0, 0)
                samples = [(0.0, initial_group[0], initial_group[1])]
                leader_samples = [(0.0, cpu_initial[0], cpu_initial[1])]
                thread_samples = [(0.0, initial_threads)]
                sample_deltas = []
                anon_after_close = None
                for target in sample_targets:
                    remaining = target - (time.monotonic() - cpu_start)
                    if remaining > 0:
                        time.sleep(remaining)
                    thread_ticks = process_thread_cpu_ticks(self.shell.pid)
                    leader_user, leader_system = process_cpu_ticks(self.shell.pid)
                    elapsed = time.monotonic() - cpu_start
                    for tid, row in thread_ticks.items():
                        previous = thread_latest.get(tid)
                        if (previous is None or row[2] != previous[2]
                                or row[0] < previous[0] or row[1] < previous[1]):
                            thread_baselines[tid] = (0, 0)
                        thread_latest[tid] = row
                    user_ticks = sum(max(0, row[0] - thread_baselines[tid][0])
                                     for tid, row in thread_latest.items())
                    system_ticks = sum(max(0, row[1] - thread_baselines[tid][1])
                                       for tid, row in thread_latest.items())
                    samples.append((elapsed, user_ticks, system_ticks))
                    leader_samples.append((elapsed, leader_user, leader_system))
                    thread_samples.append((elapsed, thread_ticks))
                    prior = samples[-2]
                    tick_delta = (user_ticks - prior[1]) + (system_ticks - prior[2])
                    start_threads = thread_samples[-2][1]
                    thread_deltas = []
                    for tid, (end_user, end_system, name) in thread_ticks.items():
                        start_user, start_system, _ = start_threads.get(tid, (0, 0, name))
                        thread_user = max(0, end_user - start_user)
                        thread_system = max(0, end_system - start_system)
                        if thread_user or thread_system:
                            thread_deltas.append({"tid": tid, "comm": name,
                                                  "user_ticks": thread_user,
                                                  "system_ticks": thread_system})
                    sample_deltas.append({"start_s": round(prior[0], 3),
                                          "end_s": round(elapsed, 3),
                                          "user_ticks": user_ticks - prior[1],
                                          "system_ticks": system_ticks - prior[2],
                                          "leader_user_ticks": leader_user - leader_samples[-2][1],
                                          "leader_system_ticks": leader_system - leader_samples[-2][2],
                                          "total_ticks": tick_delta,
                                          "thread_deltas": thread_deltas})
                    if target == 2.5:
                        anon_after_close = measure.memory(self.shell.pid)["pss_anon_mib"]
                zero_windows = []
                for index in range(1, len(samples)):
                    end_s = samples[index][0]
                    starts = [j for j in range(index)
                              if end_s - samples[j][0] >= .95]
                    if not starts:
                        continue
                    start_index = starts[-1]
                    elapsed = end_s - samples[start_index][0]
                    user_ticks = samples[index][1] - samples[start_index][1]
                    system_ticks = samples[index][2] - samples[start_index][2]
                    ticks = user_ticks + system_ticks
                    percent = ticks / os.sysconf("SC_CLK_TCK") / elapsed * 100
                    start_threads = thread_samples[start_index][1]
                    end_threads = thread_samples[index][1]
                    thread_deltas = []
                    for tid, (end_user, end_system, name) in end_threads.items():
                        start_user, start_system, _ = start_threads.get(tid, (0, 0, name))
                        thread_user = max(0, end_user - start_user)
                        thread_system = max(0, end_system - start_system)
                        if thread_user or thread_system:
                            thread_deltas.append({"tid": tid, "comm": name,
                                                  "user_ticks": thread_user,
                                                  "system_ticks": thread_system})
                    zero_windows.append({"start_s": round(samples[start_index][0], 3),
                                         "end_s": round(end_s, 3),
                                         "window_s": round(elapsed, 3),
                                         "user_ticks": user_ticks,
                                         "system_ticks": system_ticks,
                                         "total_ticks": ticks,
                                         "cpu_percent": round(percent, 3),
                                         "thread_deltas": thread_deltas})
                nonzero_after_close = [row for row in sample_deltas if row["total_ticks"]]
                last_nonzero_sample_end = (nonzero_after_close[-1]["end_s"]
                                           if nonzero_after_close else 0.0)
                post_two_thread_ticks = [
                    {"start_s": row["start_s"], "end_s": row["end_s"], **delta}
                    for row in sample_deltas if row["end_s"] > 2.0
                    for delta in row["thread_deltas"]
                ]
                baseline_tick_counts = collections.Counter()
                for row in baseline_deltas:
                    for delta in row["thread_deltas"]:
                        baseline_tick_counts[(delta["tid"], delta["comm"])] += (
                            delta["user_ticks"] + delta["system_ticks"])
                post_two_tick_counts = collections.Counter()
                for delta in post_two_thread_ticks:
                    post_two_tick_counts[(delta["tid"], delta["comm"])] += (
                        delta["user_ticks"] + delta["system_ticks"])
                post_two_baseline_matches = []
                post_two_excess_ticks = 0
                remaining_baseline_ticks = baseline_tick_counts.copy()
                last_picker_work_sample_end = 0.0
                for identity, ticks in post_two_tick_counts.items():
                    baseline_ticks = baseline_tick_counts[identity]
                    post_two_baseline_matches.append({
                        "tid": identity[0], "comm": identity[1],
                        "post_2s_ticks": ticks, "pre_open_baseline_ticks": baseline_ticks,
                        "baseline_matched_ticks": min(ticks, baseline_ticks),
                        "excess_ticks": max(0, ticks - baseline_ticks),
                    })
                    post_two_excess_ticks += max(0, ticks - baseline_ticks)
                for delta in post_two_thread_ticks:
                    identity = (delta["tid"], delta["comm"])
                    observed = delta["user_ticks"] + delta["system_ticks"]
                    matched = min(observed, remaining_baseline_ticks[identity])
                    remaining_baseline_ticks[identity] -= matched
                    if observed > matched:
                        last_picker_work_sample_end = max(last_picker_work_sample_end,
                                                          delta["end_s"])
                idle_by_two = last_picker_work_sample_end <= 2.0
                post_two = next((row for row in zero_windows
                                 if row["start_s"] >= 1.95 and row["end_s"] <= 3.05), None)
                post_two_zero = post_two is not None and post_two["total_ticks"] == 0
            except (OSError, IndexError, KeyError, ValueError) as exc:
                zero_windows = []
                sample_deltas = []
                last_nonzero_sample_end = None
                idle_by_two = False
                post_two_thread_ticks = []
                post_two_baseline_matches = []
                post_two_excess_ticks = None
                last_picker_work_sample_end = None
                post_two = None
                post_two_zero = False
                cpu_error = f"{type(exc).__name__}: {exc}"
            else:
                cpu_error = ""
            close_events = self.log_path.read_text(errors="replace")[close_log_offset:]
            (self.evidence / f"wallpaper-live-{cycle}-close-events.log").write_text(close_events)
            criterion_passed = idle_by_two
            self.results.append({
                "check": f"picker work idle by 2s after wallpaper close (cycle {cycle})",
                "result": "PASS" if criterion_passed else "FAIL",
                "pid": self.shell.pid,
                "cpu_start_monotonic_ns": cpu_start_monotonic_ns,
                "sample_resolution_s": "20–250 ms; 100 Hz /proc clock ticks",
                "sample_deltas": sample_deltas,
                "last_nonzero_sample_end_s": last_nonzero_sample_end,
                "post_2s_thread_ticks": post_two_thread_ticks,
                "post_2s_baseline_comparison": post_two_baseline_matches,
                "post_2s_excess_ticks_above_matched_baseline": post_two_excess_ticks,
                "last_picker_work_sample_end_s": last_picker_work_sample_end,
                "pre_open_idle_baseline": baseline_deltas,
                "close_event_log": f"wallpaper-live-{cycle}-close-events.log",
                "windows": zero_windows,
                "zero_window_after_2s": post_two,
                "post_2s_window_zero_ticks": post_two_zero,
                "pss_anon_after_close_plus_2_5s_mib": round(anon_after_close, 3) if anon_after_close is not None else None,
                "pss_anon_delta_from_start_mib": (
                    round(anon_after_close - self.baseline_mem, 3)
                    if anon_after_close is not None else None
                ),
                "error": cpu_error,
            })
        idle_started_ns = time.monotonic_ns()
        idle_started = time.monotonic()
        time.sleep(120)
        idle_elapsed = time.monotonic() - idle_started
        idle_shell_alive = self.shell.poll() is None
        idle_memory = measure.memory(self.shell.pid) if idle_shell_alive else None
        idle_pss_anon = idle_memory["pss_anon_mib"] if idle_memory else None
        idle_memory_result = {
            "idle_started_monotonic_ns": idle_started_ns,
            "requested_idle_seconds": 120,
            "measured_idle_seconds": round(idle_elapsed, 3),
            "shell_alive": idle_shell_alive,
            "render_backend": self.render_backend,
            "baseline_pss_anon_mib": round(self.baseline_mem, 3) if self.baseline_mem is not None else None,
            "pss_anon_mib_after_idle": round(idle_pss_anon, 3) if idle_pss_anon is not None else None,
            "delta_from_baseline_mib": (round(idle_pss_anon - self.baseline_mem, 3)
                                         if idle_pss_anon is not None and self.baseline_mem is not None else None),
        }
        (self.evidence / "wallpaper-live-after-120s-idle.json").write_text(
            json.dumps(idle_memory_result, indent=2) + "\n")
        print("WALLPAPER_IDLE_MEMORY " + json.dumps(idle_memory_result), flush=True)
        all_switches = [value for row in rounds for value in row["switch_ms"]]
        ordered_switches = sorted(all_switches)
        aggregate_p95 = (ordered_switches[max(0, math.ceil(.95 * len(ordered_switches)) - 1)]
                         if ordered_switches else None)
        enough_per_cycle = all(row["switch_count"] >= 5 for row in rounds)
        switch_acceptance = (enough_per_cycle and aggregate_p95 is not None
                             and aggregate_p95 < 120)
        self.results.append({
            "check": "wallpaper live set; at least 5 presented selections per cycle, aggregate p95 <120ms",
            "result": "PASS" if switch_acceptance else "FAIL",
            "expected_presented_selections_per_cycle": 5,
            "p95_limit_ms": 120,
            "aggregate_presented_selection_count": len(all_switches),
            "aggregate_switch_p95_ms": aggregate_p95,
            "rounds": json.dumps(rounds),
        })

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
        move_committed = True
        try:
            self._wait_log("UI_COMMIT", 2, offset)
        except RuntimeError:
            move_committed = False
        self._shot(middle)
        ghost_visible = not self._diff(before, middle)
        self._pointer("move", drag["bar_drop"])
        release_offset = self.log_path.stat().st_size
        self._pointer("up", drag["bar_drop"])
        release_committed = True
        try:
            self._wait_log("UI_COMMIT", 2, release_offset)
        except RuntimeError:
            release_committed = False
        self._shot(after)
        errors = re.findall(r"^.*UI_ERROR.*$", self.log_path.read_text(errors="replace"), re.MULTILINE)
        passed = move_committed and release_committed and ghost_visible and not errors
        self.results.append({"check": "widget drag bar→desktop→bar",
                             "result": "PASS" if passed else "FAIL",
                             "desktop_ghost_visible": ghost_visible,
                             "move_ui_commit": move_committed,
                             "release_ui_commit": release_committed,
                             "ui_errors_seen": len(errors)})
        if not passed:
            stem = f"r{self.round_no}-widget-drag"
            shutil.copy2(before, self.failures / f"{stem}-before.png")
            shutil.copy2(middle, self.failures / f"{stem}-desktop.png")
            shutil.copy2(after, self.failures / f"{stem}-after.png")

    def launcher_open_and_type(self):
        self._super("space")
        self._key("nacre")

    def one_round(self, round_no):
        self.round_no = round_no
        targets = self.profile["bar_targets"]
        if self.args.focus_dropdown:
            name = self.args.focus_dropdown
            self.click_dropdown(name, targets[name], 1)
            self._shot(self.evidence / f"round-{round_no}-end.png")
            return
        if self.args.interaction:
            cases = self.args.interaction.split(",")
            if "title" in cases:
                self.title_hover_opens_dashboard()
            if "switch" in cases:
                for first, second in self.profile["switch_pairs"]:
                    self.dropdown_switch(first, second)
            if "pill" in cases:
                self.pill_has_no_gaps()
            self._shot(self.evidence / f"round-{round_no}-end.png")
            return
        for name, point in targets.items():
            self.click_dropdown(name, point, 1)
        self.battery_gap_rearm()
        # Keyboard bindings mirror Nacre's calls in scripts/try-bead.py, with
        # Super in the nested compositor to exercise the requested chord.
        self.simple_surface("launcher open/type/Escape", self.launcher_open_and_type,
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
        self.wallpaper_stress()
        self.widget_drag()
        self._shot(self.evidence / f"round-{round_no}-end.png")

    def run(self):
        self.start()
        if self.args.wallpaper_open_only:
            self.wallpaper_open_only()
            self.write_report()
            return 1 if any(row["result"] == "FAIL" for row in self.results) else 0
        if self.args.wallpaper_live:
            self.wallpaper_live()
            self.write_report()
            return 1 if any(row["result"] == "FAIL" for row in self.results) else 0
        if self.args.quick:
            self.one_round(1)
            self._record_memory("quick round end")
        else:
            for number in range(1, 4):
                self.one_round(number)
            print("IDLE_BEGIN seconds=120", flush=True)
            time.sleep(120)
            self._record_memory("after 120-second idle")
            idle_layers, idle_inventory = self._layer_count("after-idle")
            idle_layers_ok = self._idle_layers_match(idle_inventory)
            self.results.append({"check": "closed panel layer retirement after idle",
                                 "result": "PASS" if idle_layers_ok else "FAIL",
                                 "namespace_layer_count_after_idle": idle_layers,
                                 "baseline_namespace_layer_count": self.initial_layer_count,
                                 "calibrated_closed_layer_count": len(self.closed_layer_inventory),
                                 "waived_blank_layers": self.waived_closed_layer_identities,
                                 "panels_checked": len(self.layer_results),
                                 "method": "niri msg layers, matching bead-daily-acceptance namespace"})
            self.one_round(4)
            self._record_memory("after post-idle round")
        text = self.log_path.read_text(errors="replace")
        protocol_errors = re.findall(r"^.*(?:#\d+: error \d+:|Protocol error|WAYLAND_FATAL).*$",
                                     text, re.MULTILINE)
        self.results.append({"check": "Wayland protocol errors", "result": "FAIL" if protocol_errors else "PASS",
                             "count": len(protocol_errors), "first": protocol_errors[:1]})
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
        self.compositor_loss()
        if any(row["result"] != "PASS" for row in self.results):
            last_round = 1 if self.args.quick else 4
            final_shot = self.evidence / f"round-{last_round}-end.png"
            if final_shot.exists():
                shutil.copy2(final_shot, self.failures / "global-metric-failure.png")
        self.guard.stop_event.set()
        self.guard.join(timeout=2)
        self.write_report()
        return 1 if any(row["result"] != "PASS" for row in self.results) else 0

    def compositor_loss(self):
        """Losing the compositor must end the shell once, non-zero, without a log spin."""
        offset = self.log_path.stat().st_size
        self.niri.terminate()
        try:
            code = self.shell.wait(timeout=5)
        except subprocess.TimeoutExpired:
            code = None
        tail = self.log_path.read_bytes()[offset:].decode(errors="replace")
        lines = len(re.findall(r"Protocol error|WAYLAND_FATAL|Broken pipe|Connection reset", tail))
        # Each thread and the video child may report the broken pipe once; a spin logs thousands.
        passed = code not in (None, 0) and lines <= 10
        self.results.append({"check": "compositor loss exits once", "result": "PASS" if passed else "FAIL",
                             "shell_exit": code, "error_lines": lines})

    def _record_memory(self, phase):
        sample = measure.memory(self.shell.pid)
        anon = sample["pss_anon_mib"]
        delta = anon - self.baseline_mem
        self.results.append({"check": f"Pss_Anon +20 MiB ({phase})",
                             "result": "PASS" if delta <= 20 else "FAIL",
                             "start_mib": round(self.baseline_mem, 3),
                             "sample_mib": round(anon, 3),
                             "delta_mib": round(delta, 3), "limit_mib": 20,
                             "render_backend": self.render_backend})

    def write_report(self):
        mode = ("wallpaper open only" if self.args.wallpaper_open_only else
                "quick (one round, no idle)" if self.args.quick else
                "full (three rounds, 120-second idle, repeat)")
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
            if self.cache_snapshot and (self.root / "cache").is_dir():
                self.cache_snapshot.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
                if self.cache_snapshot.exists():
                    shutil.rmtree(self.cache_snapshot)
                shutil.copytree(self.root / "cache", self.cache_snapshot)
                os.chmod(self.cache_snapshot, 0o700)
            if self.persistent_root is None:
                shutil.rmtree(self.root, ignore_errors=True)
            if self.runtime_root != self.root:
                shutil.rmtree(self.runtime_root, ignore_errors=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bead", type=pathlib.Path,
                        default=ROOT / "target/release/nacre-shell")
    parser.add_argument("--shell", type=pathlib.Path, default=ROOT,
                        help="project root containing the arm's shell/ directory")
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
    parser.add_argument("--interaction", nargs="?", const="title,switch,pill",
                        help="with --quick, run only these comma-separated pointer cases "
                             "(title, switch, pill)")
    parser.add_argument("--wallpaper-live", action="store_true",
                        help="B0171: only run the real-wallpaper picker open/select/close/reopen cycles")
    parser.add_argument("--wallpaper-open-only", action="store_true",
                        help="measure repeated wallpaper opens and close without selections")
    parser.add_argument("--wallpaper-opens", type=int, default=3,
                        help="opens for --wallpaper-open-only; all share one shell and daemon")
    parser.add_argument("--wallpaper-stacks", action="store_true",
                        help="sample the shell's thread stacks during the first picker open")
    parser.add_argument("--wallpaper-cycles", type=int, default=3)
    parser.add_argument("--wallpaper-gap", type=float, default=.25,
                        help="seconds between selections in the rapid sequence")
    parser.add_argument("--quick", action="store_true",
                        help="run one interaction round without the 120-second idle and final repeat")
    parser.add_argument("--focus-dropdown", choices=("updates", "audio", "network", "bluetooth",
                                                        "battery", "notifications", "calendar", "power"),
                        help="run only this dropdown's click, Escape, and outside close cases")
    parser.add_argument("--latency-limit-ms", type=float,
                        help="calibrated limit for this nested display; default is two output frames")
    parser.add_argument("--panel-renderer", choices=("cpu", "femtovg-gl"),
                        help="override the bar popout renderer for backend diagnosis")
    parser.add_argument("--renderer", choices=("cpu", "auto"),
                        help="override the bar renderer for backend diagnosis")
    parser.add_argument("--evidence", type=pathlib.Path,
                        default=pathlib.Path.home() / "nacre-lead/b0162-daily-acceptance/evidence")
    parser.add_argument("--wayland-debug", action="store_true",
                        help="log the shell's client-side Wayland requests (WAYLAND_DEBUG=client)")
    parser.add_argument("--slide-trace", action="store_true",
                        help="log every presented panel frame (PANEL_PRESENTED) to measure slides")
    parser.add_argument("--capture-guard-child", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--capture-guard-ready-file", type=pathlib.Path, help=argparse.SUPPRESS)
    parser.add_argument("--capture-guard-release-file", type=pathlib.Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.latency_limit_ms is not None and args.latency_limit_ms <= 0:
        parser.error("--latency-limit-ms must be positive")
    if not 1 <= args.wallpaper_opens <= 10:
        parser.error("--wallpaper-opens must be between 1 and 10")
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
        runner.results.append({"check": "harness execution", "result": "FAIL",
                               "detail": f"{type(exc).__name__}: {exc}"})
        runner.write_report()
        return 2
    finally:
        runner.close()


if __name__ == "__main__":
    raise SystemExit(main())
