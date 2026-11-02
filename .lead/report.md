Status: B0214 phase 2 complete; ready for lead review, nested niri remains unverified.
Commits: eff688db step 1a identities; fac0c957 step 1b fixtures; 5e6511eb step 2 independent example; all [no-publish], approved author AND committer.
Phase-1 FULL CI: 099e1c34 — PASS in Ubuntu 24.04 on dev-1, including all ten deny checks.
Phase-2 FULL CI: 5e6511eb19eb8815b360d6bc89fcae6753c6b83e — PASS, clean complete run, 09:21:58–09:25:13 +0100.
CI: cargo fmt --all --check; cargo clippy --workspace --all-targets --locked -j2 -- -D warnings — PASS.
CI: workspace/trace tests, renderer feature checks, headless facade/shell, Luau 0.742 (181 modules), all Python proofs, packaging/video and private buses — PASS.
CI: cargo deny check plus independent all-feature checks for every bead-* crate, facade and minimal-shell — all eleven PASS.
Wire proof: cargo test -p nacre-shell --bin nacre-shell nacre_policy_legacy_wire_api_delegates_to_neutral_policy --locked -j2 — PASS.
Nacre: existing accessibility label, pearl IDs and notification information supplied by app; old private policy interface delegates shared neutral operations.
Framework test fixtures: authored neutral palette/image/kit; moved Outfit stem regression passes in nacre-shell workspace tests.
Headless: cargo run -p bead-minimal-shell --locked -j2 -- --check — PASS, action/input/reload/plugin verified.
Asset-absence proof: hid shell/ and nacre-shell/assets, cleaned bead-ui/render/test, recompiled and ran tests plus example — PASS.
Independent proof: copied framework only, omitted product member/lock, resolved offline; forced all ten local packages to recompile with original source/product trees hidden — PASS.
Independent proof: cargo tree has no Nacre; default copied example --check (without --root) — PASS. Only unused patch/profile-entry warnings.
Nested niri: NOT RUN; ssh -F ~/.ssh/config -o ConnectTimeout=8 desktop true timed out (255); no capture armed or live actions.
Initial CI reuse unzip prompt was corrected with a fresh RUNNER_TEMP and a complete successful rerun; no final source failure remains.
Evidence: .lead/tmp/ci-b0214/{baseline,final-ci-clean,product-proof,framework-only-strong-proof}.log; focused logs retained too.
Protected: frontend.rs, schema.rs and all shell/ files unchanged; no installed configs/services, preview tools or external Nacre repo changed.
Deferred: 1c waits for B0213; 3 waits for B0212/B0213 landing and remaining generalization. Plan already approved; no files moved.
Cleanup: ci-b0214, its prepared image, task build directory and tmux sessions absent; local worker worktrees removed.
Size (implementation commits): +1196/-176 lines, 0 deleted files; Nacre subtree +332/-2 includes moved/new tests.
