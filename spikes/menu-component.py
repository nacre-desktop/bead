#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Persistent-link component diagnostic: no keyboard/UI or fresh CLI per call."""
import hashlib
import json
import socket
import time


def sample(endpoint, revision, daemon_clock, workers, output, compositor, helper):
    def receive(stream):
        line = stream.readline(1048578)
        assert line.endswith(b'\n') and len(line) <= 1048577, 'invalid/bounded frame'
        return json.loads(line), len(line) - 1

    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as link:
        link.settimeout(10)
        link.connect(str(endpoint))  # endpoint belongs to the existing private lab
        with link.makefile('rb') as stream:
            link.sendall(b'{"t":"hello","proto":1,"min":1,"client":"bead-component"}\n')
            welcome, _ = receive(stream)
            assert welcome['t'] == 'welcome' and 'menu.query' in welcome['methods']

            def call(index):
                query = 'a' if index % 2 == 0 else 'ap'
                message = dict(t='call', id=index, m='menu.query',
                               p=dict(query=query, mode='apps', route='apps'), timeout=10000)
                started = time.perf_counter_ns()
                link.sendall(json.dumps(message).encode() + b'\n')
                reply, wire_bytes = receive(stream)
                elapsed = (time.perf_counter_ns() - started) / 1e6
                assert reply['t'] == 'ret' and reply['id'] == index and 'err' not in reply
                result = reply['ok']
                assert result['query'] == query and result['catalog_rev'] == revision
                assert len(result['order']) == 101 and set(result['order']) == set(result['items'])
                return elapsed, wire_bytes

            for index in range(100):
                call(index)
            before_workers = workers()
            before_daemon = daemon_clock.sample_ns()
            rows = [call(index) for index in range(100, 2100)]
            after_daemon = daemon_clock.sample_ns()
            after_workers = workers()
            assert set(before_workers) == set(after_workers), 'worker population changed'
    elapsed = sorted(row[0] for row in rows)
    result = dict(compositor=compositor, calls=len(rows), warmups=100, catalog=320,
                  results=101, method='persistent link; one outstanding RPC; whole-process CPU; JSON decode in RTT only; nested',
                  daemon_cpu_ms_per_query=(after_daemon-before_daemon)/1e6/len(rows),
                  provider_cpu_ms_per_query=sum(after_workers[p]-before_workers[p] for p in before_workers)/1e6/len(rows),
                  rtt_p50_ms=elapsed[len(rows)//2], rtt_p99_ms=elapsed[int(len(rows)*.99)-1],
                  reply_bytes_min=min(r[1] for r in rows), reply_bytes_max=max(r[1] for r in rows),
                  helper_sha256=hashlib.sha256(helper.read_bytes()).hexdigest(), valid=True)
    output.write_text(json.dumps(result, indent=2)+'\n')
    print('MENU_COMPONENT_PASS calls=2000 results=101', flush=True)
