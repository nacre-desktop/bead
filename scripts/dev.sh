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
nice -n 10 cargo build -p nacre-shell --release --locked -j2 --quiet
export PATH="${CARGO_TARGET_DIR:-$bead_root/target}/release:$PATH"
if [[ -z ${WAYLAND_DISPLAY:-} ]]; then
    printf '%s\n' 'Run from a Wayland desktop terminal; this command starts a nested window.' >&2
    exit 1
fi
if [[ ${2:-} == --backend ]]; then
    printf '%s\n' 'Nested workspace/title proof with two fixture windows. Focus a window; click a workspace. Close the nested window or Ctrl+C to stop.'
else
    printf '%s\n' 'Stateful UI in a nested window. Click Search/type/scroll/Home. Close the nested window or Ctrl+C to stop.'
fi
unset NACRE_COMPOSITOR NIRI_SOCKET HYPRLAND_INSTANCE_SIGNATURE DISPLAY
if [[ ${2:-} == --demo ]]; then
    exec python3 "$bead_root/scripts/demo.py" "${1:-niri}"
fi
if [[ ${2:-} == --backend ]]; then
    case "${1:-niri}" in
        niri) exec nice -n 10 niri --config "$bead_root/examples/niri-backend.kdl" ;;
        hyprland) export AQ_DRM_DEVICES=/dev/null; exec nice -n 10 Hyprland --config "$bead_root/examples/hyprland-backend.lua" ;;
        *) printf '%s\n' 'Usage: ./scripts/dev.sh [niri|hyprland] [--demo|--backend]' >&2; exit 1 ;;
    esac
fi
case "${1:-niri}" in
    niri) exec niri --config "$bead_root/examples/niri.kdl" ;;
    hyprland) export AQ_DRM_DEVICES=/dev/null; exec Hyprland --config "$bead_root/examples/hyprland.lua" ;;
    *) printf '%s\n' 'Usage: ./scripts/dev.sh [niri|hyprland]' >&2; exit 1 ;;
esac
