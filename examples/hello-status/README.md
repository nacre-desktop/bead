# Hello status

A Luau top bar with workspace buttons and the focused window title, followed by clock, battery, audio, media, power profile, and the kernel release from its declared `uname -r` command.

The `workspaces` service detects niri before Hyprland. A computed list assigns palette roles to highlight the focused workspace; each button sends its opaque workspace ID to `desktop.workspace.focus`. With neither compositor, the workspace row and window title stay empty. Services declare their host stores automatically.

Run `bead check examples/hello-status` to validate without starting services, or `bead run examples/hello-status` in a Wayland session.
