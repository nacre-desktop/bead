#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
nice -n 10 python3 swap-preflight.py
nice -n 10 python3 b0108-capture.py \
  --nacre "$PWD/nacre" --bead-bin "${B0108_BIN:-$PWD/bin}" \
  --bead-shell "${B0108_SHELL:-$PWD/candidate/shell}" \
  --out "$PWD/evidence/${B0108_PHASE:-parity}" \
  --arms "${B0108_ARMS:-qml,bead}" \
  --states "${B0108_STATES:-idle,bar-title-hover,bar-title-open,tabs-0,tabs-1,tabs-2,tabs-3,tabs-hover,tabs-pressed}" \
  --cpu --size 2880x1800 --scale 1.5 --seq-scale 1.5 --seq-interval 25 --closed-settle 2.5 \
  --pointer-tool "$PWD/tools/virtual-pointer" --parent-output-tool "$PWD/tools/wlr-randr"
