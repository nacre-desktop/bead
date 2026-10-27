#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Run one daily-acceptance arm through Bead's existing capture safety gate."""
import importlib.util
import argparse
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
raise SystemExit(safety.run_guarded(command, report_path=report, unit_id="b0162d1a1e6f"))
