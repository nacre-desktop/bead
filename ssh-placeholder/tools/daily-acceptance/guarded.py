#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Run one daily-acceptance arm through Bead's existing capture safety gate."""
import importlib.util
import argparse
import os
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[1]
spec = importlib.util.spec_from_file_location("daily_capture_safety", ROOT / "spikes/capture_safety.py")
if spec is None or spec.loader is None:
    raise SystemExit("cannot load spikes/capture_safety.py")
safety = importlib.util.module_from_spec(spec)
spec.loader.exec_module(safety)

parser = argparse.ArgumentParser(add_help=False)
parser.add_argument("--evidence", type=pathlib.Path,
                    default=pathlib.Path.home() / "nacre-lead/b0162-daily-acceptance/evidence")
args, _ = parser.parse_known_args(sys.argv[1:])
report = args.evidence / "capture-safety.json"
command = ["nice", "-n", "10", "dbus-run-session", "--", sys.executable,
           str(HERE / "acceptance.py"), *sys.argv[1:]]
env = os.environ.copy()
for key in ("GNOME_KEYRING_CONTROL", "GNOME_KEYRING_PID"):
    env.pop(key, None)
private_root = env.get("BEAD_DAILY_HOME_ROOT")
if private_root:
    root = pathlib.Path(private_root).expanduser().resolve()
    env["HOME"] = str(root / "home")
    for variable, name in (("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache"),
                           ("XDG_DATA_HOME", "data"), ("XDG_STATE_HOME", "state")):
        env[variable] = str(root / name)
raise SystemExit(safety.run_guarded(command, report_path=report,
                                    env=env, unit_id="b0162d1a1e6f"))
