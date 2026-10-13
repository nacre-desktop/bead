#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
set -euo pipefail
bead_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$bead_root"
export CARGO_HOME="$bead_root/.tools/cargo"
export RUSTUP_HOME="$bead_root/.tools/rustup"
export PATH="$CARGO_HOME/bin:$PATH"
if [[ ! -x "$CARGO_HOME/bin/cargo" || ! -x .tools/native/usr/bin/labwc ]]; then
  printf '%s\n' 'Isolated research tools are missing. See docs/research/baseline-summary.md prerequisites.' >&2
  exit 1
fi
mkdir -p spikes/results/raw
cargo build --release --locked --quiet
printf '%s\n' 'Measuring blank surfaces in offscreen nested niri. Results: spikes/results/baseline.json'
python spikes/measure.py "$@"
