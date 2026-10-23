set -eu
W=/work/nacre
S=$W/nacre/shell-tools
D=$HOME/.local/share/nacre/shell
mkdir -p $D/tools $D/bin $HOME/.local/bin $HOME/.config/systemd/user $HOME/.config/quickshell $HOME/src
cp -a $W $HOME/src/nacre-src; rm -rf $HOME/src/nacre-src/target
cp -a $W/nacre/shell $D/source
cp $W/target/release/nacre-helper $D/tools/nacre-helper
cp $S/qt-check.sh $D/tools/ 2>/dev/null || true
for n in nacrectl nacre-daemon; do ln -s ../tools/nacre-helper $D/bin/$n; ln -sf $D/bin/$n $HOME/.local/bin/$n; done
install -m755 $S/launch.sh $D/bin/qs
install -m755 $S/cli-bridge.sh $D/bin/nacre_shell
install -m755 $S/control.sh $HOME/.local/bin/nacre-shell
ln -sfn $D/source $HOME/.config/quickshell/nacre
for u in nacre-daemon.socket nacre-daemon.service nacre-shell.service; do cp $S/$u $HOME/.config/systemd/user/$u; done
mkdir -p $HOME/.local/share/dbus-1/services
printf '[D-BUS Service]\nName=org.freedesktop.Notifications\nExec=/usr/bin/systemctl --user start nacre-shell.service\nSystemdService=nacre-shell.service\n' > $HOME/.local/share/dbus-1/services/org.freedesktop.Notifications.service
sudo pacman -U --noconfirm /work/bead/packaging/arch/bead-shell-0.1.0-1-x86_64.pkg.tar.zst 2>&1 | tail -3
ls -l /usr/bin/bead-shell
