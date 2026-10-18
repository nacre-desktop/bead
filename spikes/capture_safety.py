#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Private, bounded systemd scope and memory guard for nested captures."""

import argparse
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import time
import uuid


START_MIN_KIB = 4 * 1024 * 1024
ABORT_BELOW_KIB = 2 * 1024 * 1024
SCOPE_MEMORY_MAX = 2 * 1024 * 1024 * 1024
CHILD_GATE_TIMEOUT_SECONDS = 30
SCOPE_NAME_RE = re.compile(r"^bead-capture-[0-9a-f]{12}\.scope$")


class CaptureSafetyError(RuntimeError):
    pass


def parse_memavailable_kib(contents):
    match = re.search(r"^MemAvailable:\s+(\d+)\s+kB\s*$", contents, re.MULTILINE)
    if not match:
        raise CaptureSafetyError("MemAvailable is missing or malformed")
    return int(match.group(1))


def read_memavailable_kib(path="/proc/meminfo"):
    try:
        return parse_memavailable_kib(pathlib.Path(path).read_text())
    except OSError as error:
        raise CaptureSafetyError(f"cannot read MemAvailable: {error}") from error


def require_wayland_info(which=shutil.which):
    if which("wayland-info") is None:
        raise CaptureSafetyError(
            "required capture dependency wayland-info is missing; no replacement probe or installation is authorized")


def process_start_ticks(pid):
    try:
        row = pathlib.Path(f"/proc/{pid}/stat").read_text()
        tail = row[row.rfind(")") + 2:].split()
        return int(tail[19])
    except (OSError, ValueError, IndexError) as error:
        raise CaptureSafetyError(f"cannot identify watchdog owner process {pid}") from error


def process_cgroup(pid):
    try:
        for row in pathlib.Path(f"/proc/{pid}/cgroup").read_text().splitlines():
            hierarchy, controllers, path = row.split(":", 2)
            if hierarchy == "0" and not controllers:
                return path
    except (OSError, ValueError) as error:
        raise CaptureSafetyError(f"cannot inspect process cgroup {pid}: {error}") from error
    raise CaptureSafetyError(f"process {pid} has no unified cgroup entry")


def validate_scope(scope_name, memory_max, control_group, child_group):
    if not SCOPE_NAME_RE.fullmatch(scope_name):
        raise CaptureSafetyError("refusing an unrecognized, non-task-owned scope name")
    try:
        limit = int(memory_max)
    except (TypeError, ValueError) as error:
        raise CaptureSafetyError("scope MemoryMax is not a byte count") from error
    if limit != SCOPE_MEMORY_MAX:
        raise CaptureSafetyError(f"scope MemoryMax is {limit}, expected {SCOPE_MEMORY_MAX} bytes")
    if not control_group or not control_group.startswith("/"):
        raise CaptureSafetyError("scope has no effective cgroup path")
    if child_group != control_group and not child_group.startswith(control_group.rstrip("/") + "/"):
        raise CaptureSafetyError("capture harness is outside the recorded systemd scope")


def scope_command(scope_name, command):
    if not SCOPE_NAME_RE.fullmatch(scope_name):
        raise CaptureSafetyError("refusing an unrecognized, non-task-owned scope name")
    return [
        "systemd-run", "--user", "--scope", f"--unit={scope_name}",
        f"--property=MemoryMax={SCOPE_MEMORY_MAX}", "--wait", "--collect", "--",
        *command,
    ]


def append_event(path, event):
    with open(path, "a", encoding="utf-8") as stream:
        stream.write(json.dumps(event, sort_keys=True) + "\n")
        stream.flush()
        os.fsync(stream.fileno())


def cgroup_memory_snapshot(control_group):
    base = pathlib.Path("/sys/fs/cgroup") / control_group.lstrip("/")
    result = {}
    for name in ("memory.events", "memory.peak"):
        try:
            result[name] = (base / name).read_text().strip()
        except OSError as error:
            result[name] = f"unavailable: {error}"
    return result


def stop_owned_scope(scope_name, run=subprocess.run):
    if not SCOPE_NAME_RE.fullmatch(scope_name):
        raise CaptureSafetyError("refusing to stop a scope not created by this task")
    result = run(["systemctl", "--user", "stop", scope_name],
                 capture_output=True, text=True, timeout=10, check=False)
    if result.returncode != 0:
        raise CaptureSafetyError(
            f"could not stop owned capture scope {scope_name}: {result.stderr.strip()}")
    return result


def watchdog_loop(*, owner_pid, owner_start, scope_name, stop_file, armed_file,
                  events_file, control_group, memory_reader=read_memavailable_kib,
                  sleeper=time.sleep, run=subprocess.run, owner_alive=None):
    """Run outside the measured scope; return zero on requested clean stop."""
    if owner_alive is None:
        owner_alive = lambda: process_start_ticks(owner_pid) == owner_start
    while True:
        if pathlib.Path(stop_file).exists():
            return 0
        try:
            if not owner_alive():
                append_event(events_file, {"kind": "watchdog", "reason": "guard-owner-exited"})
                stop_owned_scope(scope_name, run)
                return 2
            if pathlib.Path(armed_file).exists():
                available = memory_reader()
                event = {"kind": "memory", "available_kib": available,
                         "cgroup": cgroup_memory_snapshot(control_group)}
                append_event(events_file, event)
                if available < ABORT_BELOW_KIB:
                    append_event(events_file, {"kind": "abort", "reason": "MemAvailable-below-2GiB",
                                               "available_kib": available})
                    stop_owned_scope(scope_name, run)
                    return 3
        except Exception as error:
            append_event(events_file, {"kind": "abort", "reason": "watchdog-failed",
                                       "error": str(error)})
            stop_owned_scope(scope_name, run)
            return 4
        sleeper(1)


def watchdog_process_main(argv):
    parser = argparse.ArgumentParser()
    parser.add_argument("--watchdog", action="store_true")
    parser.add_argument("--owner-pid", type=int, required=True)
    parser.add_argument("--owner-start", type=int, required=True)
    parser.add_argument("--scope", required=True)
    parser.add_argument("--stop-file", required=True)
    parser.add_argument("--armed-file", required=True)
    parser.add_argument("--events-file", required=True)
    parser.add_argument("--control-group", required=True)
    args = parser.parse_args(argv)
    try:
        return watchdog_loop(owner_pid=args.owner_pid, owner_start=args.owner_start,
                             scope_name=args.scope, stop_file=args.stop_file,
                             armed_file=args.armed_file, events_file=args.events_file,
                             control_group=args.control_group)
    except Exception as error:
        # A failed watchdog must be visible to its parent, which then stops the
        # exact scope it owns. It never chooses a process by name or pattern.
        print(f"capture watchdog failed: {error}", file=sys.stderr, flush=True)
        return 5


def _show_scope(scope_name, property_name, run=subprocess.run):
    result = run(["systemctl", "--user", "show", scope_name,
                  f"--property={property_name}", "--value"],
                 capture_output=True, text=True, timeout=5, check=False)
    if result.returncode != 0:
        return ""
    return result.stdout.strip()


def run_guarded(command, *, report_path, env=None, memory_reader=read_memavailable_kib,
                popen=subprocess.Popen, run=subprocess.run, sleeper=time.sleep,
                monotonic=time.monotonic, unit_id=None):
    """Run a harness command inside a verified 2-GiB scope with an outer watchdog."""
    env = os.environ.copy() if env is None else env
    scope_name = f"bead-capture-{(unit_id or uuid.uuid4().hex)[:12]}.scope"
    if not SCOPE_NAME_RE.fullmatch(scope_name):
        raise CaptureSafetyError("generated invalid private scope name")
    destination = pathlib.Path(report_path)
    destination.parent.mkdir(parents=True, exist_ok=True)
    report = {"scope": scope_name, "memory_max_bytes": SCOPE_MEMORY_MAX,
              "start_memavailable_kib": None, "minimum_memavailable_kib": None,
              "status": "preflight"}
    try:
        available_at_start = memory_reader()
        report["start_memavailable_kib"] = available_at_start
        if available_at_start < START_MIN_KIB:
            report["status"] = "refused-low-memory"
            report["reason"] = f"MemAvailable {available_at_start} KiB is below the 4-GiB start rail"
            destination.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
            raise CaptureSafetyError(report["reason"])
        if not shutil.which("systemd-run") or not shutil.which("systemctl"):
            report["status"] = "refused-no-systemd"
            report["reason"] = "systemd-run/systemctl are required for guarded capture"
            destination.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
            raise CaptureSafetyError(report["reason"])
    except CaptureSafetyError as error:
        if not destination.exists():
            report["status"] = "preflight-error"
            report["reason"] = str(error)
            destination.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
        raise

    report["status"] = "starting"
    wrapper_pid = os.getpid()
    wrapper_start = process_start_ticks(wrapper_pid)
    scope_owned = False
    watchdog = None
    scope_process = None

    with tempfile.TemporaryDirectory(prefix="bead-capture-guard-") as temporary:
        temporary = pathlib.Path(temporary)
        ready_file = temporary / "ready.json"
        release_file = temporary / "release"
        armed_file = temporary / "armed"
        stop_file = temporary / "stop"
        events_file = temporary / "watchdog.jsonl"
        child_command = [*command, "--capture-guard-child",
                         "--capture-guard-ready-file", str(ready_file),
                         "--capture-guard-release-file", str(release_file)]
        try:
            invocation = scope_command(scope_name, child_command)
            scope_process = popen(invocation, env=env)
            scope_owned = True
            # The inner harness waits before creating a bus, compositor or UI.
            deadline = monotonic() + 10
            control_group = ""
            effective_limit = ""
            while monotonic() < deadline:
                if scope_process.poll() is not None:
                    raise CaptureSafetyError("systemd scope command exited before readiness")
                effective_limit = _show_scope(scope_name, "MemoryMax", run)
                control_group = _show_scope(scope_name, "ControlGroup", run)
                if ready_file.exists() and effective_limit and control_group:
                    break
                sleeper(0.1)
            else:
                raise CaptureSafetyError("timed out waiting for scope and child readiness")

            ready = json.loads(ready_file.read_text())
            child_group = ready.get("cgroup", "")
            validate_scope(scope_name, effective_limit, control_group, child_group)
            report.update({"control_group": control_group, "child_pid": ready.get("pid"),
                           "child_cgroup": child_group, "effective_memory_max": effective_limit})
            watchdog_command = [sys.executable, str(pathlib.Path(__file__).resolve()), "--watchdog",
                                "--owner-pid", str(wrapper_pid), "--owner-start", str(wrapper_start),
                                "--scope", scope_name, "--stop-file", str(stop_file),
                                "--armed-file", str(armed_file), "--events-file", str(events_file),
                                "--control-group", control_group]
            watchdog = popen(watchdog_command, env=env, start_new_session=True,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            armed_file.touch()
            release_file.touch()

            while scope_process.poll() is None:
                if watchdog.poll() is not None:
                    stop_owned_scope(scope_name, run)
                    raise CaptureSafetyError("memory watchdog exited before capture completed")
                sleeper(1)

            stop_file.touch()
            watchdog_code = watchdog.wait(timeout=5)
            watchdog = None
            events = _read_events(events_file)
            available_samples = [row["available_kib"] for row in events if row.get("kind") == "memory"]
            report["minimum_memavailable_kib"] = min(available_samples, default=available_at_start)
            report["watchdog_samples"] = len(available_samples)
            report["cgroup_last"] = next((row.get("cgroup") for row in reversed(events)
                                           if row.get("kind") == "memory"), None)
            if any(row.get("kind") == "abort" for row in events) or watchdog_code != 0:
                raise CaptureSafetyError("watchdog invalidated the capture")
            if scope_process.returncode != 0:
                report["status"] = "scope-command-failed"
                raise CaptureSafetyError(f"capture scope exited {scope_process.returncode}")
            report["status"] = "complete"
            return scope_process.returncode
        except BaseException as error:
            report["status"] = "aborted"
            report["reason"] = str(error)
            if scope_owned:
                try:
                    stop_owned_scope(scope_name, run)
                    if scope_process is not None and scope_process.poll() is None:
                        try:
                            scope_process.wait(timeout=10)
                        except subprocess.TimeoutExpired:
                            # This is the exact systemd-run child started above;
                            # the cgroup stop was already requested by unit ID.
                            scope_process.terminate()
                            scope_process.wait(timeout=5)
                except Exception as stop_error:
                    report["cleanup_error"] = str(stop_error)
            raise
        finally:
            if watchdog is not None:
                stop_file.touch()
                try:
                    watchdog.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    watchdog.terminate()
                    watchdog.wait(timeout=5)
            if events_file.exists():
                events = _read_events(events_file)
                samples = [row["available_kib"] for row in events if row.get("kind") == "memory"]
                report["minimum_memavailable_kib"] = min(samples, default=report["minimum_memavailable_kib"] or available_at_start)
                report["watchdog_samples"] = len(samples)
                report["watchdog_events"] = events
            destination.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")


def _read_events(path):
    if not pathlib.Path(path).exists():
        return []
    rows = []
    for line in pathlib.Path(path).read_text().splitlines():
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError:
            rows.append({"kind": "invalid-event-line", "line": line})
    return rows


def wait_for_release(ready_file, release_file, timeout=CHILD_GATE_TIMEOUT_SECONDS,
                     sleeper=time.sleep, monotonic=time.monotonic):
    ready_file, release_file = pathlib.Path(ready_file), pathlib.Path(release_file)
    ready_file.parent.mkdir(parents=True, exist_ok=True)
    ready_file.write_text(json.dumps({"pid": os.getpid(), "cgroup": process_cgroup(os.getpid())}) + "\n")
    deadline = monotonic() + timeout
    while monotonic() < deadline:
        if release_file.exists():
            return
        sleeper(0.1)
    raise CaptureSafetyError("scope verification gate timed out; no capture was started")


if __name__ == "__main__" and "--watchdog" in sys.argv:
    raise SystemExit(watchdog_process_main(sys.argv[1:]))
