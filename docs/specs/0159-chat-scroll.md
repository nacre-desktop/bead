# B0159 — AI chat scrolling

Both chat views now use the shared `kit.Transcript` scroller. Message cells are created only for the visible window plus overscan; measured Markdown and text shaping stay cached per row. Appending a streamed message preserves existing row cells. Scroll/input diagnostics identify the surface as `ai-chat`.

## Acceptance evidence

| Desktop capture (2880×1800, scale 1.5; detached pearl; requested 120 Hz) | Before control | Candidate | Gate |
| --- | ---: | ---: | --- |
| GL reversal, delivery→visible | median 31.97 ms, max 39.97 ms (3 repeats) | median 15.96 ms, 15.95–15.96 ms (2 valid repeats) | ≤8.33 ms: fail |
| CPU reversal, delivery→visible | median 40.01 ms, max 40.01 ms (3 repeats) | median 23.93 ms, 15.93–31.93 ms (2 valid repeats) | ≤8.33 ms: fail |
| GL render p99 / presented interval p99 | 3.44 / 16.31 ms | 3.17 / 16.05 ms | no dropped frames: fail |
| CPU render p99 / presented interval p99 | 3.34 / 16.31 ms | 3.24 / 16.06 ms | no dropped frames: fail |

Detached GL/CPU evidence is capture `B0159-r11` (control) and `B0159-r13` (candidate). These captures use the private synthetic provider's maximum 64 messages (32 user/assistant turns), not 500. The nested compositor presented at about 16 ms despite a requested 120 Hz; this is not physical scanout evidence. Valid candidate repeats had upper-bound presented-drop counts of 39.5 GL / 58 CPU versus 80 GL / 80 CPU control medians. The attached/docked route capture `B0159-direct-gl-r3` produced three control repeats but no candidate window because the route failed to open; those candidate data are invalid. The momentum click harness `B0159-hit-r2` also failed before measurement because it could not obtain a fresh chat state snapshot. No hit-test pass is claimed.

The published `ai.chat` schema caps snapshots at 64 messages. A 500-row Markdown/code fixture (Rust block every 10 rows) exercises `Transcript` and chat virtualization in unit tests; the UI cannot receive that many rows until the protocol limit changes. The component test confirms fewer than 100 scene cells materialize at either end. Streaming-layout regression tests verify unchanged message cells are reused. These checks do not substitute for desktop 500-row frame timing.

## Implementation and remaining work

`Transcript::layout` measures rows without constructing scene cells for the full history; `Transcript::window` builds cells only for visible rows plus overscan and retires off-window cells while retaining measured Markdown. Chat snapshots are shared through `Arc` to avoid deep clones during scroll projection. No rendered layout was intentionally changed. QML parity screenshots and first-press hit-testing during momentum remain unverified; the capture failures above are retained as open acceptance gaps. No temporary duplicate or expiry is introduced.
