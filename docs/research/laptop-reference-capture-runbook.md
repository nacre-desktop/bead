# Bead laptop reference memory capture

The capture is a supervised, private nested-session run. The owner starts it only
after choosing an idle AC-powered window and signaling the lead. Niri is required.
Hyprland is optional and adds about 45 minutes, so run it in a separate window.

## Prepare and stage the artifacts

The artifact bundle contains the current `main` Bead release binary, the
original frozen control (`b01dfcca…`), its matching `shell/` source tree from
`1cff9d17`, and a manifest. The control was recovered from the desktop's private
B0002 archive and rehashed against its recorded identity. Keep the bundle and
manifest together. Stage the bundle contents at:

```text
~/.local/share/nacre/ai/lead/bead-laptop-reference-20261023/artifacts/
```

The candidate runs the checkout's `shell/` tree, so its binary must be built from
that exact tree. The manifest pins `candidate.shell_git_tree`
(`git rev-parse HEAD:shell`); the launcher refuses a candidate built from a
different tree. (Run 4 stopped because a binary from Bead `9ce4209` ran a newer
`shell/` tree: `UI_ERROR bar/main.luau:14 ... nil value`, then a bar-log timeout.
The repeated `SERVICE_AUDIO disconnected` lines were not the cause: both arms run
with `PIPEWIRE_REMOTE=bead-private-unavailable`, so audio is equally absent.)
The first-presentation wait also aborts immediately on any `UI_ERROR`.
The helper examples (`private-pointer`, `tray-fixture`, `notification-client`)
are staged under `artifacts/examples/`; copy them to `<repo>/target/release/examples/`
if the checkout lacks them. The 2026-10-23 candidate was built from `fdd5cc1`
with Rust 1.99.0, `cargo build -p nacre-shell --release --locked`.

The checkout's `shell/tests/fixtures/dashboard` tree must match the fixture-tree
SHA-256 in the manifest. The launcher verifies both UI binaries and that tree
before creating capture evidence.

## Owner and lead steps

The default production-backend batch takes **about 45 minutes** for the required
three alternating Niri pairs (A=frozen-source control, B=current main candidate).
The lead runs this exact command from the Bead repository root after the owner
signals:

```sh
BEAD_OWNER_SIGNAL=1 nice -n 10 python3 spikes/laptop-reference-capture.py --include-cpu --window-minutes 90
```

This runs the production-default batch (about 45 minutes) and, with
`--include-cpu`, an additional CPU-forced policy-control batch (about 45
minutes; 90 total). Both resolve to CPU in this capture, so the second batch
checks the renderer selection explicitly rather than testing a distinct
backend. The wrapper prints the combined estimate before preflight and stores
evidence under `production/` and `cpu/`. Although nacre-shell enables
`native-gl` by default and `auto` can select Femtovg GL, this measured command
is `--bar --core-views --renderer auto`: `--bar` auto-fallback selects CPU for
the main bar, and the core dashboard, launcher and popout constructors select
`RendererKind::Cpu`. The GL companion path is not launched. Each sample records
the exact UI PID's command line and mapped GPU libraries plus Bead renderer log
markers. Never-opened means the panel backend is inactive; unknown backend on
any active surface invalidates the arm and blocks an absolute verdict.

To run the default production-only batch, omit `--include-cpu` and allow about
45 minutes:

```sh
BEAD_OWNER_SIGNAL=1 nice -n 10 python3 spikes/laptop-reference-capture.py --window-minutes 45
```

The i915 DRM total includes `drm-total-*` where reported and falls
back to `drm-resident-*`/legacy fdinfo regions; the absolute total is exactly
PSS + per-client DRM + SwapPss. The summary also records `wallpaper_shm` as
inactive (0 MiB) only when no `WALLPAPER_SURFACE_COMMITTED` marker appears in
the measured UI logs; the private nested `spikes/niri.kdl` does not configure a
wallpaper surface. If Bead commits one, the summary marks its size unknown and
blocks the absolute verdict because B0021's niri-held SHM is separate from the
UI PID measurement.

The command prints its runtime estimate before preflight. It refuses if the
one-minute load average is at least 1, a build/capture scope is active, AC is
absent, or swap cannot be brought below 64 MiB under the guarded-reset rule. A
reset is considered only when `MemAvailable > swap_used + 8 GiB`; it logs
`free -h` before and after and verifies the original zram capacity. Every arm
rechecks the host gates. UI capture runs in its own XDG runtime and nested
compositor under a `systemd-run --user` scope with `memory.max=12 GiB` and
`memory.swap.max=0`; it does not connect to the owner's compositor socket.

The default is Niri only. An optional Hyprland batch adds about 45 minutes and
requires a separately chosen, roughly 90-minute idle window:

```sh
BEAD_OWNER_SIGNAL=1 nice -n 10 python3 spikes/laptop-reference-capture.py --compositors niri hyprland --window-minutes 90
```

The kit rejects a Hyprland-only run.

## Verdict and interruption

Every arm needs zero scope OOM deltas, final cgroup counters read while held,
host swap-in below 1 MiB, zero UI SwapPss, full settled windows, valid output
geometry, and closes within that arm's bar + 5 MiB. Laptop samples also require
per-client DRM fdinfo. The sampler records the driver's `drm-pdev` identity and
`drm-total-*`, `drm-resident-*`, or legacy `drm-memory-*` regions. The laptop GPU
identity was not inspected during preparation because laptop execution is
forbidden; absent or unsupported fdinfo produces **INCOMPLETE**, never zero DRM
or an absolute PASS. Field semantics follow the [Linux DRM client usage
stats](https://docs.kernel.org/next/gpu/drm-usage-stats.html) documentation.

The summary reports absolute candidate PSS + DRM + SwapPss, per-phase candidate
minus control medians and pair deltas, anonymous/file/shared deltas, close rails,
and the 25 MiB ceiling. A settled candidate sample at 24.5 MiB stops the run;
unexplained anonymous growth above 1 MiB stops the verdict. Ctrl-C stops only
the task-owned capture scope and preserves partial evidence as invalid. Raw
evidence and `summary.json` stay in the private output directory; a partial run
must be restarted into a new output directory.

The desktop rehearsal is separate and labeled `REHEARSAL ONLY`; its numbers do
not contribute to the laptop verdict.
