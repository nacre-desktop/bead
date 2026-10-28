# Bead and Nacre Shell

Bead is a Rust framework for building Wayland shells. It combines retained
rendering and layout with Luau views, stable-ID state, reusable UI components,
and atomic hot reload. The framework crates provide general Wayland, rendering,
input, service, and UI primitives; they do not contain Nacre-specific view
layout or policy.

Nacre Shell is Nacre's desktop shell built on Bead. It includes the bar,
launcher, dashboard, control panel, notifications, OSD, wallpaper picker,
standalone Widgets app (dragged between the bar and desktop), AI panel, and the
Settings, Colors, and Welcome apps. Bead is now the daily Nacre shell and is at
parity with the QML shell for the migrated desktop workflow. Some migration
work remains in progress; the current gaps and their evidence are tracked in
the [core-switch review](docs/research/core-switch-gap-20261022.md) and
[project status](docs/STATUS.md).

## How it fits Nacre

The shell uses Nacre's daemon contracts for shared state and actions. Nacre
owns shell selection and lifecycle: `nacre ui-shell bead` selects Bead, and
`nacre ui-shell qml` returns to the QML/Quickshell fallback. The shells install
side by side; installing the separate `bead-shell` package does not switch the
active shell. See the [packaging and rollback spec](docs/specs/0029-packaging-coexistence-rollback.md).

## Build and run

Build the shell from this checkout with the pinned workspace dependencies:

```sh
cargo build --release --locked -j2 -p nacre-shell
```

For an isolated interactive preview from a Wayland session, use the nested
preview script. It starts a private compositor, daemon, bus, runtime directory,
and HOME; it does not install or switch the running Nacre shell:

```sh
python3 scripts/try-bead.py --daemon /path/to/nacre-helper --bead target/release/nacre-shell
```

For the smaller framework development preview, `scripts/dev.sh` builds the
shell and starts a nested niri session by default; choose Hyprland explicitly
with `./scripts/dev.sh hyprland`. These previews require their compositor and
Wayland tools to be installed.

Nacre's Arch package recipe is in [`packaging/arch/PKGBUILD`](packaging/arch/PKGBUILD).
It builds `nacre-shell` and stages the executable and Luau shell bundle beside
the QML shell. [`packaging/stage.sh`](packaging/stage.sh) also supports staging
under a chosen prefix and `DESTDIR`; package selection remains the job of
`nacre ui-shell`.

## Architecture

- [`crates/`](crates/README.md) — Rust framework crates (`bead-wayland`,
  `bead-render`, `bead-ui`, `bead-core`, `bead-services`, and related tools).
- [`crates/nacre-shell/`](crates/nacre-shell/) — Nacre-specific adapters and
  shell runtime, kept under the GPL boundary.
- `shell/` — Luau views, reusable components, themes, and assets used by
  Nacre Shell.
- [`docs/specs/`](docs/specs/README.md) — numbered design and implementation
  specifications; start with the [spec index](docs/specs/README.md).
- [`spikes/`](spikes/) — research and measurement tools, fixtures, and proofs.
- [`docs/STATUS.md`](docs/STATUS.md) — current milestone status and open gates.
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — architecture direction and feature gates.

Views describe presentation and behavior in Luau. Rust owns reusable platform
and framework services. Nacre-specific daemon routes are adapted at the shell
edge. Reload validates a complete new view generation before atomically
switching it in; invalid changes leave the last working UI active and report a
source location and error.

## CI

The [CI workflow](.github/workflows/ci.yml) checks formatting, warnings-denied
Clippy, service boundaries, workspace tests, renderer configurations, Luau
schemas, packaging and service lifecycle proofs, and dependency/license policy.

## Contributing

Changes follow the spec-driven workflow: agree the scope and acceptance gates,
implement the smallest complete unit, record evidence and remaining gaps in its
spec and `docs/STATUS.md`, then run the relevant checks and full CI. User-visible
shell changes require matched QML/Bead review and appropriate nested-session
evidence; the live desktop is not a test environment. See [the roadmap](docs/ROADMAP.md),
[spec index](docs/specs/README.md), and repository [standing rules](AGENTS.md).

## License and distribution

- `crates/bead-*`: MIT OR Apache-2.0, with permissive dependencies only.
- `crates/nacre-shell` and `shell/`: GPL-3.0, retaining Nacre's terms and notices.
- Every crate sets `publish = false`. Do not publish crates, packages, or releases.

See [LICENSE.md](LICENSE.md) for asset and research-material boundaries and the
full license texts. Contributions must preserve these boundaries; GPL/LGPL
implementation code does not belong in framework crates.

For the detailed research and preview commands retained from the experiment
README, see [docs/PREVIEW.md](docs/PREVIEW.md).
