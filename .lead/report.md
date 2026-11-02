Status: B0214 phase 1 complete; remote CI validation pending host availability; lead reviews before migration.
Commits: 8b93f365 audit/plan; 6c39c178 MIT-only relicense; both [no-publish], siverteh author AND committer.
Spec: docs/specs/0214-framework-split.md; exhaustive grouped 1,963-file ownership inventory plus 718-file product mirror and framework coupling audit.
Plan: parameterize identity/theme/fixtures, independent minimal-shell, filtered history import AFTER B0212/B0213, Nacre artifact/CI/packaging/preview cutover, then remove product from Bead.
History: filter-repo only in disposable clone; normal import merge; no public history rewrite or cross-repo move this phase.
Install: preserve existing three-executable + shell bundle and transactional nacre ui-shell install; no live changes.
License: nine Bead manifests and workspace MIT; SPDX/tools/spikes/examples/prose updated; root MIT holder siverteh.
Preserved: nacre-shell/shell GPL; vendor MIT, SCTK MIT, test-font Apache/OFL, CC0 assets, historical third-party records.
Authors: git log --format='%an <%ae> | %cn <%ce>' -- crates/bead-* crates/bead found only siverteh; AI co-author trailers listed in spec, no other human author.
Check: nice -n 10 git diff --check — PASS.
Check: nice -n 10 python3 per-file tomllib parsing — PASS, 18 TOML files (workspace/crates/spikes/deny/example).
Check: command -v rustfmt; command -v luau-analyze — both unavailable on PATH; no syntax/format execution claimed.
Probe ONCE: ssh -F ~/.ssh/config -o ConnectTimeout=8 dev-2 true — exit 255, network unreachable.
Probe ONCE: ssh -F ~/.ssh/config -o ConnectTimeout=8 desktop true — exit 255, connection timed out.
Not run: cargo deny, builds, clippy, tests, nested sessions or captures; no laptop substitution.
Required: all .github/workflows/ci.yml steps on exact pushed revision in prescribed dev-2 Ubuntu 24.04 container; exact pending checks/commands in spec validation section.
Includes: fmt, clippy -D warnings, workspace/trace tests, renderer features, facade/schema/Luau 0.742, 13 Python proof suites, py_compile, packaging/video smoke, private service buses.
License checks required: cargo deny check; for manifest in crates/bead-*/Cargo.toml crates/bead/Cargo.toml; do cargo deny --manifest-path "$manifest" --all-features --config deny-framework.toml check; done
Product behavior lines added/removed: 0/0; removed: root LICENSE-APACHE, 1 file / 176 lines. Source edits are license headers only.
Open: lead plan approval and unavailable full CI; no optional runtime changes, repo move, main push, installation or publishing.
