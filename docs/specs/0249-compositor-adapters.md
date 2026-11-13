# 0249 — Compositor adapters

`bead-backends` provides clean-room niri and Hyprland Unix-socket clients behind
the default-on `compositors` feature. They implement the existing neutral
`Backend`, `Snapshot`, subscription and error contracts using only the standard
library, serde and serde_json. Disabling the feature retains the neutral types.
Detection prefers a non-empty `NIRI_SOCKET`, then `HYPRLAND_INSTANCE_SIGNATURE`
with `XDG_RUNTIME_DIR`; no detected compositor means no adapter or worker thread.

Snapshots contain outputs, per-output active and globally focused workspaces,
urgency, windows and focused window metadata. Niri consumes its initial event
burst and subsequent updates; Hyprland subscribes before reading its initial
JSON state and refreshes only for relevant events. Hyprland urgency follows
urgent events and clears when the window gains focus. There is no snapshot polling.
Niri's documented event API has no output-change notification: outputs are read
at connection and workspace/configuration changes; geometry-only changes may
remain stale until the next such event. Socket loss clears state and reconnects
with bounded exponential backoff. Snapshot IDs include the connection epoch;
actions reject stale or unknown IDs.

| Neutral action | niri | Hyprland |
|---|---|---|
| `SwitchWorkspace` by opaque ID | `FocusWorkspace`, `Id` reference | `dispatch workspace` |
| `SwitchWorkspaceIndex` | `FocusWorkspace`, `Index` reference (per output) | `dispatch workspace` (global) |
| `FocusWindow` | `FocusWindow` | `dispatch focuswindow address:…` |
| `FocusOutput` | `FocusMonitor` | `dispatch focusmonitor` |

Other existing action variants return `Unsupported`. Window management beyond
focus, layouts, overview and other compositors remain out of scope. Capability
flags describe implemented, observable features, not every upstream facility.

Folder apps opt in with `[services] desktop = ["workspaces"]`. One shared
`FolderServicesRuntime` owns the detected adapter. It declares
`desktop.workspaces` (records with `id,index,name,output,active,focused,urgent`),
`desktop.window.title`, `desktop.window.app_id`, and `desktop.compositor`.
Workspace and output IDs are opaque Text; an unknown output is empty Text.
`desktop.workspace.focus` accepts an integer Number index or Text ID;
`desktop.window.focus` accepts a Text window ID. Both require the declaration
and report an error if no compositor is detected. Pointer presses, deferred
releases and keyboard activation forward host actions before Luau dispatch.
Undetected defaults are an
empty list and empty strings. `bead check` declares defaults without connecting.

`hello-status` demonstrates a computed workspace list with focused palette roles
and ID-based buttons, plus the focused title. Its two rows keep the example
readable. `tutorial-bar` stays unchanged to preserve its 50-line limit. This is
a framework example with no product/QML counterpart. No new kit primitive or
temporary duplicate implementation is introduced. The new service's resident
memory budget is 1 MiB. Five alternating detected/undetected CPU bar pairs
measured a +136 KiB median in Anonymous + Pss_Shmem + SwapPss, excluding clean
file pages; the budget passes.

Licensing: implementation and fixtures were authored from the public
[niri IPC guide](https://niri-wm.github.io/niri/IPC.html), its linked generated
protocol API documentation, the [Hyprland IPC guide](https://wiki.hypr.land/ipc/)
and [hyprctl documentation](https://wiki.hypr.land/configuring/core/advanced-configuration/using-hyprctl/).
No compositor implementation source or application implementation was read or
copied; no niri-ipc or GPL dependency is added. Fixtures are hand-authored
protocol examples, not recordings copied from an upstream implementation.

Evidence: the complete Ubuntu 24.04 CI replica passed: formatting, warnings-denied
workspace Clippy, 615 workspace tests (15 standard ignores), feature configurations,
boundaries and negative controls, both minimal hosts, all four folder apps, private
service/tray/clock proofs, and cargo-deny for all 13 manifest roots. Focused socket
and folder-host tests also pass, including a populated-view click/keyboard regression.
Private headless sway captures were inspected for idle, hover, pressed and switched
states: a click sends native workspace ID 2 and events update the highlight and title.
Raw logs, captures and harnesses remain outside committed documentation.
