# SPDX-License-Identifier: GPL-3.0-only
"""CPU clock for a harness-owned live child, not /proc's coarse tick accounting.
See https://man7.org/linux/man-pages/man3/clock_getcpuclockid.3.html
"""
import ctypes
import os
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
        except FileNotFoundError:
            continue
        for pid in map(int, children.split()):
            if pid not in found:
                found.add(pid)
                pending.append(pid)
    return found


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
        except FileNotFoundError:
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
