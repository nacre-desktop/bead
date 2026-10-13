# 04 — Service adapters and retained helpers

Research date: 2026-10-13. Candidate versions/release dates/licenses are in
[crate-versions.json](crate-versions.json); code revisions in
[source-pins.json](source-pins.json); primary API captures/hashes/failures in
[spec-sources.json](spec-sources.json). No device mutation, notification-name
takeover or global shortcut registration was performed on the daily desktop.

## D-Bus foundation

Use zbus 5.19.0 (MIT) with one shared connection per required bus and typed proxy
interfaces generated or independently declared from public specifications.
Session bus: notifications, SNI/DBusMenu and MPRIS. System bus: UPower,
NetworkManager, BlueZ and logind. Subscribe before/alongside an initial snapshot
so changes during enumeration are not lost; track name owners and restart stale
proxies when a daemon/player disappears. Worker tasks publish normalized values
into stores; never block rendering on a D-Bus roundtrip. Source inspected:
zbus/src/{connection,proxy,object_server} and its Cargo manifest.
See [zbus documentation](https://docs.rs/zbus/5.19.0/zbus/).

| Service | Implementation candidate and required behavior | Test and risk |
| --- | --- | --- |
| Notifications server | zbus interface for org.freedesktop.Notifications: capabilities/server information, Notify/replace/close, action/closed signals, hints, expiration, persistence adapter | Use private bus name only in tests; replacement IDs, app death, actions and DND; must not displace live Nacre |
| SNI tray | zbus StatusNotifierWatcher/Host, item properties/signals, registration/name ownership, pixmap or themed icons, actions, tooltip, DBusMenu tree/events | Watcher already present vs own watcher, item restarts, malformed pixmaps and menu revision changes; a “tray icon” is not complete menu support |
| MPRIS | zbus root/Player proxies and PropertiesChanged for bus names org.mpris.MediaPlayer2.*; metadata, status, controls, seek/track identity | Multiple players, player exit/restart, capability gating and seek feedback; extrapolate position rather than busy polling |
| UPower | zbus DisplayDevice and battery/device properties; enumerate devices, signal changes; preserve Nacre alert hysteresis/preferences | Plug/unplug, missing daemon, desktop without battery; charge percent and energy/time units must be normalized |
| NetworkManager | zbus manager/device/active connection/access point interfaces; retain existing network-state/helper for first parity | Hardware rfkill, unavailable daemon, rescans, connection failures and secret-agent requirements; display and connection provisioning are distinct work |
| BlueZ | zbus ObjectManager for Adapter1/Device1 plus pairing Agent1 when parity requires it | Discovery cancellation, device removal, connect/pair permissions, multiple adapters; existing helper handles actions initially |
| logind | zbus login1 Manager/Session for session/power state/actions and inhibition | PolicyKit denial is explicit; preview never suspends/logs out/locks the real session; inhibitors own FDs with defined lifetime |

Primary APIs checked/read:
[Notifications 1.3](https://specifications.freedesktop.org/notification/1.3/),
[SNI](https://specifications.freedesktop.org/status-notifier-item/latest/),
[MPRIS 2](https://specifications.freedesktop.org/mpris-spec/latest/),
[UPower](https://upower.freedesktop.org/docs/UPower.html),
[NetworkManager](https://networkmanager.dev/docs/api/latest/spec.html),
[BlueZ Adapter](https://bluez.readthedocs.io/en/latest/adapter-api/),
[logind](https://www.freedesktop.org/software/systemd/man/latest/org.freedesktop.login1.html).
Live hardware/system service behavior remains unverified. Latest API pages often
do not expose a dated version; capture date/hash is the evidence, not an invented
release number. Exact fetch failures are retained in spec-sources.

Do not add a GPL/LGPL convenience crate merely because its public interface is
convenient. Reuse a permissive zbus proxy crate only after audit; a few small
typed interface declarations are preferable to an opaque unaudited daemon stack.

## PipeWire

pipewire 0.10.1 Rust bindings are MIT. Its source/manifest were read from the
official GitLab repository; the initial guessed GitHub location failed and is
superseded in the source manifest. Use a PipeWire main/thread loop, registry/node
and metadata subscriptions for sink/source/default volume/mute and device
lifetime. The Rust bindings' loop/thread rules must be respected: forward
bounded store updates to the UI, do not call loop-affine objects arbitrarily from
Tokio tasks. Initial acceptance is output/source state, volume/mute and default
selection, not implementing an audio server.

Native core PipeWire is principally MIT; its LICENSE lists separately licensed
compatibility files. Select the core client library/SPAs actually used and audit
their native license boundary, avoiding PulseAudio compatibility wrappers as an
accidental LGPL framework dependency. See
[PipeWire license](https://github.com/PipeWire/pipewire/blob/master/LICENSE),
[Rust bindings](https://gitlab.freedesktop.org/pipewire/pipewire-rs), and
[published crate](https://docs.rs/pipewire/0.10.1/pipewire/).
Cross-thread safety, default device policy and disconnected daemon recovery need
runtime proof; the absence of a crate license conflict does not prove them.

## Compositor backends

One neutral interface follows Nacre's configuration contract **and** runtime
facade documented in report 00. Opaque IDs, output-relative workspace references,
normalized windows/workspaces/outputs, typed actions, events and capabilities.
Hyprland construction never leaks into generic widgets/scripts. Unsupported
operations return a specific capability error rather than silently approximate.

Hyprland has a command socket and event socket under the runtime instance
directory. Use independent request connections or a serialized command worker;
requests for JSON snapshots and dispatch actions have different replies. Event
lines must be bounded, unknown events tolerated and bursts coalesced. Reconnect
with a fresh snapshot after compositor restart, not just resume old window IDs.
See [Hyprland IPC](https://wiki.hypr.land/IPC/). Published wrapper crates are not
needed for a small documented Unix-socket protocol.

niri-ipc 26.4.0 is **GPL-3.0-or-later**, even though the serialization is plain
JSON. It cannot be a dependency of a Bead crate. Its official source documents
newline-delimited requests/replies and an EventStream request after which that
socket stops receiving commands: use two connections. Maintain independent
request/event DTOs from protocol documentation, with compatibility fixtures and
unknown field/variant policy; do not paste GPL Rust enum implementations into
Bead. Alternative: a GPL nacre-shell adapter may use niri-ipc without changing
the permissive core's dependency graph. See
[niri IPC documentation](https://github.com/niri-wm/niri/tree/main/niri-ipc),
and [published license](https://crates.io/crates/niri-ipc/26.4.0).

Preferred first backend proof uses the existing Python compositor helper to
preserve its already-validated identity/capability normalization, then adds
independent native Rust transports behind the same interface in Feature 5.
This does not port the helpers or use their GPL code as a framework library.
Performance must count the helper process when comparing that implementation.

## Video wallpapers: libmpv versus GStreamer

| Option | Integration/cost mechanism | License boundary and verdict |
| --- | --- | --- |
| libmpv render API | Decode, scheduling and hardware paths are largely supplied; caller owns rendering context/update callback/frame presentation. OpenGL integration may require interop with wgpu or a separate rendering path; CPU render can add copies | libmpv2 6.0.0 wrapper is LGPL-2.1; native mpv build may include GPL code. Excluded from Bead; GPL shell adapter or separate process only |
| GStreamer appsink / GL / DMA-BUF | Flexible decode pipeline; appsink can hand frames to CPU textures, GL/DMA-BUF paths can reduce copies but add synchronization/context/format work | gstreamer Rust 0.25.4 is MIT/Apache, native GStreamer core LGPL and plugins vary. Native linking is still non-permissive; place on GPL shell side |

Read [mpv render.h](https://github.com/mpv-player/mpv/blob/master/include/mpv/render.h)
and [copyright/build options](https://github.com/mpv-player/mpv/blob/master/Copyright).
Render calls have threading/lifetime restrictions and update callbacks cannot do
arbitrary rendering; a decoder is not just “return an Image”. Read
[GStreamer appsink](https://gstreamer.freedesktop.org/documentation/app/appsink.html)
and published crate metadata; failed page fetches are explicitly recorded.

Recommendation: defer implementation to Feature 13, with a neutral video-frame
provider contract in Bead and GPL shell-side playback. Compare static image versus
1080p/4K video, CPU copies versus hardware decode, one/two outputs, GPU memory,
CPU and pacing. Do not implement a permissive decoder from scratch to satisfy a
paper architecture. If “Bead itself links every video dependency” is insisted
on, the permissive-only constraint conflicts with these candidates: **blocker**,
not a hidden license exception. No native video path is verified in Phase 0.

## Processes, file watching and helper client

Tokio 1.53.2 process/net/time with bounded channels is an eligible candidate;
std::process plus a small worker is sufficient for the first proof. Preserve
argument arrays, environment scoping, cwd, stdin flow control, stdout/stderr
separation, exit status, cancellation and kill/reap. Large/silent/stuck children
must not hold UI updates hostage. Long-lived subprocesses survive view reload
when their owner is a service/store. Detached application launches are explicit
actions; there is no implicit shell interpolation inside markup.

notify 8.2.0 is CC0-1.0, not one of the user's enumerated accepted licenses.
Do not quietly broaden the allowlist. Linux-specific inotify through MIT/Apache
rustix or a reviewed MIT inotify wrapper is a practical first choice. Watch parent
directories as well as files so atomic replacements remain visible; debounce
bursts, re-read after overflow, and hash contents to avoid repeated no-op reloads.
Unreadable/missing files retain good state and display a diagnostic.

The existing JSON-lines helper client protocol, exact bounds, pending IDs,
startup/restart/timeout and no-replay rules are in report 00. File/palette/helper
I/O adapters are not interchangeable. Keep external executable paths configurable
through a runtime environment/CLI option; do not embed developer paths or modify
installed helper code. Fixtures may provide a fake worker for failure tests.

## Shortcuts, IME and accessibility

- Hyprland global shortcut protocol is compositor-specific. The LGPL published
  wrapper is excluded; independently generated permissive XML bindings require
  a per-file notice check. Niri retains generated binding → shell IPC behavior.
  xdg-desktop-portal GlobalShortcuts is another API to investigate, not a guaranteed
  silent replacement for the existing compositor bindings.
- SCTK/wayland-protocols provide protocol plumbing for text-input-v3, but an
  editor still needs preedit/commit, surrounding text, deletion offsets, selection,
  cursor geometry, focus and clipboard. Add a basic IME proof in Feature 4 so
  launcher/sidebar parity does not discover this gap at the end.
- AccessKit 0.25.1 and accesskit_unix 0.24.0 are MIT/Apache. The Unix adapter source
  uses AT-SPI via zbus and expects an accessibility tree plus action routing.
  Custom Wayland surfaces do not automatically get those semantics. Stable IDs
  also benefit the accessibility tree. Add minimal button/text-input semantics
  in Feature 4 and screen-reader inspection before bar parity. See
  [AccessKit application guidance](https://github.com/AccessKit/accesskit/blob/main/README-APPLICATION-DEVELOPERS.md).

## Verdict

**Concern, not a service-library blocker.** zbus/SCTK/PipeWire provide transport,
not the complete shell policies. Preserve Python services now, test daemon
restart/error behavior early, keep licensed video/niri adapters out of the Bead
dependency graph, and do not claim IME/a11y simply because a crate exists.
