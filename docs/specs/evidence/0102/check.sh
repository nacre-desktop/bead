#!/bin/bash
set -euo pipefail
export HOME=/root CARGO_HOME=/root/.cargo RUSTUP_HOME=/root/.rustup RUNNER_TEMP=/tmp/rt GITHUB_WORKSPACE=/w/src
mkdir -p $RUNNER_TEMP $HOME $CARGO_HOME/bin
export PATH=$CARGO_HOME/bin:$PATH
export BEAD_TEST_ARTIFACTS=$RUNNER_TEMP/bead-headless
cd /w/src
echo '=== Prepare Arch container'
pacman -Syu --noconfirm --needed base-devel curl git python unzip systemd wayland fontconfig libxkbcommon libpipewire clang gdk-pixbuf2 libwebp pkgconf dbus mpv ffmpeg sway swaybg wlr-randr ttf-dejavu ttf-ibm-plex grim python-pillow rustup cmake freetype2 lcms2 libraqm nasm >/dev/null
setcap -r /usr/bin/sway 2>/dev/null || true
rustup toolchain install 1.99.0 --profile minimal --component rustfmt --component clippy
rustup default 1.99.0
echo '=== Formatting'
cargo fmt --all --check
echo '=== Clippy'
cargo clippy --workspace --all-targets --locked -j2 -- -D warnings
echo '=== Headless services dependency boundary'
python3 tools/check-service-boundary.py
echo '=== Tests'
cargo test --workspace --locked -j2
cargo test -p nacre-shell --features trace --locked -j2 memory_policy::tests::

echo '=== Renderer feature configurations'
cargo check -p nacre-shell --no-default-features --locked -j2
cargo check --workspace --all-features --locked -j2

echo '=== Facade, headless UI and Luau schema'
cargo run -p bead --example minimal --locked -j2 -- --check
cargo build -p nacre-shell --no-default-features --locked -j2
target/debug/nacre-shell --check shell/ui/main.luau
mkdir -p "$RUNNER_TEMP/luau"
curl -fsSL https://github.com/luau-lang/luau/releases/download/0.740/luau-ubuntu.zip -o "$RUNNER_TEMP/luau.zip"
echo "7d4035db01816f977f6e4c42e3d5e8ef4235d95e9afe63a3c38905b42dde60f2  $RUNNER_TEMP/luau.zip" | sha256sum -c -
unzip -oq "$RUNNER_TEMP/luau.zip" -d "$RUNNER_TEMP/luau"
chmod +x "$RUNNER_TEMP/luau/luau-analyze"
python3 tools/check-luau.py --analyzer "$RUNNER_TEMP/luau/luau-analyze" --binary "$PWD/target/debug/nacre-shell" --check-generated

echo '=== Dashboard capture-window regression tests'
nice -n 10 python3 -m unittest discover -s spikes/tests -p test_dashboard_capture_windows.py
nice -n 10 python3 -m unittest discover -s spikes/tests -p test_menu_engine_trace.py
nice -n 10 python3 -m unittest discover -s spikes/tests -p test_b0003_arm.py
nice -n 10 python3 -m unittest discover -s spikes/tests -p test_b0002_arm.py
nice -n 10 python3 -m unittest discover -s spikes/tests -p test_laptop_reference_capture.py
nice -n 10 python3 -m unittest discover -s spikes/tests -p test_foundation_preview.py
nice -n 10 python3 -m unittest discover -s spikes/tests -p test_fake_system.py
nice -n 10 python3 -m py_compile spikes/capture_safety.py spikes/measure.py spikes/bar-matrix.py spikes/dashboard-residency.py spikes/launcher-proof.py spikes/b0002-arm.py spikes/laptop-reference-capture.py spikes/laptop-reference-artifacts.py spikes/parity-compare.py spikes/fake-bluez.py spikes/fake-power.py spikes/fake_system.py spikes/foundation-preview.py spikes/menu-engine-proof.py spikes/menu_engine_trace.py spikes/b0003-arm.py spikes/tests/test_dashboard_capture_windows.py spikes/tests/test_menu_engine_trace.py spikes/tests/test_b0002_arm.py spikes/tests/test_laptop_reference_capture.py spikes/tests/test_b0003_arm.py spikes/tests/test_fake_system.py

echo '=== Packaging staging and launcher'
python3 -m unittest discover -s packaging/tests
echo '=== Motion wallpaper renderer smoke'
cargo build -p nacre-shell --bin bead-wallpaper-video --locked -j2
python3 spikes/wallpaper-video-smoke.py --engine target/debug/bead-wallpaper-video

echo '=== Private service owner lifecycle'
python3 spikes/services-check.py
echo '=== Private SNI and DBusMenu lifecycle'
dbus-run-session -- bash -c 'export DBUS_SYSTEM_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS" BEAD_PRIVATE_SERVICES_TEST=1 BEAD_FIXTURE_UPDATE=1; cargo test -p bead-services --test tray_private_bus --locked -j2 -- --ignored --nocapture'

echo '=== Install cargo-deny'
asset=cargo-deny-0.20.2-x86_64-unknown-linux-musl.tar.gz
base=https://github.com/EmbarkStudios/cargo-deny/releases/download/0.20.2
curl -fsSL "$base/$asset" -o "$RUNNER_TEMP/$asset"
curl -fsSL "$base/$asset.sha256" -o "$RUNNER_TEMP/deny.sha256"
cd "$RUNNER_TEMP"
printf '%s  %s\n' "$(cat deny.sha256)" "$asset" | sha256sum -c -
tar -xzf "$asset"
install -m 755 "$RUNNER_TEMP/${asset%.tar.gz}/cargo-deny" "$HOME/.cargo/bin/cargo-deny"

cd /w/src
echo '=== Dependency and license checks'
cargo deny check
for manifest in crates/bead-*/Cargo.toml crates/bead/Cargo.toml; do
  cargo deny --manifest-path "$manifest" --all-features --config deny-framework.toml check
done

echo CI_ALL_OK
