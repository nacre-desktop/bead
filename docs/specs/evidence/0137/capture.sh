#!/bin/bash
# Caller supplies private fixture/source/output paths; run on the capture host.
set -euo pipefail
flock "$CAPTURE_LOCK" python3 spikes/edge-panel-hover.py \
  --nacre "$NACRE_SOURCE" --bead-bin "$BEAD_BIN" --bead-shell "$BEAD_SHELL" \
  --out "$CAPTURE_OUT" --arms "${ARMS:-bead}" --states idle --cpu \
  --wallpaper-fixture --wallpapers 1 --presentation "$CAPTURE_OUT/absent.json" \
  --pointer-tool "$POINTER_TOOL" --parent-output-tool "$OUTPUT_TOOL"
# Bead: default 20 trials per panel. Reference: BEAD_HOVER_TRIALS=1 ARMS=qml.
