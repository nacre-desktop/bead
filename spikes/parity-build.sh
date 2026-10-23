#!/bin/sh
# Build Bead (this tree) and the Nacre daemon for the parity harness, on a remote host in an Arch container.
# The Nacre daemon is kept between runs; REBUILD_NACRE=1 rebuilds it after Nacre moved.
# usage: spikes/parity-build.sh HOST TASK NACRE_CHECKOUT ORIENT_CHECKOUT   (run from the Bead worktree root)
# Output: ~/nacre-lead/TASK/out/{nacre-shell,nacre-helper,...} on HOST. Fails on any cargo error.
set -eu
HOST=$1; TASK=$2; NACRE=$3; ORIENT=$4
T=$(mktemp -d); R=nacre-lead/$TASK
git archive --format=tar.gz -o "$T/bead.tgz" HEAD
git -C "$NACRE" archive --format=tar.gz -o "$T/nacre.tgz" origin/main
O=$(git -C "$NACRE" ls-tree origin/main deps/orient | awk '{print $3}')
git -C "$ORIENT" archive --format=tar.gz -o "$T/orient.tgz" "$O"
cat > "$T/build.sh" <<'EOS'
#!/bin/sh
set -eu
pacman -Syu --noconfirm --needed base-devel libraqm lcms2 freetype2 cmake nasm rust clang pkgconf wayland fontconfig libxkbcommon libpipewire gdk-pixbuf2 mpv git ca-certificates >/dev/null
rm -rf /w/src /w/out; mkdir -p /w/src/bead /w/src/nacre/deps/orient /w/out
tar xzf /w/in/bead.tgz -C /w/src/bead; tar xzf /w/in/nacre.tgz -C /w/src/nacre; tar xzf /w/in/orient.tgz -C /w/src/nacre/deps/orient
export CARGO_TARGET_DIR=/w/target CARGO_HOME=/w/cargo
cd /w/src/bead && nice -n 10 cargo build --release --locked -j8 -p nacre-shell
[ -x /w/target/release/nacre-helper ] && [ -z "${REBUILD_NACRE:-}" ] || { cd /w/src/nacre && nice -n 10 cargo build --release --locked -j8 -p nacre-helper --bin nacre-helper; }
cd /w/target/release && cp nacre-shell nacre-wallpaper-decode bead-wallpaper-video nacre-helper /w/out/
rm -rf /w/bundle; mkdir -p /w/bundle; cp -r /w/src/bead/shell /w/out/shell
chown -R "$OWNER_UID:$OWNER_UID" /w/out /w/in; rm -rf /w/src
echo BUILD_OK
EOS
ssh -F "$HOME/.ssh/config" "$HOST" "mkdir -p ~/$R/in && rm -rf ~/$R/in/* ~/$R/out"
scp -q -F "$HOME/.ssh/config" "$T/bead.tgz" "$T/nacre.tgz" "$T/orient.tgz" "$T/build.sh" "$HOST:$R/in/"
ssh -F "$HOME/.ssh/config" "$HOST" "cd ~/$R && docker run --rm --name $TASK-build --cpus=8 --memory=12g -e OWNER_UID=\$(id -u) -e REBUILD_NACRE=${REBUILD_NACRE:-} -v \$PWD:/w archlinux:latest sh /w/in/build.sh" | tail -5
rm -rf "$T"
