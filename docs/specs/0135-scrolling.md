# B0135 — shared scrolling

Status: implementation and private validation in progress. No live installation.
The updater bounds row construction to its viewport; notification cards and
Settings previously laid out ordinary repeated trees. All surfaces already route
through one Wayland handler, but it ignored axis source/stop, amplified finger
pixels threefold, inferred release after 65 ms, and carried old wheel travel
through direction reversal. The worker queued each scroll sample separately.
Native app windows additionally repainted and damaged their entire surface for
every offset update, independently of the panel renderer's viewport damage.

## Scope and acceptance

Use one source-aware policy: direct finger pixels, fling on finger axis_stop,
125 ms retargeted wheel ease-out, value120 precedence and immediate cancellation
of old motion on new input. Preserve action gestures (volume/workspace/tray).
No slider changes. General primitives remain independent of shell routes.
Offset-only scrolling must reuse layout; long lists must bound row construction.
Target reversal ≤ one frame, p95 frame cost ≤8 ms at synthetic 120 Hz and zero
missed cadence slots. Nested scheduling is not physical scanout certification.
Memory budget: no new always-on cache budget; reuse existing byte-accounted
materialized and renderer caches. Report measured cost, never substitute zeros.

## Container inventory

| Surface | Implementation |
| --- | --- |
| Notifications | shared Scroll; variable-height VirtualList replaces full Repeat |
| Settings content / side tabs / selects | shared Scroll / kit.Select |
| Launcher results / apps | kit.VirtualGrid |
| Updates / clipboard / keybindings / picker | shell VirtualList |
| Widgets editor | replacement app unavailable; no reachable scroll surface |
| Welcome | shared Scroll canvas |
| Colors | shared Scroll rail and canvas |
| AI chat | kit.Transcript window; shared Scroll questions and saved chats |
| Control center / network / tray | shared Scroll |
| Overview / wallpaper | shared Scroll; gallery already windows tiles |
| Dashboard weather and artwork crops | shared Scroll (crops are bounded artwork) |
| Kit demo / layout example | shared Scroll |

The common input policy lives in bead-wayland/scroll.rs, not in per-view
animations. The variable-height option extends the existing shell VirtualList;
no second scrolling engine or retained legacy replacement is added. Same-direction
content samples coalesce in the worker; reversals remain ordered because summing
opposite travel changes clamping at the top/bottom edge. Action gestures retain
every physical input. The materialized cache updates descendant poses and nested
viewport clips, retaining offscreen scene descriptions inside the existing byte
budget. Virtual windows relayout only when their row set changes. Native windows
use the renderer's semantic damage plus independent repaint history per SHM buffer.

## Evidence

Pending: full remote CI, regenerated/inspected headless goldens, paired private
QML/Bead states, before/after per-surface render/cadence and reversal timing.
Measurements must distinguish input dispatch from visible direction change;
request-to-next-commit alone does not establish the reversal gate.
