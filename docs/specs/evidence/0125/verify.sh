#!/bin/bash
set -euxo pipefail
cd /w/src
export CARGO_HOME=/w/cargo CARGO_TARGET_DIR=/w/target RUNNER_TEMP=/w/tmp-verified BEAD_TEST_ARTIFACTS=/w/artifacts RUST_TEST_THREADS=2
mkdir -p "$RUNNER_TEMP"
nice -n 10 bash -euxo pipefail /w/check.sh
echo CI_OK
