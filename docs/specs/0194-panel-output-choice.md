# B0194: Panel output choice

Status: candidate. Unit and CI checks below; the lead's live two-monitor check is the acceptance.

## Cause

Owner layout: eDP-1 at logical 0,0 (left), DP-1 at 1920,0 (right). The right control centre "opened correctly" and the AI sidebar always opened on the laptop.

1. The AI sidebar (`nacre left`, Super+A) is an on-demand app process. The shell sent it `bead.panel toggle` (or spawned it) with no output, and the app's panel start resolves "no output" to the first output the compositor lists: eDP-1, always. The lip and hover routes dropped the output they knew.
2. IPC and keybinding panels (`bead.panel toggle|open`, `drawers toggle ...`) with no output also fell to the first output with a bar, and the primary-session routing for remote calls used the last pointer or keyboard-surface output, not the compositor focus (so they followed the mouse, or alternated).

The panels are anchored per output (each output has its own frame and edges); the right panel looked correct only because the layout put the external monitor at the right.

## Rule

One resolver, `panels::invocation_output` (bead-wayland): explicit output, else the output already routed for this event (pointer-driven events run in the pointer output's session), else the compositor-focused output, else the last pointer or keyboard-surface output; names that are not a current output are skipped.

- Keyboard and IPC: the shell hands bead-wayland a focus probe (`Options.focused_output`, niri snapshot `focused` output). Used for remote-call routing, `bead.panel` calls, the restored/default panel and overlay (OSD, toast) fallback.
- Pointer (bar click, status dropdowns, edge hover and lip clicks): the pointer's output. The AI sidebar's lip passes its output to the app (`app::panel_request`).
- AI sidebar: `app::route_on` fills the focused output for keyboard/IPC, sends it as `call.output` to a running app, or as `--output` when spawning; the app keeps all outputs eligible and opens its first panel on it (`Options.panel_output`).
- Detached/pinned pearls and windows keep their own placement (untouched).
- Notifications already used the daemon-published focused output (B0150 area); unchanged.

## Checks

- Unit: `panels::tests::panels_open_on_the_invoking_output_of_two` (two fake outputs: focus either way, pointer-routed, explicit, unknown skipped).
- Existing app host tests cover spawn/route argv.
- Nested two-output proof and live check: see report (lead's live audit is the acceptance).

Temporary duplicates: none.
