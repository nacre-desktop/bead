# B0149: Smooth held-key OSD

Status: **Shipped (2026-10-26); shadow-raster follow-up recorded.** Nacre routes volume and brightness repeats through daemon-backed steps with existing-tool fallback. Bead updates the level text immediately and retargets the fill over 130 ms. In the nested synthetic capture, all changed volume values reached an overlay commit within one 60 Hz software frame. Brightness steps reached a commit in 8.3 ms median; the first cross-kind shadow raster took 131.6 ms and remains a follow-up.

| Measure | Candidate | Acceptance |
| --- | --- | --- |
| Volume: publish → first overlay commit | p50 10.8 ms; max 12.8 ms; 16 changed values | ≤ 16.7 ms |
| Brightness: publish → first overlay commit | p50 8.3 ms; max 131.6 ms; 17/20 within 16.7 ms | first-frame shadow cost tracked as follow-up |
| Held values | volume and brightness marker values monotonic; 128 samples over 2.04 s per sequence | monotonic; no jump larger than one step |
| Fill behavior | 130 ms out-cubic retarget; percent text follows latest value | no queued steps; text immediate |

## Scope and proof

- Replace the Nacre volume key's per-repeat `sh` + `wpctl` spawn with a daemon step call; brightness uses the existing daemon adjustment route. Preserve fallback commands if daemon IPC fails.
- Add a rate-limited `OSD_STEP` journal marker with kind, value and ms since key input for live verification.
- Animate only the Bead OSD level fill; keep percent text synchronized to the latest published value.
- Add both-repo tests and capture repeated-key timing plus matched QML/Bead OSD states in a private nested session. The parity preview fixture left the Bead pill blank; held-key final screenshots show the actual pill. In those pairs Bead says “Default output” where QML says “Fixture speakers”; this label detail remains a parity gap. The OSD is passive, so pointer hover/press has no intended state change.
- Brightness comparison: the legacy QML sysfs watcher updates its local level on file change but does not emit the `adjusted` signal or a daemon OSD event for an external `brightnessctl` write. The daemon route adds the optimistic OSD step and is at least as good for key feedback; the measured shadow bake is a renderer follow-up, not a reason to retain the old binding.
- Temporary compatibility: QML keeps the legacy light-level watcher beside daemon OSD events while daemon-client mode remains opt-in; remove the watcher 30 days after daemon-client mode becomes the trusted default.

## Evidence

The full remote Bead and Nacre desktop CI jobs passed. A private nested capture used a 2880×1800 output at scale 1.5 and synthetic 60 Hz scheduling; it did not establish physical key-repeat or scanout timing. Owner approved shipping the brightness route against the non-notifying legacy `brightnessctl` path. Follow up on the first cross-kind shadow raster (131.6 ms) and two near-clamp updates (19.3 and 25.1 ms); 17/20 changed brightness values committed within 16.7 ms.

- [Idle, QML and Bead](evidence/0149/idle.jpg)
- [Volume held final, QML and Bead](evidence/0149/osd-volume-held-final.jpg)
- [Brightness held final, QML and Bead](evidence/0149/osd-brightness-held-final.jpg)
- [All recorded step values and key ages](evidence/0149/held-key-steps.csv)
- [Bead daemon publish to next overlay commit](evidence/0149/bead-publish-to-commit.csv)
- [Preview-state pairs and pointer probes](evidence/0149/osd-volume.jpg), [brightness](evidence/0149/osd-brightness.jpg)


## N0085 retained-fill follow-up (2026-10-27)

Bead keeps the OSD surface and fill artwork fixed, and declares one host-managed scalar target for the fill clip, cap position, and bounded value label. Session commits the latest value and retargets the authored easing from the current interpolated value. Animation ticks and scalar-only publishes patch the accepted scene in place without Taffy, full `FrontEnd.build`, Scene/Node vector cloning, or mirror snapshots per value. Text storage is bounded to 256 bytes and reuses capacity. The retained scalar target is visual-only and is rejected when it would change interaction semantics. The reusable primitive is recorded in [ADR 0004](../decisions/0004-structure-licensing-generalization.md). Nacre-side companion work is tracked in `docs/specs/0085-fast-key-path.md`.

**Acceptance remains open for label allocation and zero-drop cadence.** The bounded `String` does not imply a zero-allocation label update. `CpuRenderer::prepare` keys text rasters by rendered text and creates a new `Pixmap` for each distinct label. The OSD value box is 64×19 logical px; at scale 1.5 each raster is 96×29×4 = 11,136 bytes, or 178,176 bytes for 16 distinct labels, plus key/cache metadata. Animation ticks carry no label and do not rasterize text. Safe reuse needs exclusive ownership of the mutable pixels; mutating a shared `Arc<Pixmap>` is not valid. The retained frame update and damage path is implemented and its focused tests pass, but the label raster remains a per-value allocation.

The exact OSD shadow cache retains 1,774,080 RGBA bytes (1.693 MiB): brightness and volume at the 594 px 1.5x raster width, plus the 792 px 2x volume fallback prepared before Wayland reports output scale. A paired CPU reference measured +1,604 KiB Pss_Anon versus the matched no-warm control; Pss_Shmem, SwapPss, and DRM were unchanged. This is within the lead-approved 6 MiB total incremental budget for the hidden OSD Runtime, prepared scene, and shadow variants. The retained overlay is warmed at output registration, stays unmapped with empty input and no SHM buffers while hidden, and is reused on first offer. This is a scale-aware three-shape cache.

Final paired 2 s hold capture used CPU rendering, a private 2880×1800 desktop at scale 1.5, 60 Hz, and the task-private Nacre helper. Output registration warmed the hidden OSD before first offer; smaps markers showed +1.6 MiB Pss_Anon in the matched control comparison, with 0 added shmem/DRM. First published value became visible 9.47 ms after the daemon key timestamp; all 16 publish-to-first-presentation deltas were 0.18–15.36 ms (each ≤16.67 ms). `PANEL_OPEN_LATENCY` was 4.010 ms from offer to first content. The frame gate still fails: one active presented interval was 32.07 ms (all other active intervals ≤16.38 ms); the capture summary counted 9 commit-gap estimates and 8 presentation-gap upper bounds, including idle boundaries. No presentation feedback was discarded. The remaining active gap follows an animation tick that committed close to the compositor's refresh deadline; immediate scalar-patch submission fixed all value visibility deltas but did not eliminate this animation-frame miss.

Full Bead CI passed on dev-2 in `n0085-bead-ci-20261027`. The private clean-source validation commit was `5c04273b2352515ec246b305cb21bce216377f31`; its `crates/`, `shell/`, `vendor/`, and `Cargo.lock` tree IDs match local Bead commit `e3bbd555`. The final verification printed `FULL_CI_PASS`, including 10 cargo-deny checks and all Python/spikes tests. The full log remains at `/tmp/n0085-bead-full-ci-final.log` in that container.

Paired QML/Bead captures continue to show a fill-strip gradient mismatch (RMSE 2.697% overall, 5.964% in the bar ROI); it remains a known Bead/QML gap. The seven directly affected OSD goldens were updated to the reviewed candidate pixels. See [capture details](evidence/0149/n0085/held-key-cadence.md), [QML image](evidence/0149/n0085/volume-qml.jpg), [Bead image](evidence/0149/n0085/volume-bead.jpg), and [diff](evidence/0149/n0085/volume-diff.jpg).
