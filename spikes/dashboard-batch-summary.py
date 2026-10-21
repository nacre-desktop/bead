#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Summarize one complete matched B0002 five-pair compositor batch."""
import argparse
import importlib.util
import json
import pathlib


ROOT = pathlib.Path(__file__).resolve().parent.parent
PHASES = ("never-opened", "opened-once", "opened-several", "combined-legacy")
COMPOSITORS = ("hyprland", "niri")


def load_residency(path=None):
    source = pathlib.Path(path or pathlib.Path(__file__).with_name("dashboard-residency.py"))
    spec = importlib.util.spec_from_file_location("dashboard_residency_for_batch", source)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def read_json(path):
    return json.loads(path.read_text())


def summarize(root, compositors=COMPOSITORS, report_path=None, residency_path=None,
              allow_27_compositors=()):
    root = pathlib.Path(root)
    residency = load_residency(residency_path)
    allow_27_compositors = set(allow_27_compositors)
    assert allow_27_compositors <= set(compositors), \
        "27 MiB allowance can only be granted to a compositor included in this summary"
    report_path = pathlib.Path(report_path or root / "batch-median-summary.json")
    report = {"complete": False, "compositors": list(compositors), "phases": {},
              "allow_27_compositors": sorted(allow_27_compositors),
              "one_time_27_mib_report_used": False, "failures": []}
    for compositor in compositors:
        if compositor not in COMPOSITORS:
            report["failures"].append(f"unknown compositor: {compositor}")
            continue
        for phase in PHASES:
            key = compositor + "/" + phase
            control_paths = [root / f"A{pair}" / compositor / (phase + ".json") for pair in range(1, 6)]
            candidate_paths = [root / f"B{pair}" / compositor / (phase + ".json") for pair in range(1, 6)]
            missing = [str(path.relative_to(root)) for path in control_paths + candidate_paths if not path.is_file()]
            if missing:
                report["failures"].append(key + ": missing endpoints " + ", ".join(missing))
                continue
            try:
                control = [read_json(path) for path in control_paths]
                candidate = [read_json(path) for path in candidate_paths]
                decision = residency.summarize_batch_phase(
                    control, candidate, allow_27_report=compositor in allow_27_compositors)
                decision["control_raw"] = [str(path.relative_to(root)) for path in control_paths]
                decision["candidate_raw"] = [str(path.relative_to(root)) for path in candidate_paths]
                if decision["verdict"] == "PASS-one-time-27MiB-file-backed-report":
                    report["one_time_27_mib_report_used"] = True
                report["phases"][key] = decision
            except Exception as error:
                report["phases"][key] = {"verdict": "STOP/report", "error": str(error),
                                         "control_raw": [str(path.relative_to(root)) for path in control_paths],
                                         "candidate_raw": [str(path.relative_to(root)) for path in candidate_paths]}
                report["failures"].append(key + ": " + str(error))
            report_path.parent.mkdir(parents=True, exist_ok=True)
            temporary = report_path.with_suffix(report_path.suffix + ".tmp")
            temporary.write_text(json.dumps(report, indent=2) + "\n")
            temporary.replace(report_path)
    report["complete"] = len(report["phases"]) == len(compositors) * len(PHASES)
    report["pass"] = report["complete"] and not report["failures"]
    report_path.parent.mkdir(parents=True, exist_ok=True)
    temporary = report_path.with_suffix(report_path.suffix + ".tmp")
    temporary.write_text(json.dumps(report, indent=2) + "\n")
    temporary.replace(report_path)
    return report


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=pathlib.Path, help="private root containing A1..A5/B1..B5")
    parser.add_argument("--compositors", nargs="+", choices=COMPOSITORS, default=COMPOSITORS)
    parser.add_argument("--allow-27-compositor", nargs="*", choices=COMPOSITORS, default=(),
                        help="apply the spec-agent-granted one-time allowance to all phases for these comparators")
    parser.add_argument("--report", type=pathlib.Path)
    args = parser.parse_args()
    report = summarize(args.root, args.compositors, args.report,
                       allow_27_compositors=args.allow_27_compositor)
    print(json.dumps({"complete": report["complete"], "pass": report["pass"],
                      "phases": len(report["phases"]), "failures": report["failures"]}, sort_keys=True))
    return 0 if report["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
