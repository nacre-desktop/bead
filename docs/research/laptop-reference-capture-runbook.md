# Bead laptop reference memory capture

The capture is a supervised, private nested-session run. The owner starts it only
after choosing an idle AC-powered window and signaling the lead. Niri is required.
Hyprland is optional and adds about 45 minutes, so run it in a separate window.

## Prepare and stage the artifacts

The artifact bundle contains the current `main` Bead release binary, the
original frozen control (`b01dfcca…`), and a manifest. The control was recovered
from the desktop's private B0002 archive and rehashed against its recorded
identity. Keep the bundle and manifest together. Stage the bundle contents at:

```text
~/.local/share/nacre/ai/lead/bead-laptop-reference-20261022/artifacts/
```

The checkout's `shell/tests/fixtures/dashboard` tree must match the fixture-tree
SHA-256 in the manifest. The launcher verifies both UI binaries and that tree
before creating capture evidence.

## Owner and lead steps

The owner leaves the laptop idle on AC for **about 45 minutes** for the required
three alternating Niri pairs (A=frozen-source control, B=current main candidate).
The lead runs this exact command from the Bead repository root after the owner
signals:

```sh
BEAD_OWNER_SIGNAL=1 nice -n 10 python3 spikes/laptop-reference-capture.py
```

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
