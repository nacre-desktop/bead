#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Private dashboard residency decomposition; reuse the nested proof setup."""
import json, os, pathlib, time


def sample(client, raw, bar, matrix, call, state_wait, log, presented, committed, observe, enabled):
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
    def capture(label):
        state = state_wait(lambda s: not s["resident"] and not s["target_open"] and s["surfaces"] == 0)
        row = matrix.idle(client.pid, 1)
        row["label"] = label
        row["panel"] = state
        row["legacy"] = json.loads(call("state", target="nacre"))
        row["external"] = observe()
        assert row["memory"]["swap_pss_mib"] == 0, "UI swap invalidates comparison"
        assert all(x["memory"]["swap_pss_mib"] == 0 for x in row["external"]["processes"]), "daemon swap invalidates comparison"
        assert total(row) <= total(bar) + 5, "closed exceeds own bar+5 MiB"
        assert total(row) <= 25, "diagnostic exceeds current hard ceiling"
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
        elapsed = time.monotonic() - began
        assert elapsed <= duration - 30, "phase leaves less than30s idle"
        time.sleep(duration - elapsed)
        row = capture(label)
        assert total(row) < 24.5, "settled reaches owner stop line"
        row["phase_seconds"] = time.monotonic() - began
        (raw / "dashboard-residency-progress.json").write_text(json.dumps(samples, indent=2) + "\n")
        print("DASHBOARD_RESIDENCY", label, total(row), flush=True)
        return row

    never = phase("never-opened", 0, 35)
    once = phase("opened-once", slots, 55)
    several = phase("opened-several", slots * 2, 75)
    legacy = os.environ.get("BEAD_DASHBOARD_RESIDENCY_LEGACY", "1") == "1"
    combined = phase("combined-legacy", 6, 65, legacy=True) if legacy else several
    result = {"valid": True, "dashboard_enabled": enabled, "pages": pages, "page_slots": page_slots, "bar": bar,
              "never": never, "once": once, "several": several,
              "combined": combined, "combined_legacy": legacy, "closed": closed, "warm": warm, "method": "same process: never; one pass over fixed page slots; three passes over fixed page slots; fixed matched-age endpoints with >=30s idle; control opens only legacy panels in final phase"}
    (raw / "dashboard-residency.json").write_text(json.dumps(result, indent=2) + "\n")
    return result
