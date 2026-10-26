# B0150 — OS-wide smoothness audit

Audit date: 2026-10-26. Marker commit shipped before capture; remote Arch CI passed.
Nested Niri captures used 2880×1800, scale 1.5, owner wallpaper/icon/application data and 128 synthetic notification rows.
Both arms stopped at the same settings-open fixture error (`'result'`); later interactions are unavailable.
The parity harness samples presentation every 30 ms, so dropped-frame counts and physical scanout are unavailable.

| Rank | Interaction | Measured numbers (CPU / GL) | Target | Suspected cause / pointer | Proposed one-worker follow-up |
|---:|---|---|---|---|---|
| 1 | Launcher after expiry / retained memory | Post-expiry closed Pss_Anon 501.5 / 499.5 MiB versus initial closed 38.7 / 36.4 MiB; one arm each, attribution unknown. | First content ≤1 refresh; smooth 60/120 Hz; input ≤8 ms; no drops. | Worker/cache retention or capture-process growth; `crates/nacre-shell/src/reload.rs`, `crates/nacre-shell/src/launcher/`. | Re-run five matched open/close/expiry samples with per-process memory attribution; fix only the confirmed retaining owner. |
| 2 | Launcher first content | 6 marker events/arm: p50/p95 612/683 ms CPU, 624/819 ms GL. Incomplete arm; not five independent cohorts. | ≤1 refresh to first content. | Launcher query, icons, or initial view construction; `crates/nacre-shell/src/launcher/`. | Trace query, icon readiness, and presented first content through a full stable cohort. |
| 3 | Launcher typing | Marker p50/p95 5,137/5,436 ms CPU; 5,160/5,271 ms GL. Values decrease through the sequence, so per-key timing is invalid. | ≤8 ms key-to-glyph. | Timestamp origin/correlation in `crates/nacre-shell/src/reload.rs` input marker. | Correct key and glyph timestamp pairing; validate against independently timestamped events. |
| 4 | Dashboard open | One sample: 917 ms CPU / 808 ms GL. | ≤1 refresh to first content. | Panel preparation; `crates/nacre-shell/src/reload.rs`. | Repeat five stable opens and correlate first presented content. |
| 5 | Control and AI panel opens | One CPU and one GL sample each completed; no reliable cohort summary. | ≤1 refresh; no drops. | Panel view creation; `crates/nacre-shell/src/reload.rs`. | Repeat and report per-panel first-content and animation results. |
| 6 | Settings, scroll, notifications, dropdowns, wallpapers, OSD | Unavailable: capture stopped at settings-open with fixture error `'result'`. | ≤1 refresh; input ≤8 ms; no drops. | Existing parity fixture contract; `spikes/picker-fixes-proof.py`. | Repair fixture shape, then capture the remaining state matrix on both backends. |

Marker commit `0159548b` adds `PANEL_OPEN_LATENCY`, `INPUT_ECHO_LATENCY`, `FRAME_JANK`, and rate-limited `SCROLL_JANK` journal markers. No `FRAME_JANK` or `SCROLL_JANK` records appeared in these partial captures. Historical wallpaper 0.4–1.6 s and CPU anon 48→201 MiB remain owner-reported, unverified observations.
