#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Pinned, self-recording launcher for the B0002 matched memory batch."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
ARTIFACTS = pathlib.Path.home() / ".local/share/nacre/ai/lead/b0002-artifacts-20261020"
UI_WORKTREE = pathlib.Path.home() / ".local/share/nacre/ai/worktrees/luna-b0002-gc-fix-20261020"
HELPER = ARTIFACTS / "nacre-helper-4a65f4ec"
CONTROL = ARTIFACTS / "bead-ui-control-e21caa16"
FIXED = ARTIFACTS / "bead-ui-fixed-306474f0"
EXPECTED_HASHES = {
    "control": "e21caa16cee883008aeceeea4bb438a2b4ee30944e6d9c8bfe4f9626373b1117",
    "candidate": "306474f058c8a8ab044e6153b69b26e8a48bc1714bbcd8d615db20b9a51777bb",
    "helper": "4a65f4ec887bc85e93b5b862f1786fe6f1b7a811875333765ee8f8c052cce0aa",
}
RAW_ROOT = pathlib.Path.home() / "nacre-quiet/evidence/bead-b0002-20261021/full-batch-fresh-launchers"
LABELS = [f"{side}{pair}" for pair in range(1, 6) for side in "AB"]
PHASES = ("never", "once", "several", "combined")
MIB = 1024 * 1024
CAPTURE_PROFILE = "legacy"
# Host-gate parameters (set from arguments; defaults keep the original behaviour).
HOST_GATES = {"max_load": None, "forbid_nacre_lead": False, "opened_once_only": False}
CAPTURE_CONFIG = {
    "BEAD_RESEARCH_TOOLS": str(pathlib.Path.home() / "Projects/bead/.tools"),
    "CARGO_TARGET_DIR": str(UI_WORKTREE / "target"),
    "BEAD_HARDENING": "menu-engine",
    "BEAD_MENU_ENGINE_BINARY": str(HELPER),
    "BEAD_MENU_ENGINE_BUNDLE": str(UI_WORKTREE / "shell"),
    "BEAD_ENGINE_LIFECYCLE_ONLY": "1",
    "BEAD_CORE_VIEWS_PROOF": "1",
    "BEAD_CORE_STATUS_PROOF": "1",
    "BEAD_DASHBOARD_VIEWS_PROOF": "1",
    "BEAD_DASHBOARD_FIXTURES": str(UI_WORKTREE / "shell/tests/fixtures/dashboard"),
    "BEAD_PANEL_IDLE_STATE": "release",
    "BEAD_DASHBOARD_RESIDENCY": "1",
    "BEAD_DASHBOARD_PAGES": "today,weather,media,system",
    "BEAD_DASHBOARD_RESIDENCY_SLOTS": "4",
    "BEAD_DASHBOARD_RESIDENCY_LEGACY": "1",
    "BEAD_DASHBOARD_ACCEPTANCE": "1",
    "BEAD_DASHBOARD_BATCH_MEDIANS": "1",
    "BEAD_DASHBOARD_WARMUP_SECONDS": "600",
}


def residency_valid(residency: dict) -> bool:
    """Full-batch mode needs all four phases; opened-once-only mode needs only 'once'."""
    if not residency.get("valid") or not residency.get("batch_medians"):
        return False
    if HOST_GATES["opened_once_only"]:
        return bool(residency.get("opened_once_only")) and bool(residency.get("once"))
    return not residency.get("opened_once_only") and all(residency.get(key) for key in PHASES)


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def artifact_hashes() -> dict[str, dict[str, str]]:
    result = {}
    artifacts = [("control", CONTROL), ("candidate", FIXED)]
    if EXPECTED_HASHES.get("helper") is not None:
        artifacts.append(("helper", HELPER))
    for name, path in artifacts:
        actual = sha256(path)
        if actual != EXPECTED_HASHES[name]:
            raise RuntimeError(f"{name} artifact SHA-256 mismatch: {actual}")
        result[name] = {"path": str(path), "sha256": actual}
    return result


def inherited_capture_environment(base: dict[str, str], side: str, compositor: str,
                                 raw_dir: pathlib.Path) -> dict[str, str]:
    # Preserve only host/session inputs needed to enter the already-running nested
    # compositor. All BEAD/NACRE inputs are cleared and reconstructed below.
    keep_exact = {
        "PATH", "HOME", "USER", "LOGNAME", "SHELL", "LANG", "LC_ALL", "LC_CTYPE",
        "DISPLAY", "DBUS_SESSION_BUS_ADDRESS", "XAUTHORITY", "LD_LIBRARY_PATH",
        "SSL_CERT_FILE", "SSL_CERT_DIR", "TMPDIR",
    }
    env = {key: value for key, value in base.items()
           if (key in keep_exact or key.startswith(("XDG_", "WAYLAND_", "QT_", "GDK_", "GTK_", "XCURSOR_")))
           and not re.search(r"(TOKEN|PASSWORD|SECRET|CREDENTIAL|PRIVATE_KEY)", key, re.I)}
    # The harness starts its own private nested compositor. Never forward a live
    # compositor socket/signature into the capture process.
    env.pop("HYPRLAND_INSTANCE_SIGNATURE", None)
    env.pop("NIRI_SOCKET", None)
    env.update(CAPTURE_CONFIG)
    env["BEAD_RAW_DIR"] = str(raw_dir)
    env["BEAD_HARDENING_BINARY"] = str(CONTROL if side == "A" else FIXED)
    env["BEAD_MENU_ENGINE_BINARY"] = str(HELPER)
    if CAPTURE_PROFILE != "legacy":
        env.pop("BEAD_MENU_ENGINE_BINARY", None)
    env.pop("BEAD_DASHBOARD_OPENED_ONCE_ONLY", None)
    if HOST_GATES["opened_once_only"]:
        env["BEAD_DASHBOARD_OPENED_ONCE_ONLY"] = "1"
    if compositor == "niri":
        env["BEAD_BAR_MATRIX"] = "1"
    else:
        env.pop("BEAD_BAR_MATRIX", None)
    if not env.get("XDG_RUNTIME_DIR"):
        raise RuntimeError("XDG_RUNTIME_DIR is required for the private user scope")
    return env


def capture_argv(compositor: str) -> list[str]:
    python = shutil.which("python3")
    if not python:
        raise RuntimeError("python3 is not on PATH")
    argv = [python, str(ROOT / "spikes/measure.py"), "--compositor", compositor,
            "--bar-ui", "--modes", "bead", "--runs", "1"]
    if compositor == "hyprland":
        argv.append("--virtual120")
    return argv


def previous_for(label: str) -> str:
    index = LABELS.index(label)
    return "START" if index == 0 else LABELS[index - 1]


def verify_previous(raw_comp_dir: pathlib.Path, label: str, previous: str) -> str:
    expected = previous_for(label)
    if previous != expected:
        raise RuntimeError(f"schedule violation: {label} requires previous label {expected}, got {previous}")
    if previous == "START":
        return "START"
    candidates = (raw_comp_dir / previous,
                  raw_comp_dir / f"{previous}-invocation-retry1",
                  raw_comp_dir / f"{previous}-replacement1")
    for prior in candidates:
        if (prior / "launcher-failure.json").is_file():
            continue
        safety_path = prior / f"{raw_comp_dir.name}-capture-safety.json"
        residency_path = prior / raw_comp_dir.name / "dashboard-residency.json"
        if not safety_path.is_file() or not residency_path.is_file():
            continue
        safety = json.loads(safety_path.read_text())
        residency = json.loads(residency_path.read_text())
        if safety.get("status") != "complete" or safety.get("capture_returncode") != 0 or safety.get("scope_runner_returncode") != 0:
            continue
        if not residency_valid(residency):
            continue
        return str(prior)
    raise RuntimeError(f"no valid completed evidence found for previous arm {previous}")


def sampling_has_started(raw_dir: pathlib.Path, compositor: str) -> bool:
    measured = raw_dir / compositor
    if any((measured / name).exists() for name in (
        "dashboard-residency-sampling-start.json", "dashboard-residency-baseline.json",
        "dashboard-residency-progress.json", "dashboard-residency.json", "bar.json", "bar.smaps")):
        return True
    return any(any(name.startswith(prefix) for prefix in (
        "never-opened", "opened-once", "opened-several", "combined-legacy", "closed-"))
        for name in (entry.name for entry in measured.iterdir())) if measured.exists() else False


def invocation_retry_allowed(raw_dir: pathlib.Path, compositor: str) -> bool:
    failure_path = raw_dir / "launcher-failure.json"
    if not failure_path.is_file() or sampling_has_started(raw_dir, compositor):
        return False
    failure = json.loads(failure_path.read_text())
    if failure.get("sampling_started"):
        return False
    safety_path = raw_dir / f"{compositor}-capture-safety.json"
    if not safety_path.exists():
        return failure.get("stage") == "preflight"
    safety = json.loads(safety_path.read_text())
    # The command may finish with a nonzero code inside a still-valid held scope
    # when the measured process rejects its invocation (for example an incorrect
    # priority). Permit one retry only before the sampling marker, with readable
    # held-scope OOM counters, zero OOM delta, and clean host swap-in.
    oom_delta = safety.get("oom_delta")
    swapin = failure.get("host_swapin_delta_bytes")
    return (safety.get("status") in ("complete", "aborted")
            and isinstance(safety.get("capture_returncode"), int)
            and safety.get("capture_returncode") != 0
            and safety.get("final_cgroup_read_while_scope_held") is True
            and isinstance(oom_delta, dict)
            and oom_delta.get("oom") == 0 and oom_delta.get("oom_kill") == 0
            and isinstance(swapin, int) and swapin < MIB)


def environmental_replacement_allowed(raw_dir: pathlib.Path, compositor: str, cause: str) -> bool:
    failure_path = raw_dir / "launcher-failure.json"
    safety_path = raw_dir / f"{compositor}-capture-safety.json"
    if not failure_path.is_file() or not safety_path.is_file():
        return False
    failure = json.loads(failure_path.read_text())
    safety = json.loads(safety_path.read_text())
    if cause == "host-swap-in":
        delta_path = raw_dir / "host-swapin-delta.txt"
        match = re.search(r"host_swapin_delta_bytes=(\d+)", delta_path.read_text()) if delta_path.is_file() else None
        return bool(match and int(match[1]) >= MIB and safety.get("status") == "complete"
                    and safety.get("capture_returncode") == 0
                    and not any((safety.get("oom_delta") or {}).values()))
    if cause == "scope-setup":
        reason = str(safety.get("reason", "")).lower()
        return (safety.get("status") == "aborted" and not sampling_has_started(raw_dir, compositor)
                and not any((safety.get("oom_delta") or {}).values())
                and any(token in reason for token in ("scope", "cgroup", "watchdog", "memory events")))
    return False


def latest_failed_attempt(raw_comp_dir: pathlib.Path, label: str) -> pathlib.Path:
    for suffix in ("-invocation-retry1", ""):
        candidate = raw_comp_dir / f"{label}{suffix}"
        safety_path = candidate / f"{raw_comp_dir.name}-capture-safety.json"
        residency_path = candidate / raw_comp_dir.name / "dashboard-residency.json"
        if safety_path.is_file() and residency_path.is_file():
            safety = json.loads(safety_path.read_text())
            residency = json.loads(residency_path.read_text())
            if (safety.get("status") == "complete" and safety.get("capture_returncode") == 0
                    and safety.get("scope_runner_returncode") == 0 and residency_valid(residency)):
                return candidate
        if (candidate / "launcher-failure.json").is_file():
            return candidate
    return raw_comp_dir / label


def relevant_environment(env: dict[str, str]) -> dict[str, str]:
    prefixes = ("BEAD_", "NACRE_", "XDG_", "WAYLAND_", "QT_", "GDK_", "GTK_", "XCURSOR_")
    exact = {"PATH", "HOME", "USER", "LOGNAME", "SHELL", "LANG", "LC_ALL", "LC_CTYPE",
             "DISPLAY", "DBUS_SESSION_BUS_ADDRESS", "XAUTHORITY", "LD_LIBRARY_PATH",
             "SSL_CERT_FILE", "SSL_CERT_DIR", "TMPDIR", "HYPRLAND_INSTANCE_SIGNATURE", "NIRI_SOCKET"}
    return dict(sorted((key, value) for key, value in env.items()
                       if (key.startswith(prefixes) or key in exact or key.endswith(("_PATH", "_DIR", "_FLAG", "_FLAGS")))
                       and not re.search(r"(TOKEN|PASSWORD|SECRET|CREDENTIAL|PRIVATE_KEY)", key, re.I)))


def launcher_revision() -> str:
    """Git HEAD of the harness, or the REVISION file of an rsync'd (git-less) copy."""
    result = subprocess.run(["git", "-C", str(ROOT), "rev-parse", "HEAD"], capture_output=True, text=True)
    if result.returncode == 0 and result.stdout.strip():
        return result.stdout.strip()
    revision = ROOT / "REVISION"
    if revision.is_file():
        return revision.read_text().strip()
    raise RuntimeError("cannot determine the harness revision (no git checkout and no REVISION file)")


def save_json(path: pathlib.Path, value) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def check_idle_host() -> None:
    for name in ("cargo", "rustc", "gcc", "cc1", "cc1plus", "clang", "cmake", "ninja", "make"):
        if subprocess.run(["pgrep", "-x", name], capture_output=True).returncode == 0:
            raise RuntimeError(f"heavy build process is running on the capture host: {name}")
    scopes = subprocess.run(["systemctl", "--user", "list-units", "--type=scope", "--state=running", "--no-legend"],
                            check=True, capture_output=True, text=True).stdout
    if "bead-capture" in scopes:
        raise RuntimeError("another bead capture scope is running")
    if HOST_GATES["max_load"] is not None:
        load1 = float(pathlib.Path("/proc/loadavg").read_text().split()[0])
        if load1 >= HOST_GATES["max_load"]:
            raise RuntimeError(f"1-minute load average {load1} is not below {HOST_GATES['max_load']}")
    if HOST_GATES["forbid_nacre_lead"]:
        found = subprocess.run(["pgrep", "-fa", "nacre-lead"], capture_output=True, text=True).stdout.splitlines()
        # The shared lock holder (our own flock/tmux wrapper) legitimately mentions the lock path.
        own = ("desktop-heavy.lock", "pgrep", "b0002-arm.py", "laptop-reference-capture.py", "drivers/batch.py")
        others = [line for line in found if not any(token in line for token in own)]
        if others:
            raise RuntimeError("other nacre-lead sessions are running: " + " | ".join(others)[:300])


def memavailable_and_swap() -> tuple[int, int]:
    memavailable = int(re.search(r"^MemAvailable:\s+(\d+)", pathlib.Path("/proc/meminfo").read_text(), re.M)[1]) * 1024
    rows = pathlib.Path("/proc/swaps").read_text().splitlines()[1:]
    if len(rows) != 1:
        raise RuntimeError(f"expected only the zram swap device, found {rows!r}")
    row = rows[0].split()
    if row[0] != "/dev/zram0" or row[1] not in ("partition", "file"):
        raise RuntimeError(f"unexpected swap device: {row!r}")
    used = int(row[3]) * 1024
    return memavailable, used


def log_swap_state(raw_dir: pathlib.Path, suffix: str) -> None:
    with (raw_dir / f"free-{suffix}.txt").open("w") as target:
        subprocess.run(["free", "-h"], check=True, stdout=target)
    with (raw_dir / f"swapon-{suffix}.txt").open("w") as target:
        subprocess.run(["swapon", "--show"], check=True, stdout=target)
        subprocess.run(["swapon", "--show", "--bytes", "--output", "NAME,SIZE,USED,PRIO"],
                       check=True, stdout=target)


def guarded_swap_reset(raw_dir: pathlib.Path) -> None:
    check_idle_host()
    log_swap_state(raw_dir, "before-arm")
    available, used = memavailable_and_swap()
    before = pathlib.Path("/proc/swaps").read_text().splitlines()[1].split()
    size_before = int(before[2]) * 1024
    priority_before = int(before[4])
    reset = used >= 64 * MIB
    (raw_dir / "swap-precondition.txt").write_text(
        f"MemAvailable_bytes={available}\nswap_used_bytes={used}\n"
        f"required_strictly_greater_than_bytes={used + 8 * 1024**3}\n"
        f"swap_capacity_before_bytes={size_before}\nswap_priority_before={priority_before}\n")
    (raw_dir / "swap-reset-command.txt").write_text(
        "sudo -n /usr/bin/swapoff -a && sudo -n /usr/bin/swapon -p 100 /dev/zram0\n"
        if reset else "not run: swap already below 64 MiB\n")
    if reset:
        if available <= used + 8 * 1024**3:
            raise RuntimeError(f"MemAvailable {available} is not greater than swap_used+8GiB ({used + 8*1024**3})")
        subprocess.run(["/bin/bash", "-lc",
                        "sudo -n /usr/bin/swapoff -a && sudo -n /usr/bin/swapon -p 100 /dev/zram0"], check=True)
    log_swap_state(raw_dir, "after-arm-preflight")
    rows = pathlib.Path("/proc/swaps").read_text().splitlines()[1:]
    if len(rows) != 1:
        raise RuntimeError(f"zram did not return as the only swap device: {rows!r}")
    row = rows[0].split()
    size = int(row[2]) * 1024
    used = int(row[3]) * 1024
    priority = int(row[4])
    expected_priority = 100 if reset else priority_before
    if row[0] != "/dev/zram0" or size != size_before or priority != expected_priority or used >= 64 * MIB:
        raise RuntimeError(f"zram post-reset verification failed: {row!r}")


def main() -> int:
    global CONTROL, FIXED, HELPER, EXPECTED_HASHES, CAPTURE_CONFIG, CAPTURE_PROFILE
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--side", choices=("A", "B"), required=True)
    parser.add_argument("--compositor", choices=("hyprland", "niri"), required=True)
    parser.add_argument("--label", choices=LABELS, required=True)
    parser.add_argument("--previous-label", required=True,
                        help="last VALID arm label for this compositor, or START")
    parser.add_argument("--raw-root", type=pathlib.Path, default=RAW_ROOT)
    parser.add_argument("--control-binary", type=pathlib.Path, default=CONTROL)
    parser.add_argument("--control-sha256", default=EXPECTED_HASHES["control"])
    parser.add_argument("--candidate-binary", type=pathlib.Path, default=FIXED)
    parser.add_argument("--candidate-sha256", default=EXPECTED_HASHES["candidate"])
    parser.add_argument("--helper-binary", type=pathlib.Path, default=HELPER)
    parser.add_argument("--helper-sha256", default=EXPECTED_HASHES["helper"])
    parser.add_argument("--research-tools", type=pathlib.Path, default=pathlib.Path(CAPTURE_CONFIG["BEAD_RESEARCH_TOOLS"]))
    parser.add_argument("--cargo-target-dir", type=pathlib.Path, default=pathlib.Path(CAPTURE_CONFIG["CARGO_TARGET_DIR"]))
    parser.add_argument("--menu-engine-bundle", type=pathlib.Path, default=pathlib.Path(CAPTURE_CONFIG["BEAD_MENU_ENGINE_BUNDLE"]))
    parser.add_argument("--dashboard-fixtures", type=pathlib.Path, default=pathlib.Path(CAPTURE_CONFIG["BEAD_DASHBOARD_FIXTURES"]))
    parser.add_argument("--invocation-retry", action="store_true",
                        help="retry one pre-sampling invocation failure without consuming the environmental replacement")
    parser.add_argument("--environmental-replacement", choices=("scope-setup", "host-swap-in"),
                        help="use the single authorized cell replacement after an environmental failure")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--profile", choices=("legacy", "laptop-reference", "rehearsal"), default="legacy")
    parser.add_argument("--opened-once-only", action="store_true",
                        help="diagnostic mode: BEAD_DASHBOARD_OPENED_ONCE_ONLY=1, only the opened-once phase")
    parser.add_argument("--max-load", type=float, default=None,
                        help="refuse to start an arm when the 1-minute load average is at or above this")
    parser.add_argument("--forbid-nacre-lead", action="store_true",
                        help="refuse to start an arm when other nacre-lead sessions are running")
    args = parser.parse_args()
    if args.label[0] != args.side:
        parser.error("--side must match the arm label")
    CAPTURE_PROFILE = args.profile
    HOST_GATES.update(max_load=args.max_load, forbid_nacre_lead=args.forbid_nacre_lead,
                      opened_once_only=args.opened_once_only)
    CONTROL, FIXED, HELPER = args.control_binary, args.candidate_binary, args.helper_binary
    EXPECTED_HASHES = {"control": args.control_sha256, "candidate": args.candidate_sha256,
                       "helper": args.helper_sha256}
    if args.profile in ("laptop-reference", "rehearsal"):
        EXPECTED_HASHES["helper"] = None
        CAPTURE_CONFIG = {**CAPTURE_CONFIG,
            "BEAD_HARDENING": "launcher",
            "BEAD_CORE_VIEWS_PROOF": "1", "BEAD_CORE_STATUS_PROOF": "1",
            "BEAD_DASHBOARD_VIEWS_PROOF": "1",
            "BEAD_DASHBOARD_FIXTURES": str(ROOT / "shell/tests/fixtures/dashboard"),
            "BEAD_PANEL_IDLE_STATE": "release", "BEAD_DASHBOARD_RESIDENCY": "1",
            "BEAD_DASHBOARD_PAGES": "today,weather,media,system",
            "BEAD_DASHBOARD_RESIDENCY_SLOTS": "4", "BEAD_DASHBOARD_RESIDENCY_LEGACY": "1",
            "BEAD_DASHBOARD_ACCEPTANCE": "1", "BEAD_DASHBOARD_BATCH_MEDIANS": "1",
            "BEAD_DASHBOARD_WARMUP_SECONDS": "0",
            "CARGO_TARGET_DIR": str(ROOT / "target")}
        if args.profile == "laptop-reference":
            CAPTURE_CONFIG["BEAD_REQUIRE_DRM"] = "1"
            CAPTURE_CONFIG["BEAD_REFERENCE_ABSOLUTE_GATE"] = "1"
    CAPTURE_CONFIG = {**CAPTURE_CONFIG,
                      "BEAD_MENU_ENGINE_BINARY": str(HELPER),
                      "BEAD_RESEARCH_TOOLS": str(args.research_tools),
                      "CARGO_TARGET_DIR": str(args.cargo_target_dir),
                      "BEAD_MENU_ENGINE_BUNDLE": str(args.menu_engine_bundle),
                      "BEAD_DASHBOARD_FIXTURES": str(args.dashboard_fixtures)}
    if args.profile in ("laptop-reference", "rehearsal"):
        CAPTURE_CONFIG.update({
            "BEAD_RESEARCH_TOOLS": str(args.research_tools),
            "CARGO_TARGET_DIR": str(ROOT / "target"),
            "BEAD_DASHBOARD_FIXTURES": str(ROOT / "shell/tests/fixtures/dashboard"),
        })
        CAPTURE_CONFIG.pop("BEAD_MENU_ENGINE_BINARY", None)
        CAPTURE_CONFIG.pop("BEAD_MENU_ENGINE_BUNDLE", None)
        if args.profile == "rehearsal":
            CAPTURE_CONFIG.pop("BEAD_REQUIRE_DRM", None)
            CAPTURE_CONFIG.pop("BEAD_REFERENCE_ABSOLUTE_GATE", None)
    raw_comp_dir = args.raw_root / args.compositor
    raw_dir = None
    stage = "preflight"
    try:
        previous_evidence = verify_previous(raw_comp_dir, args.label, args.previous_label)
        hashes = artifact_hashes()
        if args.invocation_retry and args.environmental_replacement:
            raise RuntimeError("choose either --invocation-retry or --environmental-replacement")
        if args.invocation_retry:
            first = raw_comp_dir / args.label
            if not invocation_retry_allowed(first, args.compositor):
                raise RuntimeError("invocation retry requires a recorded pre-sampling invocation failure")
            raw_dir = raw_comp_dir / f"{args.label}-invocation-retry1"
        elif args.environmental_replacement:
            first = latest_failed_attempt(raw_comp_dir, args.label)
            if not environmental_replacement_allowed(first, args.compositor, args.environmental_replacement):
                raise RuntimeError("replacement requires qualifying environmental evidence; product results cannot be replaced")
            raw_dir = raw_comp_dir / f"{args.label}-replacement1"
        else:
            raw_dir = raw_comp_dir / args.label
        if raw_dir.exists():
            raise RuntimeError(f"raw directory already exists: {raw_dir}")
        raw_dir.mkdir(parents=True)
        env = inherited_capture_environment(dict(os.environ), args.side, args.compositor, raw_dir)
        argv = capture_argv(args.compositor)
        save_json(raw_dir / "capture-launch.json", {
            "side": args.side, "label": args.label, "compositor": args.compositor,
            "previous_valid_label": args.previous_label, "previous_label_verified": True,
            "previous_evidence_dir": previous_evidence,
            "artifacts": hashes, "environment_filter": "BEAD_*, NACRE_*, XDG_*, WAYLAND_*, QT_*, GDK_*, GTK_*, XCURSOR_*, plus capture paths/session flags; secret-named variables omitted",
            "environment": relevant_environment(env), "argv": argv,
            "launcher_argv": sys.argv, "launcher_sha256": sha256(pathlib.Path(__file__).resolve()),
            "launcher_git_revision": launcher_revision(),
            "cwd": str(ROOT),
            "replacement_kind": "invocation" if args.invocation_retry else args.environmental_replacement,
            "dry_run": args.dry_run, "opened_once_only": args.opened_once_only,
            "max_load": args.max_load,
            "capture_profile": args.profile,
        })
        save_json(raw_dir / "artifact-hashes.json", hashes)
        (raw_dir / "argv.json").write_text(json.dumps(argv, indent=2) + "\n")
        print(f"ARM {args.compositor} {args.label}; previous valid label verified: {args.previous_label}", flush=True)
        print(f"RAW {raw_dir}", flush=True)
        print("ARTIFACTS " + json.dumps(hashes, sort_keys=True), flush=True)
        print("ARGV " + json.dumps(argv), flush=True)
        if args.dry_run:
            return 0
        stage = "swap-reset"
        guarded_swap_reset(raw_dir)
        pagesize = os.sysconf("SC_PAGE_SIZE")
        before = int(re.search(r"^pswpin\s+(\d+)", pathlib.Path("/proc/vmstat").read_text(), re.M)[1])
        (raw_dir / "host-swapin-before.txt").write_text(f"pswpin_pages={before} page_size_bytes={pagesize}\n")
        print(f"STARTING {args.compositor} {args.label} after verified reset", flush=True)
        stage = "capture"
        completed = subprocess.run(argv, cwd=ROOT, env=env)
        after = int(re.search(r"^pswpin\s+(\d+)", pathlib.Path("/proc/vmstat").read_text(), re.M)[1])
        delta = max(0, after - before) * pagesize
        (raw_dir / "host-swapin-after.txt").write_text(f"pswpin_pages={after} page_size_bytes={pagesize}\n")
        (raw_dir / "host-swapin-delta.txt").write_text(f"host_swapin_delta_bytes={delta}\n")
        if completed.returncode != 0:
            raise RuntimeError(f"capture exited {completed.returncode}; inspect raw evidence")
        if delta >= MIB:
            raise RuntimeError(f"host swap-in {delta} bytes reached/exceeded 1 MiB")
        safety = json.loads((raw_dir / f"{args.compositor}-capture-safety.json").read_text())
        residency = json.loads((raw_dir / args.compositor / "dashboard-residency.json").read_text())
        if safety.get("status") != "complete" or safety.get("oom_delta", {}).get("oom", 1) or safety.get("oom_delta", {}).get("oom_kill", 1):
            raise RuntimeError("capture safety report is invalid")
        if not residency_valid(residency):
            raise RuntimeError("residency report is invalid or lacks the expected phases for this mode")
        return 0
    except (OSError, RuntimeError, subprocess.CalledProcessError, KeyError, ValueError) as error:
        if raw_dir is not None and raw_dir.exists():
            sampling_started = sampling_has_started(raw_dir, args.compositor)
            safety_path = raw_dir / f"{args.compositor}-capture-safety.json"
            safety = json.loads(safety_path.read_text()) if safety_path.exists() else {}
            swapin_path = raw_dir / "host-swapin-delta.txt"
            swapin_match = re.search(r"host_swapin_delta_bytes=(\d+)", swapin_path.read_text()) if swapin_path.exists() else None
            save_json(raw_dir / "launcher-failure.json", {
                "stage": stage, "error": str(error), "sampling_started": sampling_started,
                "capture_report_exists": safety_path.exists(), "safety_status": safety.get("status"),
                "safety_reason": safety.get("reason"), "capture_returncode": safety.get("capture_returncode"),
                "oom_delta": safety.get("oom_delta"),
                "host_swapin_delta_bytes": int(swapin_match[1]) if swapin_match else None,
            })
        print(f"B0002 ARM STOP: {error}", file=sys.stderr, flush=True)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
