# Bead repository instructions

Bead is an independent MIT-licensed framework for building Wayland shells. Nacre and other applications may use Bead as a dependency; framework crates must not depend on application crates or product paths.

## Scope and licensing

- Keep framework APIs application-neutral. Hosts supply app identity, content, assets, service routes, configuration roots, and installation policy.
- Do not add product-specific behavior, paths, protocol knowledge, or assets to framework crates.
- Framework crates and examples use permissive dependencies. Preserve every third-party license and notice, including notices in `vendor/` and test fonts.
- Do not copy GPL/LGPL implementation code into framework crates.
- Every crate sets `publish = false`; do not publish packages or releases.
- Do not include personal paths, usernames, or private machine details in committed files.

## Development

- For multi-step project work, act as lead using the lead-workflow skill and built-in subagents; workers use isolated worktrees.
- Commit author and committer: siverteh <92440237+Siverteh@users.noreply.github.com>. Include `[no-publish]` in every commit message. Never kill processes you did not start.

- Keep framework checks independent of any application checkout. The minimal host in `examples/minimal-shell` must remain usable with Bead alone.
- Run formatting, warnings-denied Clippy, tests, feature checks, dependency/license checks, and minimal-shell proofs for implementation changes. CI is authoritative; see `docs/development/ci.md`.
- Specs document framework contracts and decisions. Keep them focused and remove product-specific history from this repository.
- Preserve the direction rule: framework crates do not depend on or reference `nacre-*`, Nacre product paths, or `nacrectl`.

## Owner boundaries

Do not touch the live desktop, installed applications, live configuration, keybindings, or system/user units without explicit owner approval. Do not change the deliberate AI permission settings or the configured `paru -Syu --skipreview` command. Never force-push. Do not publish tags, releases, or packages without the owner.
