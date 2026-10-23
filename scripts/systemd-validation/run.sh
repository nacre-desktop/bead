#!/bin/sh
# B0033 driver. Run on a Docker build host (not the owner's laptop) inside an empty directory D that holds
# nacre.tar (git archive of Nacre origin/main), orient.tar (its deps/orient submodule) and bead.tar (this repo),
# plus this directory's files. Everything is a disposable container named b0033-*; `clean` removes it all.
#   ./run.sh build   image + helper + bead-shell package     ./run.sh up      systemd container + install + F4
#   ./run.sh test    run validate.py inside it (results.json)  ./run.sh clean   remove containers, image, work dir
set -eu
D=$(pwd)
t() { docker exec -u tester -e XDG_RUNTIME_DIR=/run/user/1000 -e DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus -e HOME=/home/tester -w /home/tester b0033-sd "$@"; }
put() { docker exec -i -u tester b0033-sd sh -c "cat > /home/tester/$2" < "$1"; }
case ${1:-} in
build)
	docker build -t b0033-base . > build.log 2>&1
	mkdir -p work
	docker run --rm --name b0033-build --cpus=8 --memory=12g -v "$D/work:/work" -v "$D:/in:ro" b0033-base sh /in/build.sh
	;;
up)
	docker run -d --name b0033-sd --privileged --cgroupns=private --cpus=8 --memory=12g \
		--tmpfs /run --tmpfs /run/lock --tmpfs /tmp -v "$D/work:/work:ro" b0033-base > /dev/null
	until t true 2>/dev/null && [ "$(t systemctl --user is-system-running 2>/dev/null)" != "" ]; do sleep 1; done
	for f in install.sh fixtures.sh validate.py crash_switch.py; do put "$f" "$f"; done
	t sh install.sh
	t sh fixtures.sh
	t systemctl --user start b33-session.target
	t sh -c 'sleep 15; mkdir -p -m700 ~/.local/state/nacre/releases; touch ~/.local/state/nacre/releases/release.lock; chmod 600 ~/.local/state/nacre/releases/release.lock'
	t python3 /home/tester/src/nacre-src/tools/releases.py f4 enable
	;;
test)
	shift
	put validate.py validate.py; put crash_switch.py crash_switch.py
	t python3 /home/tester/validate.py "$@" 2>&1 | tee results.txt || true
	docker exec b0033-sd cat /home/tester/results.json > results.json
	;;
clean)
	docker rm -f b0033-sd b0033-build 2>/dev/null || true
	docker rmi b0033-base 2>/dev/null || true
	;;
*) echo "usage: run.sh build|up|test [scenarios]|clean" >&2; exit 2 ;;
esac
