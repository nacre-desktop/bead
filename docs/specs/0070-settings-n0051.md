# B0070 - Settings pages consume Nacre N0051

Status: **Review** (Claude worker, branch `task/b0070-settings-n0051-20261023`). Closes the "requires a Nacre contract"
lines B0063 left in the Settings window. Contracts: Nacre N0051 (`f3193ec0`, `docs/specs/0051-settings-asks-batch-2.md`),
answering B0063's asks C1-C10. `nacre-protocol` pin moved from `09cf91fc` to `f3193ec0baf5133a1b20eb7f41112abc8d331d2d`
(no dependency change; the adapter still reads topics and calls methods as JSON, so the pin only keeps the mirror honest).

## What changed (all in the Nacre edge, `nacre-shell/src/settings/pages.rs`; Luau unchanged)

| Ask | Page / control | Now |
| --- | --- | --- |
| C1 | capability flags | `bluetooth` and `battery` are flags of the `capabilities` topic. B0063's derivation from the `bluetooth` and `battery` topics (and the two extra leases) is deleted: a flag the topic does not name is false. |
| C2 | Privacy (clipboard, launcher; launcher page too) | Inline confirm like the notification history (first press arms, second press calls `clipboard.clear_history` / `launcher.clear_history`). One `HISTORIES` table drives all three; arming one history never arms another. Notice "History cleared." |
| C3 | Keyboard and mouse | `keyboard-layout` and `touchpad-enabled` registry adapters are now plain `software-input` store rows written by `settings.set` (with `rev`). Touchpad is the usual toggle. Layouts: an info row with the chosen ids, a searchable picker over the host's `/usr/share/X11/xkb/rules/base.lst` (`! layout` section, read once when the page opens, 4 MiB cap, ids `[a-z0-9_]{1,16}`), pressing a layout adds or removes it (comma-separated, at most 160 characters), "Use the system default" writes `""`. Ids not in the host list are never sent. |
| C4 | Keyboard and mouse, shortcuts | `shortcuts.refresh` is called when the page opens (once per page visit); rows come from the `shortcuts` topic (description, key; the key when there is no description), first 150 with a "more" line. |
| C5 | Power | "Try again" calls `capabilities.refresh`. "Fix" calls `tools.launch {tool:"power-service"}`; when the daemon answers `unavailable` (no `powerprofilesctl`) Bead asks `ui.dispatch nacre menu software.packages power-profiles-daemon` instead (one stored fallback per call). |
| C6 | System | `system.status` is read when the page opens: release (12 chars), service states (16), compositor config, drifted managed files, Brain sync, last session event, last resource sample, update-cache age. AUR review policy shows the provenance (`mode · source · profile`) and a button for `tools.launch {tool:"aur-review-policy"}`. The updates row is not shown (Bead has no updates view, B0068). |
| C7 | Open sidebar, Brain, Welcome, Colors | `ui.dispatch nacre left`, `brain.open`, `ui.dispatch welcomeApp open`, `ui.dispatch colorsApp open` (the Colors row was a placeholder too). |
| C8 | Add-ons, Apps "optional tools" | `tools.launch {tool:"setup"}` for each missing add-on. |
| C9 | Sound | Default stays `audio.set_default {node}` with the PipeWire `node.name` (the audio item id), now advertised. Audio manager: `tools.launch audio-manager`. |
| C10 | Bluetooth | Second button per device, Trust / Untrust, `bluetooth.set_trusted {device, trusted}` (id re-resolved against the topic). |
| - | Network, Accounts, usage and projects | `tools.launch connection-editor` / `ai-workspace`. |

Launch buttons share one row builder, `launch_row`, driven by a closed set (`launch(key)`): the button argument is a key from that
set, never a method or parameters, so the argument text cannot choose a call. A button is dim unless its daemon method is
advertised. Nothing resident was added: the status, shortcuts and layout reads happen when their page opens and the results
are dropped with the window.

## Still a placeholder / not built

- `lock-preview`, `idle-policy`, `fonts`, `settings-backup`/`-restore`, `launcher-entries` keep their B0063 treatment (no row or
  "not here"); N0051 does not cover them.
- The Maintenance actions (profile sample, check session, sync, repair portals, restart, rollback) have no daemon method.
- `capabilities` still sees a touchpad change only at start or after `capabilities.refresh`.

## Checks

Fake-daemon tests (`settings_pages_tests.rs`): per call the exact method and parameter shape (clipboard/launcher clear with the
arming rules, layouts add/reset and touchpad with `rev`, unknown layout id sends nothing, shortcuts refresh on open and rows,
power refresh and the `unavailable` fallback, system status rows, AUR/Welcome/setup/Brain/sidebar/ai-workspace/audio-manager/
connection-editor launches, an unknown launch key sends nothing, dim buttons without the method, Bluetooth trust, an unpublished
device id sends nothing) plus a `base.lst` parser unit test. Goldens regenerated for power, privacy, keyboard-mouse (+ layouts
filter), system, nacre-ai, addons, brain, sound, bluetooth.

## Look-and-feel parity

**Not compared.** The desktop lock (`~/nacre-lead/desktop-heavy.lock`) was held by another worker's nested session
(2026-10-23 evening), so `spikes/parity-compare.py` was not run for privacy, keyboard-mouse, shortcuts, power, system, addons,
sound, bluetooth, nacre-ai or the launch buttons. Open question for the lead: run it on these pages; the known B0063 shell-wide
gaps (header, section cards, sidebar size, muted "unavailable" captions) apply unchanged. Goldens were eyeballed (privacy,
keyboard-mouse with the layouts picker and shortcuts, system status, nacre-ai, bluetooth, brain) but goldens matching themselves is not parity.
Visible differences to QML worth a look in the run: action buttons are right-aligned chips (QML: full buttons), the layout
picker is a list (QML: a text field with a suggestion list).

## CI

Arch container on dev-2 (`ghcr.io/archlinux/archlinux:latest`, `--cpus=8 --memory=12g`, toolchain 1.99.0), the `ci.yml` steps: fmt,
clippy `-D warnings`, service boundary, `cargo test --workspace` (nacre-shell 465 passed incl. 33 Settings tests and the i18n
table test), trace memory-policy test, `--no-default-features`, `--all-features`, minimal example, `nacre-shell --check
shell/ui/main.luau`, Luau analyze (`check-luau.py --check-generated`), spikes unit tests (need a git checkout), packaging
tests, services check, cargo-deny (root and every framework crate) all passed. Not green in the container: `wallpaper-video-smoke`
(`PermissionError: sway` - the container may not start sway; untouched code, as in B0063) and the tray private-bus test was not run.
The i18n mirror in Nacre needs `tools/i18n-sync.py --write` for the new `bead.settings.*` ids (and the two removed ones).

## Temporary duplicates

None added.
