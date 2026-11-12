#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Run opt-in native service-owner proofs on private buses and sockets."""

import os
import pathlib
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="bead-service-test-") as directory:
        env = os.environ.copy()
        env.update(
            XDG_RUNTIME_DIR=directory,
            PIPEWIRE_REMOTE="bead-test-unavailable",
            BEAD_PRIVATE_SERVICES_TEST="1",
        )
        command = [
            "dbus-run-session",
            "--",
            "bash",
            "-c",
            'export DBUS_SYSTEM_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS"; cargo test -p bead-services --locked -j2 --quiet private_bus_sensor -- --ignored && cargo test -p bead-services --test tray_watcher_private_bus --locked -j2 -- --ignored',
        ]
        result = subprocess.run(
            ["nice", "-n", "10", *command],
            cwd=ROOT,
            env=env,
            capture_output=True,
            text=True,
        )
        if result.returncode:
            print("\n".join((result.stdout + result.stderr).splitlines()[-30:]))
        else:
            print("Private UPower/NM/BlueZ owner lifecycle and subscription proof passed")
        return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
