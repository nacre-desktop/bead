# B0125 — Fullscreen covers the edge lips

The side arrows were Overlay layer surfaces, above niri's fullscreen clients.
The bar and frame already use Top, which niri puts above ordinary windows and
below fullscreen clients. Lips now use Top too, including any configured edge.
The compositor covers their pointer regions together with their artwork, and
uncovering them restores normal hover and click without rebuilding the shell.
They continue to request no keyboard interactivity.

## QML parity and surface audit

`NacreFrameLips.available` and `NacrePanelInput.autoEdges` reject fullscreen;
`NacreScreen` is Top on niri and hides in fullscreen unless a modal is open.
There is no hover-reveal strip in fullscreen. Bead's bar-hosted top lip is
already covered with the bar; standalone lips now follow it. Reserved edges
were already Top and inputless. Normal panels also use Top; lip recreation
after opening keeps the arrows above the panel in that layer.

QML notification cards belong to `NacrePanelHost` on the main screen, so they
also disappear in fullscreen. Bead now declares `notices` in
`frame.fullscreen_covered_overlays` in `shell/frame/behavior.luau`.
`Overlays::with_fullscreen_covered` is a general per-view Top/Overlay selection;
the engine contains no Nacre view names. The QML level feedback window
(`NacreLevelNotice`) explicitly uses Overlay, no keyboard focus and an empty
input mask, with no fullscreen suppression. Bead retains its level OSD on
Overlay. The control drawer called `osd` in QML is a separate panel, called
`control` in Bead. Dock guides and status hints are transient explicit UI;
wallpaper and widget canvases are below normal windows.

## Validation

`spikes/fullscreen-lips.py` reuses the guarded private parity harness: matched
QML/Bead niri sessions, 2880×1800 at 1.5, generated wallpaper/palette, CPU Bead.
It checks ordinary hover, all edge surfaces on Top, a fullscreen opaque terminal,
both arrows and the top edge free of chrome pixels, dwell without reveal,
clicks and typed input delivered to the client at all three edges, restoration,
notifications covered and explicit level OSD still visible. Capture uses the
desktop lock, private HOME/buses and zero-swap-in guard; it stops its processes.

The fake-compositor tests validate the capture oracle across a fullscreen toggle,
restoration and the original Overlay failure. They do not execute the Rust
Wayland client; the nested proof checks actual layer requests and input routing.
The Rust catalog test checks independent per-view layer selection, rejects an
unknown view, and ensures configuration does not instantiate workers.

The full `.github/workflows/ci.yml` check job passes in a remote Arch container
(rustc/cargo 1.99.1, two test threads): fmt, denied-warning clippy, service
boundary, workspace/trace tests, renderer configurations, facade/Luau/schema,
Python/package suites, wallpaper smoke, private service/SNI lifecycles and all
license graphs. The final uninterrupted run exits 0 with `CI_OK`.
[Commands](evidence/0125/check.sh), [environment/entry point](evidence/0125/verify.sh),
[receipt](evidence/0125/ci-summary.txt), [source hashes](evidence/0125/source-sha256.txt).
Initial archive setup stops were repaired with the expected target path, a
private Git snapshot and fresh scratch directory; no test was bypassed.

Both arms pass every check; all five Bead edge surfaces report Top/None.
Each client receives SGR mouse reports and `LEFT`, `RIGHT`, `TOP` after edge clicks.
The notification proof requires an additional mapped Top surface, so an absent
producer cannot make a clear screenshot pass. Host swap is zero before
both arms; swap-in and capture-scope OOM deltas are zero. The guarded nested
sessions use a synthetic parent refresh, not physical scanout timing.

[Results, binary identity and guard summaries](evidence/0125/results.json).
Paired captures (QML / Bead):

| State | QML | Bead |
| --- | --- | --- |
| Idle | [image](evidence/0125/qml-idle.png) | [image](evidence/0125/bead-idle.png) |
| Hover/open | [image](evidence/0125/qml-hover.png) | [image](evidence/0125/bead-hover.png) |
| Pressed | [image](evidence/0125/qml-pressed.png) | [image](evidence/0125/bead-pressed.png) |
| Click/open | [image](evidence/0125/qml-click-open.png) | [image](evidence/0125/bead-click-open.png) |
| Fullscreen | [image](evidence/0125/qml-fullscreen.png) | [image](evidence/0125/bead-fullscreen.png) |
| Fullscreen notification | [image](evidence/0125/qml-fullscreen-notification.png) | [image](evidence/0125/bead-fullscreen-notification.png) |
| Fullscreen level OSD | [image](evidence/0125/qml-fullscreen-osd.png) | [image](evidence/0125/bead-fullscreen-osd.png) |
| Restored | [image](evidence/0125/qml-restored.png) | [image](evidence/0125/bead-restored.png) |
| Restored hover | [image](evidence/0125/qml-restored-hover.png) | [image](evidence/0125/bead-restored-hover.png) |

The frame/lip geometry, arrow direction and fullscreen coverage match. Existing
unrelated differences remain: QML's control grid retains a VPN tile without a
NetworkManager fixture while Bead omits it and reflows the other controls;
capability dimming differs; the update readout is 150 versus 15; terminal focus
decoration differs with the control drawer open. QML's notification remains
after leaving fullscreen, while Bead's is absent after the intervening level
OSD; this proof does not distinguish popup expiry from OSD preemption. It checks
fullscreen coverage, not popup lifetime parity. Clock time also differs.
No animation implementation changed; these captures check settled states.

No new product dependencies or temporary duplicates. No new render surface or buffer;
the shipped catalog's additional metadata is estimated below 1 KiB. No memory measurement is
claimed. No live desktop, installed shell or settings changes.
CI-only tools: `python-yaml` for workflow extraction, `libcap` for the disposable
Sway setup; capture uses an existing private `swaybg` binary.
