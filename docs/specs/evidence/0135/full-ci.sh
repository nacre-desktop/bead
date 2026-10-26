set -euo pipefail
export PATH="/root/.cargo/bin:$PATH"
export RUNNER_TEMP=/work/runner-temp
export BEAD_TEST_ARTIFACTS=/work/bead-headless
mkdir -p "$RUNNER_TEMP" "$BEAD_TEST_ARTIFACTS"
cd /work/source
git config --global --add safe.directory /work/source
git rev-parse HEAD
rustc --version
cargo --version
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -j2 -- -D warnings
python3 tools/check-service-boundary.py
cargo test --workspace --locked -j2
cargo test -p nacre-shell --features trace --locked -j2 memory_policy::tests::
cargo check -p nacre-shell --no-default-features --locked -j2
cargo check --workspace --all-features --locked -j2
cargo run -p bead --example minimal --locked -j2 -- --check
cargo build -p nacre-shell --no-default-features --locked -j2
target/debug/nacre-shell --check shell/ui/main.luau
python3 tools/check-luau.py --analyzer "$RUNNER_TEMP/luau/luau-analyze" --binary "$PWD/target/debug/nacre-shell" --check-generated
nice -n 10 python3 -m unittest discover -s spikes/tests -p test_dashboard_capture_windows.py
nice -n 10 python3 -m unittest discover -s spikes/tests -p test_menu_engine_trace.py
nice -n 10 python3 -m unittest discover -s spikes/tests -p test_b0003_arm.py
nice -n 10 python3 -m unittest discover -s spikes/tests -p test_b0002_arm.py
nice -n 10 python3 -m unittest discover -s spikes/tests -p test_laptop_reference_capture.py
nice -n 10 python3 -m unittest discover -s spikes/tests -p test_foundation_preview.py
nice -n 10 python3 -m unittest discover -s spikes/tests -p test_fake_system.py
nice -n 10 python3 -m unittest discover -s spikes/tests -p test_fullscreen_lips.py
nice -n 10 python3 -m unittest discover -s spikes/tests -p test_pearl_release_proof.py
nice -n 10 python3 -m unittest discover -s spikes/tests -p test_launcher_icons_proof.py
nice -n 10 python3 -m unittest discover -s spikes/tests -p test_scrolling_proof.py
nice -n 10 python3 -m py_compile spikes/capture_safety.py spikes/measure.py spikes/bar-matrix.py spikes/dashboard-residency.py spikes/launcher-proof.py spikes/b0002-arm.py spikes/laptop-reference-capture.py spikes/laptop-reference-artifacts.py spikes/parity-compare.py spikes/fake-bluez.py spikes/fake-power.py spikes/fake_system.py spikes/foundation-preview.py spikes/menu-engine-proof.py spikes/menu_engine_trace.py spikes/b0003-arm.py spikes/tests/test_dashboard_capture_windows.py spikes/tests/test_menu_engine_trace.py spikes/tests/test_b0002_arm.py spikes/tests/test_laptop_reference_capture.py spikes/tests/test_b0003_arm.py spikes/tests/test_fake_system.py spikes/animation-pacing.py
python3 -m unittest discover -s packaging/tests
cargo build -p nacre-shell --bin bead-wallpaper-video --locked -j2
python3 spikes/wallpaper-video-smoke.py --engine target/debug/bead-wallpaper-video --expect-presentation shm
python3 spikes/services-check.py
dbus-run-session -- bash -c 'export DBUS_SYSTEM_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS" BEAD_PRIVATE_SERVICES_TEST=1 BEAD_FIXTURE_UPDATE=1; cargo test -p bead-services --test tray_private_bus --locked -j2 -- --ignored --nocapture'
cargo deny check
for manifest in crates/bead-*/Cargo.toml crates/bead/Cargo.toml; do
  cargo deny --manifest-path "$manifest" --all-features --config deny-framework.toml check
done
