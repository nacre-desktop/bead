# Status

2026-10-13: Retained renderer comparison complete locally. Recommend femtovg/wgpu
with content-sized retained layers; tiny-skia remains the no-GPU fallback.
**Adjust verdict:** virtual callback tails fail strict120Hz; no physical scanout
pass claimed. Owner authorized this comparison only; STOP before Feature3.
Never test on live desktop without asking first. All work here private/nested.

Built: same immutable CPU-baked RGBA panel for CPU/femtovg/Vello; position/opacity
only per frame. Direct reusable SHM fallback; GPU picture uploaded once, shared
context, Vello-only intermediate texture. Content1164×652 logical /1746×978
pixels versus full1920×1200 /2880×1800 at1.5. Same text, color emoji, image/icon,
shadow/corners/palette. Native font selection in GPL shell chooses5 files/14
faces; permissive Bead consumes paths/data and loads lazily, no C Fontconfig graph.
Old full shadow342ms→small Gaussian template12ms; error bounded2/255. An old
font-discovery786ms outlier was retained. UI source/layout/Orient engine not ported.

Formal16 cases,60s each, then4 Vello-only repeats after fixing straight-alpha
output→Wayland premultiplied transfer. All3 private static screenshots pixel-
identical after fix. Auto fallback with Vulkan disabled passes; all own processes
stopped. Measurements/scopes/problems in [FEASIBILITY](../FEASIBILITY.md).
Private Hypr virtual2880×1800/1.5/120Hz; presentation refresh0. Other active QML
loads interfered; no unrelated processes stopped. First/later private caches,
OS caches uncontrolled. CPU/DRM memory per process; PSS not total GPU memory.
First/late femtovg content startup223.29/109.99ms, QML330.85/223.74ms. Femtovg
content draw/submit p99.591/.592ms, callback11.524/10.756ms. Later PSS36.00/USS32.11,
DRMsystem121.05MiB; fullDRM162.40. CPU still misses budget, Vello expensive and
pacing poor; latest corrected tail676.60ms kept, not hidden. See full table.

Raw evidence in canonical ignored .research/retained-comparison (smoke, first-
pass, corrected-vello, private screenshots), plus worktree ignored raw files.
No generated ledger. No system packages installed. Existing native libs used.
Framework all-feature cargo-deny graphs pass;8 tests, fmt/clippy incl CPU-only
build pass locally. Runtime fallback passed; no Feature3/reload implementation.

Run from repo Wayland terminal:
`BEAD_RENDERER=femtovg ./scripts/dev.sh niri`
Nested preview only; maximize if needed. Inspect fonts/emoji/corners/shadow and
smooth slide/fade. Close window or Ctrl+C. `cpu`/`vello`/`auto` are alternatives.
[Private preview](../examples/retained-renderers.png). Default palette matches
active dark; benchmark retains frozen light Orient fixture for reproducibility.

Active isolated worktree bead-retained-renderers based on f6904e3. Publication/
CI pending: commit/integrate/push main, verify exact-head CI, then STOP for owner
renderer decision. No further feature work. Repo public; contributor/noreply
Git identity only, global untouched. References Nacre/Orient remain read-only.
