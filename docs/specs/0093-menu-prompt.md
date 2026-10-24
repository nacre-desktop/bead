# B0093 - the selector prompt (`menuPrompt open`)

Status: **Review** (Claude worker, branch `task/b0093-menu-prompt-20261024`). Closes Asks 1 of B0084
([0084](0084-trial-switch-routes.md)): under Bead `nacre-menu pick|input`, the Wi-Fi chooser (`nacre wifi`) and the
several-windows chooser (`desktop-actions pick`) used to be answered "cancelled" at once. They now open a real prompt.
Nacre baseline `c6dc627b`; no Nacre change is needed.

## What it is

`menuPrompt open ID` starts the on-demand app `nacre-shell --app prompt ID` (B0052): one bottom panel with the launcher's
size, look and frame attachment, its own Luau VM and no daemon topic lease. The view exists only while a prompt is open; the
process ends when the panel closes, so the always-on shell holds nothing for it (one path in `Host`, no state).
`menuPrompt cancel ID` closes the prompt that is up and never starts one. An `open` while one is still up (the previous prompt
closing in a chained `pick` then `input`, or a stale one of a killed tool; the CLI's `selector.lock` rules out two live ones)
closes it and waits up to 1 s for it to leave, else answers `busy`.

Request and answer are the QML ones, through the same helper (`nacre-helper menu-cli`):

| Step | Call | Detail |
| --- | --- | --- |
| read | `menu-cli read-request ID` | validated JSON: `kind` pick/input, `title`, `initial`, `choices[{label,value,description,icon,disabled}]`, `expires` |
| answer | `menu-cli respond ID` with one stdin line `{"value":..,"cancelled":..,"error":..}` | the helper checks the value against the choices; the CLI prints it |
| timeout | `expires` | the prompt answers `cancelled` with "Selector timed out" and closes (the CLI's own deadline and `menuPrompt cancel` also close it) |
| closed any other way | drop of the prompt state | answers `cancelled` so the tool does not wait for its timeout |

The view never sees a value: rows are `c0..cN` and `prompt.rs` maps the id back to the request, ignoring disabled rows, unknown
ids and the wrong kind. Typed text is only handed to the responder (at most 16384 chars, the responder's limit); it is never
logged or kept. QML's request has no password flag, so there is no masked mode (Bead's `masked` TextInput exists if Nacre adds one).

## Where the code is

| Part | File | Lines |
| --- | --- | --- |
| Luau view, tokens, behaviour (filter, ranking, selection, scroll) | `shell/prompt/{main,tokens,behavior}.luau`, `bundle.json` | 330 |
| Edge adapter: request, responder, panel spec | `crates/nacre-shell/src/prompt.rs` | 270 |
| Runtime hook (keys, mouse, Esc) | `reload.rs` `with_prompt` and three small blocks | 85 |
| Routing | `app.rs` (`Host::prompt`, `--app prompt`), `commands.rs` (B0068's 55-line refusal removed) | 60 |

Keyboard and mouse: type to filter (the launcher's ranking without typo tolerance), Up/Down move over enabled rows and stop at
the ends, Enter chooses, Esc cancels, click chooses; an input prompt has a field (focused, prefilled with `initial`), Cancel and
Continue buttons, Enter submits. Sizes, spacing and fonts are in `shell/prompt/tokens.luau`; nothing in Rust names a pixel.
No code is shared with the launcher's typing path or lanes (the prompt has its own view and VirtualList only), so the
launcher first-key gate is not touched.

## Known differences from QML (deliberate)

- QML refuses a prompt while the launcher, session menu, left drawer or a pinned dashboard is open ("Close the current menu or
  drawer..."); Bead's prompt is its own surface and opens above them. Revisit if the trial shows overlap.
- QML shows a no-op favourite heart on the selected row; omitted. QML's empty text for a picker reads "No matches. Try a
  different word, > for actions, or / for search scopes." (launcher wording); Bead says "No matches".
- QML queues up to two requests while one is being read; the CLI lock already makes that impossible, so there is no queue.
- No typo-tolerant matching (the launcher's edit-distance step); everything else of `matchScore` is ported.

## Temporary duplicates

None. `commands.rs::refuse_prompt` (B0068's fail-fast answer) is deleted, not kept beside the new path.

## Tests

`prompt::tests` (request parsing, responder call shape and payload, once-only answers, disabled/unknown rows, input truncation,
cancel and drop, deadline, filter reset, goldens for pick / filtered / input), `reload::prompt_runtime_tests` (typing, arrows
over a disabled row, Enter, Esc, mouse, input Enter/Cancel/Continue through the real Runtime) and
`route_coverage_tests::a_selector_prompt_routes_to_its_app_and_bad_ids_are_refused`.

## Evidence

QML (left) | Bead (right) on the owner's desktop, private nested niri 2880x1800 at scale 1.5, one fixed request per state sent by
`nacre-helper menu-cli pick --json|input` in both shells (`spikes/parity-compare.py` states `menu-pick`, `menu-pick-hover`,
`menu-input`; QML needs its launcher opened and closed once first, because the `menuPrompt` route registers with the launcher's
selector singleton: on a fresh QML session `nacre-menu pick` fails until the launcher was used, a QML quirk Bead does not have).
Files: `docs/specs/evidence/0093/*-{qml,bead,diff}.jpg`, `results.json`. Bead on its default renderer, swap 12 MiB before and after.

| State | rmse | changed % |
| --- | --- | --- |
| menu-pick | 3.558 | 3.03 |
| menu-pick-hover | 3.552 | 3.03 |
| menu-input | 3.180 | 2.75 |

The shared wallpaper keeps rmse low; inside the panel the diff shows matching positions of title, rows, hint and field, with
residual text anti-aliasing. Remaining differences (known gaps): QML's favourite heart on the selected row (a no-op there), a
scrollbar on the input form and a focus ring on Continue, which Bead does not draw; Bead's title sits about 1 px different.
Animation frame strips are in the gallery of the run (open/close of the same bottom panel as the launcher, 0.2 s ease);
not separately measured.

## Checks

Full CI of `.github/workflows/ci.yml` replicated in an Arch container on dev-2 (fmt, clippy -D warnings, workspace tests incl.
17 prompt tests, Luau analysis, spikes tests, packaging, services, deny): `FULL_CI_PASS`. Two earlier runs failed on load-flaky
tests (`bead-test sources` 50 ms markup limit, wallpaper smoke `crop_keeps_centre`), both green on rerun. New i18n surface
`prompt` added to the i18n source test; Nacre must take `tools/i18n-sync.py <nacre> --write` for the 8 `bead.prompt.*` ids.
