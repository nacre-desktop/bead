#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Paired cache baseline/candidate in the same existing private compositor."""
import importlib.util
import json
import os
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent

def sample(env, raw, compositor, stop):
    spec = importlib.util.spec_from_file_location('panel_proof', ROOT/'spikes/panel-proof.py')
    panel = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(panel)
    baseline = os.environ['BEAD_CACHE_BASELINE']
    order = ['baseline', 'candidate'] if compositor == 'hyprland' else ['candidate', 'baseline']
    saved = {k: os.environ.get(k) for k in ['BEAD_NATIVE_BINARY', 'BEAD_CACHE_REPORT']}
    results = {}
    try:
        for variant in order:
            directory = raw/variant
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
            results[variant] = json.loads((directory/'panel-proof.json').read_text())
    finally:
        for key, value in saved.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value
    (raw/'cache-pair.json').write_text(json.dumps(results, indent=2)+'\n')
    print('CACHE_PAIR compositor='+compositor+' complete=true', flush=True)
