#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
set -euo pipefail
here=$(cd -- "$(dirname -- "$0")" && pwd)
lock=${NACRE_DESKTOP_LOCK:-"$HOME/nacre-lead/desktop-heavy.lock"}
mkdir -p -- "$(dirname -- "$lock")"
exec flock --exclusive "$lock" systemd-run --user --scope --wait \
  --property=MemoryMax=12G --property=MemorySwapMax=0 -- \
  nice -n 10 dbus-run-session -- python3 "$here/acceptance.py" "$@"
