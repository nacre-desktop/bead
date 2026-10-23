#!/bin/sh
# Inside the throwaway builder container: unpack the sources and build nacre-helper and the bead-shell package.
set -e
cd /work
tar xf /in/nacre.tar; tar xf /in/orient.tar; mkdir -p nacre/deps && rm -rf nacre/deps/orient && mv orient nacre/deps/orient
tar xf /in/bead.tar
chown -R tester:tester /work
su tester -c "cd /work/nacre && nice -n 10 cargo build --locked --release -j4 -p nacre-helper --bin nacre-helper 2>&1 | tail -3"
su tester -c "cd /work/bead/packaging/arch && nice -n 10 env MAKEFLAGS=-j4 CARGO_BUILD_JOBS=4 makepkg -f --noconfirm --nodeps 2>&1 | tail -3"
