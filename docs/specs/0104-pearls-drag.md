# B0104 — Pearl drag, dock and close

Status: lead-accepted for integration on 2026-10-25; rebased validation in progress.

Decision: **lead-accepted 2026-10-25, follow-up memory attribution**. The lead
prioritizes fixing an unclosable live pearl over the measured detached/closed
+0.891/+0.828 MiB deltas for this unit. This accepts the recorded memory failure,
including its +2.133 MiB anon outlier; it does not raise the general memory budget.
The measurements and remaining proof gaps below are unchanged.

## Reference and design

The reference is Nacre `origin/main` `3afcdc64`: `NacrePearlWindow`, `NacreDockGuide`,
`geometry.js`, `NacrePanelDrag`, `NacreDockFilm`, `NacreDockTarget`, `NacrePearlBorder`,
`NacrePearls` and `NacrePanelDragRouting`. All geometry is logical pixels. A drag
uses the original body and a fixed grab point; framebuffer scale never multiplies
pointer travel. Docking keeps the existing 170 px capture / 72 px arm / 104 px hold.
The reference helper is built from that same snapshot and pinned Orient `3efe8d1`.

Docked pulls require the compositor’s relative-pointer protocol. During a pull the
layer remains the pointer-grab owner. On detached release
the same worker and scene move to a fresh xdg toplevel. Native pearls receive the
compositor's close request. Nacre's existing `pearl-window` helper places owned
windows; the framework receives only generic identity and placement callbacks.
The helper queue keeps the newest position per pearl and releases its thread when
empty. Its work never blocks input handling.

On niri, QML intentionally hides the magnetic film: there is no live native cursor
contract. Landing guides remain visible and the completed native geometry
determines docking. Bead consumes the same published compositor snapshot on the
existing UI-provider connection. It reuses the permanent notices topic when
present, otherwise holds a lease only while pearls exist. Both paths have a
subscription-lifecycle test.

## Close, dock and recovery

- Compositor close-window (including the owner's Super+Q binding).
- Ctrl+W on the focused pearl; view-owned close requests; numbered `pearls close ID`.
- Release armed at the view's edge, or `pearls dock ID`, to return it to the shell.
- `nacrectl ui pearls close all` closes open pearls and clears remembered entries,
  including disconnected outputs. Direct equivalent: `nacre-shell ipc --socket
  SOCKET bead.panel pearls-reset`. This extends the existing close route.
- Restored rectangles are bounded to the current output using the rendered body
  size, including stale undersized rectangles; invalid geometry is replaced.
  Only bounded, whitelisted view state is persisted, never provider data
  or credentials. QML's pearls do not use Escape as a window-close shortcut.

Native geometry follows the complete body, independently of reveal hit clipping.
Accepted worker scenes update native dimensions after a cold restore. Fully visible
rectangles keep their position; recovery margins repair invalid/offscreen state.
Output selection follows the released rectangle before falling back to its origin.

## Validation and evidence

Full `check` job replica passed on integrated code `2e7a824` in a disposable Arch container:
fmt, clippy with warnings denied, workspace/trace tests, both renderer feature
configurations, facade/Luau checks, Python suites, packaging, wallpaper smoke,
private services/tray lifecycle, and all dependency/license graphs. The archive
was restored to an exact shallow checkout for the packaging revision checks.
Arch needed its separate `swaybg` package and removal of Sway's unnecessary file
capability inside the container; the container privilege policy stayed intact.
Geometry tests include scales 1, 1.25 and 1.5, output clipping and stale saved
rectangles. State tests cover the view whitelist and owned compositor moves;
native geometry stays full-sized when reveal hit regions have zero height.
The native handoff resets idle-buffer timers tied to the old surface. Docking
retires GPU resources before replacing the native role with a fresh layer.

Capture binaries use `19b529b`, whose runtime and shell bytes equal `2e7a824`
(the rebase adds only upstream private capture-tool fixes). Both measurement arms
include the shared queued-worker shutdown fix on top of
control `417ac32`. Stop preempts queued stages before joining the worker; its
regression fails on the old queue and passes with the fix. Input coalescing is
preserved. This prevents an acknowledgement wait from deadlocking pearl close.

Final private nested niri CPU captures use 2880×1800 at scale 1.5 (logical
1920×1200), the same dark poster, palette, private settings and stripped debug
profile. Audio, battery and Bluetooth use fixtures; updates were not stubbed
and read the host provider, so this is not a fully fixture-controlled acceptance run. The order is control/candidate repeated three times. Each primary state
settles for 12 seconds; closed samples follow an exact numbered IPC close.
All six arms pass containment, zero OOM, geometry, full-frame and state checks.
Host swap used and swap-in are zero. Running executable hashes and argv0 match
the selected artifacts; measured shell priority is nice 10. CPU DRM=0 is verified
by empty DRM fd/fdinfo audits, not substituted for an unknown value.

| CPU phase | Control median, KiB | Candidate median, KiB | Paired median Δ, KiB | ≤+512 gate |
| --- | ---: | ---: | ---: | --- |
| Idle | 21622 | 21590 | +8 | pass |
| Docked | 34662 | 34294 | −376 | pass |
| Detached | 34828 | 35756 | +912 | **fail** |
| Closed | 22418 | 23266 | +848 | **fail** |

The metric is Pss_Anon + Pss_Shmem + DRM + SwapPss. Independent arm medians do
not necessarily subtract to the median paired delta. Detached/closed increases
are entirely anonymous; SHM is unchanged and SwapPss is zero. Their per-pair anon
deltas are `[912,308,2184]` / `[848,384,852]` KiB. The third detached pair exceeds
the owner's unexplained 1 MiB anon stop line. The cause remains unresolved.
One candidate main-stack sample is 236 KiB, insufficient to explain that excess;
no full mapping comparison was captured. No attribution to fonts or heaps is claimed.

Total PSS paired deltas are +202/−266/+1285/+1016 KiB; Pss_File deltas are
+194/+110/+164/+164 KiB (idle/docked/detached/closed). These are informational,
not gated. This desktop debug comparison makes no absolute laptop-cap claim.
[All primary components, paired rows, gates and binary identities](evidence/0104/memory.json).

Final QML and Bead Ctrl+W checks both close the focused pearl. Both also accept a
direct niri close-window request. Synthetic Super+Q fails in both shells and falls
back to that request, so the final key chord is **not proved**. JSON close results
are included with the memory evidence. Raw paired endpoints remain private:
the unstubbed updates provider exposed host package-update inventory in the bar.

The unexplained anon stop line ended the capture batch. The active direct-pull
arm was interrupted through its capture guard; its scope is inactive and partial
output is excluded. Final-source native move/dock, stale restart/reset and GL
refreshes did not run. Earlier development checks showed a 180-step held pull at
`[410,250,1100,590]`, a bottom landing strip at y=1190, and stale recovery from
`(1700,1050,100×100)` to `[808,598,1100,590]` with query retention over restart.
Those checks are preliminary, not final-source acceptance evidence.

Preliminary held-pull and native-away visual pairs are retained privately, with
the same updates-provider privacy limitation and incomplete executable provenance.
They are excluded from final-source acceptance. Final parity remains incomplete;
a new capture needs a stubbed updates provider. Nested niri forwards absolute
motion without relative-pointer events, so earlier held-pull probes used private
headless Labwc at the same scale with `--backend none` and the private any-press flag.
All input is synthetic; physical scanout timing is unmeasured.

The original stop was lifted by the lead decision above for integration and a
bounded memory-attribution follow-up. No live session, installation or configuration
is changed by this unit.

## Remaining parity scope

Native pearls remain content-sized; compositor resizing still needs the generic
resize/layout work absent in B0048. The landing guide's 160 ms fade remains an
existing visual gap. QML's first-frame dock handoff barrier/rollback is not
implemented; endpoint captures do not prove sub-frame handoff parity. Native
Hyprland movement and paired GL memory have not been measured by this unit.
Existing B0097 launcher typography, search-field insets/focus ring and quick-control
availability/content differences remain visible in the paired frames.

## Generalization and expiry

Native surface handoff, motion notifications, saved-state hooks and output-bound
recovery are reusable framework primitives. Product titles, daemon decoding and
the placement helper remain in the shell adapter. No new temporary duplicate is
introduced; the existing Bead-local pearl store retains B0048's expiry after a
shared published store replaces it.
