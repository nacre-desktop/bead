Done — B0216 pushed to `task/codex-b0216-icon-centre-20261102`; ready for lead integration.

- Code commits: `8fdb1f0c`, `ed42d9a6`, `8a6142ab`, `b5aeec25`, `58eee189`, `7b35831e`; all `[no-publish]`, correct author/committer.
- Shared fix: centre Material Symbols ink, zero-pad/centre IconButton, use IconButton for icon-only pills; labeled layouts retained.
- Inventory: `docs/specs/0216-icon-centring.md` covers all requested surfaces plus the service tray.
- Product fixes: bar glyph wrapper, media seek, launcher favorite images, tray back/close; no per-glyph call-site offsets.
- Ink tests: six renderer glyphs, seven kit variants, real control-centre/bar/media/tray buttons; both axes <=0.5 logical px at scales 1 and 1.5 — PASS.
- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets --locked -j2 -- -D warnings` — PASS.
- `cargo test --workspace --locked -j2` plus trace memory-policy tests — PASS.
- Renderer no-default/all-feature checks, facade examples, headless shell and Luau schema — PASS.
- Python proofs, packaging, video smoke, private service/SNI/clock lifecycle checks — PASS.
- `cargo deny check` plus all-feature checks for ten framework manifests — PASS.
- Full `.github/workflows/ci.yml` replicated in Ubuntu 24.04 — PASS.
- All 229 changed/new goldens personally reviewed in 21 before/after sheets; original bar geometry assertions retained.
- Product size: +90/-11 lines; removed 0 files; no new dependencies.
- Open issue: paired QML/Bead nested captures skipped after test-host SSH timeout. No live desktop/config/install changes or main push.
