#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/.research/qml-probe"
mkdir -p "$out"
xml="$(pkg-config --variable=pkgdatadir wayland-protocols)/stable/presentation-time/presentation-time.xml"
wayland-scanner client-header "$xml" "$out/presentation-time-client-protocol.h"
wayland-scanner private-code "$xml" "$out/presentation-time-protocol.c"
cc -fPIC $(pkg-config --cflags wayland-client) -c "$out/presentation-time-protocol.c" -o "$out/protocol.o"
version="$(pkg-config --modversion Qt6Gui)"
include="$(pkg-config --variable=includedir Qt6Gui)"
/usr/lib/qt6/moc "$root/spikes/qml-presentation.cpp" -o "$out/qml-presentation.moc"
c++ -std=c++17 -shared -fPIC "$root/spikes/qml-presentation.cpp" "$out/protocol.o" -I"$out" -I"$include/QtGui/$version/QtGui" $(pkg-config --cflags --libs Qt6Quick wayland-client) -o "$out/probe.so"
printf '%s\n' "$out/probe.so"
