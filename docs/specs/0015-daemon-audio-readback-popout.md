# B0015 — Read-only daemon audio in the Sound popout

Status: Review — source integrated on main; exact-source hosted CI is green.
Final published-main hosted CI is pending; see delivery evidence below.
Worker: Luna; coordinator final review. Start from current published Bead main.
Goal: core mode Sound output reads the existing published audio topic, without
starting a new PipeWire/service owner or sending volume/mute actions.

Read standing rules, B0009–B0011 lease patterns, core-compat-audio-readback.jsonl,
and current sound/main.luau, status_views.rs, main.rs/popouts.rs/reload.rs.
No protocol/dependency/pin/provider changes; no Nacre edit, direct backend,
measurements, lab use, hosted CI or heavy build. Local commits only, no push.
Legacy/preview Sound remains unchanged, including existing controls.

Implementation:
1. Add typed core Audio projection using current nacre_protocol AudioState,
   AudioKind and CoreSnapshot validation. Extend Status with Kind::Audio and
   CoreAudio mode; keep existing three-kind status-popout registration list
   unchanged (document that Audio is the dedicated Sound route, not a fourth
   duplicate bar popout). Add Runtime::with_core_audio_status via existing
   single-projector/lifetime path. Cache only bounded rendered display fields.
2. Select ONLY nodes.items[default_output] with kind Output. Never select first
   node/order/default_input. Missing/invalid output remains unavailable. Sanitize
   and bound device text with existing display helpers. Health/disconnect uses
   existing last-good/stale semantics; fresh healthy snapshot replaces old data.
3. With readback Some, each optional field is authoritative independently:
   percent = rounded(perceptual_volume*100) only if finite/nonnegative; absent
   percent/mute stays unknown, never zero/false or fallback to legacy fields.
   Preserve valid externally boosted values >100%, no clamping. With readback
   None AND ready=true, legacy linear volume maps via cube root to percentage
   and legacy muted is known. Otherwise readings unknown. ready=false must not
   erase independently supplied readback fields. Validate complete snapshots.
4. Core Sound uses new read-only sound/core.luau with current right-panel
   attachment, 400x286 geometry/fallback, title/close and bounded device/volume/
   mute/health text. No slider, mute action or misleading adjustment hint in
   core mode. Legacy sound/main.luau and its direct-service path stay unchanged.
5. Build the dedicated core Sound factory AFTER shared daemon Client exists;
   acquire client.lease_topic("audio") only in factory on open, even when
   unadvertised (existing unavailable-source semantics). Core factory never
   calls with_shared_desktop or starts Desktop/PipeWire. No duplicate connection
   or fallback reads. Runtime drop retires subscription/source; factory retains
   only shared client/immutable inputs. Keep existing side placement/close wiring.

Tests (source now; execution only approved light checks, Rust/CI after heavy release):
- audio_core_uses_only_explicit_default_output
- audio_core_readback_unknown_is_not_zero_or_unmuted
- audio_core_partial_readback_and_boosted_volume
- audio_core_legacy_ready_volume_uses_perceptual_scale
- audio_core_disconnect_retains_last_good_until_recovery
- audio_core_lease_exists_only_for_open_runtime
- audio_core_unadvertised_topic_has_no_fallback
- audio_legacy_preview_remains_unchanged
Projection fixtures replay the existing nullable-readback transcript. Private
socket lifecycle waits for Sub AND daemon-backed snapshot (not initial placeholder),
answers interleaved Ping/daemon.ping, proves no pre-open Sub, drop Unsub/no callback,
no audio Ret/action/backend use and same-client Ping/Pong afterward. Missing-topic
case sends no Sub and remains unavailable. Reject wrong source/type/range data.
No real daemon/device/session. Do not claim tests passed without execution.

## Delivery evidence

Source dc8832fe16db0db39185f342a2ae9a82d62a9924 is integrated on main. The
exact-source full hosted [CI 39388972154](https://github.com/nacre-desktop/bead/actions/runs/39388972154)
passed formatting, Clippy, workspace tests, renderer configurations,
facade/headless UI/Luau schema, capture-window regressions, private service
lifecycle, and dependency/license checks. Tests cover the cases above plus
invalid snapshots and an unavailable read-only factory. The main push triggered
hosted CI 39390038309 on the same commit; it was still running when this status
was recorded. No local full build/test, desktop capture, live device access,
producer availability, or memory measurement is claimed. The original source
phase's local-only/CI hold was superseded after the F4 capture release; no
protocol, dependency, provider, or runtime-control scope was added.
