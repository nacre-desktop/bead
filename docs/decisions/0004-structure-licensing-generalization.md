# ADR 0004 — Structure, licensing, and future generalization

Date: 2026-10-21. Status: **Accepted by owner**.

## Context

Bead is the general Rust framework intended as a future alternative to
Quickshell. Nacre is its flagship desktop. Keeping their boundaries explicit
allows Bead and third-party shells to use protocol definitions without taking
on Nacre's GPL-3.0 implementation.

## Decision

1. Bead remains MIT OR Apache-2.0, with permissive-only dependencies. It is the
   general framework. Nacre remains GPL-3.0 and is the flagship desktop built on
   Bead. GPL-only integrations (including niri-ipc, Hyprland bindings,
   mpv/GStreamer, and the daemon) stay on Nacre's side behind neutral adapters.
   Orient remains MIT.
2. `nacre-protocol` is relicensed MIT OR Apache-2.0, covering its schemas,
   generated types, golden transcripts, and protocol documentation. It has its
   own license files and Cargo metadata. The daemon and all other Nacre code
   remain GPL-3.0. Before relicensing, verify every commit touching the
   protocol is by siverteh or their agents. On 2026-10-21, against
   `origin/main` at `84f4656b776f7ffdfe590166af24f8fbcbd5d061`, the file list
   came from that base and the per-file history command was:
   ```sh
   git ls-tree -r --name-only origin/main -- crates/nacre-protocol protocol docs/architecture/protocol.md |
     while IFS= read -r f; do
       git log --follow --format='%H%x09%an <%ae>' origin/main -- "$f"
     done
   ```
   Result: 176 per-file commit records representing 21 unique commits; the only
   author identity was siverteh
   `<92440237+Siverteh@users.noreply.github.com>`. The co-author audit used:
   ```sh
   git ls-tree -r --name-only origin/main -- crates/nacre-protocol protocol docs/architecture/protocol.md |
     while IFS= read -r f; do
       git log --follow --format='%(trailers:key=Co-authored-by)' origin/main -- "$f"
     done | sed '/^$/d' | sort -u
   ```
   It returned no co-author trailers. Stop and list any non-owner identity or
   co-author found by this check.
   Afterwards, Bead's `deny-framework.toml` may allow `bead-*` crates to depend
   on `nacre-protocol` under MIT OR Apache-2.0.
3. Do not split the daemon into another repository now. Revisit only if it
   becomes a general services daemon with outside users. Work Nacre-first:
   prioritize daemon and F4 quality on the owner's machines, then move Bead
   onto always-on Nacre parts (bar, launcher, popouts, dashboard, notifications,
   static wallpaper, lock/session, and packaging). Do not begin generalization
   work now beyond the protocol relicense. Keep Nacre-specific look, sizing,
   colours, Orient coupling, and daemon assumptions out of `bead-*`; keep
   compositor specifics behind adapters. Record discovered framework
   assumptions in the roadmap without fixing them during unrelated work.

## Consequences

The protocol becomes usable by third-party Bead shells without pulling GPL
code into their dependency graph. Nacre retains its GPL boundary and owns the
concrete daemon and compositor integrations. Bead's framework crates remain
permissive and compositor-neutral by design.

## Future milestone: Bead for others

After Bead's core switch on Nacre, plan support for:

- Services without Nacre: a standalone daemon enabled by the MIT protocol, or
  basic built-in Bead services (clock, audio, battery, network, notifications,
  tray, media, and brightness).
- Editor support: generated Luau type definitions from schemas and a
  `luau-lsp` setup for autocomplete and early errors.
- A “Make your first bar” template, getting-started documentation, and a
  generic default theme separate from Nacre's theme.
- More compositor adapters for Sway, river, Labwc, and Wayfire, with bottom
  bars and panels validated.
- A stable, versioned public view API with a deprecation policy.
- Later: plugins with explicit permissions, IME and screen-reader support,
  and published packages.

## Generalization list (framework pieces added by feature work)

- B0043: `bead_ui::transcript` (message list layout, windowing, bounded caches),
  `bead_ui::markdown` (block subset), `kit.Transcript`, `kit.Composer`, and
  `TextInput multiline` (Shift+Enter, wrapped caret, vertical moves). Chat-agnostic;
  the `ai.chat` contract lives only in `nacre-shell/src/chat.rs`.

## Housekeeping (later)

- Move the GPL `nacre-shell` crate and `shell/` assets from Bead to Nacre at a
  natural boundary, not mid-feature, so Bead's repository is purely permissive.
- Before accepting outside contributions, require DCO sign-off in CONTRIBUTING
  for Bead, Nacre, and Orient.
- Before publishing, choose a distinctive main crate name: `bead` and
  `bead-core` are taken by an abandoned 2021 container runtime; `bead-ui`,
  `bead-render`, `bead-wayland`, `bead-services`, `bead-test`, and
  `bead-backends` are available. Present the project as “Bead, the Nacre shell
  framework” to distinguish it from “Beads,” an AI issue tracker. Publishing
  remains an owner decision.

## Reference project

[Pleamar](https://github.com/k4ditano/pleamar) is a similar Rust + Luau
Quickshell alternative under BSD-3-Clause. Evaluate its ideas at the right
time, not now: animation on a separate render thread for smoothness during Luau
stalls (including the current launcher smoothness work if it fits), language
server/editor integration, and plugins with declared user-approved
permissions. Protect Bead's differentiators: about 25 MiB total UI memory,
services outside views for crash safety, state-preserving live reload, and a
real flagship desktop.
