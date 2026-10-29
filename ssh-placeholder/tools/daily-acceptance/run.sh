#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
set -euo pipefail
here=$(cd -- "$(dirname -- "$0")" && pwd)
lock=${NACRE_DESKTOP_LOCK:-"$HOME/nacre-lead/desktop-heavy.lock"}
mkdir -p -- "$(dirname -- "$lock")"
exec flock --exclusive "$lock" nice -n 10 python3 "$here/guarded.py" "$@"
