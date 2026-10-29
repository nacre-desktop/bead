#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
set -euo pipefail
here=$(cd -- "$(dirname -- "$0")" && pwd)
out=${1:?usage: build-pointer.sh PRIVATE_OUTPUT_DIRECTORY}
mkdir -p -- "$out"
wayland-scanner client-header "$here/wlr-virtual-pointer-unstable-v1.xml" \
  "$out/wlr-virtual-pointer-unstable-v1-client-protocol.h"
wayland-scanner private-code "$here/wlr-virtual-pointer-unstable-v1.xml" \
  "$out/wlr-virtual-pointer-unstable-v1-protocol.c"
cc -O2 -Wall -Wextra -Werror -I"$out" $(pkg-config --cflags wayland-client) \
  "$here/pointer.c" "$out/wlr-virtual-pointer-unstable-v1-protocol.c" \
  $(pkg-config --libs wayland-client) -o "$out/daily-acceptance-pointer"
