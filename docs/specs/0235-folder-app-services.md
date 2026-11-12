# 0235 — Folder app services

Folder manifests may opt in to any subset of desktop snapshot services and declare named argv-only commands. Undeclared desktop services are not published; no services start for an empty declaration. `bead check` uses an empty snapshot and does not start services or commands. Views declare default stores in their behavior for the values they consume. Desktop snapshots expose raw clock/date, audio, battery, network, Bluetooth, tray, and session values; unknown numeric readings use `-1`. Older snapshot revisions are ignored.

`bead run` owns one process-wide runtime shared by every view/output; command results remain available to newly attached views, and the interruptible interval scheduler stops with the runtime. Folder actions named `desktop.volume`, `desktop.mute`, and the tray activate/secondary/menu actions route to the declared service, with volume clamped to 0–1 and pointer coordinates supplied for tray actions. Other actions retain behavior-script routing. Declared commands use the bounded process runner without a shell; process output is capped at 64 KiB and stored stdout at 4 KiB, timeout is at most 60 seconds, and interval runs do not overlap.

The example `hello-status` demonstrates a layer surface, desktop values, and a local `uname -r` command. This unit also centres the existing hello-bar labels and adds a FolderView coordinate-frame input test. Clock seconds are leased whenever `clock` is declared.

Out of scope: notification server and popups/multiple surfaces, files/watch stores, richer MPRIS/UPower data, per-output data, and Nacre integration (B0236+). No temporary duplicate implementation paths were introduced.

Visual parity gap: `hello-status` is a new framework example with no corresponding Nacre QML surface, so a paired QML comparison is not applicable.

Evidence: dev-1 formatting, warnings-denied Clippy, focused crate tests, framework/service boundaries, all folder-app checks, the SDK minimal proof, and minimal-shell proof passed. The runtime screenshot was inspected: hello-bar labels sit around the 36-pixel bar's vertical midpoint; hello-status shows `20:07`, unavailable battery/audio states, and `Kernel 6.14.0-37-generic` in its 42-pixel layer. Command fan-out and prompt scheduler-stop tests pass; see `.lead/report.md` for exact output.
