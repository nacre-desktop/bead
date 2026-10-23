# B0043 - AI sidebar chat

Status: **Review** (Claude worker, branch `task/b0043-ai-sidebar-20261023`). Owner decision 2026-10-23: no QML,
go fully Bead/Rust; Bead is a framework, so the view is Luau on kit components and Nacre ids stay in one adapter.
Reference: QML `LeftDrawer.qml`, `ChatPane.qml`, `NacreChatSession.qml`; contract N0032 `nacre_protocol::ai_chat` (pin `4d816ca2`).

## Structure (who owns what)

| Layer | Piece | Notes |
| --- | --- | --- |
| Framework (MIT/Apache) | `bead_ui::transcript` | Messages -> absolutely positioned cells (fill/text/button), Markdown blocks, 1500 B text groups, per-message layout cache, window of visible cells. Sizes/spacing/radii from `size-scale.json`. Knows nothing of any assistant. |
| Framework | `bead_ui::markdown` | Paragraph, heading, ordered/bullet list, quote, rule, fenced code; inline marks flattened (a text box has one style). |
| Framework | `bead_render::{text_lines,text_caret,text_vertical,text_selection}` | Same shaper as painting, for sizing and multi-line carets. |
| Framework | `TextInput multiline`, `kit.Transcript`, `kit.Composer` | Shift+Enter newline, Enter activates, Up/Down by drawn line, wrapped caret and per-line selection. |
| Adapter (GPL) | `nacre-shell/src/chat.rs` | The only file naming `ai.chat`: topic -> `Entry` list, calls, approval rows, heartbeat, geometry stores. |
| View | `shell/chat/{main,behavior}.luau` | Panel composed from kit components and `kit.tokens`; geometry numbers are projected from the scale, no pixel literals. |

## Hosting (B0052)

The sidebar is an on-demand app process (`nacre-shell --app chat`), a layer-shell left-edge panel with the same placement
(`Frame::positioner(Attached(Left))`) and view; the shell only routes `nacre left` / `drawers toggle left`. Everything
below that says "closed panel drops X" is now "the process exits".

## Behavior

- Open: `nacre left` and `drawers toggle left` toggle the `chat` panel (left edge, 470 wide, at most 810 high and 150 short of the output). Esc closes.
- Chat tab: New chat, Workspace (disabled while busy / without a thread), `Provider · title`, transcript, status/error line, approval card, composer, Send, Stop (while busy), Latest.
- Transcript: user text plain, assistant text Markdown; `Copy` per message and `Copy code` per closed fence (clipboard via the shell). Follows the newest text until the reader scrolls up; near the bottom (48 px) or Latest resumes. Only cells within one overscan of the viewport exist (virtualization); streaming re-measures only the last 1500 B group.
- Composer: multi-line, IME through the existing text input, Enter sends the exact text (`ai.chat.send`, no attachments), Shift+Enter newline, draft clears when the call is queued. `send_failed` events put the text back before what was typed since. Draft and tab survive a closed panel in process memory only (never on disk).
- Approval / question: the `state.question` card as rows (prompt, option buttons, answer field, Reply), up to 4 questions; Reply needs every answer (`ai.chat.approve`).
- Settings tab: default provider (`ai.chat.provider`), New chat. Chats tab: saved chats, see ask A.
- Daemon: `ai.chat.open` on open, refreshed every 10 s while shown and after a reconnect (QML heartbeat); every call once, never retried, enabled only when advertised at v1. Closed: no heartbeat, no layout cache, fonts released, lease dropped with the panel at the settled-close gate.

## Contract asks (Nacre side, not published)

- **A. Saved chats.** QML lists them through `nacre-helper desktop-extras chats` (`chat_list`). Needed: method `ai.chat.saved {channel}` v1 -> `{chats:[{key,id,title,agent,account,state}]}` (at most 30, `key` accepted by `ai.chat.resume`). Bead already consumes it, gated on the advertised method; without it the tab says so. Not implemented in Nacre (N0041 reserved; daemon method, schema, fixtures and tests were not small).
- **B. Attachments.** File picker / drop / screenshot attach (`composer pick|files|screenshot`) have no daemon contract; Bead sends `attachments: []`.
- **C. Brain tab** (`DesktopExtras brain/capture/note`) has no ai.chat contract; outside this unit.
- **D. i18n.** 29 new `bead.panel.chat-*` ids; Nacre's catalog needs `tools/i18n-sync.py NACRE --write` in the N0041 worktree.

## Parity left

Attachments (B), Brain tab (C), saved chats (A), workspace-window and parked-chat lists, pin (keep open on outside click), "Accounts and usage" shortcut, drafts persisted on disk, inline emphasis/links styling (flattened to text; link text shows `text (url)`), table rendering, composer text beyond about 240 px is clipped (no inner scroll), mouse caret placement and text selection in the transcript.

## Gates

| Gate | Result |
| --- | --- |
| Streaming 150 tokens at 50 tok/s cadence, project + Luau + layout + CPU render per frame, release build, dev-2 | p50 3.76 ms, p99 4.43 ms (budget 16.7 ms). In-process CPU renderer, not compositor presentation. |
| Memory vs control, nested smoke | Not run: no nested session or capture was taken (no desktop lock requested); backend would be CPU. |
| Idle/closed | Hidden panel returns before any work, drops caches, fonts and the heartbeat thread. |

## Validation

`chat_tests.rs`: streaming keeps earlier cells byte-identical and follows the bottom; reading away / Latest; Markdown and copy actions; approval rows; composer growth; error status; fake daemon for open, heartbeat, send, stop, approve, provider, refused call, `send_failed` restore, reconnect resync, saved list and resume; eight goldens (empty, conversation, code block, approval at 1 and 1.5). `chat_runtime_tests.rs`: typing, Shift+Enter, Enter, Esc on the real template. Framework tests: `bead_render` wrapped text, `bead_ui` multiline input and Markdown.

## Temporary duplicates

None added. The QML drawer stays until the switch is trusted (Nacre rule: delete it a week after); no Python helper was ported.
