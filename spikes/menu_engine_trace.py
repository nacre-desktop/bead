# SPDX-License-Identifier: GPL-3.0-only
"""Pure validation for one existing serial menu-query typing trace segment."""
import re


def parse_serial_key_trace(segment, expected_catalog_rev, profile_mode):
    """Validate one input-to-presentation segment and return its measured fields."""
    assert profile_mode in ('clocks', '1'), 'unknown typing profile mode'
    assert 'UI_ERROR' not in segment and 'MENU_RESULT ok=false' not in segment

    received = re.findall(r'INPUT_RECEIVED monotonic_ns=(\d+) cpu_ns=(\d+)', segment)
    sent = re.findall(r'MENU_RPC_SENT id=(\d+)', segment)
    returned = re.findall(r'MENU_RPC_RETURN id=(\d+) ok=true catalog_rev=(\d+) count=(\d+)', segment)
    busy = re.findall(r'MENU_RPC_RETURN id=(\d+) ok=false code=busy retry=true', segment)
    assert len(received) == len(returned) == 1 and 1 <= len(sent) <= 11, \
        'missing/extraneous completed menu RPC'
    assert len(set(sent)) == len(sent) and busy == sent[:-1], \
        'only bounded retryable busy replies may precede completion'

    rpc_id, revision, count = map(int, returned[0])
    assert rpc_id == int(sent[-1]) and revision == expected_catalog_rev and count > 0
    typed = re.search(
        r'MENU_RESULT ok=true catalog_rev=' + str(revision) + r' count=' + str(count) + r'\b',
        segment,
    )
    assert typed, 'completed RPC has no matching successful MENU_RESULT'

    commit_pattern = re.compile(r'UI_COMMIT generation=(\d+).*update=(\d+).*source_error=false')
    commits = [
        (row.group(1), row.group(2), typed.end() + row.end())
        for row in commit_pattern.finditer(segment[typed.end():])
    ]
    presentation_pattern = re.compile(
        r'UI_PRESENTED generation=(\d+) monotonic_ns=(\d+) refresh_ns=(\d+) '
        r'companion=(\w+) update=(\d+) cpu_ns=(\d+)'
    )
    presented = list(presentation_pattern.finditer(segment))
    start, cpu = map(int, received[0])
    found = [
        row for row in presented
        if any(row.group(1) == generation and row.group(5) == update and row.start() > commit_end
               for generation, update, commit_end in commits)
    ]
    assert len(found) == 1, 'completed result did not match its exact committed/presented update'
    _generation, presented_ns, refresh, _companion, update, end_cpu = found[0].groups()
    presented_ns, end_cpu = int(presented_ns), int(end_cpu)
    assert end_cpu > cpu > 0 and presented_ns > start > 0, 'invalid input/presentation clocks'

    profiles = [
        {'stage': stage, 'wall_ms': float(wall), 'cpu_ms': float(stage_cpu)}
        for stage, wall, stage_cpu in re.findall(
            r'INPUT_PROFILE stage=(\w+) wall_ms=([\d.]+) cpu_ms=([\d.]+)', segment
        )
    ]
    if profile_mode == 'clocks':
        assert not profiles, 'clock-only capture emitted stage traces'

    return {
        'rpc_id': rpc_id,
        'busy_retries': len(busy),
        'catalog_rev': revision,
        'result_count': count,
        'update': int(update),
        'input_ns': start,
        'presented_ns': presented_ns,
        'latency_ms': (presented_ns - start) / 1e6,
        'ui_cpu_ms': (end_cpu - cpu) / 1e6,
        'refresh_ns': int(refresh) or None,
        'profiles': profiles,
    }
