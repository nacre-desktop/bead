#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""One-command supervised Bead reference-laptop residency capture."""
from __future__ import annotations

import argparse
import datetime
import fcntl
import hashlib
import json
import math
import os
import pathlib
import re
import signal
import socket
import statistics
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
ARM = ROOT / "spikes/b0002-arm.py"
PHASES = {
    "never-opened": "never",
    "opened-once": "once",
    "opened-several": "several",
    "combined-legacy": "combined",
}
MIB = 1024 * 1024
GIB = 1024**3
SWAP_LIMIT = 64 * MIB
ARM_SECONDS = 60 + 65 + 85 + 105 + 95
FROZEN_CONTROL_SHA256 = "b01dfccaac148b984a6e1089f7a54d41bced473455b3d41890d15ec074a8038b"
FROZEN_FIXTURE_GIT_TREE = "51dbf56ff49bf55656d7f04257f532dde73ece47"


def estimate_runtime(compositors: list[str], include_cpu: bool) -> tuple[int, float]:
    batches = 2 if include_cpu else 1
    fixed_seconds = len(compositors) * batches * 6 * ARM_SECONDS
    total_seconds = fixed_seconds + len(compositors) * batches * 4 * 60
    return math.ceil(total_seconds / 60), fixed_seconds / 60


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def canonical_tree_hash(root: pathlib.Path) -> str:
    digest = hashlib.sha256()
    for path in sorted(p for p in root.rglob("*") if p.is_file()):
        digest.update(path.relative_to(root).as_posix().encode())
        digest.update(b"\0")
        digest.update(bytes.fromhex(sha256(path)))
    return digest.hexdigest()


def check_candidate_shell_tree(candidate: dict) -> None:
    """The candidate runs the checkout's shell/ tree, so the binary must come from that exact tree.

    A binary built from an older Bead revision than the checkout fails inside the
    UI (Luau API mismatch, `UI_ERROR`) long before a valid arm exists.
    """
    expected = candidate.get("shell_git_tree")
    actual = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD:shell"], text=True).strip()
    if not expected or expected != actual:
        raise RuntimeError(
            f"candidate was built from shell/ tree {expected} (revision {candidate.get('source_revision')}) "
            f"but this checkout has shell/ tree {actual}; rebuild the candidate from this checkout")


def load_manifest(path: pathlib.Path) -> dict:
    value = json.loads(path.read_text())
    if value.get("schema_version") != 1:
        raise RuntimeError("artifact manifest schema_version must be 1")
    for key in ("control", "candidate"):
        artifact = value.get(key)
        if not isinstance(artifact, dict):
            raise RuntimeError(f"manifest is missing {key} artifact metadata")
        binary = pathlib.Path(artifact["path"]).expanduser()
        if not binary.is_absolute():
            binary = path.parent / binary
        binary = binary.resolve(strict=True)
        actual = sha256(binary)
        if actual != artifact.get("sha256"):
            raise RuntimeError(f"{key} SHA-256 mismatch: expected {artifact.get('sha256')}, got {actual}")
        artifact["path"] = str(binary)
    if value["control"].get("sha256") != FROZEN_CONTROL_SHA256 or not value["control"].get("exact_original_binary_identity"):
        raise RuntimeError("control is not the exact B0002 frozen binary; no reference capture may start")
    check_candidate_shell_tree(value["candidate"])
    fixture_root = pathlib.Path(value.get("dashboard_fixtures", ROOT / "shell/tests/fixtures/dashboard")).resolve(strict=True)
    fixture_hash = canonical_tree_hash(fixture_root)
    if fixture_hash != value.get("dashboard_fixtures_sha256"):
        raise RuntimeError(f"dashboard fixture tree hash mismatch: {fixture_hash}")
    fixture_tree = subprocess.check_output(
        ["git", "-C", str(ROOT), "rev-parse", "HEAD:shell/tests/fixtures/dashboard"], text=True).strip()
    if (value.get("dashboard_fixtures_git_tree") != FROZEN_FIXTURE_GIT_TREE
            or fixture_tree != FROZEN_FIXTURE_GIT_TREE):
        raise RuntimeError("dashboard fixtures do not match the frozen B0002 control tree")
    value["dashboard_fixtures"] = str(fixture_root)
    bundle_root = pathlib.Path(value.get("control_bundle_root", ""))
    if not bundle_root.is_absolute():
        bundle_root = path.parent / bundle_root
    bundle_root = bundle_root.resolve(strict=True)
    if (not (bundle_root / "shell/ui/main.luau").is_file()
            or not (bundle_root / "shell/palette.json").is_file()
            or not (bundle_root / "shell/tests/icon.svg").is_file()):
        raise RuntimeError("matched frozen control UI bundle is incomplete")
    if canonical_tree_hash(bundle_root) != value.get("control_bundle_sha256"):
        raise RuntimeError("frozen control UI bundle hash mismatch")
    if canonical_tree_hash(bundle_root / "shell/tests/fixtures/dashboard") != fixture_hash:
        raise RuntimeError("frozen control dashboard fixtures differ from candidate fixtures")
    value["control_bundle_root"] = str(bundle_root)
    return value


def swap_state() -> tuple[int, int, list[str]]:
    available_match = re.search(r"^MemAvailable:\s+(\d+)", pathlib.Path("/proc/meminfo").read_text(), re.M)
    if not available_match:
        raise RuntimeError("cannot read MemAvailable")
    rows = pathlib.Path("/proc/swaps").read_text().splitlines()[1:]
    if len(rows) != 1:
        raise RuntimeError(f"expected exactly one swap device (/dev/zram0), found {rows!r}")
    row = rows[0].split()
    if len(row) < 5 or row[0] != "/dev/zram0":
        raise RuntimeError(f"unexpected active swap device: {row!r}")
    return int(available_match.group(1)) * 1024, int(row[3]) * 1024, row


def ac_power_supplies() -> list[dict[str, str]]:
    supplies = []
    for path in pathlib.Path("/sys/class/power_supply").glob("*"):
        try:
            values = {name: (path / name).read_text().strip()
                      for name in ("type", "online", "status") if (path / name).is_file()}
        except OSError:
            continue
        if values.get("type", "").lower() in ("mains", "usb", "usb_c") and values.get("online") == "1":
            supplies.append({"name": path.name, **values})
    return supplies


def host_preflight(max_load: float, require_ac: bool) -> dict:
    load = float(pathlib.Path("/proc/loadavg").read_text().split()[0])
    if load >= max_load:
        raise RuntimeError(f"1-minute load average {load:.2f} is not below {max_load:.2f}")
    for name in ("cargo", "rustc", "gcc", "cc1", "clang", "cmake", "ninja", "make"):
        if subprocess.run(["pgrep", "-x", name], capture_output=True).returncode == 0:
            raise RuntimeError(f"heavy build process is running: {name}")
    scopes = subprocess.run(["systemctl", "--user", "list-units", "--type=scope", "--state=running", "--no-legend"],
                            capture_output=True, text=True, check=True).stdout
    if "bead-capture-" in scopes:
        raise RuntimeError("another Bead capture scope is active")
    available, used, swap = swap_state()
    if used < SWAP_LIMIT:
        pass
    elif available <= used + 8 * GIB:
        raise RuntimeError(f"MemAvailable {available} bytes is not greater than swap_used+8GiB ({used + 8*GIB})")
    power = ac_power_supplies()
    if require_ac and not power:
        raise RuntimeError("no online AC/Mains/USB power supply found in /sys/class/power_supply")
    return {"hostname": socket.gethostname(), "load1": load, "max_load": max_load,
            "memavailable_bytes": available, "swap_used_bytes": used, "swap_row": swap,
            "ac_power_supplies": power if require_ac else (power or "not-applicable: desktop rehearsal"),
            "running_scope_list": scopes.strip()}


def validate_arm(raw_root: pathlib.Path, compositor: str, label: str) -> tuple[bool, str]:
    arm = raw_root / compositor / label
    safety_path = arm / f"{compositor}-capture-safety.json"
    swapin_path = arm / "host-swapin-delta.txt"
    residency_path = arm / compositor / "dashboard-residency.json"
    if not all(path.is_file() for path in (safety_path, swapin_path, residency_path)):
        return False, "missing capture, swap-in or residency evidence"
    safety = json.loads(safety_path.read_text())
    residency = json.loads(residency_path.read_text())
    match = re.search(r"host_swapin_delta_bytes=(\d+)", swapin_path.read_text())
    if safety.get("status") != "complete" or safety.get("capture_returncode") != 0 or safety.get("scope_runner_returncode") != 0:
        return False, "scope or capture command did not complete successfully"
    if not safety.get("final_cgroup_read_while_scope_held"):
        return False, "final held-scope memory events were not read"
    oom = safety.get("oom_delta") or {}
    if oom.get("oom") != 0 or oom.get("oom_kill") != 0:
        return False, f"OOM counter changed: {oom}"
    if not match or int(match[1]) >= MIB:
        return False, "host swap-in reached 1 MiB or was not recorded"
    if not residency.get("valid") or not residency.get("batch_medians") or residency.get("opened_once_only"):
        return False, "residency batch is invalid or missing phases"
    main_rows = [residency.get("bar", {}).get("backend")]
    main_rows.extend(row.get("backend") for row in residency.get("closed", []))
    main_rows.extend(residency.get(phase, {}).get("backend")
                     for phase in ("never", "once", "several", "combined"))
    # Panel backend is only observable while a panel is open: the "warm" rows are the
    # samples taken with target_open. The settled once/several/combined phase rows are
    # captured after close (panel_active=False -> "inactive" by design), so they are
    # judged as main-bar rows only, not as panel rows.
    panel_rows = [row.get("backend") for row in residency.get("warm", [])] or [None]
    if (any(not isinstance(row, dict) or row.get("main_bar") != "cpu" for row in main_rows)
            or any(not isinstance(row, dict) or row.get("opened_panel") not in
                   ("cpu", "cpu (configured dashboard renderer)", "cpu-fallback", "native-gl")
                   for row in panel_rows)):
        return False, "actual renderer backend is unknown for one or more measured samples"
    expected = {"never", "once", "several", "combined"}
    if any(not isinstance(residency.get(phase), dict) for phase in expected):
        return False, "one or more settled phases are absent"
    if int(safety.get("effective_memory_max", -1)) != 12 * GIB:
        return False, "scope memory.max was not 12 GiB"
    if int(safety.get("effective_memory_swap_max", -1)) != 0:
        return False, "scope swap limit was not zero"
    geometry_path = arm / compositor / "output-geometry.json"
    if not geometry_path.is_file():
        return False, "verified nested output geometry is missing"
    geometry = json.loads(geometry_path.read_text())
    try:
        if compositor == "niri":
            mode = geometry["modes"][geometry["current_mode"]]
            valid_geometry = ((mode["width"], mode["height"]) == (2880, 1800)
                              and geometry["logical"]["width"] == 1920
                              and geometry["logical"]["height"] == 1200
                              and geometry["logical"]["scale"] == 1.5
                              and abs(float(mode["refresh_rate"]) / 1000 - 60) < 0.01)
        else:
            valid_geometry = ((geometry["width"], geometry["height"]) == (2880, 1800)
                              and geometry["scale"] == 1.5
                              and abs(float(geometry["refreshRate"]) - 120) < 0.01)
    except (KeyError, IndexError, TypeError, ValueError):
        valid_geometry = False
    if not valid_geometry:
        return False, "nested output geometry does not match the B0002 reference profile"
    isolation_path = arm / compositor / "session-isolation.json"
    if not isolation_path.is_file():
        return False, "private XDG runtime and Wayland socket proof is missing"
    isolation = json.loads(isolation_path.read_text())
    if not all(isolation.get(key) is True for key in (
            "runtime_is_task_private", "runtime_has_no_group_or_other_access",
            "wayland_socket_inside_private_runtime")):
        return False, "nested compositor did not remain inside private XDG/Wayland endpoints"
    for phase in residency.values():
        if isinstance(phase, dict) and "seconds" in phase and phase["seconds"] < 60:
            return False, "one or more settled idle windows were shorter than 60 seconds"
    return True, "valid"


def rows_for(raw_root: pathlib.Path, compositor: str, side: str, phase_name: str) -> list[dict]:
    result = []
    for pair in range(1, 4):
        path = raw_root / compositor / f"{side}{pair}" / compositor / f"{phase_name}.json"
        if not path.is_file():
            raise RuntimeError(f"missing valid {side}{pair} endpoint: {path}")
        result.append(json.loads(path.read_text()))
    return result


def summarize(raw_root: pathlib.Path, compositors: list[str], rehearsal: bool) -> dict:
    wallpaper_active = any("WALLPAPER_SURFACE_COMMITTED" in path.read_text(errors="replace")
                           for path in raw_root.rglob("launcher.log") if path.is_file())
    report = {"schema_version": 1, "complete": True, "rehearsal_only": rehearsal,
              "absolute_verdict": "REHEARSAL ONLY" if rehearsal else "INCOMPLETE",
              "relative_deltas_mib": {}, "arm_validity": {}, "arm_backend": {},
              "compositors": compositors, "failures": [],
              "wallpaper_shm": {"active": wallpaper_active,
                                "mib": None if wallpaper_active else 0.0,
                                "reason": ("Bead committed a wallpaper surface; niri-held SHM was not separately measured" if wallpaper_active else
                                           "no WALLPAPER_SURFACE_COMMITTED marker in measured UI logs; private niri.kdl has no wallpaper surface")}}
    for compositor in compositors:
        arm_failures = []
        for side in ("A", "B"):
            for pair in range(1, 4):
                label = f"{side}{pair}"
                valid, reason = validate_arm(raw_root, compositor, label)
                report["arm_validity"][f"{compositor}/{label}"] = {"valid": valid, "reason": reason}
                backend_path = raw_root / compositor / label / compositor / "dashboard-residency.json"
                if backend_path.is_file():
                    residency = json.loads(backend_path.read_text())
                    report["arm_backend"][f"{compositor}/{label}"] = {
                        "bar": residency.get("bar", {}).get("backend"),
                        "opened_panels": [row.get("backend") for row in residency.get("warm", [])],
                        "wallpaper_surface_committed": any(
                            row.get("wallpaper_surface_committed") is True
                            for row in [residency.get("bar", {}).get("backend", {})]
                            + [item.get("backend", {}) for item in residency.get("warm", [])])}
                if not valid:
                    arm_failures.append(f"{label}: {reason}")
        for file_phase, key in PHASES.items():
            try:
                if arm_failures:
                    raise RuntimeError("invalid arm evidence: " + "; ".join(arm_failures))
                control = rows_for(raw_root, compositor, "A", file_phase)
                candidate = rows_for(raw_root, compositor, "B", file_phase)
                all_rows = control + candidate
                drm_unknown = any(row.get("drm_mib") is None for row in all_rows)
                for row in all_rows:
                    memory = row.get("memory", {})
                    if abs(memory.get("pss_mib", -100) - sum(memory.get(field, 0) for field in
                           ("pss_anon_mib", "pss_file_mib", "pss_shmem_mib"))) > 0.01:
                        raise RuntimeError("PSS categories do not add up to PSS")
                    if row.get("drm_mib") is not None and abs(
                            memory["pss_mib"] + sum(row["drm_mib"].values()) - row["pss_plus_drm_mib"]) > 0.01:
                        raise RuntimeError("PSS plus DRM does not add up to the captured total")
                def amount(row: dict) -> float:
                    return row["memory"]["pss_mib"] + row["memory"]["swap_pss_mib"] + sum((row.get("drm_mib") or {}).values())
                def median(rows: list[dict], field: str) -> float:
                    return statistics.median(row["memory"][field] for row in rows)
                control_total = statistics.median(amount(row) for row in control)
                candidate_total = statistics.median(amount(row) for row in candidate)
                control_pss = median(control, "pss_mib")
                candidate_pss = median(candidate, "pss_mib")
                control_drm = (None if any(row.get("drm_mib") is None for row in control)
                               else statistics.median(sum(row["drm_mib"].values()) for row in control))
                candidate_drm = (None if any(row.get("drm_mib") is None for row in candidate)
                                 else statistics.median(sum(row["drm_mib"].values()) for row in candidate))
                anon_delta = median(candidate, "pss_anon_mib") - median(control, "pss_anon_mib")
                file_delta = median(candidate, "pss_file_mib") - median(control, "pss_file_mib")
                shmem_delta = median(candidate, "pss_shmem_mib") - median(control, "pss_shmem_mib")
                total_delta = candidate_total - control_total
                phase_report = {
                    "n_pairs": 3, "control_median_mib": control_total,
                    "candidate_median_mib": candidate_total, "candidate_minus_control_mib": total_delta,
                    "control_pss_median_mib": control_pss, "candidate_pss_median_mib": candidate_pss,
                    "pss_delta_mib": candidate_pss - control_pss,
                    "control_drm_median_mib": control_drm, "candidate_drm_median_mib": candidate_drm,
                    "anonymous_delta_mib": anon_delta, "file_delta_mib": file_delta,
                    "shmem_delta_mib": shmem_delta,
                    "control_pair_values_mib": [amount(row) for row in control],
                    "candidate_pair_values_mib": [amount(row) for row in candidate],
                    "pairwise_total_deltas_mib": [amount(candidate[i]) - amount(control[i]) for i in range(3)],
                    "drm_attribution": "unknown" if drm_unknown else "measured",
                    "gpu_paths": sorted({key.split("/", 1)[0] for row in all_rows
                                          for key in (row.get("drm_mib") or {})}),
                    "measurement_basis": "PSS + SwapPss only; DRM unknown" if drm_unknown else "PSS + per-client DRM + SwapPss",
                    "absolute_total_definition": "PSS + per-client DRM fdinfo + SwapPss",
                }
                gate_failures = []
                if not rehearsal:
                    if wallpaper_active:
                        gate_failures.append("niri wallpaper surface is active but its compositor-held SHM was not separately measured")
                    if any(row["memory"]["swap_pss_mib"] != 0 for row in all_rows):
                        gate_failures.append("SwapPss was nonzero")
                    if drm_unknown:
                        gate_failures.append("DRM fdinfo unavailable; absolute reference gate cannot pass")
                    if candidate_total > 25:
                        gate_failures.append(f"candidate median {candidate_total:.3f} MiB exceeds 25 MiB absolute gate")
                    if not drm_unknown and any(amount(row) >= 24.5 for row in candidate):
                        gate_failures.append("candidate arm reached the 24.5 MiB stop line")
                    if anon_delta > 1:
                        gate_failures.append(f"candidate anonymous growth {anon_delta:.3f} MiB exceeds 1 MiB")
                    phase_report["verdict"] = "STOP" if gate_failures else "PASS"
                else:
                    phase_report["verdict"] = "REHEARSAL ONLY"
                phase_report["gate_failures"] = gate_failures
                report["relative_deltas_mib"][f"{compositor}/{key}"] = phase_report
                close_checks = []
                for side in ("A", "B"):
                    for pair in range(1, 4):
                        residency_path = raw_root / compositor / f"{side}{pair}" / compositor / "dashboard-residency.json"
                        residency = json.loads(residency_path.read_text())
                        bar_total = amount(residency["bar"])
                        closed = residency.get("closed") or []
                        if not closed:
                            raise RuntimeError(f"{side}{pair} has no close samples")
                        max_closed = max(amount(row) for row in closed)
                        close_checks.append({"arm": f"{side}{pair}", "bar_mib": bar_total,
                                             "max_closed_mib": max_closed,
                                             "delta_mib": max_closed - bar_total,
                                             "limit_mib": 5,
                                             "pass": max_closed <= bar_total + 5})
                phase_report["close_checks"] = close_checks
                if any(not check["pass"] for check in close_checks):
                    gate_failures.append("one or more close samples exceeded own bar + 5 MiB")
                    phase_report["verdict"] = "STOP"
                if gate_failures:
                    report["complete"] = False
                    report["failures"].append(f"{compositor}/{key}: " + "; ".join(gate_failures))
            except Exception as error:
                report["complete"] = False
                report["failures"].append(f"{compositor}/{key}: {error}")
                report["relative_deltas_mib"][f"{compositor}/{key}"] = {"verdict": "INCOMPLETE", "error": str(error)}
    if report["complete"] and not rehearsal:
        report["absolute_verdict"] = "PASS"
    elif report["complete"] and rehearsal:
        report["absolute_verdict"] = "REHEARSAL ONLY — no laptop verdict"
    else:
        report["absolute_verdict"] = "INCOMPLETE / STOP"
    report_path = raw_root / "summary.json"
    temp_path = report_path.with_suffix(".json.tmp")
    temp_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    temp_path.replace(report_path)
    return report


def save_overall_summary(output: pathlib.Path, reports: dict[str, dict]) -> dict:
    result = {"schema_version": 1,
              "complete": bool(reports) and all(report["complete"] for report in reports.values()),
              "absolute_verdict": "PASS" if reports and all(report["absolute_verdict"] == "PASS" for report in reports.values()) else "INCOMPLETE / STOP",
              "renderers": reports}
    path = output / "summary.json"
    temp = path.with_suffix(".json.tmp")
    temp.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    temp.replace(path)
    return result


def arm_command(args, manifest: dict, compositor: str, label: str, previous: str,
                raw_root: pathlib.Path, renderer: str = "production") -> list[str]:
    side = label[0]
    control = manifest["control"]
    candidate = manifest["candidate"]
    profile = "rehearsal" if args.rehearsal else "laptop-reference"
    command = [sys.executable, str(ARM), "--side", side, "--compositor", compositor,
               "--label", label, "--previous-label", previous, "--raw-root", str(raw_root),
               "--control-binary", control["path"], "--control-sha256", control["sha256"],
               "--candidate-binary", candidate["path"], "--candidate-sha256", candidate["sha256"],
               "--control-bundle-root", manifest["control_bundle_root"],
               "--reference-renderer", renderer,
               "--profile", profile, "--max-load", str(args.max_load), "--forbid-nacre-lead"]
    return command


def terminate_owned(process: subprocess.Popen) -> None:
    if process.poll() is not None:
        return
    try:
        os.killpg(process.pid, signal.SIGINT)
        process.wait(timeout=25)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    default_home = pathlib.Path.home() / ".local/share/nacre/ai/lead/bead-laptop-reference-20261023"
    parser.add_argument("--manifest", type=pathlib.Path, default=default_home / "artifacts/manifest.json")
    parser.add_argument("--output", type=pathlib.Path,
                        default=default_home / ("evidence-" + datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")))
    parser.add_argument("--compositors", nargs="+", choices=("niri", "hyprland"), default=("niri",))
    parser.add_argument("--max-load", type=float, default=1.0)
    parser.add_argument("--rehearsal", action="store_true", help="desktop-only dry rehearsal; no absolute verdict")
    parser.add_argument("--window-minutes", type=float, default=90.0)
    parser.add_argument("--include-cpu", action="store_true",
                        help="run an additional full CPU-rendered batch after production-default arms")
    args = parser.parse_args()
    if not args.rehearsal and os.environ.get("BEAD_OWNER_SIGNAL") != "1":
        parser.error("laptop capture starts only after the owner signals: set BEAD_OWNER_SIGNAL=1")
    if "niri" not in args.compositors:
        parser.error("Niri is the required reference compositor")
    if args.rehearsal and socket.gethostname() in ("siverteh", "siverteh-laptop"):
        parser.error("desktop rehearsal is forbidden on the owner's laptop")
    if args.rehearsal and os.environ.get("BEAD_DESKTOP_HEAVY_LOCKED") != "1":
        parser.error("desktop rehearsal must run under flock ~/nacre-lead/desktop-heavy.lock")
    backends = ["production"] + (["cpu"] if args.include_cpu else [])
    estimate_minutes, fixed_minutes = estimate_runtime(args.compositors, args.include_cpu)
    print(f"ESTIMATED_RUNTIME about {estimate_minutes} minutes; fixed idle windows {fixed_minutes:.1f} minutes plus startup/preflight allowance; renderers={','.join(backends)}; compositors={','.join(args.compositors)}", flush=True)
    if estimate_minutes > args.window_minutes:
        parser.error(f"estimated {estimate_minutes} minutes exceeds the selected {args.window_minutes:g}-minute idle window")
    try:
        default_home.mkdir(mode=0o700, parents=True, exist_ok=True)
        lock = (default_home / "capture.lock").open("a")
        try:
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError("another laptop reference capture is already active")
        manifest = load_manifest(args.manifest.expanduser().resolve(strict=True))
        preflight = host_preflight(args.max_load, require_ac=not args.rehearsal)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"PREFLIGHT STOP: {error}", file=sys.stderr, flush=True)
        return 2
    args.output = args.output.expanduser().resolve()
    args.output.mkdir(mode=0o700, parents=True, exist_ok=False)
    (args.output / "preflight.json").write_text(json.dumps(preflight, indent=2, sort_keys=True) + "\n")
    (args.output / "artifact-manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print("PREFLIGHT PASS " + json.dumps(preflight, sort_keys=True), flush=True)
    pairs = [f"{side}{pair}" for pair in range(1, 4) for side in "AB"]
    reports = {}
    for backend in backends:
        backend_root = args.output / backend
        backend_root.mkdir(mode=0o700)
        for compositor in args.compositors:
            prior = "START"
            for label in pairs:
                try:
                    current = host_preflight(args.max_load, require_ac=not args.rehearsal)
                    arm_root = backend_root / compositor / label
                    if arm_root.exists():
                        raise RuntimeError(f"refusing to overwrite evidence: {arm_root}")
                    command = arm_command(args, manifest, compositor, label, prior, backend_root, backend)
                    checks = backend_root / compositor / "host-preflight"
                    checks.mkdir(mode=0o700, parents=True, exist_ok=True)
                    (checks / f"{label}.json").write_text(json.dumps(
                        {"label": label, "renderer": backend, "recorded_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                         **current}, indent=2, sort_keys=True) + "\n")
                    logs = backend_root / "logs"
                    logs.mkdir(mode=0o700, exist_ok=True)
                    arm_log = logs / f"{compositor}-{label}.log"
                    arm_started = time.monotonic()
                    print(f"ARM_START {backend}/{compositor}/{label} previous_valid={prior} load1={current['load1']:.2f} swap_used={current['swap_used_bytes']}", flush=True)
                    with arm_log.open("w") as log:
                        process = subprocess.Popen(command, cwd=ROOT, start_new_session=True,
                                                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                                   text=True, bufsize=1)
                        try:
                            for line in process.stdout:
                                print(line, end="", flush=True)
                                log.write(line)
                        finally:
                            process.stdout.close()
                        code = process.wait()
                    if code != 0:
                        raise RuntimeError(f"arm command exited {code}; inspect {arm_log} and preserved raw evidence")
                    valid, reason = validate_arm(backend_root, compositor, label)
                    if not valid:
                        raise RuntimeError(f"arm invalid: {reason}")
                    prior = label
                    print(f"ARM_VALID {backend}/{compositor}/{label} elapsed_seconds={time.monotonic()-arm_started:.1f}", flush=True)
                except KeyboardInterrupt:
                    if "process" in locals():
                        terminate_owned(process)
                    print("ABORTED by Ctrl-C; owned capture scope/session cleanup requested; partial evidence preserved and invalid", file=sys.stderr, flush=True)
                    try:
                        reports[backend] = summarize(backend_root, args.compositors, args.rehearsal)
                        save_overall_summary(args.output, reports)
                    except Exception:
                        pass
                    return 130
                except Exception as error:
                    print(f"CAPTURE STOP {backend}/{compositor}/{label}: {error}", file=sys.stderr, flush=True)
                    report = summarize(backend_root, args.compositors, args.rehearsal)
                    reports[backend] = report
                    save_overall_summary(args.output, reports)
                    print("SUMMARY " + json.dumps({"renderer": backend, "complete": report["complete"], "absolute_verdict": report["absolute_verdict"], "failures": report["failures"]}, sort_keys=True), flush=True)
                    return 1
        reports[backend] = summarize(backend_root, args.compositors, args.rehearsal)
    combined = save_overall_summary(args.output, reports)
    print("SUMMARY " + json.dumps({"complete": combined["complete"], "absolute_verdict": combined["absolute_verdict"], "renderers": backends}, sort_keys=True), flush=True)
    return 0 if combined["complete"] and (args.rehearsal or combined["absolute_verdict"] == "PASS") else 1


if __name__ == "__main__":
    raise SystemExit(main())
