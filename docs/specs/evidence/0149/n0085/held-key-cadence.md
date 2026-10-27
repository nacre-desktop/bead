# N0085 Bead held-key OSD capture

Private desktop test machine; nested session only. Both arms used output mode 2880×1800, scale 1.5, CPU backend, mode refresh 60000 mHz (16.667 ms nominal). Bead used the task-private current Nacre helper (SHA256 `b8709c93e59ed205d9faebc0d0902087ba45b0a088a3d5cd9c7b6d67ab19e1df`); the desktop/laptop installed shell was not used.

Sixteen `audio.step` calls arrived 125 ms apart over 2 s. In candidate logs, all 16 `OSD_STEP` markers reported 2–3 ms since the supplied key timestamp. The first request was monotonic 946376401832641 ns; first corresponding `WINDOW_PRESENTED` was 946376409094668 ns, 7.262 ms later. Each of the remaining 15 requests reached its next presentation in 4.84–6.54 ms. Daemon publication plus compositor presentation therefore met the ≤16.667 ms one-frame gate in this run.

| Bead shell arm | Active presents | Intervals >25 ms during value hold | Internal FRAME_JANK marker | Longest hold interval |
| --- | ---: | ---: | --- | ---: |
| Candidate | 105 | 16 | `surface=osd dropped=0` | 42.21 ms |
| Baseline (`9d266afb`) | 105 | 15 | `surface=osd dropped=0` | 42.70 ms |

Most presentation intervals were 16.0–16.1 ms. Repeated 27–42 ms intervals fell immediately after steps 3–16, with nearly identical timing in baseline and candidate. For example, presentation update 30→33 was 36.8 ms candidate / 38.3 ms baseline, update 86→89 was 42.2 / 42.7 ms, and update 110→113 was 39.4 / 37.5 ms. The initial interval was 44.2 / 44.5 ms. There was also a 1923.32 ms interval across close; it is outside the active hold and excluded from the table.

`FRAME_JANK` is emitted by Bead `presented()` after the generation animation deadline ends; its `dropped=0` disagrees with cadence reconstructed from raw `WINDOW_PRESENTED` events. The private presentation protocol exposed `refresh_ns=0`, but output mode reports 60 Hz. Treat the strict no-drop gate as unresolved/failing until this sampler discrepancy is explained; neither arm proves 0 missed 60 Hz output intervals. The patch did not improve this cadence on the matched run.

Gallery: desktop task-private `out/parity-final2/index.html`. Paired final open-state images are archived in this directory. Pair metrics: RMSE 2.697%, changed pixels 1.527%, bar RMSE 5.964%. The visible difference is confined to the fill gradient: Bead clips a gradient painted across full track length, while QML maps its gradient to the current fill width. Goldens remain unchanged pending a parity-preserving implementation.
