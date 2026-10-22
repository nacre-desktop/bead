#!/bin/sh
# Install a built Bead shell under DESTDIR/PREFIX (used by the PKGBUILD and by user-prefix installs).
#   packaging/stage.sh DESTDIR [PREFIX=/usr] [CARGO_RELEASE_DIR=target/release]
set -eu
dest=${1:?usage: stage.sh DESTDIR [PREFIX] [CARGO_RELEASE_DIR]}
prefix=${2:-/usr}
build=${3:-target/release}
root=$(cd "$(dirname "$0")/.." && pwd)
base=$dest$prefix
install -Dm755 "$build/nacre-shell" "$base/lib/bead/nacre-shell"
install -Dm755 "$build/nacre-wallpaper-decode" "$base/lib/bead/nacre-wallpaper-decode"
install -Dm755 "$root/packaging/bin/bead-shell" "$base/bin/bead-shell"
# Test fixtures and goldens stay out of the install; the UI bundle is the Luau tree.
(cd "$root/shell" && find . -path ./tests -prune -o -type f -print) | while read -r file; do
	install -Dm644 "$root/shell/$file" "$base/share/bead/shell/$file"
done
install -Dm644 "$root/LICENSE-GPL-3.0" "$base/share/licenses/bead-shell/LICENSE-GPL-3.0"
