#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Summarize B0123 outputs from picker-spotlight-proof.py captures.

Memory inputs are the parity driver's idle-memory.json, drive.json, and
memory-phases.json sidecars. Timing inputs are sequence-clocks.json,
selection-clocks.json, and the Bead shell.log. The latter supplies bounded
per-open UI_FRAME/UI_CALLBACK intervals, hidden GL preparation cost, and
palette-serial-correlated UI_PRESENTED/WALLPAPER_PRESENTED clocks. Pass each
matched control and candidate capture root with --pair LABEL=CONTROL,CANDIDATE.
A root contains the driver's bead/ directory (or is that directory itself).

Unreclaimable MiB = (Pss_Anon + Pss_Shmem + SwapPss + DRM) KiB / 1024.
The script reports medians of paired candidate-minus-control deltas. It does
not infer scanout time from log order: precise palette timing needs
UI_TIMELINE marks emitted with BEAD_PROFILE_INPUT enabled.
"""
from __future__ import annotations

import argparse
import json
import math
import re
import statistics
import sys
from pathlib import Path


PHASES = ("idle", "expired", "open", "closed")
OPEN_STATES = ("spotlight-cold", "spotlight-repeat", "spotlight-expired")
PALETTE_STATE = "palette-sync"
COMMIT_RE = re.compile(r"\bUI_COMMIT\b(?P<fields>.*)")
FRAME_RE = re.compile(r"\bUI_FRAME\b(?P<fields>.*)")
PRESENT_RE = re.compile(r"\bUI_PRESENTED\b(?P<fields>.*)")
TIMELINE_RE = re.compile(r"\bUI_TIMELINE\b(?P<fields>.*)")
CALLBACK_RE = re.compile(r"\bUI_CALLBACK\b(?P<fields>.*)")
WALLPAPER_PRESENTED_RE = re.compile(r"\bWALLPAPER_PRESENTED\b(?P<fields>.*)")
WALLPAPER_COMMITTED_RE = re.compile(r"\bWALLPAPER_SURFACE_COMMITTED\b(?P<fields>.*)")
GPU_PREPARED_RE = re.compile(r"\bPANEL_GPU_PREPARED\b(?P<fields>.*)")


def read_json(path: Path):
    try:
        return json.loads(path.read_text())
    except (OSError, json.JSONDecodeError):
        return None


def arm_dir(root: Path) -> Path:
    candidate = root / "bead"
    return candidate if (candidate / "drive.json").is_file() else root


def fields(text: str) -> dict[str, str]:
    result = {}
    for part in text.strip().split():
        if "=" in part:
            key, value = part.split("=", 1)
            result[key] = value
    return result


def number(value):
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return None
    value = float(value)
    return value if math.isfinite(value) else None


def unreclaimable_mib(sample) -> float | None:
    if not isinstance(sample, dict):
        return None
    components = [number(sample.get(key)) for key in ("Pss_Anon", "Pss_Shmem", "SwapPss", "DRM")]
    if any(value is None for value in components):
        return None
    # parity-compare.Arm.rss stores each component in KiB; DRM is converted
    # from the measure.py MiB result to KiB at capture time.
    return sum(components) / 1024.0


def memory_samples(root: Path) -> tuple[dict[str, float | None], dict[str, dict], list[str]]:
    arm = arm_dir(root)
    notes = []
    idle = read_json(arm / "idle-memory.json")
    phases = read_json(arm / "memory-phases.json") or {}
    drive = read_json(arm / "drive.json") or {}
    expired_state = drive.get("spotlight-expired") if isinstance(drive, dict) else None
    raw = {
        "idle": idle,
        "expired": phases.get("expired-closed") if isinstance(phases, dict) else None,
        # Memory cohort open/closed values are specifically sampled after the
        # 125-second warm expiry. Visual-only cold captures are not substitutes.
        "open": expired_state.get("rss_kib") if isinstance(expired_state, dict) else None,
        "closed": expired_state.get("rss_closed_kib") if isinstance(expired_state, dict) else None,
    }
    values = {phase: unreclaimable_mib(raw[phase]) for phase in PHASES}
    information = {}
    for phase in PHASES:
        sample = raw[phase] if isinstance(raw[phase], dict) else {}
        information[phase] = {
            "pss_mib": None if number(sample.get("Pss")) is None else number(sample.get("Pss")) / 1024.0,
            "clean_file_pss_mib": None if number(sample.get("Pss_File")) is None else number(sample.get("Pss_File")) / 1024.0,
            "pss_anon_mib": None if number(sample.get("Pss_Anon")) is None else number(sample.get("Pss_Anon")) / 1024.0,
            "pss_shmem_mib": None if number(sample.get("Pss_Shmem")) is None else number(sample.get("Pss_Shmem")) / 1024.0,
            "swap_pss_mib": None if number(sample.get("SwapPss")) is None else number(sample.get("SwapPss")) / 1024.0,
            "drm_mib": None if number(sample.get("DRM")) is None else number(sample.get("DRM")) / 1024.0,
        }
    for phase in PHASES:
        if values[phase] is None:
            notes.append(f"{phase}: missing or incomplete memory sample")
    return values, information, notes


def summarize_memory(pairs, minimum: int) -> dict:
    per_pair = []
    for label, control, candidate in pairs:
        before, before_info, before_notes = memory_samples(control)
        after, after_info, after_notes = memory_samples(candidate)
        row = {"label": label, "control_mib": before, "candidate_mib": after,
               "control_info_mib": before_info, "candidate_info_mib": after_info, "delta_mib": {}}
        row["notes"] = [f"control {note}" for note in before_notes] + [f"candidate {note}" for note in after_notes]
        for phase in PHASES:
            a, b = before[phase], after[phase]
            row["delta_mib"][phase] = None if a is None or b is None else b - a
        per_pair.append(row)

    summaries = {}
    for phase in PHASES:
        usable = [row for row in per_pair if row["delta_mib"][phase] is not None]
        deltas = [row["delta_mib"][phase] for row in usable]
        control_values = [row["control_mib"][phase] for row in usable]
        candidate_values = [row["candidate_mib"][phase] for row in usable]
        control_pss = [row["control_info_mib"][phase]["pss_mib"] for row in usable]
        candidate_pss = [row["candidate_info_mib"][phase]["pss_mib"] for row in usable]
        control_file = [row["control_info_mib"][phase]["clean_file_pss_mib"] for row in usable]
        candidate_file = [row["candidate_info_mib"][phase]["clean_file_pss_mib"] for row in usable]
        component_medians = {}
        for component in ("pss_anon_mib", "pss_shmem_mib", "swap_pss_mib", "drm_mib"):
            control_component = [row["control_info_mib"][phase][component] for row in usable]
            candidate_component = [row["candidate_info_mib"][phase][component] for row in usable]
            paired_component = [b - a for a, b in zip(control_component, candidate_component)
                                if a is not None and b is not None]
            component_medians[component] = {
                "control_median_mib": statistics.median(v for v in control_component if v is not None) if any(v is not None for v in control_component) else None,
                "candidate_median_mib": statistics.median(v for v in candidate_component if v is not None) if any(v is not None for v in candidate_component) else None,
                "paired_delta_median_mib": statistics.median(paired_component) if paired_component else None,
            }
        delta = statistics.median(deltas) if deltas else None
        if len(usable) < minimum:
            status = f"INSUFFICIENT ({len(usable)}/{minimum} paired samples)"
        elif phase == "idle" and delta is not None and delta > 1.0:
            status = "OVER 1 MiB never-opened budget"
        elif phase == "idle":
            status = "WITHIN 1 MiB never-opened budget"
        else:
            status = "reported smoothness trade; no standalone pass threshold"
        summaries[phase] = {
            "n": len(usable),
            "control_median_mib": statistics.median(control_values) if control_values else None,
            "candidate_median_mib": statistics.median(candidate_values) if candidate_values else None,
            "paired_delta_median_mib": delta,
            "paired_deltas_mib": deltas,
            "control_pss_median_mib": statistics.median(v for v in control_pss if v is not None) if any(v is not None for v in control_pss) else None,
            "candidate_pss_median_mib": statistics.median(v for v in candidate_pss if v is not None) if any(v is not None for v in candidate_pss) else None,
            "control_clean_file_pss_median_mib": statistics.median(v for v in control_file if v is not None) if any(v is not None for v in control_file) else None,
            "candidate_clean_file_pss_median_mib": statistics.median(v for v in candidate_file if v is not None) if any(v is not None for v in candidate_file) else None,
            "unreclaimable_components": component_medians,
            "status": status,
        }
    return {"minimum_pairs": minimum, "phases": summaries, "pairs": per_pair}


def parse_timeline(log_path: Path) -> dict:
    commits, main_commits, frames, main_frames, presented, marks = [], [], [], [], [], []
    frame_rows, callbacks, palette_commits, wallpaper_commits, wallpaper_presented, gpu_prepared = [], [], [], [], [], []
    try:
        lines = log_path.read_text(errors="replace").splitlines()
    except OSError:
        return {"available": False, "reason": "shell.log missing"}
    image_bytes_by_output = {}
    for line in lines:
        if "UI_RESOURCES " in line:
            resource_fields = fields(line.split("UI_RESOURCES ", 1)[1])
            if resource_fields.get("extras_images_bytes", "").isdigit():
                image_bytes_by_output[resource_fields.get("output", "shared")] = int(resource_fields["extras_images_bytes"])
        hit = COMMIT_RE.search(line)
        if hit:
            f = fields(hit.group("fields"))
            if f.get("monotonic_ns", "").isdigit():
                output = f.get("output", "shared")
                item = {"ns": int(f["monotonic_ns"]), "update": f.get("update"),
                        "extras_images_bytes": image_bytes_by_output.pop(output, None),
                        "wallpaper_palette": f.get("wallpaper_palette") == "true",
                        "output": output}
                if f.get("companion") == "true":
                    commits.append(item)
                elif f.get("companion") == "false":
                    main_commits.append(item)
                if item["wallpaper_palette"]:
                    palette_commits.append({**item, "companion": f.get("companion")})
        hit = FRAME_RE.search(line)
        if hit:
            f = fields(hit.group("fields"))
            if f.get("monotonic_ns", "").isdigit():
                stamp = int(f["monotonic_ns"])
                frame_rows.append({"ns": stamp, "companion": f.get("companion"),
                                   "render_ms": number(float(f["render_ms"])) if f.get("render_ms") else None})
                if f.get("companion") == "true":
                    frames.append(stamp)
                elif f.get("companion") == "false":
                    main_frames.append(stamp)
        hit = PRESENT_RE.search(line)
        if hit:
            f = fields(hit.group("fields"))
            if f.get("monotonic_ns", "").isdigit():
                item = {"ns": int(f["monotonic_ns"]), "update": f.get("update"),
                        "companion": f.get("companion"), "refresh_ns": number(int(f["refresh_ns"]))
                        if f.get("refresh_ns", "").isdigit() else None}
                presented.append(item)
        hit = CALLBACK_RE.search(line)
        if hit:
            f = fields(hit.group("fields"))
            if f.get("monotonic_ns", "").isdigit():
                callbacks.append({"ns": int(f["monotonic_ns"]), "companion": f.get("companion")})
        hit = TIMELINE_RE.search(line)
        if hit:
            f = fields(hit.group("fields"))
            if f.get("stage") in ("wallpaper_palette_accepted", "wallpaper_surface_committed") and f.get("monotonic_ns", "").isdigit():
                marks.append({"stage": f["stage"], "ns": int(f["monotonic_ns"])})
        hit = WALLPAPER_COMMITTED_RE.search(line)
        if hit:
            f = fields(hit.group("fields"))
            if f.get("revision", "").isdigit():
                attach = next((item["ns"] for item in reversed(marks)
                               if item["stage"] == "wallpaper_surface_committed"), None)
                wallpaper_commits.append({"output": f.get("output"), "revision": int(f["revision"]),
                                          "ns": attach, "transition": f.get("transition")})
        hit = WALLPAPER_PRESENTED_RE.search(line)
        if hit:
            f = fields(hit.group("fields"))
            if f.get("monotonic_ns", "").isdigit() and f.get("revision", "").isdigit():
                wallpaper_presented.append({"output": f.get("output"), "revision": int(f["revision"]),
                                            "ns": int(f["monotonic_ns"]),
                                            "refresh_ns": number(int(f["refresh_ns"]))
                                            if f.get("refresh_ns", "").isdigit() else None})
        hit = GPU_PREPARED_RE.search(line)
        if hit:
            f = fields(hit.group("fields"))
            if f.get("monotonic_ns", "").isdigit() and f.get("prepare_ms"):
                gpu_prepared.append({"ns": int(f["monotonic_ns"]),
                                     "prepare_ms": number(float(f["prepare_ms"]))})
    commits.sort(key=lambda item: item["ns"])
    main_commits.sort(key=lambda item: item["ns"])
    frames.sort()
    main_frames.sort()
    presented.sort(key=lambda item: item["ns"])
    marks.sort(key=lambda item: item["ns"])
    frame_rows.sort(key=lambda item: item["ns"])
    callbacks.sort(key=lambda item: item["ns"])
    palette_commits.sort(key=lambda item: item["ns"])
    wallpaper_commits.sort(key=lambda item: item["ns"] or -1)
    wallpaper_presented.sort(key=lambda item: item["ns"])
    gpu_prepared.sort(key=lambda item: item["ns"])
    return {"available": True, "commits": commits, "main_commits": main_commits,
            "frames": frames, "main_frames": main_frames, "presented": presented, "marks": marks,
            "frame_rows": frame_rows, "callbacks": callbacks, "palette_commits": palette_commits,
            "wallpaper_commits": wallpaper_commits,
            "wallpaper_presented": wallpaper_presented, "gpu_prepared": gpu_prepared}


def content_timings(arm: Path) -> dict:
    clocks = read_json(arm / "sequence-clocks.json") or {}
    timeline = parse_timeline(arm / "shell.log")
    result = {"capture": arm.name, "states": {}}
    if not timeline["available"]:
        result["notes"] = [timeline["reason"]]
        return result
    for state in OPEN_STATES:
        key = f"{state}-open"
        clock = clocks.get(key) if isinstance(clocks, dict) else None
        if not isinstance(clock, dict) or number(clock.get("mono_start")) is None:
            result["states"][state] = {"status": "sequence clock unavailable"}
            continue
        start_ns = round(float(clock["mono_start"]) * 1e9)
        panel_before = clock.get("panel_before")
        if isinstance(panel_before, str):
            try:
                panel_before = json.loads(panel_before)
            except json.JSONDecodeError:
                panel_before = None
        prepared = isinstance(panel_before, dict) and panel_before.get("prepared") is True
        if prepared:
            # A warm panel may draw its retained cached scene before the async
            # refresh produces a new UI_COMMIT. In that case the first panel
            # UI_FRAME is the first-content signal. UI_PRESENTED is not assigned
            # to this frame because callbacks are deduplicated by UI update id.
            frame = next((stamp for stamp in timeline["frames"] if stamp >= start_ns), None)
            row = {"content_method": "retained prepared scene; first companion UI_FRAME",
                   "panel_prepared_before_open": True}
            if frame is not None:
                row["content_frame_ms"] = round((frame - start_ns) / 1e6, 3)
            else:
                row["status"] = "no companion UI_FRAME after warm open trigger"
            commit = next((item for item in timeline["commits"] if item["ns"] >= start_ns), None)
            if commit is not None:
                row["refresh_commit_ms"] = round((commit["ns"] - start_ns) / 1e6, 3)
                row["commit_update"] = commit["update"]
                presented = next((item for item in timeline["presented"] if item["ns"] >= commit["ns"]
                                  and item["companion"] == "true" and item["update"] == commit["update"]), None)
                if presented:
                    row["refresh_presented_ms"] = round((presented["ns"] - start_ns) / 1e6, 3)
            if "status" not in row:
                row["status"] = "warm retained frame; refresh commit reported separately"
            result["states"][state] = row
            continue
        commit = next((item for item in timeline["commits"] if item["ns"] >= start_ns), None)
        if commit is None:
            result["states"][state] = {"status": "no companion UI_COMMIT after open trigger"}
            continue
        def frame_row(commit_row, prefix):
            frame_ns = next((stamp for stamp in timeline["frames"] if stamp >= commit_row["ns"]), None)
            if frame_ns is not None:
                row[f"{prefix}_frame_ms"] = round((frame_ns - start_ns) / 1e6, 3)
            commit_update = commit_row["update"]
            presented_row = next((item for item in timeline["presented"] if item["ns"] >= commit_row["ns"]
                                  and item["companion"] == "true"
                                  and (commit_update is None or item["update"] == commit_update)), None)
            if presented_row:
                row[f"{prefix}_presented_ms"] = round((presented_row["ns"] - start_ns) / 1e6, 3)
        row = {"panel_prepared_before_open": False,
               "first_scene_commit_ms": round((commit["ns"] - start_ns) / 1e6, 3),
               "first_scene_update": commit["update"],
               "first_scene_image_bytes": commit["extras_images_bytes"]}
        frame_row(commit, "first_scene")
        image_commit = next((item for item in timeline["commits"]
                             if item["ns"] >= start_ns and item["extras_images_bytes"] is not None
                             and item["extras_images_bytes"] > 0), None)
        if image_commit is not None:
            row["content_method"] = "first companion UI_COMMIT with extras_images_bytes>0 (content proxy)"
            row["content_scene_commit_ms"] = round((image_commit["ns"] - start_ns) / 1e6, 3)
            row["content_scene_update"] = image_commit["update"]
            row["content_scene_image_bytes"] = image_commit["extras_images_bytes"]
            frame_row(image_commit, "content_scene")
            commits_by_update = {item["update"]: item for item in timeline["commits"]
                                 if item["update"] is not None and item["extras_images_bytes"] is not None
                                 and item["extras_images_bytes"] > 0}
            visible_image = next((item for item in timeline["presented"]
                                  if item["companion"] == "true" and item["update"] in commits_by_update
                                  and item["ns"] >= commits_by_update[item["update"]]["ns"]
                                  and item["ns"] >= start_ns), None)
            if visible_image is not None:
                row["content_presented_ms"] = round((visible_image["ns"] - start_ns) / 1e6, 3)
                row["content_presented_update"] = visible_image["update"]
            row["status"] = "first positive extras_images_bytes scene commit; UI_FRAME marks surface submission and UI_PRESENTED its nested presentation feedback"
        else:
            row["status"] = "first scene commit measured; no positive extras_images_bytes scene commit found"
        result["states"][state] = row
    return result


def percentile(values: list[float], p: float):
    if not values:
        return None
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, max(0, math.ceil(len(ordered) * p) - 1))]


def interval_stats(stamps: list[int]) -> dict:
    intervals = [(right - left) / 1e6 for left, right in zip(stamps, stamps[1:])]
    return {
        "sample_count": len(stamps),
        "interval_count": len(intervals),
        "interval_ms_p50": round(percentile(intervals, 0.50), 3) if intervals else None,
        "interval_ms_p99": round(percentile(intervals, 0.99), 3) if intervals else None,
        "interval_ms_max": round(max(intervals), 3) if intervals else None,
    }


def trace_pacing(arm: Path) -> dict:
    """Bounded open-window renderer/callback statistics from shell.log clocks."""
    clocks = read_json(arm / "sequence-clocks.json") or {}
    timeline = parse_timeline(arm / "shell.log")
    result = {"capture": arm.name, "states": {}}
    if not timeline["available"]:
        result["status"] = timeline["reason"]
        return result
    for state in OPEN_STATES:
        clock = clocks.get(f"{state}-open") if isinstance(clocks, dict) else None
        if not isinstance(clock, dict) or number(clock.get("mono_start")) is None:
            result["states"][state] = {"status": "sequence clock unavailable"}
            continue
        start = round(float(clock["mono_start"]) * 1e9)
        stop = start + round(float(number(clock.get("seconds")) or 2.) * 1e9)
        frames = [row for row in timeline["frame_rows"]
                  if row["companion"] == "true" and start <= row["ns"] < stop]
        callbacks = [row["ns"] for row in timeline["callbacks"]
                     if row["companion"] == "true" and start <= row["ns"] < stop]
        render = [row["render_ms"] for row in frames if row["render_ms"] is not None]
        preparations = [row for row in timeline["gpu_prepared"] if start <= row["ns"] < stop]
        row = {
            "window_ms": round((stop - start) / 1e6, 1),
            "ui_frame_render_wall_ms": {
                "sample_count": len(render),
                "p50": round(percentile(render, 0.50), 3) if render else None,
                "p99": round(percentile(render, 0.99), 3) if render else None,
            },
            "ui_frame_intervals": interval_stats([frame["ns"] for frame in frames]),
            "ui_callback_intervals": interval_stats(callbacks),
            "panel_gpu_prepared": [
                {"after_open_ms": round((item["ns"] - start) / 1e6, 3),
                 "prepare_ms": item["prepare_ms"]}
                for item in preparations
            ],
            "limits": "client render/frame-callback clocks, not compositor refresh or physical scanout",
        }
        result["states"][state] = row
    return result


def renderer_fallbacks(arm: Path) -> dict:
    try:
        lines = (arm / "shell.log").read_text(errors="replace").splitlines()
    except OSError:
        return {"capture": arm.name, "status": "shell.log missing"}
    return {
        "capture": arm.name,
        "companion_gpu_unavailable": sum("FALLBACK companion GPU unavailable:" in line for line in lines),
        "companion_gpu_to_cpu": sum(
            "RENDER_POLICY companion=true transition=gpu-to-cpu fallback=true" in line
            for line in lines
        ),
        "companion_cpu_to_gl": sum(
            "RENDER_POLICY companion=true transition=cpu-to-gl" in line for line in lines
        ),
        "panel_gpu_prepared": sum("PANEL_GPU_PREPARED " in line for line in lines),
    }


def palette_timing(arm: Path) -> dict:
    timeline = parse_timeline(arm / "shell.log")
    if not timeline["available"]:
        return {"capture": arm.name, "status": timeline["reason"]}
    selection = read_json(arm / "selection-clocks.json") or []
    selection_ns = next((int(item["send_ns"]) for item in selection
                         if isinstance(item, dict) and str(item.get("send_ns", "")).isdigit()), None)
    if selection_ns is None:
        return {"capture": arm.name, "status": "palette selection clock unavailable"}
    marks = timeline["marks"]
    accepted = next((item for item in marks if item["stage"] == "wallpaper_palette_accepted"
                     and (selection_ns is None or item["ns"] >= selection_ns)), None)
    boundary_ns = accepted["ns"] if accepted is not None else selection_ns
    attached = next((item for item in marks if item["stage"] == "wallpaper_surface_committed"
                     and boundary_ns is not None and item["ns"] >= boundary_ns), None)
    active_patch_ns = None
    observed_path = arm / "wallpaper-observed.jsonl"
    if selection_ns is not None:
        try:
            for line in observed_path.read_text(errors="replace").splitlines():
                observed = json.loads(line)
                message = observed.get("message", {})
                if (message.get("topic") == "wallpaper.active"
                        and message.get("t") == "patch"
                        and str(observed.get("receive_ns", "")).isdigit()
                        and int(observed["receive_ns"]) >= selection_ns):
                    active_patch_ns = int(observed["receive_ns"])
                    break
        except (OSError, json.JSONDecodeError):
            pass

    def tagged_present(companion: str):
        candidates = [
            item for item in timeline["palette_commits"]
            if item["companion"] == companion
            and accepted is not None
            and item["ns"] >= accepted["ns"] - 100_000_000
            and item["ns"] <= accepted["ns"] + 500_000_000
        ]
        for commit in candidates:
            present = next((item for item in timeline["presented"]
                            if item["companion"] == companion
                            and item["update"] == commit["update"]
                            and item["ns"] >= commit["ns"]
                            and (accepted is None or item["ns"] >= accepted["ns"])), None)
            if present is not None:
                return commit, present
        return None, None

    main_palette_commit, main_palette_present = tagged_present("false")
    panel_palette_commit, panel_palette_present = tagged_present("true")
    surface_commit = next((item for item in timeline["wallpaper_commits"]
                           if attached is not None
                           and item["ns"] is not None
                           and item["ns"] >= attached["ns"]), None)
    wallpaper_present = None
    if surface_commit is not None:
        wallpaper_present = next((item for item in timeline["wallpaper_presented"]
                                  if item["output"] == surface_commit["output"]
                                  and item["revision"] == surface_commit["revision"]
                                  and item["ns"] >= surface_commit["ns"]), None)

    row = {
        "capture": arm.name,
        "selection_send_ns": selection_ns,
        "active_topic_patch_ns": active_patch_ns,
        "wallpaper_palette_accepted_ns": accepted["ns"] if accepted else None,
        "wallpaper_attached_ns": attached["ns"] if attached else None,
        "wallpaper_presented_ns": wallpaper_present["ns"] if wallpaper_present else None,
        "wallpaper_presented_refresh_ns": wallpaper_present["refresh_ns"] if wallpaper_present else None,
        "main_palette_commit_update": main_palette_commit["update"] if main_palette_commit else None,
        "main_palette_commit_ns": main_palette_commit["ns"] if main_palette_commit else None,
        "main_palette_presented_ns": main_palette_present["ns"] if main_palette_present else None,
        "main_palette_presented_refresh_ns": main_palette_present["refresh_ns"] if main_palette_present else None,
        "panel_palette_commit_update": panel_palette_commit["update"] if panel_palette_commit else None,
        "panel_palette_commit_ns": panel_palette_commit["ns"] if panel_palette_commit else None,
        "panel_palette_presented_ns": panel_palette_present["ns"] if panel_palette_present else None,
        "panel_palette_presented_refresh_ns": panel_palette_present["refresh_ns"] if panel_palette_present else None,
    }
    if selection_ns is not None and active_patch_ns is not None:
        row["selection_to_active_topic_ms"] = round((active_patch_ns - selection_ns) / 1e6, 3)
    if selection_ns is not None and accepted is not None:
        row["selection_to_palette_accept_ms"] = round((accepted["ns"] - selection_ns) / 1e6, 3)
    if selection_ns is not None and attached is not None:
        row["selection_to_wallpaper_attach_ms"] = round((attached["ns"] - selection_ns) / 1e6, 3)
    if selection_ns is not None and wallpaper_present is not None:
        row["selection_to_wallpaper_present_ms"] = round((wallpaper_present["ns"] - selection_ns) / 1e6, 3)
    if accepted is not None:
        for name, item in (("main_palette", main_palette_present),
                           ("panel_palette", panel_palette_present),
                           ("wallpaper", wallpaper_present)):
            if item is not None:
                row[f"palette_accept_to_{name}_present_ms"] = round(
                    (item["ns"] - accepted["ns"]) / 1e6, 3)
        if attached is not None:
            row["palette_accept_to_wallpaper_attach_ms"] = round(
                (attached["ns"] - accepted["ns"]) / 1e6, 3)
    if wallpaper_present is not None:
        for name, item in (("main_palette", main_palette_present),
                           ("panel_palette", panel_palette_present)):
            if item is not None:
                row[f"{name}_minus_wallpaper_present_ms"] = round(
                    (item["ns"] - wallpaper_present["ns"]) / 1e6, 3)
    row["presentation_method"] = (
        "UI_COMMIT wallpaper_palette=true update matched to same-update UI_PRESENTED; "
        "wallpaper WALLPAPER_PRESENTED matched by output/revision after successful attach"
    )
    row["presentation_limit"] = (
        "refresh_ns=0 or absent supplies no refresh interval; protocol timestamps are nested "
        "presentation feedback, not physical scanout"
    )
    if accepted is None:
        row["status"] = "palette acceptance mark unavailable; exact palette-commit/presentation link unavailable"
    elif attached is None:
        row["status"] = "wallpaper attach timeline mark unavailable; enable BEAD_PROFILE_INPUT"
    elif wallpaper_present is None:
        row["status"] = "wallpaper presentation feedback unavailable for attached output/revision"
    return row


def pacing_summary(arm: Path) -> list[dict]:
    """Read animation-pacing.py's per-window schema when this capture has it."""
    data = read_json(arm / "pacing.json")
    if not isinstance(data, dict) or not isinstance(data.get("windows"), list):
        return []
    rows = []
    for window in data["windows"]:
        if not isinstance(window, dict):
            continue
        animation = str(window.get("animation", ""))
        if not ("wallpaper" in animation or "spotlight" in animation):
            continue
        rows.append({key: window.get(key) for key in (
            "animation", "phase", "repeat", "commits", "dropped", "drop_rate",
            "interval_ms", "first_frame_ms", "content_commit_ms", "render_ms",
        ) if key in window})
    return rows


def image_progress(arm: Path) -> dict:
    """Image-sequence proxy for when wallpaper and flat palette regions move.

    Endpoints are the driver's before/settled screenshots. Sequence JPEG times
    are request-spawn proxies, not compositor scanout timestamps.
    """
    before_path = arm / "shots" / "palette-sync-before.png"
    after_path = arm / "shots" / "palette-sync.png"
    sequence = arm / "seq" / "palette-sync-open"
    stamps = read_json(sequence / "stamps.json")
    if not before_path.is_file() or not after_path.is_file() or not isinstance(stamps, list):
        return {"status": "palette endpoint screenshots or image sequence unavailable"}
    try:
        from PIL import Image, ImageChops, ImageStat
        before_image = Image.open(before_path).convert("RGB")
        after_image = Image.open(after_path).convert("RGB")
        selection = read_json(arm / "selection-clocks.json") or []
        clocks = read_json(arm / "sequence-clocks.json") or {}
        clock = clocks.get("palette-sync-open") if isinstance(clocks, dict) else None
        start = number(clock.get("mono_start")) if isinstance(clock, dict) else None
        send_ns = next((int(row["send_ns"]) for row in selection
                        if isinstance(row, dict) and str(row.get("send_ns", "")).isdigit()), None)
        trigger_offset_ms = (send_ns / 1e6 - start * 1000) if send_ns is not None and start is not None else 0.0
        frames = []
        for index, stamp in enumerate(stamps):
            path = sequence / f"{index:03}.jpg"
            if not path.is_file() or number(stamp) is None:
                continue
            with Image.open(path) as opened:
                frames.append((float(stamp) - trigger_offset_ms, opened.convert("RGB").copy()))
        if not frames:
            return {"status": "palette image sequence has no readable frames"}

        # Normalized regions follow the release output's stable geometry. The
        # desktop ROI is above/right of the picker; the narrow top band samples
        # the bar, and panel_header samples the blank left side of the carousel
        # panel above its rows. Report endpoint magnitude so weak crops are visible.
        rois = {
            "wallpaper": (0.60, 0.15, 0.85, 0.50),
            "palette": (0.30, 0.005, 0.40, 0.010),
            "panel_palette": (0.20, 0.75, 0.28, 0.79),
        }

        def crop_fraction(image, roi):
            w, h = image.size
            left, top, right, bottom = roi
            box = (round(left * w), round(top * h), max(round(left * w) + 1, round(right * w)),
                   max(round(top * h) + 1, round(bottom * h)))
            return image.crop(box)

        regions = {}
        for name, roi in rois.items():
            first = crop_fraction(before_image.resize(frames[0][1].size, Image.Resampling.LANCZOS), roi)
            last = crop_fraction(after_image.resize(frames[0][1].size, Image.Resampling.LANCZOS), roi)
            endpoint_l1 = sum(ImageStat.Stat(ImageChops.difference(first, last)).mean) / 3.0
            if endpoint_l1 < 0.25:
                regions[name] = {"status": "endpoint change too small for a stable proxy", "endpoint_mean_l1": round(endpoint_l1, 4)}
                continue
            samples = []
            for stamp_ms, frame in frames:
                current = crop_fraction(frame, roi)
                distance = sum(ImageStat.Stat(ImageChops.difference(current, last)).mean) / 3.0
                progress = max(0.0, min(1.0, 1.0 - distance / endpoint_l1))
                samples.append((stamp_ms, progress))
            thresholds = {}
            for threshold in (0.1, 0.5, 0.9):
                found = next((stamp for stamp, progress in samples if progress >= threshold), None)
                thresholds[f"t{int(threshold * 100)}_ms"] = None if found is None else round(found, 1)
            regions[name] = {"endpoint_mean_l1": round(endpoint_l1, 4), **thresholds}
        wallpaper = regions.get("wallpaper", {})
        palette = regions.get("palette", {})
        panel_palette = regions.get("panel_palette", {})
        delays = {}
        for key in ("t10_ms", "t50_ms", "t90_ms"):
            if isinstance(wallpaper.get(key), (int, float)) and isinstance(palette.get(key), (int, float)):
                delays[f"bar_minus_wallpaper_{key}"] = round(palette[key] - wallpaper[key], 1)
            if isinstance(wallpaper.get(key), (int, float)) and isinstance(panel_palette.get(key), (int, float)):
                delays[f"panel_palette_minus_wallpaper_{key}"] = round(panel_palette[key] - wallpaper[key], 1)
        return {
            "status": "image-sequence L1 proxy; not scanout timing",
            "sample_count": len(frames),
            "trigger_offset_ms": round(trigger_offset_ms, 2),
            "wallpaper_roi_fraction": rois["wallpaper"],
            "palette_roi_fraction": rois["palette"],
            "panel_palette_roi_fraction": rois["panel_palette"],
            "regions": regions,
            "palette_minus_wallpaper_ms": delays,
        }
    except (OSError, ValueError, ImportError) as error:
        return {"status": f"image progress unavailable: {type(error).__name__}"}


def parse_pair(spec: str):
    if "=" not in spec or "," not in spec:
        raise argparse.ArgumentTypeError("pair format is LABEL=CONTROL_DIR,CANDIDATE_DIR")
    label, rest = spec.split("=", 1)
    control, candidate = rest.split(",", 1)
    if not label or not control or not candidate:
        raise argparse.ArgumentTypeError("pair label and both capture directories are required")
    return label, Path(control).expanduser(), Path(candidate).expanduser()


def parse_capture(spec: str):
    if "=" not in spec:
        raise argparse.ArgumentTypeError("capture format is LABEL=DIR")
    label, path = spec.split("=", 1)
    if not label or not path:
        raise argparse.ArgumentTypeError("capture label and directory are required")
    return label, Path(path).expanduser()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pair", action="append", type=parse_pair, default=[], metavar="LABEL=CONTROL,CANDIDATE",
                        help="matched capture roots; repeat in the order arms were alternated")
    parser.add_argument("--capture", action="append", type=parse_capture, default=[], metavar="LABEL=DIR",
                        help="additional timing/pacing capture root; repeat for visual or palette-only runs")
    parser.add_argument("--min-pairs", type=int, default=5)
    parser.add_argument("--backend", choices=("cpu", "femtovg-gl", "cpu-bar-gl-panel"), default=None,
                        help="assert the captured Bead renderer for the report")
    parser.add_argument("--skip-image-progress", action="store_true",
                        help="skip screenshot/JPEG L1 processing; use logs and JSON only")
    parser.add_argument("--json-out", type=Path, help="optional path for machine-readable results")
    args = parser.parse_args()
    if args.json_out is not None:
        args.json_out = args.json_out.expanduser()
    if not args.pair and not args.capture:
        parser.error("provide at least one --pair or --capture")
    if args.min_pairs < 1:
        parser.error("--min-pairs must be positive")

    memory = summarize_memory(args.pair, args.min_pairs)
    timing = []
    palettes = []
    image_proxies = []
    pacing = []
    frame_pacing = []
    renderer_fallbacks_by_capture = []
    captures = []
    for label, control, candidate in args.pair:
        captures.extend(((f"{label}/control", control), (f"{label}/candidate", candidate)))
    captures.extend(args.capture)
    for label, root in captures:
        arm = arm_dir(root)
        timing.append({**content_timings(arm), "capture": label})
        palettes.append({**palette_timing(arm), "capture": label})
        image_proxies.append(
            {"status": "image processing skipped", "capture": label}
            if args.skip_image_progress else
            {**image_progress(arm), "capture": label}
        )
        pacing.extend({"capture": label, **row} for row in pacing_summary(arm))
        frame_pacing.append({**trace_pacing(arm), "capture": label})
        renderer_fallbacks_by_capture.append({**renderer_fallbacks(arm), "capture": label})

    result = {
        "schema": 1,
        "backend": args.backend or "unspecified (pass --backend for Bead captures)",
        "memory_method": "Pss_Anon + Pss_Shmem + SwapPss + DRM from parity driver RSS sidecars; MiB; paired candidate-minus-control median",
        "memory": memory,
        "content_timing": timing,
        "palette_timing": palettes,
        "palette_image_proxy": image_proxies,
        "pacing": pacing,
        "frame_pacing": frame_pacing,
        "renderer_fallbacks": renderer_fallbacks_by_capture,
        "limitations": [
            "Pair order is supplied by the caller; the artifacts do not prove that capture arms alternated.",
            "A missing/unknown DRM value is not treated as zero; it leaves that memory sample unavailable.",
            "Cold content uses the first companion UI_COMMIT with positive extras_images_bytes; that cache-byte proxy may precede all visible thumbnails settling. Prepared warm panels use the first companion UI_FRAME. Warm UI_PRESENTED callbacks are not paired with that frame because callback logging deduplicates by UI update id.",
            "UI_FRAME render_ms and UI_CALLBACK intervals are bounded to each sequence clock's open window. They describe client draw/callback clocks, not physical refresh. PANEL_GPU_PREPARED reports hidden first-context upload time separately when frame tracing is enabled.",
            "Direct palette presentation links require a wallpaper_palette=true UI_COMMIT matched by companion/update to same-update UI_PRESENTED, plus WALLPAPER_PRESENTED matched to the attached output/revision. Missing tags remain unavailable; refresh_ns=0 and zero sequence fields do not prove a physical frame interval.",
            "Palette image-progress t10/t50/t90 uses normalized L1 crops between before/settled screenshots and sampled JPEGs; capture stamps are scheduling proxies, not scanout clocks.",
            "Exact palette and wallpaper presentation clocks require BEAD_PROFILE_INPUT=clocks and the opt-in presentation feedback capture.",
        ],
    }
    print(f"Backend: {result['backend']}")
    print("Unreclaimable memory (MiB): Pss_Anon + Pss_Shmem + SwapPss + DRM; paired deltas are candidate − control.")
    print("phase     n   control median   candidate median   paired delta median   status")
    for phase in PHASES:
        row = memory["phases"][phase]
        def fmt(value):
            return "—" if value is None else f"{value:.3f}"
        print(f"{phase:<9} {row['n']:<3} {fmt(row['control_median_mib']):>15} {fmt(row['candidate_median_mib']):>18} {fmt(row['paired_delta_median_mib']):>21}   {row['status']}")
    print("PSS / clean-file PSS medians (MiB, informational; excluded from the gate):")
    print("phase     control PSS   candidate PSS   control file   candidate file")
    for phase in PHASES:
        row = memory["phases"][phase]
        print(f"{phase:<9} {fmt(row['control_pss_median_mib']):>12} {fmt(row['candidate_pss_median_mib']):>15} "
              f"{fmt(row['control_clean_file_pss_median_mib']):>14} {fmt(row['candidate_clean_file_pss_median_mib']):>16}")
    print("\nFirst-content timing (Bead; ms from sequence open trigger):")
    for row in timing:
        states = row.get("states", {})
        for state in OPEN_STATES:
            item = states.get(state, {"status": "unavailable"})
            if any(key in item for key in ("content_scene_commit_ms", "content_frame_ms", "first_scene_commit_ms")):
                fields_out = []
                for key, name in (("first_scene_commit_ms", "first-scene-commit"),
                                  ("first_scene_frame_ms", "first-scene-frame"),
                                  ("first_scene_presented_ms", "first-scene-presented"),
                                  ("content_scene_commit_ms", "content-scene-commit"),
                                  ("content_frame_ms", "image-frame"),
                                  ("content_presented_ms", "image-presented"),
                                  ("refresh_commit_ms", "refresh-commit"),
                                  ("refresh_presented_ms", "refresh-presented")):
                    if key in item:
                        fields_out.append(f"{name}={item[key]:.3f}")
                print(f"{row['capture']} {state}: " + " ".join(fields_out) + f" ({item['content_method']})")
            else:
                print(f"{row['capture']} {state}: {item.get('status', 'unavailable')}")
    print("\nClient render/callback intervals (open-window bounded; not physical refresh):")
    for capture in frame_pacing:
        for state, item in capture.get("states", {}).items():
            if "ui_frame_render_wall_ms" not in item:
                print(f"{capture['capture']} {state}: {item.get('status', 'unavailable')}")
                continue
            render = item["ui_frame_render_wall_ms"]
            frame_interval = item["ui_frame_intervals"]
            callback = item["ui_callback_intervals"]
            prep = item.get("panel_gpu_prepared", [])
            prep_text = (", ".join(
                f"hidden GL prep +{row['after_open_ms']:.3f}ms/{row['prepare_ms']:.3f}ms"
                for row in prep
            ) if prep else "no hidden GL prep mark")
            print(
                f"{capture['capture']} {state}: UI_FRAME render wall n={render['sample_count']} "
                f"p50/p99={render['p50']}/{render['p99']}ms; "
                f"UI_FRAME interval p50/p99={frame_interval['interval_ms_p50']}/"
                f"{frame_interval['interval_ms_p99']}ms (n={frame_interval['interval_count']}); "
                f"UI_CALLBACK interval p50/p99={callback['interval_ms_p50']}/"
                f"{callback['interval_ms_p99']}ms (n={callback['interval_count']}); {prep_text}"
            )
    print("\nPalette timing:")
    for row in palettes:
        if row.get("wallpaper_palette_accepted_ns") is not None:
            print(
                f"{row['capture']}: selection→active={row.get('selection_to_active_topic_ms')}ms, "
                f"selection→accept={row.get('selection_to_palette_accept_ms')}ms, "
                f"accept→attach={row.get('palette_accept_to_wallpaper_attach_ms')}ms, "
                f"accept→wallpaper-present={row.get('palette_accept_to_wallpaper_present_ms')}ms, "
                f"accept→bar-present={row.get('palette_accept_to_main_palette_present_ms')}ms "
                f"(update={row.get('main_palette_commit_update')}), "
                f"accept→panel-present={row.get('palette_accept_to_panel_palette_present_ms')}ms "
                f"(update={row.get('panel_palette_commit_update')}), "
                f"bar/panel−wallpaper={row.get('main_palette_minus_wallpaper_present_ms')}/"
                f"{row.get('panel_palette_minus_wallpaper_present_ms')}ms; "
                f"refresh_ns={row.get('wallpaper_presented_refresh_ns')}/"
                f"{row.get('main_palette_presented_refresh_ns')}/"
                f"{row.get('panel_palette_presented_refresh_ns')}"
            )
        else:
            print(
                f"{row['capture']}: selection→active={row.get('selection_to_active_topic_ms')}ms, "
                f"selection→attach={row.get('selection_to_wallpaper_attach_ms')}ms; "
                f"{row.get('status', 'unavailable')}"
            )
    print("\nPalette image-progress proxy (sequence request stamps; not scanout timing):")
    for row in image_proxies:
        regions = row.get("regions", {})
        wallpaper = regions.get("wallpaper", {})
        palette = regions.get("palette", {})
        panel_palette = regions.get("panel_palette", {})
        if row.get("status", "").startswith("image-sequence"):
            print(f"{row['capture']}: wallpaper t10/t50/t90={wallpaper.get('t10_ms')}/{wallpaper.get('t50_ms')}/{wallpaper.get('t90_ms')} ms; "
                  f"bar={palette.get('t10_ms')}/{palette.get('t50_ms')}/{palette.get('t90_ms')} ms; "
                  f"panel={panel_palette.get('t10_ms')}/{panel_palette.get('t50_ms')}/{panel_palette.get('t90_ms')} ms; "
                  f"bar/panel−wallpaper={row.get('palette_minus_wallpaper_ms')}")
        else:
            print(f"{row['capture']}: {row.get('status', 'unavailable')}")
    print("\nAnimation-pacing windows (only when pacing.json includes wallpaper/spotlight rows):")
    if not pacing:
        print("unavailable in supplied captures")
    for row in pacing:
        interval = row.get("interval_ms") or {}
        render = row.get("render_ms") or {}
        print(f"{row['capture']} {row.get('animation')} {row.get('phase')} repeat={row.get('repeat')} "
              f"commits={row.get('commits')} dropped={row.get('dropped')} p99={interval.get('p99')} "
              f"first={row.get('first_frame_ms')} render_p99={render.get('p99')}")
    print("\nRenderer fallback/preparation marker counts:")
    for row in renderer_fallbacks_by_capture:
        print(
            f"{row['capture']}: unavailable={row.get('companion_gpu_unavailable')}, "
            f"GPU→CPU={row.get('companion_gpu_to_cpu')}, CPU→GL={row.get('companion_cpu_to_gl')}, "
            f"prepared={row.get('panel_gpu_prepared')} ({row.get('status', 'shell.log')})"
        )
    if args.json_out:
        args.json_out.write_text(json.dumps(result, indent=2) + "\n")
        print(f"\nJSON: {args.json_out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
