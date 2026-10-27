#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Markdown tables from animation_audit.py runs: animation_report.py label=pacing.json ..."""
import json
import statistics
import sys


def med(values):
    values = [v for v in values if v is not None]
    return statistics.median(values) if values else None


def f(v, d=1):
    return '-' if v is None else f'{v:.{d}f}'


runs = {}
for spec in sys.argv[1:]:
    label, _, path = spec.partition('=')
    runs[label] = json.load(open(path))
surfaces = []
for data in runs.values():
    for w in data['windows']:
        if (w['animation']) not in surfaces:
            surfaces.append(w['animation'])
print('| surface/phase | build | slides (mid frames / visible ms) | commits | commit interval p50/p99 ms | dropped frames (60 Hz) | longest gap ms | first ms | CPU ms (top thread) |')
print('|---|---|---|---|---|---|---|---|---|')
for s in surfaces:
    for phase in ('open', 'close'):
        for label, data in runs.items():
            ws = [w for w in data['windows'] if w['animation'] == s and w['phase'] == phase]
            if not ws:
                continue
            mo = [x for x in data.get('motion', {}).get(f'{s}/{phase}', []) if isinstance(x, dict)]
            slide = f"{f(med([x['mid_frames'] for x in mo]), 0)} / {f(med([x['visible_ms'] for x in mo]), 0)}" if mo else '-'
            cpu = f(med([w.get('cpu_ms') for w in ws]), 0)
            tops = [next(iter(w.get('cpu_top', {})), '') for w in ws]
            pi = [w.get('interval_ms') or {} for w in ws]
            print(f"| {s}/{phase} | {label} | {slide} | {f(med([w.get('commits') for w in ws]), 0)} | "
                  f"{f(med([p.get('p50') for p in pi]))}/{f(med([p.get('p99') for p in pi]))} | "
                  f"{f(med([w.get('dropped') for w in ws]), 0)} | "
                  f"{f(med([w.get('longest_gap_ms') for w in ws]), 0)} | {f(med([w.get('first_frame_ms') for w in ws]), 0)} | "
                  f"{cpu} ({tops[0] if tops else ''}) |")
print()
print('| build | idle CPU % | idle Pss_Anon MiB | idle Pss_Shmem MiB | idle DRM MiB | SwapPss KiB | idle top thread ms/15s |')
print('|---|---|---|---|---|---|---|')
for label, data in runs.items():
    m = data.get('idle_mem_kib', {})
    print(f"| {label} | {data.get('idle_cpu_pct')} | {f((m.get('Pss_Anon') or 0) / 1024, 2)} | {f((m.get('Pss_Shmem') or 0) / 1024, 2)} | "
          f"{f((m.get('DRM') or 0) / 1024, 2)} | {m.get('SwapPss')} | {data.get('idle_cpu_top_threads_ms')} |")
