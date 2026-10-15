#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Paired cache baseline/candidate in the same existing private compositor."""
import importlib.util
import json
import os
import pathlib
import statistics

ROOT = pathlib.Path(__file__).resolve().parent.parent

def sample(env, raw, compositor, stop):
    spec = importlib.util.spec_from_file_location('panel_proof', ROOT/'spikes/panel-proof.py')
    panel = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(panel)
    baseline = os.environ['BEAD_CACHE_BASELINE']
    order = ['baseline', 'candidate'] if compositor == 'hyprland' else ['candidate', 'baseline']
    saved = {k: os.environ.get(k) for k in ['BEAD_NATIVE_BINARY', 'BEAD_CACHE_REPORT']}
    pairs = int(os.environ.get('BEAD_CACHE_PAIRS', '1'))
    if not 1 <= pairs <= 5:
        raise ValueError('cache comparison requires 1..5 pairs')
    results = {'version':2, 'complete':False, 'runs':{'baseline':[], 'candidate':[]}}
    def checkpoint():
        pending = raw/'cache-pair.writing'
        pending.write_text(json.dumps(results, indent=2)+'\n')
        pending.replace(raw/'cache-pair.json')
    try:
        for index, variant in enumerate(order * pairs):
            directory = raw/(variant if pairs == 1 else f"{index+1:02d}-{variant}")
            directory.mkdir()
            os.environ['BEAD_NATIVE_BINARY'] = baseline if variant == 'baseline' else str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell')
            # Keep diagnostic/timer options identical during the PSS comparison.
            os.environ.pop('BEAD_CACHE_REPORT', None)
            # Share compositor/bus only, never persisted panel-open state.
            case_env = dict(env)
            for key, suffix in [('XDG_STATE_HOME','state'), ('XDG_CACHE_HOME','cache')]:
                path = directory/suffix
                path.mkdir(mode=0o700)
                case_env[key] = str(path)
            pointer_env = {**case_env, 'BEAD_PRIVATE_INPUT_PROOF':'1'}
            pointer = panel.matrix.services.Probe([str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/examples/private-pointer')], pointer_env, directory/'neutral-pointer.log')
            try:
                pointer.wait('POINTER_READY')
                pointer.command('move 1000 600', 'POINTER_DONE')
            finally:
                stop(pointer.process)
                pointer.reader.join(timeout=2)
            panel.sample(case_env, directory, compositor, stop)
            results['runs'][variant].append(json.loads((directory/'panel-proof.json').read_text()))
            checkpoint()
            print(f'CACHE_CASE compositor={compositor} index={index+1} variant={variant}', flush=True)
    finally:
        for key, value in saved.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value
    metrics = {
        'bar': lambda d: d['bar']['total_mib'],
        'first_warm': lambda d: d['cycles'][0]['warm']['total_mib'],
        'third_warm': lambda d: d['cycles'][-1]['warm']['total_mib'],
        'closed': lambda d: d['closed_idle']['pss_plus_drm_mib'],
    }
    results['summary'] = {}
    for name, extract in metrics.items():
        summary = {}
        for variant, runs in results['runs'].items():
            values = [extract(run) for run in runs]
            summary[variant] = {'median':statistics.median(values), 'min':min(values), 'max':max(values)}
        summary['median_delta'] = summary['candidate']['median'] - summary['baseline']['median']
        summary['within_group_spread'] = max(v['max']-v['min'] for v in [summary['baseline'],summary['candidate']])
        summary['delta_within_spread'] = abs(summary['median_delta']) <= summary['within_group_spread']
        results['summary'][name] = summary
    results['complete'] = True
    checkpoint()
    print('CACHE_PAIR compositor='+compositor+' complete=true pairs='+str(pairs), flush=True)
