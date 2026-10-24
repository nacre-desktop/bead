# B0076 — Docked device detail pages

Status: **Review**. Completes B0067’s docked Wi-Fi, Bluetooth, battery and audio
pages, unsupported-tile filtering and unsaved catalog order. B0074’s landed
admission/reveal changes are retained; launcher and Nacre sources are unchanged.

## Design

The control dock hosts the existing popout Luau builders with a Back header,
its Power-page width/framed viewport and scroll policy. Standalone bar entry
points call those same builders. Battery hides the standalone summary and adds
supported brightness controls. All four level tiles have their QML detail route.
There is no second surface, Runtime, provider or action engine. The shell
composes existing service adapters; leases, flow state and modules retire with
the control view after closed+idle. Back, command navigation and dismissal clear
secrets and cancel forms/pairing; Escape leaves a form, then the page, then dock.
Focus targets are Luau metadata, so rearranging the view needs no Rust change.

General primitives: `bundle.json` explicitly lists root-relative shared Luau
modules in a view’s validated reload snapshot; `kit.SliderTile.on_detail` adds an
optional detail button. ADR 0004 records both. Shared-module reload failure keeps
the previous scene and reports the source error. No dependency was added.
Default order follows QML’s complete catalog, including mic volume before
brightness. Unsupported hardware/action tiles disappear; supported unavailable
tiles keep their explanatory state. Existing stored order is preserved.

## Validation

Remote Arch-container CI-equivalent (Rust 1.99.1): fmt, clippy with warnings
denied, all workspace tests, trace memory-policy tests, renderer feature checks,
facade/headless checks, generated Luau schema/analyzer, spike and packaging
suites, wallpaper smoke, private services/tray, and root/independent framework
cargo-deny graphs. The focused control suite has 23 tests, including actual
fake-daemon Wi-Fi/Bluetooth/profile/audio calls, command/dismissal secret cleanup,
all topic leases retiring, default ordering and unsupported tiles.
Goldens at scales 1 and 1.5 were regenerated and inspected.
Container additions: DejaVu/IBM Plex fonts, swaybg and wlr-randr; Sway’s file
capability was removed inside the disposable container. No host package or
live desktop/configuration change was made.

## Parity evidence

Final measurements and pairs are recorded in [evidence](evidence/0076/metrics.json).

| Page | Whole-image RMSE % | Dock-crop RMSE % |
| --- | ---: | ---: |
| Wi-Fi | 4.735 | 6.663 |
| Bluetooth | 5.133 | 7.797 |
| Battery | 4.515 | 5.991 |
| Audio | 4.786 | 6.811 |

Bar popout RMSE remains 6.193/5.234/4.852/5.478% (Wi-Fi/Bluetooth/battery/audio);
idle is 3.867%. Accepted arms kept host swap at 13 MiB, process SwapPss at zero
and passed the guarded scope/geometry gates; no swap reset was needed.
QML is upstream `16436a343e3f20f9bf990a1abf6571ecb82fd44f`. Both arms use private
nested niri, 2880×1800 at scale 1.5, a CPU Bead release build (no native GL),
QML’s default Qt renderer, shared wallpaper/palette
and settings, the shared capture lock and unchanged containment/swap gates.
The OSD-only fake light descriptors are now excluded from unrelated captures;
otherwise QML alone gained brightness devices on its battery/overview pages.
Bar popouts, idle, overview, hover and short press are captured alongside details.
Paired JPEGs are QML left / Bead right; metrics include whole-image and dock crops.
The complete gallery is private at `parity-gallery/b0076/`.

## Remaining differences and limits

Qt/cosmic-text weight, glyph baselines, wrapping and row density still differ.
Battery profile glyphs remain toward the tile’s left rather than QML’s center.
The command-opened Bead Back button shows keyboard focus where QML leaves the
capture pointer-owned. Existing lip direction/bar glyph/count differences remain.
Wi-Fi Scan stays disabled and service wording differs. QML defaults to Balanced
when profile data is unavailable; Bead waits for authoritative daemon data.
Brightness uses existing source rows rather than QML’s per-monitor display labels.
Audio keeps 5% kit steps, status-only streams and no input peak meter (B0073).
These are explicit residual parity/capability gaps, not new daemon contracts.
No relative PSS comparison or laptop-cap acceptance is claimed: retirement is
covered by lease-lifecycle and memory-policy tests; no idle service/cache was added.
Temporary duplicates: none; standalone files are entry points to shared builders.
