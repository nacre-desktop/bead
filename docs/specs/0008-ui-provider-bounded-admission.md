# B0008 — UI-provider bounded admission reports Busy

- ID: B0008
- Repo: nacre-desktop/bead
- Implementer: BEAD
- Status: Ready
- Depends on: B0007 Done at736e778a; independent of B0002/capture
- Date: 2026-10-18
- Size: one focused session, maximum90minutes

## Goal

When the UI-provider pool is full, return a retryable Busy response without
invoking an action. Currently a supported method rejected at capacity falls
through to UnknownMethod with dispatched=true. Correct that terminal response
so existing core-switch callers can distinguish overload from unknown methods.

## Context

Start latest main containing736e778ad15b5f20aece9ae844c2f0ee23d6fe0f. Read
nacre_source.rs Message::Call admission branch and nacre_source/tests.rs private
socket fixtures, including B0007 failure tests. Existing condition combines
method lookup and handlers.len()<nacre_protocol::MAX_CALLS, then uses one shared
UnknownMethod fallback. Published v1 Failure already supports Busy/retry and
optional dispatched; no schema/Nacre edit is necessary. Read standing rules.

## Scope

Only crates/nacre-shell/src/nacre_source.rs and its private tests, plus concise
existing STATUS/spec report. No new methods/queue/backoff, automatic retry,
capacity changes, renderer/typing optimization, dependency or protocol change.
No capture, build on the active Nacre lab, install or live session action.

## Steps

1. Separate method recognition from capacity. Missing provider or method not
   in its advertised map returns existing UnknownMethod/message, retry=false,
   dispatched=Some(false): no handler factory has run. Keep unregistered methods
   unavailable even while supported tasks occupy the entire pool.
2. For a registered method when handlers.len()>=MAX_CALLS, return existing
   Ret::Err to original Id: Busy, message "UI handler request limit reached",
   retry=true, dispatched=Some(false). Do not invoke factory, allocate task
   metadata or queue the call. Do not change MAX_CALLS or outgoing-call policy.
3. Otherwise use B0007 admission/task-ID tracking unchanged. Completed/failed
   tasks free capacity; no automatic replay of rejected requests. Caller may
   send a new distinct request after an entry finishes. Preserve factory/future
   panic terminal errors and disconnect cleanup from B0007.
4. Add private fake-daemon/socket tests with oneshot barriers, never sleeps:
   ui_provider_full_pool_returns_busy_without_dispatch (fill exactly MAX_CALLS
   blocked admitted handlers, next supported Id gets exact fields and no extra
   factory invocation); ui_provider_unknown_method_is_not_dispatched_at_capacity
   (unknown Id gets UnknownMethod/retry=false/dispatched=false, no invocation);
   ui_provider_completion_frees_one_admission_slot (release one task, observe its
   exact Ret, then a fresh registered call succeeds, no rejected request replay).
   Include missing-provider behavior in a parameterized existing or new test.
   Keep fixtures bounded to MAX_CALLS and close their exact owned tasks/sockets.
5. Run affected nacre_source tests,fmt,nacre-shell all-targetClippy under nice10 /
   Cargo-j2 in existing independent environment or hostedCI. No local toolchain
   install; hostedCI can validate committed code. Integrate/push normally; fix
   own failures and submit exact-head full CI. Do not edit Nacre or frozen B0002
   release binary; this runtime step is independently validated, not memory-gain
   acceptance. Existing B0007 four tests must remain passing.

## Measurements

None. This is bounded admission correctness, not a performance claim. Future
captures require explicit slot, peer pause, >=4GiB available start, abort<2GiB
and capped private scope; do not use the active Nacre capture/correctness host.

## Acceptance criteria

- [ ] Registered overload yields Busy/retry=true/dispatched=false and no action.
- [ ] Unknown/missing provider yields UnknownMethod/retry=false/dispatched=false.
- [ ] MAX_CALLS unchanged; completion frees capacity; no implicit retry/replay.
- [ ] Three named private tests plus missing-provider and B0007 regressions pass.
- [ ] fmt,Clippy and exact-head full CI green; no new schema/dependency/live action.

## Stop conditions

Published contract rejects these existing fields, tests require a new public API,
new queue/timer, or90minutes exhausted: report precise gap; do not improvise.
No prerequisite involving wayland-info is needed or may block this task.

## General rules

Apply standing-rules in full. Approved author/committer siverteh
<92440237+Siverteh@users.noreply.github.com>, no private paths/inventory. Isolated
worktree, direct normal main push, no force push, rebase only own commits, check
CI after every push. nice10/Cargo-j2/private tests. No live desktop, installed
Nacre/config/keybindings/autostart/units, publication, AI permissions or paru flag
changes. Never kill by pattern or touch others/Codex/app-server; only owned PIDs.
Framework permissive/publish=false, GPL only nacre-shell; backends frozen except
fixes; no engine/catalog/ranking port. Extras report-only. Security advisories
follow delegated exact-ID policy; vulnerability/unsoundness go to owner.

## Report format

B0008 Review: start/final commits, files, named test results, exact-head CI URL,
admission/no-dispatch counts, deviations and cleanup. Say hosted-only if local
Rust unavailable. Keep raw/private logs out of Git. No measurement claim.
