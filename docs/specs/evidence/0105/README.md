# B0105 evidence

The scheduling experiment was rejected: CPU pointer latency remains above the target,
and one closed-memory pair has an unexplained +1.516 MiB anonymous increase. The native-GL
candidate arm was stopped before execution. Runtime scheduling changes are preserved only
in [the rejected patch](callback-replacement.rejected.patch).

[All sanitized measurements](measurements.json) contain matched input/frame samples,
per-stage distributions, source/binary hashes, coverage, buffer/callback markers, guarded
capture results, three alternating Home pairs and the parent-refresh diagnostic.
The frozen Bead base/frontend is `ba2642df`; QML is published `ab04e938`.
These are private nested measurements, not physical scanout or current-main performance claims.

Each paired image has QML above experimental Bead, derived from full 2880×1800 captures:

- [Idle](idle.jpg), [open](control-center.jpg), [hover](control-hover.jpg), [pressed](control-pressed.jpg).
- [Audio](control-audio.jpg), [battery](control-battery.jpg), [network](control-network.jpg), [Bluetooth](control-bluetooth.jpg).
- [Opening](animation-open.jpg) and [closing](animation-close.jpg): QML, Bead control, then rejected Bead candidate.

Image sequences are sampled approximately every 25 ms with screenshot latency. They show
right-edge reveal and differing fade/easing; they do not establish exact input→animation times.
The source-pinned comparison retains B0103's visual gaps: tile/label geometry and wrapping,
unsupported fixture controls appearing disabled only in Bead, action-icon/text placement,
and bar font/glyph/clock differences. QML's header/body and button typography remain different.
The experiment does not change visuals. B0101/B0102/B0104 updates landed after this frozen comparison.

The pointer tool is the existing nominal-250 Hz `spikes/virtual-pointer.c`. Bead input is
joined by coordinate, worker state, generation/update, commit and presentation. QML uses
`BEAD_QML_TRACE_POINTER=1` with the preload in `spikes/qml-presentation.cpp`. Exact frame IDs
join synchronized positive hover-feedback opacity or slider visual position to feedback.
The linear drag's coordinate translation is calibrated independently using hover samples.
The pinned QML slider has an 8 px handle and 1% action step. Coalesced events are not zeros.

Build the Qt probe with Qt 6 Quick/Gui and private QPA headers plus wayland-protocols:
generate the presentation client header/private code with `wayland-scanner`, run Qt's `moc`,
compile generated protocol code with **C** (`cc -fPIC -c`), then link that object and the
probe with `g++ -std=c++17 -fPIC -shared`. Compiling the protocol as C++ gives internal linkage
to its const interface objects. `ldd -r`, an actual preload load, and the final QML capture pass.
Plain `ldd` alone missed the original unresolved symbol. Probe traversal/logging cost is included.

The rejected engine passed the 39-test Wayland suite and full `.github/workflows/ci.yml`
equivalent in a remote Arch container, Rust 1.99.0, Cargo `-j2`, nice 10. The integrated
probe/docs source passes the full workflow on `e2f17a4` plus the final probe overlay: 568 shell
tests and four trace memory tests pass, along with all other workflow stages. Full log SHA256:
`8c059fb6733abd3bd014bdc1647c8ceb1c173b529f3385116d2947de6e84fa2b`. Logs remain private.
The initial CI omitted Git metadata; a source-matched validation snapshot fixed capture tests.
A later archived-source run reused a newer cached test binary; source mtimes were refreshed
before rerunning. These failed attempts remain recorded privately. The initial baseline build
briefly ran at nice 0, then exact owned compiler PIDs were corrected to nice 10; later builds
used nice 10 throughout. Its Cargo build succeeded, while a post-build helper step failed;
the built binary was copied and hashed separately.

Private raw logs, full smaps and original captures are retained for review. Harness snapshots
expire seven days after review. No live desktop or installed software was changed.
