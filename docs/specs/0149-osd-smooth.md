# B0149: Smooth held-key OSD

Status: **In progress — brightness acceptance fails.** Nacre routes volume and brightness repeats through daemon-backed steps with existing-tool fallback. Bead updates the level text immediately and retargets the fill over 130 ms. In the nested synthetic capture, all changed volume values reached an overlay commit within one 60 Hz software frame; three of 20 changed brightness values did not, including a 131.6 ms first brightness frame after a new shadow raster.

| Measure | Candidate | Acceptance |
| --- | --- | --- |
| Volume: publish → first overlay commit | p50 10.8 ms; max 12.8 ms; 16 changed values | ≤ 16.7 ms |
| Brightness: publish → first overlay commit | p50 8.3 ms; max 131.6 ms; 20 changed values, 3 over 16.7 ms | ≤ 16.7 ms |
| Held values | volume and brightness marker values monotonic; 128 samples over 2.04 s per sequence | monotonic; no jump larger than one step |
| Fill behavior | 130 ms out-cubic retarget; percent text follows latest value | no queued steps; text immediate |

## Scope and proof

- Replace the Nacre volume key's per-repeat `sh` + `wpctl` spawn with a daemon step call; brightness uses the existing daemon adjustment route. Preserve fallback commands if daemon IPC fails.
- Add a rate-limited `OSD_STEP` journal marker with kind, value and ms since key input for live verification.
- Animate only the Bead OSD level fill; keep percent text synchronized to the latest published value.
- Add both-repo tests and capture repeated-key timing plus matched QML/Bead OSD states in a private nested session. The parity preview fixture left the Bead pill blank; held-key final screenshots show the actual pill. In those pairs Bead says “Default output” where QML says “Fixture speakers”; this label detail remains a parity gap. The OSD is passive, so pointer hover/press has no intended state change.
- Temporary compatibility: QML keeps the legacy light-level watcher beside daemon OSD events while daemon-client mode remains opt-in; remove the watcher 30 days after daemon-client mode becomes the trusted default.

## Evidence

The full remote Bead and Nacre desktop CI jobs passed. A private nested capture used a 2880×1800 output at scale 1.5 and synthetic 60 Hz scheduling; it did not establish physical key-repeat or scanout timing. The first cross-kind brightness update rasterized a new pill shadow and missed the one-frame target; do not push until that path is fixed and recaptured.

- [Idle, QML and Bead](evidence/0149/idle.jpg)
- [Volume held final, QML and Bead](evidence/0149/osd-volume-held-final.jpg)
- [Brightness held final, QML and Bead](evidence/0149/osd-brightness-held-final.jpg)
- [All recorded step values and key ages](evidence/0149/held-key-steps.csv)
- [Bead daemon publish to next overlay commit](evidence/0149/bead-publish-to-commit.csv)
- [Preview-state pairs and pointer probes](evidence/0149/osd-volume.jpg), [brightness](evidence/0149/osd-brightness.jpg)
