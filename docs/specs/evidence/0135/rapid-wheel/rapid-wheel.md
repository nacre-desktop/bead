# Rapid wheel retarget probe

Matched Bead CPU control e2d/9bf versus unit candidate f327/d037; three notification-popup repeats, 12 notches at measured ~20.0ms intervals, with the same flush-based pointer tool and private notification fixture. Both arms had zero UI errors, zero discarded presentation feedbacks, and source pin validation. This stresses retargeting faster than the normal 120ms wheel matrix.

| Arm | Input→visible reversal | Delivery→visible | Render p95 | Presented p95 all-gap | Max gap | Missed-slot upper bound |
|---|---:|---:|---:|---:|---:|---:|
| Control | 63.34–63.91ms | 6.60–6.89ms | 3.08–3.21ms | 60.38–114.54ms | 60.38–114.54ms | 23–36 |
| Candidate | 19.08–19.56ms | 7.55–7.58ms | 4.20–4.38ms | 24.02–24.05ms | 24.02–24.05ms | 9 |

Both traces prove that delivered direction changed, then a presented state change followed in the same notification-scroll store. Delivery-to-visible is under one requested frame, but user-input-to-visible is not: candidate is about 19ms, and the input-to-delivery segment is material. All-gap presentation intervals and missed-slot counts are retained; no physical 120Hz scanout claim is made (`refresh_ns=0`). Per-repeat measurements are in the adjacent JSON files.
