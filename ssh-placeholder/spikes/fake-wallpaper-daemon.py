#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Private fake daemon for the B0046 proof: `wallpaper.active` v1 (with the additive `motion`
member), `wallpaper.activate`, and `settings.store.wallpaper-picker`. Not a Nacre daemon.

Usage: fake-wallpaper-daemon.py SOCKET CONTROL.json OUTPUT POSTER [CLIP]
CONTROL.json (re-read when it changes): {"motion": bool, "picker": {...store data...}}
"""
import json, os, socket, sys, threading, time

sock_path, control, output, poster = sys.argv[1:5]
clip = sys.argv[5] if len(sys.argv) > 5 else None
COLOURS = {"background": "#101820", "onSurface": "#f0f4f8", "primary": "#22aa88"}


def identity(path):
    st = os.stat(path)
    return {"device": str(st.st_dev), "inode": str(st.st_ino), "size": st.st_size,
            "mtime_sec": st.st_mtime_ns // 10**9, "mtime_nsec": st.st_mtime_ns % 10**9,
            "ctime_sec": st.st_ctime_ns // 10**9, "ctime_nsec": st.st_ctime_ns % 10**9}


def pair(motion):
    value = {"revision": 1, "palette_revision": 1, "path": poster, "file_identity": identity(poster),
             "presentation": "crop", "palette": {"mode": "dark", "colours": COLOURS}}
    if motion and clip:
        value["motion"] = {"kind": "video", "path": clip, "file_identity": identity(clip)}
    return value


def active(motion):
    current = pair(motion)
    return {"available": True, "stale": False, "revision": 1, "current": current, "outputs": {output: current}}


def picker(data):
    return {"store": "wallpaper-picker", "rev": 1, "exists": True, "data": data, "dirty": False}


def read_control():
    try:
        return json.load(open(control))
    except (OSError, ValueError):
        return {"motion": False, "picker": {}}


clients, lock = [], threading.Lock()
seqs = {"wallpaper.active": 0, "settings.store.wallpaper-picker": 0}


def send(conn, value):
    conn.sendall((json.dumps(value) + "\n").encode())


def handle(conn):
    reader = conn.makefile("rb")
    hello = json.loads(reader.readline() or "null")
    if not hello or hello.get("t") != "hello":
        return
    send(conn, {"t": "welcome", "proto": 1, "min": 1, "boot": "00000000-0000-4000-8000-000000000046",
                "topics": {"wallpaper.active": 1, "settings.store.wallpaper-picker": 1},
                "methods": {"wallpaper.activate": {"v": 1, "class": "desktop", "timeout": 1000,
                                                   "params": {"type": "object"}, "result": {}}}})
    for line in reader:
        message = json.loads(line)
        if message.get("t") == "sub":
            state = read_control()
            with lock:
                for topic in message.get("topics", []):
                    data = active(state.get("motion")) if topic == "wallpaper.active" else picker(state.get("picker", {}))
                    send(conn, {"t": "snap", "topic": topic, "seq": seqs[topic], "data": data})
                clients.append(conn)
        elif message.get("t") == "call":
            send(conn, {"t": "ret", "id": message["id"], "ok": pair(read_control().get("motion"))})


def watch():
    last = None
    while True:
        time.sleep(0.1)
        state = read_control()
        if state == last:
            continue
        first, last = last is None, state
        if first:
            continue
        with lock:
            for topic, data in (("wallpaper.active", active(state.get("motion"))),
                                ("settings.store.wallpaper-picker", picker(state.get("picker", {})))):
                base = seqs[topic]
                seqs[topic] += 1
                # Merge patch: replace the whole document (current is rebuilt, motion nulled when off).
                if topic == "wallpaper.active" and not state.get("motion"):
                    data = {"current": {"motion": None}}
                for conn in list(clients):
                    try:
                        send(conn, {"t": "patch", "topic": topic, "base": base, "seq": seqs[topic], "data": data})
                    except OSError:
                        clients.remove(conn)


if os.path.exists(sock_path):
    os.unlink(sock_path)
listener = socket.socket(socket.AF_UNIX)
listener.bind(sock_path)
os.chmod(sock_path, 0o600)
listener.listen(8)
threading.Thread(target=watch, daemon=True).start()
while True:
    conn, _ = listener.accept()
    threading.Thread(target=lambda c=conn: (handle(c), c.close()), daemon=True).start()
