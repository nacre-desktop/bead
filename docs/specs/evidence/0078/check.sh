#!/bin/bash
set -euo pipefail
cd /w/src
export CARGO_HOME=/w/cargo CARGO_TARGET_DIR=/w/target RUNNER_TEMP=/w/tmp BEAD_TEST_ARTIFACTS=/w/artifacts
nice -n 10 cargo test -p nacre-shell --locked -j2 weather_ -- --nocapture
nice -n 10 cargo fmt --all --check
nice -n 10 cargo clippy --workspace --all-targets --locked -j2 -- -D warnings
nice -n 10 python3 tools/check-service-boundary.py
nice -n 10 cargo test --workspace --locked -j2
nice -n 10 cargo test -p nacre-shell --features trace --locked -j2 memory_policy::tests::
nice -n 10 cargo check -p nacre-shell --no-default-features --locked -j2
nice -n 10 cargo check --workspace --all-features --locked -j2
nice -n 10 cargo run -p bead --example minimal --locked -j2 -- --check
nice -n 10 cargo build -p nacre-shell --no-default-features --locked -j2
/w/target/debug/nacre-shell --check shell/ui/main.luau
mkdir -p /w/tmp/luau
curl -fsSL https://github.com/luau-lang/luau/releases/download/0.740/luau-ubuntu.zip -o /w/tmp/luau.zip
printf '%s\n' '7d4035db01816f977f6e4c42e3d5e8ef4235d95e9afe63a3c38905b42dde60f2  /w/tmp/luau.zip' | sha256sum -c -
unzip -oq /w/tmp/luau.zip -d /w/tmp/luau
chmod +x /w/tmp/luau/luau-analyze
nice -n 10 python3 tools/check-luau.py --analyzer /w/tmp/luau/luau-analyze --binary /w/target/debug/nacre-shell --check-generated
for suite in test_dashboard_capture_windows.py test_menu_engine_trace.py test_b0003_arm.py test_b0002_arm.py test_laptop_reference_capture.py test_foundation_preview.py; do nice -n 10 python3 -m unittest discover -s spikes/tests -p "$suite"; done
nice -n 10 python3 -m py_compile spikes/capture_safety.py spikes/measure.py spikes/bar-matrix.py spikes/dashboard-residency.py spikes/launcher-proof.py spikes/b0002-arm.py spikes/laptop-reference-capture.py spikes/laptop-reference-artifacts.py spikes/parity-compare.py spikes/fake-bluez.py spikes/foundation-preview.py spikes/menu-engine-proof.py spikes/menu_engine_trace.py spikes/b0003-arm.py spikes/tests/test_dashboard_capture_windows.py spikes/tests/test_menu_engine_trace.py spikes/tests/test_b0002_arm.py spikes/tests/test_laptop_reference_capture.py spikes/tests/test_b0003_arm.py
nice -n 10 python3 -m unittest discover -s packaging/tests
nice -n 10 cargo build -p nacre-shell --bin bead-wallpaper-video --locked -j2
nice -n 10 python3 spikes/wallpaper-video-smoke.py --engine /w/target/debug/bead-wallpaper-video
nice -n 10 python3 spikes/services-check.py
nice -n 10 dbus-run-session -- bash -c 'export DBUS_SYSTEM_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS" BEAD_PRIVATE_SERVICES_TEST=1 BEAD_FIXTURE_UPDATE=1; cargo test -p bead-services --test tray_private_bus --locked -j2 -- --ignored --nocapture'
nice -n 10 cargo deny check
for manifest in crates/bead-*/Cargo.toml crates/bead/Cargo.toml; do nice -n 10 cargo deny --manifest-path "$manifest" --all-features --config deny-framework.toml check; done
echo CI_OK
nice -n 10 cargo build --release --locked -j2 -p nacre-shell
cp /w/target/release/nacre-shell /w/target/release/nacre-wallpaper-decode /w/target/release/bead-wallpaper-video /w/out/
echo BUILD_OK
