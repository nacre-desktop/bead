#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
set -euo pipefail
bead_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$bead_root"
# Prefer normal installed tools; use the already prepared checkout's tools if needed.
if ! command -v cargo >/dev/null && [[ -x .tools/cargo/bin/cargo ]]; then
    export CARGO_HOME="$bead_root/.tools/cargo" RUSTUP_HOME="$bead_root/.tools/rustup"
    export PATH="$CARGO_HOME/bin:$PATH"
fi
cargo build -p nacre-shell --release --locked --quiet
export PATH="${CARGO_TARGET_DIR:-$bead_root/target}/release:$PATH"
if [[ -z ${WAYLAND_DISPLAY:-} ]]; then
    printf '%s\n' 'Run from a Wayland desktop terminal; this command starts a nested window.' >&2
    exit 1
fi
printf '%s\n' 'A blank dark strip appears at the top of the nested desktop. Close the window or press Ctrl+C to stop.'
unset NACRE_COMPOSITOR NIRI_SOCKET HYPRLAND_INSTANCE_SIGNATURE DISPLAY
case "${1:-niri}" in
    niri) exec niri --config "$bead_root/examples/niri.kdl" ;;
    hyprland) export AQ_DRM_DEVICES=/dev/null; exec Hyprland --config "$bead_root/examples/hyprland.lua" ;;
    *) printf '%s\n' 'Usage: ./scripts/dev.sh [niri|hyprland]' >&2; exit 1 ;;
esac
