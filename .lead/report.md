B0214 step 4: exported Nacre product removed; Bead is the MIT framework.
Branch: task/codex-b0214-step4-remove-product-20261102; no main push.
Verified code: b18cec0ce64378a9db8218f227eeeb75b289fb13; final follow-up changes docs/report only.
Removed 3,402 tracked files: 181 product-crate, 584 shell, 718 mirror, 132 spikes, 1,735 historical docs/evidence, 52 other product files.
All deleted paths belong to Nacre's frozen export; rewritten framework guides and MIT text remain.
Kept 11 framework crates, SDK/services/harness, minimal-shell, hello plugins, 8 framework specs and required vendor/font notices.
Root license is MIT only; 30 vendor/font files remain byte-identical; 47 product-only lock packages removed, no retained version changes.
Guard: resolved all-feature graph plus source/manifests/symlinks; rejects product crates, sources, paths and nacrectl.
Case-insensitive nacre search in crates/: zero matches, so no historical/source exceptions.
CI PASS: entire ci.yml, including fmt, -D warnings Clippy, workspace tests, renderer features, examples, private buses and 12 deny roots.
CI host/container: dev-1, ci-b0214s4, Ubuntu 24.04, 8 CPUs/16 GiB; cloned pushed branch, no Nacre checkout during framework proof.
Guard controls PASS: aliased/transitive metadata and paths; actual CLI rejects import, old shell path and nacrectl fixtures, then passes restored tree.
Nacre PASS: unchanged main 8c56db89 workspace build + shell all-features check, pinned to f19eacaf; added libraqm-dev only inside the container.
Tracked files: 3,616 -> 217; nonbinary text lines (including blanks): 690,935 -> 82,564.
Evidence: ignored .lead/ci-b0214s4{,-negative,-nacre}.log and status files.
No local builds/tests, Nacre edits, live desktop/install/config/unit changes, prod, main push, force-push or publication; step 5 remains open.
