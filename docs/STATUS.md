# Status

Updated: 2026-10-13. Phase 0 in progress. Feature 1 is not authorized.

## Done

- Created isolated local main worktree backed by a separate bare repository.
- Read project/workflow instructions; Nacre and Orient reference checkouts clean.
- Nacre reference: b547d06a02f8ff0dc0a6f76d366be086657e0bea.
- Orient main reference: 51a452b2c2e49931ee96e1c932eac620557f73d4.
- Palette format document read from feature/orient-rust-core at
  a4ef06e86ea06dac8a6d35d179cfd770efc5981a.
- Isolated Rust 1.99.0 installed in ignored `.tools/`; no host setup changed.

## In progress

Shell inventory, dated primary-source research, renderer measurement spikes.

## Blocker

GitHub `gh repo create nacre-desktop/bead --private` returned HTTP 401.
Owner was asked to refresh the existing login. No remote created or publication.
Retry after authentication; organization permission failure then permits a
private personal-account fallback. Authentication failure does not justify
changing account or authentication configuration.

## Exact next step

Generate location-indexed shell inventory, pin candidate dependency versions and
sources, then build isolated blank layer-shell renderer spikes. Write research
00–05, ADR 0001 and gated roadmap. Review evidence and stop for owner approval.

## Commands

Research scripts and reproducible measurement commands will be added here.
There is no production shell command yet. No experimental shell is running.
