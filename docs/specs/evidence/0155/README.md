# B0155 paired evidence

Paired crops and switch strips are under [`paired/`](paired/), captured on the private desktop test machine at 2880×1800, scale 1.5, with matching wallpaper, palette and workspace fixture. Source revisions: Nacre `871b34c26f088159a761588fee2c682ef00b1465`; Bead `687864b2bb39f13dd6618690d1dcd623e1a0dd2c`.

The 60 Hz and 120 Hz nested runs captured idle, hover, pressed, switch, add and remove. Switch sequences contain 56 samples over about 443 ms at an 8 ms synthetic scheduler cadence; no blank/dark frames or reopens were observed. This scheduler is not physical scanout timing. Pixel comparisons show hover changes in QML and Bead at 60 Hz. QML pressed feedback changes pixels; Bead pressed feedback showed no pixel change, a known interaction parity gap. At 120 Hz the saved hover/pressed endpoints show no pixel change.

The deliberate visual difference is that Bead wraps the full row in one `surfaceContainer` pill; selected workspace highlight slides as a shared capsule. QML custom-icon mode highlights the selected cell, while QML window-hint mode also uses a shared capsule.

Five alternating control/candidate memory pairs used the native GL backend. Median candidate-minus-control unreclaimable memory (Pss_Anon + Pss_Shmem + SwapPss + DRM) was −4,308 KiB (−4.21 MiB), within the declared ≤1 MiB added budget. Anonymous measurements varied by about 4.4 MiB in both control and candidate, so the negative median is not evidence of a definite saving. Pss_Shmem was 10,836 KiB; SwapPss and DRM were zero. Clean file-backed PSS is informational, not gated.
