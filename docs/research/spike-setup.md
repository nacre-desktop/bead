# Isolated research tool setup

The prepared task checkout already has these tools. The owner's single run
command there is `./spikes/run.sh`; no setup repetition is needed.

This recipe is for a **fresh Arch-compatible Linux checkout**, matching the
measurement environment. Ubuntu CI compiles/checks Rust but does not run this
desktop benchmark. Do not assume the extracted Arch libraries are portable to
every Linux distribution. Other hosts should use their compatible compositor
packages and document the changed baseline.

From the Bead repository root, install Rust only into ignored task directories:

```sh
mkdir -p .tools
curl --fail --silent --show-error https://sh.rustup.rs -o .tools/rustup-init.sh
export CARGO_HOME="$PWD/.tools/cargo"
export RUSTUP_HOME="$PWD/.tools/rustup"
export PATH="$CARGO_HOME/bin:$PATH"
sh .tools/rustup-init.sh -y --no-modify-path --profile minimal --default-toolchain 1.99.0
rustup component add rustfmt clippy --toolchain 1.99.0
```

Native prerequisites used: cc/pkg-config, Wayland client development libraries,
niri 26.04, quickshell 0.3.2/Qt 6.12, grim, wayland-info, dbus-run-session, Python,
tar with zstd support and gpg/gpgv. Existing tools were used without changing their
configuration. No pacman packages were installed by Phase 0.

The offscreen parent was extracted from these official Arch package archives into
`.tools/native/` (not installed into the operating system):

- labwc-0.20.2-1-x86_64.pkg.tar.zst
- wlroots0.20-0.20.2-1-x86_64.pkg.tar.zst
- libsfdo-0.1.4-1-x86_64.pkg.tar.zst

Exact URLs, SHA-256 and successful package-signature verification are retained in
[native-spike-tools.json](native-spike-tools.json). Fetch the matching archives
and `.sig` files from those URLs. Verify signatures with the installed official
Arch public keyring before extraction. If that keyring is ASCII armored, dearmor
it into a task-local file (the original is read-only). For example:

```sh
mkdir -p .tools/native .tools/gpg-verify
chmod 700 .tools/gpg-verify
gpg --no-options --homedir "$PWD/.tools/gpg-verify" --dearmor \
    --output .tools/archlinux-public.gpg /usr/share/pacman/keyrings/archlinux.gpg
gpgv --keyring "$PWD/.tools/archlinux-public.gpg" ARCHIVE.sig ARCHIVE
tar -xf ARCHIVE -C .tools/native
```

Repeat verification/extraction for each archive. These uppercase archive tokens
mean the downloaded filenames above, not literal installed commands. Old mirror
files may expire; do not silently upgrade a dependency and call it the same
baseline. Use the version manifest to find the matching archive or record a new
comparison. The script needs `.tools/native/usr/bin/labwc` and sets that parent's
library path only for the task-owned process.

Then run `./spikes/run.sh`. Results replace spikes/results/baseline.json; keep a
copy if comparing with the committed research result. A shorter smoke check is
`./spikes/run.sh --runs 1 --seconds 2`; its CPU interval is **not** the 60-second
baseline. All sessions are offscreen and task-owned, with isolated D-Bus/XDG.

These tools are local prerequisites, not published packages or framework
dependencies. No desktop configuration, daily shell or installed Nacre is changed.
