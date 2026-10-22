# SPDX-License-Identifier: GPL-3.0-only
"""Pure validation for one existing serial menu-query typing trace segment."""
import json
import re


def parse_serial_key_trace(segment, expected_catalog_rev, profile_mode, expected_query=None):
    """Validate one serial key; RPC RTT includes client write/dispatch time.

    Identity mode requires exactly one successful RPC and one accepted projection;
    it deliberately refuses overlapping/burst traces rather than guessing joins.
    """
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

    identity = {}
    if expected_query is not None:
        def record(marker):
            matches = re.findall(marker + r' json=(.+)', segment)
            assert len(matches) == 1, 'missing/duplicate query identity'
            return json.loads(matches[0])

        request = record(r'MENU_RPC_QUERY id=' + str(rpc_id))
        reply = record(r'MENU_RPC_IDENTITY id=' + str(rpc_id))
        projected = record(r'MENU_RESULT_IDENTITY')
        assert request['query'] == reply['query'] == projected['query'] == expected_query, \
            'query identity mismatch'
        order = reply['order']
        assert isinstance(order, list) and len(order) == count and \
            all(isinstance(item, str) for item in order) and len(set(order)) == count, \
            'invalid ordered item identity'
        assert projected['order'] == order, 'projected item identity mismatch'
        clocks = {}
        for phase in ('send', 'return'):
            values = re.findall(r'MENU_RPC_CLOCK id=' + str(rpc_id) +
                                ' phase=' + phase + r' monotonic_ns=(\d+)', segment)
            assert len(values) == 1, 'missing/duplicate RPC clock'
            clocks[phase] = int(values[0])
        assert int(received[0][0]) <= clocks['send'] < clocks['return'], 'invalid RPC clocks'
        identity = {'query': expected_query, 'item_ids': order,
                    'rpc_rtt_ms': (clocks['return'] - clocks['send']) / 1e6}

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
    if expected_query is not None:
        assert clocks['return'] <= presented_ns, 'RPC returned after presentation'

    profiles = [
        {'stage': stage, 'wall_ms': float(wall), 'cpu_ms': float(stage_cpu)}
        for stage, wall, stage_cpu in re.findall(
            r'INPUT_PROFILE stage=(\w+) wall_ms=([\d.]+) cpu_ms=([\d.]+)', segment
        )
    ]
    if profile_mode == 'clocks':
        assert not profiles, 'clock-only capture emitted stage traces'

    return {
        **identity,
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
