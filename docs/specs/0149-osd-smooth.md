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

Bead keeps the OSD surface and fill artwork fixed, and declares one host-managed scalar target for the fill clip, cap position, and bounded value label. Session commits the latest value and retargets the authored easing from the current interpolated value. Animation ticks and scalar-only publishes patch the accepted scene in place: no Taffy, full `FrontEnd.build`, Scene/Node vector clone, or mirror snapshot per value. Text storage is bounded to 256 bytes and reuses capacity. The retained scalar target is visual-only and is rejected when it would change interaction semantics. Its scene-copy cost is logged once when a scalar-target scene is accepted; asset payloads remain shared. The reusable primitive is recorded in [ADR 0004](../decisions/0004-structure-licensing-generalization.md). Nacre-side companion work is tracked in `docs/specs/0085-fast-key-path.md`.

The earlier clip-only private desktop run reached the first visible value in 7.262 ms and the remaining 15 values in 4.84–6.54 ms after request. Raw presentation cadence showed 16 >25 ms intervals in candidate and 15 in matched baseline, while the internal marker said `dropped=0`; that run did not meet the strict no-drop gate and predates the retained scalar path above. A new same-condition hold capture is required before accepting the cadence gate. Paired QML/Bead captures show a fill-strip gradient mismatch (RMSE 2.697% overall, 5.964% in bar ROI). The seven directly affected OSD goldens were updated to the reviewed candidate pixels; the gradient difference remains documented as a known Bead/QML gap. See [capture details](evidence/0149/n0085/held-key-cadence.md), [QML image](evidence/0149/n0085/volume-qml.jpg), [Bead image](evidence/0149/n0085/volume-bead.jpg), and [diff](evidence/0149/n0085/volume-diff.jpg).
