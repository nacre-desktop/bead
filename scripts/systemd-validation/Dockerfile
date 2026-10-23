FROM ghcr.io/archlinux/archlinux:latest
RUN pacman -Syu --noconfirm --needed git python python-pillow ruff lua fish qt6-declarative qt6-wayland quickshell qt6-multimedia qt6-multimedia-ffmpeg qt6-imageformats gcc pkgconf rust clang libpipewire lcms2 cmake make jq mesa dbus ttf-dejavu niri labwc base-devel sudo systemd procps-ng iproute2 libxkbcommon gdk-pixbuf2 glib2 fontconfig \
 && pacman -Scc --noconfirm
RUN useradd -m -u 1000 -s /bin/bash tester && echo 'tester ALL=(ALL) NOPASSWD: ALL' > /etc/sudoers.d/tester \
 && mkdir -p /var/lib/systemd/linger && touch /var/lib/systemd/linger/tester
# systemd as PID 1: drop units that cannot work in a container
RUN systemctl mask systemd-udevd.service systemd-udev-trigger.service systemd-remount-fs.service getty.target console-getty.service 2>/dev/null; true
STOPSIGNAL SIGRTMIN+3
CMD ["/usr/lib/systemd/systemd"]
