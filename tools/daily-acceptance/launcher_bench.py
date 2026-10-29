# SPDX-License-Identifier: GPL-3.0-only
"""B0187 launcher typing and scroll measurement over shell.log slices (BEAD_PROFILE_INPUT=1, BEAD_FRAME_TRACE=1)."""
from __future__ import annotations

import re
import statistics

COMMIT = re.compile(r"UI_COMMIT generation=(\d+) .*?monotonic_ns=(\d+) update=(\d+) ")
PRESENTED = re.compile(r"UI_PRESENTED generation=\d+ monotonic_ns=(\d+) refresh_ns=\d+ companion=(\w+) update=(\d+) ")
RECEIVED = re.compile(r"INPUT_RECEIVED monotonic_ns=(\d+)")
STAGE = re.compile(r"INPUT_PROFILE stage=(\w+) wall_ms=([\d.]+) cpu_ms=([\d.]+)")
FRAME = re.compile(r"UI_FRAME monotonic_ns=(\d+) render_ms=([\d.]+)")


def percentiles(values):
    if not values:
        return {"n": 0}
    ordered = sorted(values)
    pick = lambda q: ordered[min(len(ordered) - 1, int(q * len(ordered)))]
    return {"n": len(ordered), "p50": round(statistics.median(ordered), 3), "p95": round(pick(.95), 3),
            "max": round(ordered[-1], 3)}


def typing_key(segment: str):
    """One key's slice: input receipt -> first presented frame (text) and -> frame presenting the result commit."""
    received = RECEIVED.search(segment)
    if not received:
        return None
    start = int(received[1])
    presented = [(int(m[1]), m[3]) for m in PRESENTED.finditer(segment) if int(m[1]) >= start]
    if not presented:
        return None
    result = re.search(r"MENU_RESULT ok=true", segment)
    commits = [(m.start(), m[3]) for m in COMMIT.finditer(segment)]
    result_update = next((update for at, update in commits if result and at > result.start()), None)
    final = next((ns for ns, update in presented if update == result_update), None)
    stages = {}
    for name, wall, _cpu in STAGE.findall(segment[received.start():]):
        stages[name] = stages.get(name, 0.) + float(wall)
    return {"text_ms": (presented[0][0] - start) / 1e6,
            "result_ms": None if final is None else (final - start) / 1e6, "stages": stages}


WORKER = re.compile(r"INPUT_PROFILE stage=worker wall_ms=([\d.]+)")


def frames(segment: str):
    """Rendered frames of a scroll gesture. Frame cost = UI worker build (`worker` stage) + draw (`render_ms`)
    attributed to the frame, i.e. the main-thread work one presented frame needed."""
    frames_out, cursor = [], 0
    for match in FRAME.finditer(segment):
        worker = sum(float(w) for w in WORKER.findall(segment[cursor:match.start()]))
        cursor = match.end()
        frames_out.append((int(match[1]), float(match[2]), worker))
    spacing = [(b[0] - a[0]) / 1e6 for a, b in zip(frames_out, frames_out[1:])]
    return {"frames": len(frames_out), "render_ms": percentiles([f[1] for f in frames_out]),
            "worker_ms": percentiles([f[2] for f in frames_out]), "spacing_ms": percentiles(spacing),
            "main_ms_per_frame": percentiles([f[1] + f[2] for f in frames_out])}
