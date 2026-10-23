# B0066 - Look-and-feel parity, slice 3: panel reveal, clock format, updates count

Status: **Review** (Claude worker, branch `task/b0066-parity-3-20261023`). Owner 2026-10-23: "Bead must look correct on all fronts." Continues B0061; items below are the ones landed before the 00:30 stop.

## Fixed

| Item | QML | Bead change |
| --- | --- | --- |
| 1 Dashboard grows out of the bar | `presentedHeight` 0 to 650 with fixed content; the panel body grows, nothing slides | New general panel motion `reveal`: the scene stays at its final position and is clipped to the revealed part of its body (`Scene::reveal_window`, applied through `Scene::clip_at` in the tree, retained and legacy CPU renderers); the presenter writes the eased `Motion::progress` each frame. `reveal=true` on the dashboard panel (Luau and the cold fallback), 360 ms in-out cubic, opaque. The tab strip and content no longer travel with the body. |
| 5 Clocks | bar and dashboard follow `desktop.clockFormat` | `Prefs::clock_12h`; store `bar.clock.twelve`; bar Luau computed `bar.clock.text` ("h:MM AM/PM", wider slot) and `dashboard::clock_text`. |
| 5 Updates count | shown only when there are updates | `bar.updates.count` host store (default 0); the group is hidden at 0. Nothing publishes it yet: a bar subscription to the updates topic would add always-on cost (B0065 runs on memory), so the Nacre ask is a one-key publish of the available count when a consumer wants it. |

## Framework notes

- General primitive: panel motion `reveal` (+ runtime `progress`); add to ADR 0004's Generalization list: "panel motion: edge, duration, curve, slide_distance, fade, reveal". Not Nacre-specific.
- Known difference: while revealing, the body's bottom edge is a straight clip (the bottom corners are square during the 360 ms) and the shadow below the body is clipped; QML keeps rounded corners at the growing edge.
- The bar contour joining the dashboard (frame decorations) is unchanged.

## Not done (ranked, for the next slice)

1. Control centre full-height dock, gear, tile sub-labels, notification list; bell and tile open docked pages; Power page in the control sidebar.
2. Status popouts geometry/content and the join to the bar contour.
3. Settings window: title headers, raised section cards, sidebar size, "unavailable" caption.
4. Launcher Quick controls column (needs launcher-provider values; Nacre ask: publish dnd/night-light/stay-awake/update-count in the launcher topic).
5. Harness: pointer injection, brightness and keyboard OSD states.
6. The reveal clip has no paired QML frame measurement yet (the gallery run was not completed in this slice).

## Temporary duplicates

None.

## Checks

On dev-2 (Arch container, `--cpus=8 --memory=12g`): `cargo fmt --all --check` (fixed by the fmt commit), `cargo clippy --workspace --all-targets --locked -- -D warnings`, `tools/check-service-boundary.py`, `cargo test --workspace --locked` and `nacre-shell --check shell/ui/main.luau`: green. Memory: no new always-on allocation (one bool and one f32 per motion, one store key).
