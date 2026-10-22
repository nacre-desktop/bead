#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Private fake N0031 daemon for the B0022 nested proof.

Advertises the bar action methods (or none with --no-methods), serves a two-row
`tray` topic, logs one JSON line per call to LOG and answers {"changed": true}.
Not a Nacre daemon: no compositor, no authority, private socket only.
"""
import json
import os
import socket
import sys
import threading

METHODS = ["workspace.activate", "workspace.scroll", "audio.adjust_volume", "audio.toggle_mute",
           "brightness.adjust", "tray.activate", "tray.secondary_activate", "tray.context_menu",
           "tray.scroll", "session.request"]
MENU = {"items": {}, "order": []}
TRAY = {"health": {"available": True, "stale": False}, "items": {"items": {
    "sni:one": {"title": "One", "visible": True, "item_is_menu": False, "attention": False,
                "icon": "application-x-executable", "menu": MENU},
    "sni:two": {"title": "Two", "visible": True, "item_is_menu": True, "attention": False,
                "icon": "application-x-executable", "menu": MENU}},
    "order": ["sni:one", "sni:two"]}}


def serve(path, log, methods, boot):
    if os.path.exists(path):
        os.unlink(path)
    listener = socket.socket(socket.AF_UNIX)
    listener.bind(path)
    os.chmod(path, 0o600)
    listener.listen(8)
    lock = threading.Lock()

    def record(entry):
        with lock, open(log, 'a') as stream:
            stream.write(json.dumps(entry, sort_keys=True) + '\n')

    def handle(conn):
        reader = conn.makefile('rb')
        send = lambda value: conn.sendall((json.dumps(value) + '\n').encode())
        hello = json.loads(reader.readline() or 'null')
        if not hello or hello.get('t') != 'hello':
            return
        record({'event': 'hello', 'client': hello.get('client')})
        send({'t': 'welcome', 'proto': 1, 'min': 1, 'boot': boot,
              'topics': {'tray': 1},
              'methods': {name: {'v': 1, 'class': 'desktop', 'timeout': 10000,
                                 'params': {'type': 'object'}, 'result': {}} for name in methods}})
        for line in reader:
            message = json.loads(line)
            if message.get('t') == 'sub' and 'tray' in message.get('topics', []):
                send({'t': 'snap', 'topic': 'tray', 'seq': 0, 'data': TRAY})
            elif message.get('t') == 'call':
                record({'event': 'call', 'm': message['m'], 'p': message.get('p')})
                send({'t': 'ret', 'id': message['id'], 'ok': {'changed': True}})

    while True:
        conn, _ = listener.accept()
        threading.Thread(target=lambda c=conn: (handle(c), c.close()), daemon=True).start()


if __name__ == '__main__':
    path, log, boot = sys.argv[1:4]
    serve(path, log, [] if '--no-methods' in sys.argv else METHODS, boot)
