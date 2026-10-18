# B0007 — UI-provider task failures return terminal replies

- ID: B0007
- Repo: nacre-desktop/bead
- Implementer: BEAD
- Status: Done
- Depends on: published89afb381; independent of B0002/capture
- Date: 2026-10-18
- Size: one focused session, maximum3hours

## Goal

A failed UI-provider task must terminate its request without hanging the caller
or reconnecting/replaying the action. Keep the daemon link usable after a handler
panics. This protects existing core-switch UI control paths independently of the
blocked performance gate.

## Context

Start latest main containing89afb381fcccc735d48edff36998c2f7ff5ae527. Read
crates/nacre-shell/src/nacre_source.rs and nacre_source/tests.rs, especially
published_ui_transcript_forwards_calls_to_an_injected_provider and the actual
provider pending-menu tests. Current handlers.join_next branch handles only
Some(Ok(...)); JoinError is silently ignored. Current handler(m,p) is invoked
before spawn, so a synchronous factory panic can also escape the session loop.
Use existing published v1 Message::Call/Ret, Failure, Id and MAX_CALLS contracts.
No Nacre protocol change or repo edit is required.

## Scope

In: nacre_source.rs, its private tests and a short existing STATUS/spec report.
Out: launcher ranking/providers, runtime optimization, dependency/schema changes,
reconnection redesign, new public framework APIs, new measurements or live work.

## Steps

1. Retain inbound request Id outside the spawned task in a session-local map
   keyed by Tokio task Id. Use JoinSet::join_next_with_id and JoinError::id to
   match both normal and failed completions. Retain existing MAX_CALLS bound;
   no detached watcher, retry loop, extra thread or new capacity.
2. Clone the existing Action Arc and invoke handler(m,p) INSIDE the spawned task,
   along with its existing timeout. Thus both synchronous factory panic and
   panic while polling its future become observable task errors. Preserve current
   timeout limit/default and normal result/error mapping without new semantics.
3. On normal completion remove task metadata and send exactly one existing Ret
   for its original request Id. On JoinError remove metadata and send one Ret::Err
   with ErrorCode::Internal, message "UI handler task failed", retry=false and
   dispatched=Some(true), conservatively meaning side effects cannot be excluded.
   Never put panic payload, params, commands, private paths or user data in
   logs/reply. Log only a sanitized failure/cancelled flag using existing logging.
   Do not re-invoke handler, reconnect, reset topics or terminate the session
   because one handler failed. If writing Ret fails, use existing disconnect path.
4. On session teardown preserve abort_all behavior and discard session metadata;
   never replay old handlers/replies after reconnect/new boot. No request Id from
   a failed old session may resolve a new-session call. Existing admission/full
   pool behavior stays unchanged; do not widen this task into its redesign.
5. Add real private socket/fake-daemon tests using existing fixtures:
   ui_provider_factory_panic_returns_terminal_error_and_keeps_link;
   ui_provider_future_panic_returns_terminal_error_and_keeps_link;
   ui_provider_cancelled_task_returns_terminal_error_without_replay;
   ui_provider_disconnect_discards_pending_task_metadata.
   First three: assert original Id, Internal/retry=false/dispatched=true, one
   invocation and one Ret, then Ping/Pong and a subsequent successful provider
   call on SAME connection. Use oneshot/barriers, no sleep synchronization.
   Cancellation may use a cfg(test)-only abort handle/channel, never public API.
   Teardown test holds a task, disconnects, reconnects with a new boot, releases
   old fixture, proves no old reply/action replay and new request succeeds.
6. Run affected nacre_source tests, fmt and nacre-shell all-target Clippy under
   nice10/Cargo-j2 in an existing independent correctness environment or hosted
   CI. No lab build overlap with Nacre's active correctness interval. Local Rust
   absence is not permission to install: use hosted CI with committed source.
   Integrate/push normally to main, inspect exact-head full CI. No package release
   or install. Product source is safe to integrate after correctness passes;
   B0002 still uses its frozen1cff runtime artifact and reviewed capture harness.

## Measurements

None. Do not claim typing/memory gains. Any later gate follows capture-safety:
peer pause, >=4GiB MemAvailable start, abort<2GiB, capped private scope and an
explicit slot. Preserve existing B0002 artifacts and captures unchanged.

## Acceptance criteria

- [x] Factory/future panic and cancellation return one sanitized terminal error.
- [x] No handler replay; link remains usable for ping and next successful call.
- [x] Normal/timeouts retain their contract; MAX_CALLS remains bounded.
- [x] Task metadata removed on completion/failure/session teardown; no old-boot reply.
- [x] Required private tests,fmt,Clippy and exact-head full CI pass.
- [x] No live action, measurement, new schema/dependency or Nacre edit.

## Stop conditions

Need protocol changes, a public API, replay, wider reconnect redesign, action
semantics changes or more than3hours: report precise gap instead of improvising.
Do not block source/tests on missing wayland-info; this task never uses it.

## General rules

Apply linked standing-rules in full. Author and committer siverteh
<92440237+Siverteh@users.noreply.github.com>; no private paths/inventory in commits.
Isolated worktree; direct normal main push, no force push, rebase only own commits,
check CI after every push. nice10/Cargo-j2/private nested tests. Never touch live
desktop/config/keybindings/autostart/systemd or installed Nacre; no publication,
permission-policy or paru flag changes. Stop only owned PIDs; never kill by name
or touch Codex/app-server. Framework permissive/publish=false; GPL nacre-shell
only; frozen backends except fixes; no engine/ranking port. Extras report-only.
Security advisories follow standing delegated exception policy; vulnerabilities
and unsoundness go to owner. No measurement grant is implied.

## Report format

B0007 Review: start/final commits, files, named tests/results, exact-head CI URL,
terminal-reply and no-replay proof, deviations and owned-process cleanup. Keep
raw private logs outside Git. If local Rust unavailable say so and cite hosted
checks honestly. Begin next eligible source task after submission.

## Review accepted — 2026-10-18

Done at736e778ad15b5f20aece9ae844c2f0ee23d6fe0f; sourceb18a4d2 plus
format-only041884a. Inspected task-ID metadata, factory invocation inside task,
normal/error mapping and teardown ownership. Four named private socket tests
reported passing; exact-head CI39125066926 independently verified green. No
local Cargo claim or capture. Existing full-pool admission behavior was outside
B0007 and remains unchanged; B0008 fixes that separate demonstrated branch.
