#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Pinned launcher and validator for one private B0003 typing arm.

The desktop driver owns the nested session and invokes this under its heavy-slot
flock. All evidence and artifact paths stay outside the repository.
"""
from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
os.environ.setdefault('BEAD_VERBOSE_LOG', '1')
import pathlib
import re
import shutil
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
MIB = 1024 * 1024
GIB = 1024**3
SWAP_SIZE_BYTES = 15_569_252_352
UI_SHA = "b01dfccaac148b984a6e1089f7a54d41bced473455b3d41890d15ec074a8038b"
HELPER_SHA = "4a65f4ec887bc85e93b5b862f1786fe6f1b7a811875333765ee8f8c052cce0aa"


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_artifact(path: pathlib.Path, expected: str) -> dict[str, str]:
    actual = sha256(path)
    if actual != expected.lower():
        raise RuntimeError(f"artifact SHA-256 mismatch for {path.name}: {actual}")
    return {"path": str(path), "sha256": actual}


def source_provenance() -> dict:
    result = subprocess.run(["git", "-C", str(ROOT), "rev-parse", "HEAD"], capture_output=True, text=True)
    revision = result.stdout.strip() if result.returncode == 0 else ""
    if not revision and (ROOT / "REVISION").is_file():
        revision = (ROOT / "REVISION").read_text().strip()
    if not revision:
        raise RuntimeError("cannot determine harness source revision")
    names = ("b0003-arm.py", "measure.py", "capture_safety.py", "hardening.py",
             "menu-engine-proof.py", "menu_engine_trace.py")
    return {"revision": revision,
            "files": {name: sha256(ROOT / "spikes" / name) for name in names}}


def capture_argv(compositor: str) -> list[str]:
    python = shutil.which("python3")
    if not python:
        raise RuntimeError("python3 is not on PATH")
    argv = [python, str(ROOT / "spikes/measure.py"), "--compositor", compositor,
            "--bar-ui", "--modes", "bead", "--runs", "1"]
    if compositor == "hyprland":
        argv.append("--virtual120")
    return argv


def tree_hash(path: pathlib.Path) -> str:
    """Stable hash of relative filenames and file bytes in a helper bundle."""
    digest = hashlib.sha256()
    for item in sorted(p for p in path.rglob("*") if p.is_file()):
        digest.update(item.relative_to(path).as_posix().encode() + b"\0")
        digest.update(bytes.fromhex(sha256(item)))
    return digest.hexdigest()


def memavailable_and_swap() -> tuple[int, int, list[str]]:
    available = int(re.search(r"^MemAvailable:\s+(\d+)", pathlib.Path("/proc/meminfo").read_text(), re.M)[1]) * 1024
    rows = pathlib.Path("/proc/swaps").read_text().splitlines()[1:]
    if len(rows) != 1:
        raise RuntimeError(f"expected only /dev/zram0 swap, found {rows!r}")
    row = rows[0].split()
    if row[0] != "/dev/zram0" or row[1] != "partition":
        raise RuntimeError(f"unexpected swap device: {row!r}")
    return available, int(row[3]) * 1024, row


def log_swap_state(raw: pathlib.Path, suffix: str) -> None:
    with (raw / f"free-{suffix}.txt").open("w") as stream:
        subprocess.run(["free", "-h"], check=True, stdout=stream)
    with (raw / f"swapon-{suffix}.txt").open("w") as stream:
        subprocess.run(["swapon", "--show", "--bytes", "--output", "NAME,SIZE,USED,PRIO"],
                       check=True, stdout=stream)


def check_idle_host(raw: pathlib.Path) -> None:
    load1 = float(pathlib.Path("/proc/loadavg").read_text().split()[0])
    (raw / "pre-arm-load.txt").write_text(f"load1={load1}\nrequired_strictly_below=1.0\n")
    if load1 >= 1.0:
        raise RuntimeError(f"1-minute load average {load1} is not below 1.0")
    if any(subprocess.run(["pgrep", "-x", name], capture_output=True).returncode == 0
           for name in ("cargo", "rustc")):
        raise RuntimeError("build process is running on desktop")
    units = subprocess.run(["systemctl", "--user", "list-units", "--type=scope", "--state=running", "--no-legend"],
                           check=True, capture_output=True, text=True).stdout
    if "bead-capture" in units:
        raise RuntimeError("competing bead capture scope is active")
    lines = subprocess.run(["pgrep", "-fa", "nacre-lead"], capture_output=True, text=True).stdout.splitlines()
    own = ("desktop-heavy.lock", "b0003-arm.py", "pgrep")
    others = [line for line in lines if not any(token in line for token in own)]
    if others:
        raise RuntimeError("other nacre-lead work is active: " + " | ".join(others)[:300])
    w_result = subprocess.run(["w", "-h"], check=True, capture_output=True, text=True)
    (raw / "w-idle.txt").write_text(w_result.stdout)
    local_ttys = []
    for line in w_result.stdout.splitlines():
        fields = line.split()
        if len(fields) < 5 or fields[0] != os.environ.get("USER"):
            continue
        tty, idle = fields[1], fields[3]
        if tty.startswith("pts/") or tty.startswith(":"):
            continue
        if idle_seconds(idle) > 300:
            local_ttys.append({"tty": tty, "idle": idle})
    if not local_ttys:
        raise RuntimeError("owner local tty has not been idle for more than 300 seconds")
    available, used, _ = memavailable_and_swap()
    (raw / "pre-arm-memory.txt").write_text(f"MemAvailable_bytes={available}\nswap_used_bytes={used}\n")
    if available < 4 * GIB:
        raise RuntimeError(f"pre-arm MemAvailable below 4 GiB: {available}")


def idle_seconds(value: str) -> float:
    """Parse the compact IDLE values printed by `w -h`."""
    if value in (".", "-", "?", ""):
        return 0
    day_match = re.fullmatch(r"(\d+)days?", value)
    if day_match:
        return int(day_match[1]) * 86400
    if ":" in value and value[0].isdigit():
        fields = value.split(":")
        return int(fields[-3]) * 3600 + int(fields[-2]) * 60 + int(fields[-1]) if len(fields) == 3 else int(fields[0]) * 60 + int(fields[1])
    match = re.fullmatch(r"(?:(\d+)d)?(?:(\d+)h)?(?:(\d+)m)?(?:(\d+(?:\.\d+)?)s)?", value)
    if not match:
        return 0
    days, hours, minutes, seconds = (float(part or 0) for part in match.groups())
    return days * 86400 + hours * 3600 + minutes * 60 + seconds


def pointer_target_dir(pointer: pathlib.Path) -> pathlib.Path:
    return pointer.resolve().parents[2]


def profile_for(mode: str, profile: str) -> str:
    if mode == "diagnostic" and profile != "1":
        raise RuntimeError("diagnostic requires stage-enabled profile 1")
    return "1" if mode == "diagnostic" else profile


def geometry_valid(compositor: str, geometry: dict) -> bool:
    if compositor == "hyprland":
        try:
            return (geometry.get("width") == 2880 and geometry.get("height") == 1800
                    and geometry.get("scale") == 1.5 and float(geometry.get("refreshRate", 0)) == 120.0)
        except (TypeError, ValueError):
            return False
    try:
        mode = geometry["modes"][geometry["current_mode"]]
        logical = geometry["logical"]
        return ((mode["width"], mode["height"]) == (2880, 1800)
                and logical["width"] == 1920 and logical["height"] == 1200
                and logical["scale"] == 1.5 and float(mode["refresh_rate"]) == 60000)
    except (KeyError, IndexError, TypeError, ValueError):
        return False


def guarded_swap_reset(raw: pathlib.Path) -> None:
    check_idle_host(raw)
    log_swap_state(raw, "before-reset")
    available, used, _ = memavailable_and_swap()
    (raw / "swap-precondition.txt").write_text(
        f"MemAvailable_bytes={available}\nswap_used_bytes={used}\nrequired_strictly_greater_than_bytes={used + 8 * GIB}\n")
    if available <= used + 8 * GIB:
        raise RuntimeError("cannot reset zram: MemAvailable is not greater than swap used + 8 GiB")
    subprocess.run(["/bin/bash", "-lc", "sudo -n /usr/bin/swapoff -a && sudo -n /usr/bin/swapon -p 100 /dev/zram0"], check=True)
    log_swap_state(raw, "after-reset")
    _, used_after, row = memavailable_and_swap()
    size, priority = int(row[2]) * 1024, int(row[4])
    if size != SWAP_SIZE_BYTES or priority != 100 or used_after >= 64 * MIB:
        raise RuntimeError(f"zram post-reset verification failed: {row!r}")


def pswpin_pages() -> int:
    return int(re.search(r"^pswpin\s+(\d+)", pathlib.Path("/proc/vmstat").read_text(), re.M)[1])


def trace_valid(path: pathlib.Path, expected_profile: str) -> tuple[bool, str]:
    text = path.read_text(errors="replace")
    inputs = re.findall(r"INPUT_RECEIVED monotonic_ns=\d+ cpu_ns=\d+", text)
    presentations = re.findall(r"UI_PRESENTED .*update=(\d+).*", text)
    if len(inputs) != 200:
        return False, f"expected 200 received keys, found {len(inputs)}"
    if len(presentations) < 200:
        return False, f"expected at least 200 presentations, found {len(presentations)}"
    if "MENU_RESULT ok=true catalog_rev=" not in text or "UI_COMMIT generation=" not in text:
        return False, "missing menu result or commit trace"
    profile_lines = re.findall(r"INPUT_PROFILE stage=", text)
    if expected_profile == "clocks" and profile_lines:
        return False, "clock-only trace unexpectedly emitted stage profiles"
    if expected_profile == "1" and not profile_lines:
        return False, "stage-enabled trace emitted no stage profiles"
    return True, f"200 received keys, {len(presentations)} presentations, menu/commit trace"


def validate_arm(directory: pathlib.Path, compositor: str, profile: str) -> dict:
    safety_path = directory / f"{compositor}-capture-safety.json"
    safety = json.loads(safety_path.read_text())
    if (safety.get("status") != "complete" or safety.get("capture_returncode") != 0
            or safety.get("scope_runner_returncode") != 0
            or safety.get("final_cgroup_read_while_scope_held") is not True):
        raise RuntimeError("guarded capture scope/watchdog/OOM result is incomplete")
    oom = safety.get("oom_delta") or {}
    if any(oom.get(key, 1) for key in ("oom", "oom_kill")):
        raise RuntimeError("capture scope recorded an OOM event")
    swapin_path = directory / "host-swapin-delta.txt"
    match = re.search(r"host_swapin_delta_bytes=(\d+)", swapin_path.read_text())
    if not match or int(match[1]) >= MIB:
        raise RuntimeError("host swap-in is missing or >= 1 MiB")
    trace = directory / compositor / "engine-typing.log"
    valid, reason = trace_valid(trace, profile)
    if not valid:
        raise RuntimeError(reason)
    proof_path = directory / compositor / "menu-engine-proof.json"
    proof = json.loads(proof_path.read_text())
    wanted = "1" if profile == "diagnostic" else profile
    if proof.get("valid") is not True or proof.get("profile_mode") != wanted:
        raise RuntimeError("typing proof is invalid or has wrong profile mode")
    if proof.get("keys") != 200 or len(proof.get("rows", [])) != 200:
        raise RuntimeError("typing proof does not contain all 200 keys")
    if any(not row.get("update") or row.get("result_count") is None for row in proof["rows"]):
        raise RuntimeError("typing proof lacks per-key result-count/update linkage")
    memory = proof.get("memory", {})
    memory_rows = [memory.get("ui", {}), memory.get("daemon", {}), *memory.get("workers", [])]
    if not memory_rows or any(row.get("swap_pss_mib") != 0 for row in memory_rows):
        raise RuntimeError("process SwapPss was not zero")
    screen = proof.get("idle_memory_screen", [])
    if len(screen) < 61 or screen[-1].get("elapsed_seconds", 0) < 60 or any(
            sample.get("swap_pss_mib") != 0 or
            any(row.get("swap_pss_mib") != 0 for row in [sample.get("ui", {}), sample.get("daemon", {}), *sample.get("workers", [])])
            for sample in screen):
        raise RuntimeError("60-second idle memory screen missing or SwapPss nonzero")
    geom = json.loads((directory / compositor / "output-geometry.json").read_text())
    if not geometry_valid(compositor, geom):
        raise RuntimeError("B0003 full-window geometry proof missing")
    return {"valid": True, "trace": str(trace), "trace_sha256": sha256(trace),
            "proof": str(proof_path), "proof_sha256": sha256(proof_path), "reason": reason}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("diagnostic", "arm"))
    parser.add_argument("--compositor", choices=("hyprland", "niri"), required=True)
    parser.add_argument("--label", required=True)
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--binary-sha256", required=True)
    parser.add_argument("--helper", type=pathlib.Path, required=True)
    parser.add_argument("--helper-sha256", required=True)
    parser.add_argument("--bundle", type=pathlib.Path, required=True)
    parser.add_argument("--pointer", type=pathlib.Path, required=True)
    parser.add_argument("--pointer-sha256", required=True)
    parser.add_argument("--profile", choices=("clocks", "1"), default="clocks")
    parser.add_argument("--raw-root", type=pathlib.Path, required=True)
    parser.add_argument("--lock-file", type=pathlib.Path, required=True,
                        help="desktop heavy-slot lock shared with other captures")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    raw = args.raw_root / args.compositor / args.label
    try:
        os.setpriority(os.PRIO_PROCESS, 0, 10)
        if os.getpriority(os.PRIO_PROCESS, 0) != 10:
            raise RuntimeError("runner must execute at nice 10 exactly")
        effective_profile = profile_for(args.mode, args.profile)
        if raw.exists():
            raise RuntimeError(f"arm evidence directory already exists: {raw}")
        artifacts = {"ui": verify_artifact(args.binary, args.binary_sha256),
                     "helper": verify_artifact(args.helper, args.helper_sha256),
                     "helper_bundle": {"path": str(args.bundle), "tree_sha256": tree_hash(args.bundle)},
                     "private_pointer": verify_artifact(args.pointer, args.pointer_sha256)}
        if not args.bundle.is_dir():
            raise RuntimeError("helper bundle directory is absent")
        if args.binary_sha256.lower() != UI_SHA or args.helper_sha256.lower() != HELPER_SHA:
            raise RuntimeError("artifact identity differs from approved B0003 source")
        raw.mkdir(parents=True)
        env = {key: value for key, value in os.environ.items()
               if key.startswith(("XDG_", "WAYLAND_", "QT_", "GDK_", "GTK_", "XCURSOR_"))
               or key in {"PATH", "HOME", "USER", "LANG", "DBUS_SESSION_BUS_ADDRESS"}}
        env["BEAD_HARDENING_BINARY"] = str(args.binary)
        env["BEAD_MENU_ENGINE_BINARY"] = str(args.helper)
        env["BEAD_RAW_DIR"] = str(raw)
        env["CARGO_TARGET_DIR"] = str(pointer_target_dir(args.pointer))
        env["BEAD_TYPING_PROFILE"] = args.profile
        env["BEAD_HARDENING"] = "menu-engine"
        env["BEAD_ENGINE_SETTLE_SECONDS"] = "32"
        env["BEAD_DASHBOARD_ACCEPTANCE"] = "1"
        if args.compositor == "niri":
            env["BEAD_BAR_MATRIX"] = "1"
        else:
            env.pop("BEAD_BAR_MATRIX", None)
        if args.mode == "diagnostic":
            env["BEAD_ENGINE_DIAGNOSTIC"] = "1"
        else:
            env.pop("BEAD_ENGINE_DIAGNOSTIC", None)
        env["BEAD_TYPING_KEYS"] = "200"
        argv = capture_argv(args.compositor)
        (raw / "capture-launch.json").write_text(json.dumps({
            "mode": args.mode, "compositor": args.compositor, "label": args.label,
            "profile": effective_profile, "artifacts": artifacts,
            "source_provenance": source_provenance(), "environment": env,
            "argv": argv, "dry_run": args.dry_run,
        }, indent=2, sort_keys=True) + "\n")
        if args.dry_run:
            return 0
        env["BEAD_ENGINE_SETTLE_SECONDS"] = "32"
        args.lock_file.parent.mkdir(parents=True, exist_ok=True)
        with args.lock_file.open("a+") as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as error:
                raise RuntimeError(f"desktop heavy slot is already held: {args.lock_file}") from error
            guarded_swap_reset(raw)
            pagesize = os.sysconf("SC_PAGE_SIZE")
            before = pswpin_pages()
            (raw / "host-swapin-before.txt").write_text(f"pswpin_pages={before} page_size_bytes={pagesize}\n")
            with (raw / "runner.log").open("w") as log:
                result = subprocess.run(argv, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
            after = pswpin_pages()
            delta = max(0, after - before) * pagesize
            (raw / "host-swapin-after.txt").write_text(f"pswpin_pages={after} page_size_bytes={pagesize}\n")
            (raw / "host-swapin-delta.txt").write_text(f"host_swapin_delta_bytes={delta}\n")
            if delta >= MIB:
                raise RuntimeError(f"host swap-in {delta} bytes reached/exceeded 1 MiB")
        if result.returncode:
            raise RuntimeError(f"measure.py returned {result.returncode}")
        validation = validate_arm(raw, args.compositor, effective_profile)
        (raw / "validation.json").write_text(json.dumps(validation, indent=2) + "\n")
        progress = pathlib.Path.home() / ".local/share/nacre/ai/lead/b0003-measured-20261022/progress.txt"
        progress.parent.mkdir(parents=True, exist_ok=True)
        with progress.open("a") as stream:
            stream.write(f"{args.compositor} {args.label} {args.mode} profile={args.profile} result=valid\n")
        return 0
    except (OSError, RuntimeError, KeyError, ValueError) as error:
        progress = pathlib.Path.home() / ".local/share/nacre/ai/lead/b0003-measured-20261022/progress.txt"
        progress.parent.mkdir(parents=True, exist_ok=True)
        with progress.open("a") as stream:
            stream.write(f"{args.compositor} {args.label} {args.mode} profile={args.profile} result=STOPPED\n")
        print(f"B0003 ARM STOP: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
