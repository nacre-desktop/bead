set -eu
U=$HOME/.config/systemd/user
mkdir -p $U
cat > $U/b33-labwc.service <<'X'
[Unit]
Description=B0033 headless labwc (test fixture)
PartOf=b33-session.target
[Service]
Type=simple
Environment=WLR_BACKENDS=headless WLR_HEADLESS_OUTPUTS=1 WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1
ExecStartPre=/bin/sh -c 'mkdir -p %t/b33cfg && : > %t/b33cfg/autostart'
ExecStart=/usr/bin/labwc --config-dir %t/b33cfg
ExecStartPost=/bin/sh -c 'for i in $(seq 100); do [ -S %t/wayland-0 ] && exit 0; sleep 0.1; done; exit 1'
X
cat > $U/b33-niri.service <<'X'
[Unit]
Description=B0033 nested niri (test fixture)
Requires=b33-labwc.service
After=b33-labwc.service
PartOf=b33-session.target
[Service]
Type=simple
Environment=WAYLAND_DISPLAY=wayland-0 LIBGL_ALWAYS_SOFTWARE=1
ExecStart=/usr/bin/niri --config %h/b33/niri.kdl
ExecStartPost=/bin/sh -c 'for i in $(seq 150); do [ -S %t/wayland-1 ] && ls %t/niri.*.sock >/dev/null 2>&1 && break; sleep 0.1; done; s=$(ls %t/niri.*.sock | head -1); systemctl --user import-environment XDG_RUNTIME_DIR; systemctl --user set-environment WAYLAND_DISPLAY=wayland-1 NIRI_SOCKET=$s NACRE_COMPOSITOR=niri QT_QPA_PLATFORM=wayland PIPEWIRE_REMOTE=b33-none LIBGL_ALWAYS_SOFTWARE=1'
X
cat > $U/b33-session.target <<'X'
[Unit]
Description=B0033 graphical session (test fixture)
Requires=b33-niri.service
After=b33-niri.service
BindsTo=graphical-session.target
Before=graphical-session.target
X
mkdir -p $HOME/b33
cp /work/bead/spikes/niri.kdl $HOME/b33/niri.kdl
systemctl --user daemon-reload
systemctl --user enable nacre-shell.service nacre-daemon.socket 2>&1 | tail -3
