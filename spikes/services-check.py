#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Opt-in native service-owner proof on private buses and an absent private PW socket."""
import os
import pathlib
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent


def main():
    with tempfile.TemporaryDirectory(prefix='bead-service-test-') as directory:
        env = os.environ.copy()
        env.update(XDG_RUNTIME_DIR=directory, PIPEWIRE_REMOTE='bead-test-unavailable', BEAD_PRIVATE_SERVICES_TEST='1')
        command = ['dbus-run-session', '--', 'bash', '-c', 'export DBUS_SYSTEM_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS"; exec cargo test -p bead-services --locked -j2 --quiet private_bus_sensor -- --ignored']
        result = subprocess.run(['nice', '-n', '10', *command], cwd=ROOT, env=env, capture_output=True, text=True)
        if result.returncode:
            print('\n'.join((result.stdout + result.stderr).splitlines()[-30:]))
        else:
            print('Private UPower/NM/BlueZ owner lifecycle and subscription proof passed')
        return result.returncode


if __name__ == '__main__':
    raise SystemExit(main())
