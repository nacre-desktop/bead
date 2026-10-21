#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Private dashboard residency decomposition; reuse the nested proof setup."""
import json, os, pathlib, time
import math


ACCEPTANCE_PHASE_BUDGETS = {
    "never-opened": 65,
    "opened-once": 85,
    "opened-several": 105,
    "combined-legacy": 95,
}
ACCEPTANCE_PAGES = ["today", "weather", "media", "system"]
MEDIAN_BATCH_ENV = "BEAD_DASHBOARD_BATCH_MEDIANS"
OPENED_ONCE_ONLY_ENV = "BEAD_DASHBOARD_OPENED_ONCE_ONLY"
MEDIAN_BATCH_ARM_LIMIT_MIB = 26.5
MEDIAN_BATCH_ANON_GROWTH_LIMIT_MIB = 1
MEDIAN_BATCH_FILE_EXPLANATION_TOLERANCE_MIB = 0.05


def acceptance_config(environ):
    mode = environ.get("BEAD_DASHBOARD_ACCEPTANCE")
    if mode in (None, "", "0"):
        return None
    if mode != "1":
        raise ValueError("BEAD_DASHBOARD_ACCEPTANCE must be 0 or 1")
    if environ.get("BEAD_DASHBOARD_RESIDENCY") != "1":
        raise ValueError("dashboard acceptance requires BEAD_DASHBOARD_RESIDENCY=1")
    pages = environ.get("BEAD_DASHBOARD_PAGES", ",".join(ACCEPTANCE_PAGES)).split(",")
    if pages != ACCEPTANCE_PAGES:
        raise ValueError("dashboard acceptance requires today,weather,media,system in order")
    try:
        slots = int(environ.get("BEAD_DASHBOARD_RESIDENCY_SLOTS", "4"))
    except ValueError as error:
        raise ValueError("dashboard acceptance requires exactly four residency slots") from error
    if slots != 4:
        raise ValueError("dashboard acceptance requires exactly four residency slots")
    if environ.get("BEAD_DASHBOARD_RESIDENCY_LEGACY", "1") != "1":
        raise ValueError("dashboard acceptance requires the combined legacy phase")
    warmup = environ.get("BEAD_DASHBOARD_WARMUP_SECONDS", "0")
    if warmup not in ("0", "600"):
        raise ValueError("BEAD_DASHBOARD_WARMUP_SECONDS must be 0 or 600 in acceptance mode")
    batch_medians = environ.get(MEDIAN_BATCH_ENV, "0")
    if batch_medians not in ("0", "1"):
        raise ValueError(MEDIAN_BATCH_ENV + " must be 0 or 1")
    opened_once_only = environ.get(OPENED_ONCE_ONLY_ENV, "0")
    if opened_once_only not in ("0", "1"):
        raise ValueError(OPENED_ONCE_ONLY_ENV + " must be 0 or 1")
    return {"warmup_seconds": int(warmup), "batch_medians": batch_medians == "1",
            "legacy": True,
            "opened_once_only": opened_once_only == "1"}


def acceptance_baseline(pid, idle, warmup_seconds, monotonic, sleep, keeper_check=None,
                        sampling_started=None):
    warmup_started = monotonic()
    if warmup_seconds:
        sleep(warmup_seconds)
    warmup_elapsed = monotonic() - warmup_started
    assert warmup_elapsed >= warmup_seconds, "measured warmup shorter than requested"
    if keeper_check:
        keeper_check("after warmup")
        keeper_check("before baseline")
    if sampling_started:
        sampling_started()
    row = observe_window(idle, pid, 60, require_full=True)
    if keeper_check:
        keeper_check("after baseline")
    return row, {
        "mode": "acceptance",
        "warmup_requested_seconds": warmup_seconds,
        "warmup_elapsed_seconds": warmup_elapsed,
        "baseline_idle_seconds": row["seconds"],
    }


def observe_window(idle, pid, seconds, require_full=False):
    row = idle(pid, seconds)
    if require_full:
        assert row["seconds"] >= seconds, "dashboard acceptance idle shorter than 60 seconds"
    return row


def assert_capture_rails(row, bar, external, total, batch_medians=False, close=False):
    assert row["memory"]["swap_pss_mib"] == 0, "UI swap invalidates comparison"
    assert all(x["memory"]["swap_pss_mib"] == 0 for x in external["processes"]), "daemon swap invalidates comparison"
    if not batch_medians or close:
        assert total(row) <= total(bar) + 5, "closed exceeds own bar+5 MiB"
    if batch_medians:
        memory = row["memory"]
        # Required category fields make a 25–27 MiB result auditable later.
        assert all(key in memory for key in ("pss_anon_mib", "pss_file_mib", "pss_shmem_mib")), \
            "batch arm lacks PSS category attribution"
    else:
        assert total(row) <= 25, "diagnostic exceeds current hard ceiling"


def summarize_batch_phase(control_rows, candidate_rows, allow_27_report=False):
    """Apply owner-approved matched five-pair median gates to one phase."""
    import statistics
    assert len(control_rows) == 5 and len(candidate_rows) == 5, \
        "batch median requires exactly five matched alternating-pair rows per side"

    def validate(rows):
        for row in rows:
            memory = row["memory"]
            values = [row["pss_plus_drm_mib"], memory["pss_mib"], memory["swap_pss_mib"],
                      memory["pss_anon_mib"], memory["pss_file_mib"], memory["pss_shmem_mib"]]
            assert memory["swap_pss_mib"] == 0, "batch median includes UI SwapPss"
            if row.get("drm_mib") is not None:
                values.extend(row["drm_mib"].values())
                drm_total = sum(row["drm_mib"].values())
                assert abs(memory["pss_mib"] + drm_total - row["pss_plus_drm_mib"]) <= 0.01, \
                    "batch median PSS plus DRM does not explain total"
            assert all(not isinstance(value, bool) and isinstance(value, (int, float))
                       and math.isfinite(value) and value >= 0
                       for value in values), "batch median contains invalid or negative measurement"
            assert abs(memory["pss_mib"] - sum(memory[key] for key in
                       ("pss_anon_mib", "pss_file_mib", "pss_shmem_mib"))) <= 0.01, \
                "batch median categories do not explain PSS"

    validate(control_rows)
    validate(candidate_rows)
    def total(row):
        return row["pss_plus_drm_mib"] + row["memory"]["swap_pss_mib"]
    def median(rows, value):
        return statistics.median(value(row) for row in rows)
    categories = ("pss_anon_mib", "pss_file_mib", "pss_shmem_mib")
    category_medians = {side: {key: median(rows, lambda r, k=key: r["memory"][k])
                               for key in categories}
                        for side, rows in (("control", control_rows), ("candidate", candidate_rows))}
    deltas = {key: median(candidate_rows, lambda r, k=key: r["memory"][k])
              - median(control_rows, lambda r, k=key: r["memory"][k]) for key in categories}
    control_total = median(control_rows, total)
    candidate_total = median(candidate_rows, total)
    anonymous_delta = deltas["pss_anon_mib"]
    assert anonymous_delta <= MEDIAN_BATCH_ANON_GROWTH_LIMIT_MIB, \
        "unexplained candidate-control anonymous growth exceeds 1 MiB"
    file_delta = deltas["pss_file_mib"]
    total_delta = candidate_total - control_total
    unexplained_increase = max(0.0, total_delta - file_delta)
    result = {"n_per_side": 5, "control_total_median_mib": control_total,
              "candidate_total_median_mib": candidate_total,
              "total_delta_median_mib": total_delta,
              "unexplained_increase_median_mib": unexplained_increase,
              "category_medians_mib": category_medians,
              "category_delta_medians_mib": deltas,
              "control_range_mib": [min(total(r) for r in control_rows), max(total(r) for r in control_rows)],
              "candidate_range_mib": [min(total(r) for r in candidate_rows), max(total(r) for r in candidate_rows)],
              "control_drm_median_mib": None if any(r.get("drm_mib") is None for r in control_rows)
                  else median(control_rows, lambda r: sum(r["drm_mib"].values())),
              "candidate_drm_median_mib": None if any(r.get("drm_mib") is None for r in candidate_rows)
                  else median(candidate_rows, lambda r: sum(r["drm_mib"].values())),
              "drm_attribution": "unknown" if any(r.get("drm_mib") is None for r in control_rows + candidate_rows)
                  else "measured"}
    if candidate_total < 25:
        result["verdict"] = "PASS"
    elif candidate_total <= MEDIAN_BATCH_ARM_LIMIT_MIB and allow_27_report:
        assert anonymous_delta < MEDIAN_BATCH_ANON_GROWTH_LIMIT_MIB, \
            "25–26.5 MiB allowance requires anonymous growth below 1 MiB"
        assert unexplained_increase <= MEDIAN_BATCH_FILE_EXPLANATION_TOLERANCE_MIB + 1e-9, \
            "25–26.5 MiB increase is not explained by file-backed PSS within 0.05 MiB"
        result["verdict"] = "PASS-one-time-27MiB-file-backed-report"
    else:
        raise AssertionError("batch median exceeds the 26.5 MiB stop ceiling or the one-time allowance is unavailable")
    return result


def finish_phase_window(label, began, budget, capture, acceptance, monotonic, sleep, keeper_check=None):
    elapsed = monotonic() - began
    if acceptance:
        idle_seconds = 60
        wait_seconds = budget - elapsed - idle_seconds
        assert wait_seconds >= 0, "phase activity overran its matched acceptance window"
        if wait_seconds:
            sleep(wait_seconds)
        if keeper_check:
            keeper_check("before phase idle observation")
        row = capture(idle_seconds)
        if keeper_check:
            keeper_check("after phase idle observation")
        actual_idle = row["seconds"]
        assert actual_idle >= idle_seconds, "dashboard acceptance phase idle shorter than 60 seconds"
        return row, {
            "mode": "acceptance",
            "phase_budget_seconds": budget,
            "activity_seconds": elapsed,
            "pre_window_wait_seconds": wait_seconds,
            "actual_idle_seconds": actual_idle,
            "phase_seconds": monotonic() - began,
        }

    assert elapsed <= budget - 30, "phase leaves less than30s idle"
    wait_seconds = budget - elapsed
    sleep(wait_seconds)
    row = capture(1)
    return row, {"mode": "legacy", "phase_seconds": monotonic() - began}


def sample(client, raw, bar, matrix, call, state_wait, log, presented, committed, observe, enabled, baseline_timing=None, keeper_check=None):
    acceptance = acceptance_config(os.environ)
    def total(row):
        return row["pss_plus_drm_mib"] + row["memory"]["swap_pss_mib"]

    pages = os.environ.get("BEAD_DASHBOARD_PAGES", "today,weather,media,system").split(",")
    assert pages and len(set(pages)) == len(pages) and all(p in ["today", "weather", "media", "system"] for p in pages)
    slots = int(os.environ.get("BEAD_DASHBOARD_RESIDENCY_SLOTS", str(len(pages))))
    assert len(pages) <= slots <= 12
    page_slots = [pages[index % len(pages)] for index in range(slots)]
    samples = []
    closed = []
    warm = []
    def capture(label, idle_seconds=1, settled_state=None):
        state = settled_state or state_wait(lambda s: not s["resident"] and not s["target_open"] and s["surfaces"] == 0)
        row = observe_window(matrix.idle, client.pid, idle_seconds, require_full=bool(acceptance and idle_seconds == 60))
        row["label"] = label
        row["panel"] = state
        row["legacy"] = json.loads(call("state", target="nacre"))
        row["external"] = observe()
        assert_capture_rails(row, bar, row["external"], total,
                             batch_medians=bool(acceptance and acceptance["batch_medians"]),
                             close="-close-" in label)
        (raw / (label + ".json")).write_text(json.dumps(row, indent=2) + "\n")
        (raw / (label + ".smaps")).write_text(pathlib.Path(f"/proc/{client.pid}/smaps").read_text())
        samples.append(row)
        return row

    def phase(label, count, duration, legacy=False):
        began = time.monotonic()
        for index in range(count):
            if enabled or legacy:
                view = (["calendar", "sound", "network", "bluetooth", "battery", "launcher"][index % 6] if legacy else ["dashboard-" + page for page in page_slots][index % slots])
                offset = len(matrix.log_text(log))
                began_ns = time.monotonic_ns()
                call("open", view)
                state_wait(lambda s: s["view"] == view and s["target_open"] and s["prepared"])
                stamp = matrix.wait_log(log, lambda text: committed(text[offset:]) if legacy else presented(text[offset:], view.removeprefix("dashboard-")))
                time.sleep(.3)
                opened = matrix.idle(client.pid, .2)
                opened.update(view=view, phase=label, index=index, open_to_presented_ms=(stamp-began_ns)/1e6)
                warm.append(opened)
                call("close")
                state_wait(lambda s: not s["resident"] and not s["target_open"] and s["surfaces"] == 0)
                closed.append(capture(f"{label}-close-{index}"))
            else:
                # A never opens a panel; phase endpoints have matched ages.
                time.sleep(4)
        budget = ACCEPTANCE_PHASE_BUDGETS[label] if acceptance else duration
        settled_state = state_wait(lambda s: not s["resident"] and not s["target_open"] and s["surfaces"] == 0) if acceptance else None
        row, timing = finish_phase_window(
            label,
            began,
            budget,
            lambda idle_seconds: capture(label, idle_seconds, settled_state),
            acceptance is not None,
            time.monotonic,
            time.sleep,
            keeper_check,
        )
        row.update(timing)
        if not (acceptance and acceptance["batch_medians"]):
            assert total(row) < 24.5, "settled reaches owner stop line"
        (raw / "dashboard-residency-progress.json").write_text(json.dumps(samples, indent=2) + "\n")
        print("DASHBOARD_RESIDENCY", label, total(row), flush=True)
        return row

    legacy = acceptance["legacy"] if acceptance else os.environ.get("BEAD_DASHBOARD_RESIDENCY_LEGACY", "1") == "1"
    opened_once_only = bool(acceptance and acceptance["opened_once_only"])
    if opened_once_only:
        never = several = combined = None
        once = phase("opened-once", slots, 55)
    else:
        never = phase("never-opened", 0, 35)
        once = phase("opened-once", slots, 55)
        several = phase("opened-several", slots * 2, 75)
        combined = phase("combined-legacy", 6, 65, legacy=True) if legacy else several
    result = {"valid": True, "dashboard_enabled": enabled, "pages": pages, "page_slots": page_slots, "bar": bar,
              "never": never, "once": once, "several": several,
              "combined": combined, "combined_legacy": legacy, "closed": closed, "warm": warm,
              "acceptance": acceptance is not None,
              "batch_medians": bool(acceptance and acceptance["batch_medians"]),
              "opened_once_only": opened_once_only,
              "mode": "acceptance" if acceptance else "legacy",
              "baseline_timing": baseline_timing,
              "method": "same process: never; one pass over fixed page slots; three passes over fixed page slots; acceptance mode uses matched 65/85/105/95s phase budgets and actual >=60s endpoint samples; control opens only legacy panels in final phase" if acceptance else "same process: never; one pass over fixed page slots; three passes over fixed page slots; fixed matched-age endpoints with >=30s idle; control opens only legacy panels in final phase"}
    (raw / "dashboard-residency.json").write_text(json.dumps(result, indent=2) + "\n")
    return result
