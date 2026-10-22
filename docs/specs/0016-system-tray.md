# B0016 — SNI and DBusMenu bar tray

Status: Review — private/legacy preview implemented; daemon core enablement
is blocked and matched memory acceptance is pending. Local commits only; lead integrates.
Objective: finish the existing headless SNI host and expose bounded tray items
and interactive menus in the Nacre bar, without live bus ownership or deployment.
Read ADRs 0001–0004, COMPONENTS, B0009–B0015, Roadmap Feature 6 and standing rules.

## Scope and ownership

Reuse the existing MIT `system-tray` client and its lifecycle patch in
`bead-services`; do not write another protocol implementation. The service owns
watcher/host, item state and menu subscriptions outside replaceable views.
ADR 0003 makes nacre-daemon the eventual Nacre host. Its existing in-process
Desktop client remains explicitly transitional and is used only for private
proofs/legacy preview here. No new daemon topic, capability or method is invented;
no Nacre source or protocol pin changes. Product daemon enablement is an open
integration dependency, not a claim of this unit. Framework types contain no
Nacre palette, dimensions, layout or daemon assumptions; GPL shell owns those.

Current Nacre `origin/main` has no SystemTray/StatusNotifier/DBusMenu QML under
`nacre/shell`; the tray settings entry is planned. Consequently there is no
current QML icon set, ordering or overflow behavior to match. Roadmap Feature 6
already records this absence. Implement the protocol behavior below and report
this parity limit; do not fabricate QML goldens.

Non-goals: per-output bar redesign, XEmbed, notification ownership, ranking,
settings persistence, live session takeover, install/publish, daemon migration,
animated attention movies, icon overlay composition, and DBusMenu row icon/shortcut
display (labels, separators, toggles and submenus are supported).

## Model, lifecycle and events

The existing neutral Snapshot.tray/TrayItem/MenuEntry and Action interface is
retained. Extend it additively for tooltip text, secondary activation and scroll.
IDs are unique bus-owner plus object path; same-owner duplicate registration is
idempotent. Owner vanish removes item and its menu/tasks; a replacement owner
gets a fresh identity. Session disconnect clears state and cancels the executor
before reconnect/backoff. Watcher changes must retire stale state, re-register
host and resnapshot registrations; never replace an existing watcher owner.
No UI factory starts a second host. Reload leaves service ownership unchanged.

Active and NeedsAttention items are visible; Passive items are excluded. Order
is deterministic by service identity. Attention selects attention name/pixmap
with normal-icon fallback. Theme name/custom theme path precedes pixmap; validate
assets, dimensions and byte counts; network ARGB converts to RGBA. Fallback icon
must leave the rest of the bar usable. Tooltip is bounded plain title/description,
never interpreted HTML. Primary activates (ItemIsMenu opens menu), right-click
opens menu, middle calls SecondaryActivate, both scroll axes call Scroll.
Without a DBusMenu path, menu activation calls the item's native ContextMenu. No invented
hidden/overflow preferences; visible item count is bounded.

Menus use the existing Popover kit, sized to content with a bounded viewport. Preserve submenu navigation, enabled/visible,
separator and toggle state; never flatten actionable descendants of disabled
parents. AboutToShow precedes presentation/submenu navigation. LayoutUpdated and
ItemsPropertiesUpdated replace/patch the service model; stale entries cannot be
activated. Event(clicked) targets the current item/menu ID, with bounded action
timeout. Close on item disappearance, Escape and explicit close. Release menu
view state on close; service state survives view reload.

## Security and memory

All item strings, labels and tooltip text are untrusted: bound and strip control
characters, render as plain text. Bound item count, pixmap count/dimensions/bytes,
menu depth/total entries and action queues. Reject malformed data rather than
allocating from unchecked width*height arithmetic. Bound retained service state,
not just rendered nodes; report any upstream transport allocation limit gap.
Never execute shell commands or desktop files supplied by an item. Icon lookup
is file reading only; validate file size/format before decoding. Menu events must
refer to current visible enabled entries; stale identities return errors.

No memory clearance is claimed from tests. Later matched private bar/tray/menu
captures must include UI PSS+DRM+SwapPss and separate service totals: settled UI
<=25 MiB, stop at24.5 MiB, each close <=own bar+5 MiB, stop unexplained anonymous
increase >1 MiB. Closed menu resources release after closed+idle; no eager panel
allocation. Unknown DRM stays unknown. Captures require standing swap/quiet/
containment gates; no capture is needed to run private headless protocol tests.

## Phases and acceptance

1. Commit service changes and fake-item tests on a dbus-run-session private bus:
   registration, duplicates, updates, activation/secondary/scroll, menu prepare/
   events and bounded malformed inputs. Reuse existing headless service split.
2. Commit bar row/menu Popover, icon/attention/tooltip and click routing tests.
3. Commit lifecycle/restart evidence: owner loss/replacement, watcher/session
   boundaries, menu updates/stale actions, reload and close cleanup. Run full
   .github/workflows/ci.yml equivalent in a dedicated Arch container remotely,
   including fmt, denied-warning Clippy, workspace/trace tests, renderer feature
   checks, Luau/schema checks, regression suites, private services and cargo-deny
   (root plus all framework graphs). No builds/tests on the laptop.

## Stop conditions

Stop for an unpublished daemon contract needed to enable core mode, an ADR
change, unsafe watcher takeover, required live access, licensing violation or
memory stop line. Do not silently enable transitional direct services in core
mode. Fix in-scope test/environment problems; report external blockers plainly.
No push. Commit author and committer use the approved owner identity and every
message contains [no-publish]. Final report records exact checks and remaining
producer/parity/memory limits.

## Delivery and remaining gates

Source through `6efca12`: reusable service hardening, lazy Popover, bounded plain
inline tooltip, attention name/pixmap refresh/fallback, primary/context/middle/
both-axis routing. Canonical owner+object-path IDs support multiple items from
one connection; duplicate registration is idempotent. Private tests cover item
and watcher loss/recovery, session-bus EOF/reconnect, AboutToShow refresh,
subtree signals preserving siblings, property updates and stale/disabled actions.

Headless view checks cover enabled/disabled/submenu/toggle/separator projection,
large-menu scrolling over actionable rows, close/reload/reopen, item disappearance,
subscription release and rejection of older snapshot revisions. Nested CPU input
checks exercise the native popover, both wheel axes, primary/middle activation,
closed+idle retirement/reopen and removal while open. They exposed a repeated
configure attempting to reattach a busy dismissal buffer; the immutable pixel
now attaches once. Epoch-checked view requests close the native panel on item
loss, including its dismissal surface and retirement timer.

Core blocker: the published protocol has reserved tray snapshots, but the
current producer/action contract does not support this complete interaction
path. Nacre-side hosting, activation/menu/scroll/tooltip/icon transport publication
and green producer CI must precede a capability-gated consumer. No Nacre edit,
protocol invention, direct-service fallback or runtime core enablement is included.
Matched UI PSS+DRM+SwapPss captures and physical input/accessibility acceptance
remain open. Current Nacre QML supplies no tray reference; no parity claim.
Wire-message allocation before retention caps remains controlled by zbus.

## Validation — 2026-10-22

A clean archive of source `6efca12` passed the complete `.github/workflows/ci.yml`
equivalent in a dedicated Arch container: fmt, denied-warning Clippy, headless
boundary, workspace and trace tests, CPU/all-feature configurations, facade,
Luau/schema checks, all regression suites, private sensor lifecycle, and root
plus every framework cargo-deny graph. The explicitly enabled private tray suite
passed 3 tests (integration/supervisor plus its child helper); ordinary workspace
runs correctly leave these private-bus tests ignored. Registration proof: 2 items,
9 actions, owner loss and watcher reacquisition. Session proof: 2 items before
EOF and after reconnect, empty state during loss, successful recovered activation.

The final nested Labwc CPU proof passed native menu opening, primary/middle and
both wheel axes, closed+idle release/reopen, and owner removal while open. Debug
CPU binary SHA-256: `93695c3d6b779556fe8b8a543a7c28efb4dd040864246884569c8b28b4dd9bc0`.
Raw logs and the bounded private controller are retained in the private handoff;
no screenshots, performance samples, PSS/DRM measurement or live session used.
Local author/committer identity and [no-publish] messages verified; no push.
