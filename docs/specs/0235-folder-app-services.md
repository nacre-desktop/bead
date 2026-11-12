# 0235 — Folder app services

Folder manifests may opt in to any subset of desktop snapshot services and declare named argv-only commands. Undeclared desktop services are not published; no services start for an empty declaration. `bead check` uses an empty snapshot and does not start services or commands. Views declare default stores in their behavior for the values they consume. Desktop snapshots expose raw clock/date, audio, battery, network, Bluetooth, tray, and session values; unknown numeric readings use `-1`. Older snapshot revisions are ignored.

Folder actions named `desktop.volume`, `desktop.mute`, and the tray activate/secondary/menu actions route to the declared service, with volume clamped to 0–1 and pointer coordinates supplied for tray actions. Other actions retain behavior-script routing. Declared commands use the bounded process runner without a shell; output is trimmed and capped at 4 KiB in stores, timeout is at most 60 seconds, and interval runs do not overlap.

The example `hello-status` demonstrates a layer surface, desktop values, and a local `uname -r` command. This unit also centres the existing hello-bar labels and adds a FolderView coordinate-frame input test. Clock seconds are leased whenever `clock` is declared.

Out of scope: notification server and popups/multiple surfaces, files/watch stores, richer MPRIS/UPower data, per-output data, and Nacre integration (B0236+). No temporary duplicate implementation paths were introduced.

Visual parity gap: `hello-status` is a new framework example with no corresponding Nacre QML surface, so a paired QML comparison is not applicable. Its requested runtime capture remains unverified.

Evidence: formatting and framework/service boundary checks completed in the dev-1 container. Workspace Clippy stopped on four lints; those source issues were fixed after the third allowed run ended. Tests, feature checks, example proofs, license checks, and the headless-sway captures remain unverified; no screenshots were produced. See the task's `.lead/report.md` for command output.
