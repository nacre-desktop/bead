#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Run the real `releases.py ui-shell` and SIGKILL (or SIGSTOP) it right before/after one journaled phase.

usage: crash_switch.py OPERATION PHASE before|after kill|stop [ui-shell args...]
"""

import os
import signal
import sys
from pathlib import Path

SRC = Path.home() / "src/nacre-src"
sys.path[:0] = [str(SRC / "tools"), str(SRC / "nacre/shell-tools")]
import f4_switch as f4  # noqa: E402
import releases  # noqa: E402

operation, phase, when, how, *rest = sys.argv[1:]
original = f4.Switch.phase


def hooked(self, record, name, action):
    def act():
        hit = name == phase and record["operation"] == operation
        if hit and when == "before":
            os.kill(os.getpid(), signal.SIGKILL if how == "kill" else signal.SIGSTOP)
        action()
        if hit and when == "after":
            os.kill(os.getpid(), signal.SIGKILL if how == "kill" else signal.SIGSTOP)

    return original(self, record, name, act)


f4.Switch.phase = hooked
sys.argv = ["releases.py", "ui-shell", operation, *rest]
releases.main()
