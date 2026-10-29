# SPDX-License-Identifier: GPL-3.0-only
"""CPU clock for a harness-owned live child, not /proc's coarse tick accounting.
See https://man7.org/linux/man-pages/man3/clock_getcpuclockid.3.html
"""
import ctypes
import os
import signal
import time
from pathlib import Path


def identity(pid):
    fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return int(fields[1]), int(fields[19])  # parent PID and start-time ticks


def descendants(process):
    """Only walk children rooted in the harness-owned live process."""
    if process.poll() is not None:
        raise RuntimeError("owned process exited")
    found, pending = set(), [process.pid]
    while pending:
        parent = pending.pop()
        try:
            children = Path(f"/proc/{parent}/task/{parent}/children").read_text()
        except (FileNotFoundError, ProcessLookupError):
            continue
        for pid in map(int, children.split()):
            if pid not in found:
                found.add(pid)
                pending.append(pid)
    return found


def thread_snapshot(process):
    if process.poll() is not None:
        raise RuntimeError("owned thread-profile child exited")
    result = {}
    for path in Path(f"/proc/{process.pid}/task").glob("*/stat"):
        try:
            line = path.read_text()
        except (FileNotFoundError, ProcessLookupError):
            continue
        left, right = line.index("("), line.rindex(")")
        fields = line[right + 2:].split()
        result[int(path.parent.name)] = {"name": line[left + 1:right],
            "start_ticks": int(fields[19]), "cpu_ticks": int(fields[11]) + int(fields[12])}
    return result


def thread_delta(before, after, keys):
    quantum_ms = 1000 / os.sysconf("SC_CLK_TCK")
    lost = [pid for pid, value in before.items()
            if pid not in after or after[pid]["start_ticks"] != value["start_ticks"]]
    rows = []
    for pid, value in after.items():
        previous = before.get(pid)
        same = previous is not None and previous["start_ticks"] == value["start_ticks"]
        ticks = value["cpu_ticks"] - (previous["cpu_ticks"] if same else 0)
        if ticks < 0:
            raise RuntimeError("thread CPU clock went backwards")
        rows.append({"tid": pid, "name": value["name"], "new": not same,
                     "cpu_ms_per_key_full_trial": ticks * quantum_ms / keys})
    return {"complete": not lost, "exited_or_reused": lost, "tick_ms": quantum_ms,
            "scope": "Whole scripted trial including gaps and keyboard management; not exact input-to-present CPU",
            "threads": rows,
            "sum_cpu_ms_per_key_full_trial": sum(row["cpu_ms_per_key_full_trial"] for row in rows) if not lost else None}


class OwnedDescendant:
    def __init__(self, process, pid):
        if pid not in descendants(process):
            raise RuntimeError("CPU-clock PID is not an owned descendant")
        self.owner, self.pid = process, pid
        self.start = identity(pid)[1]

    def poll(self):
        if self.owner.poll() is not None:
            return 1
        try:
            return None if identity(self.pid)[1] == self.start else 1
        except (FileNotFoundError, ProcessLookupError):
            return 1

class OwnedCpuClock:
    def __init__(self, process):
        self.process = process
        if process.poll() is not None:
            raise RuntimeError("owned CPU-clock child already exited")
        libc = ctypes.CDLL(None)
        getclock = libc.clock_getcpuclockid
        getclock.argtypes = [ctypes.c_int, ctypes.POINTER(ctypes.c_int)]
        getclock.restype = ctypes.c_int
        clock = ctypes.c_int()
        error = getclock(process.pid, ctypes.byref(clock))
        if error:
            raise OSError(error, os.strerror(error))
        self.clock = clock.value
        self.resolution_ns = round(time.clock_getres(self.clock) * 1_000_000_000)

    def sample_ns(self):
        if self.process.poll() is not None:
            raise RuntimeError("owned CPU-clock child exited during measurement")
        return time.clock_gettime_ns(self.clock)


def stop_owned_tree(daemon, stop):
    if daemon is None:
        return
    owned = {}
    if daemon.poll() is None:
        for pid in descendants(daemon):
            try:
                owned[pid] = identity(pid)[1]
            except (FileNotFoundError, ProcessLookupError):
                pass  # It exited between enumeration and identity read.
    stop(daemon)
    def remaining():
        live = []
        for pid, start in owned.items():
            try:
                if identity(pid)[1] == start:
                    # Zombies have already exited and consume no execution/memory.
                    state = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[0]
                    if state != 'Z': live.append(pid)
            except (FileNotFoundError, ProcessLookupError):
                pass
        return live
    deadline = time.monotonic() + 5
    while remaining() and time.monotonic() < deadline:
        time.sleep(.05)
    for pid in remaining():
        try:
            if os.getpgid(pid) == pid:
                os.killpg(pid, signal.SIGKILL)
            else:
                os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    deadline = time.monotonic() + 2
    while remaining() and time.monotonic() < deadline:
        time.sleep(.05)
    assert not remaining(), 'owned engine workers left running'

