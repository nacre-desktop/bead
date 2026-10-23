#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0059 Welcome app: private nested proof (headless labwc > niri > shell), CPU renderer, fake daemon.

  b0059-welcome.py smoke BIN [N]    open / deep-link / close the Welcome app, cold-open times over N runs

Reuses the B0052 arm (SMOKE_ROOT holds the binary and src/shell; private HOME, runtime dirs and buses only).
Checks: no app before the first open, one window with app id `nacre-welcome`, a second open deep-links the
same process, the app is its own daemon client, a window-initiated close (the path Esc, Skip tour and
Start using Nacre take) and a compositor close both end the process, and the shell survives."""
import importlib.util, json, os, pathlib, socket, statistics, sys, time

HERE = pathlib.Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("b0052", HERE / "b0052-app.py")
b0052 = importlib.util.module_from_spec(spec)
spec.loader.exec_module(b0052)


def welcome_windows(arm):
    return [w for w in arm.jlist("windows") if w.get("app_id") == "nacre-welcome"]


def close_from_inside(arm):
    """`bead.window close` on the app's own socket: the call the window's own close request makes."""
    c = socket.socket(socket.AF_UNIX)
    c.settimeout(5)
    c.connect(str(b0052.RUN / "app-welcome.sock"))
    c.sendall((json.dumps({"version": 1, "id": 1, "target": "bead.window", "method": "close", "args": []}) + "\n").encode())
    c.makefile("rb").readline()
    c.close()


def smoke(binary, runs):
    b0052.OUT.mkdir(exist_ok=True)
    arm = b0052.Arm("candidate", binary)
    arm.start("welcome")
    try:
        steps, times = {}, []
        steps["no_app_before"] = not arm.app_pids("welcome")
        for n in range(runs):
            times.append(arm.open_ms(["welcomeApp", "open", "apps"], lambda: welcome_windows(arm)))
            if n == 0:
                time.sleep(.8)
                arm.shot("welcome-apps.png")
                pid = arm.app_pids("welcome")
                arm.ipc("welcomeApp", "open", "help")
                time.sleep(.5)
                steps["second_open_same_process"] = len(pid) == 1 and arm.app_pids("welcome") == pid and len(welcome_windows(arm)) == 1
                arm.shot("welcome-help.png")
                steps["welcome_client_hello"] = "nacre-app-welcome" in arm.daemon_log.read_text()
                close_from_inside(arm)
                arm.wait(lambda: not arm.app_pids("welcome"), 8, "welcome gone after the window closed itself")
                steps["process_gone_after_window_close"] = True
                steps["no_window_left"] = not welcome_windows(arm)
            else:
                for w in welcome_windows(arm):
                    arm.niri("action", "close-window", "--id", str(w["id"]))
                arm.wait(lambda: not arm.app_pids("welcome"), 8, "welcome gone after the compositor closed it")
        steps["process_gone_after_compositor_close"] = runs > 1 and not arm.app_pids("welcome")
        steps["shell_survives"] = arm.shell.poll() is None
        rep = {"renderer": "cpu (tiny-skia), fake daemon", "steps": steps,
               "cold_open_ms": {"n": len(times), "p50": round(statistics.median(times), 1), "max": round(max(times), 1),
                                "all": [round(x) for x in times]},
               "method": "ipc client start -> first niri poll that sees the window (poll cost about 5-10 ms included)"}
        (arm.raw / "result.json").write_text(json.dumps(rep, indent=1))
        print(json.dumps(rep, indent=1))
    finally:
        arm.finish()


if __name__ == "__main__":
    if len(sys.argv) < 3 or sys.argv[1] != "smoke":
        sys.exit(__doc__)
    smoke(sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 6)
