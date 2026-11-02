Status: step 1c complete, pushed for lead review; CI passed and remote cleanup verified.
Branch: task/codex-b0214c-boundaries-20261102; code commit 77ccb7b7, with a final documentation/report commit at HEAD.
Host-injected tokens/fonts and extracted bead-kit/bead-widgets retain app-supplied IPC, catalogue, content, routes and storage paths.
Product change: +227/-984 lines; five active files moved, obsolete scale-sync tool and replaced implementations removed.
CI: complete .github/workflows/ci.yml replication on exact 77ccb7b7, dev-1 Ubuntu 24.04, 8 CPUs/16 GiB: PASS, including all 13 cargo-deny graphs.
Checks include fmt, warnings-denied Clippy, workspace/trace tests, feature builds, Luau 0.742 (188 modules), minimal hosts, Python/packaging/video/private buses.
Pixels: all 604 tracked goldens unchanged; zero-tolerance control reproduced five existing mismatches, with byte-identical base/candidate PNGs; other strict states passed.
Limits: all-state recording extension did not reach tests after Docker stalled/provisioning exited 137. No physical/live parity claim.
Two timing-sensitive tests failed transiently, then passed unchanged in focused reruns and final full CI; no assertions or goldens relaxed.
Evidence: .lead/tmp/ci-b0214c/evidence/ (final-ci77.log/exit, strict logs/images/byte verification, cleanup log).
Step 3: exact export selections and Nacre build/CI edits are in docs/specs/0214-framework-split.md, “Step 3 readiness”.
Destinations: crates/nacre-shell/ + shell/ at Nacre top level; selected packaging/scripts/spikes/tools/examples/docs/notices under bead-shell/.
Collision check: current Nacre origin/main 9c96a20ced21be08cdb4dfd40015f1db078334ef has none of those three destination prefixes.
Nacre changes: explicit edition 2024/Rust 1.99, exact Bead rev pins/vendor alignment, reconciled features/profiles, extended workflow/check.py/Arch recipes and native dependencies.
Keep the three-executable + shell/ artifact and transactional ui-shell API; preserve the existing QML wrapper and adjust preview manifests through the lead.
Cleanup: all four B0214c container names, task image/tmux and remote task directory verified absent; target cache and local source/archive scratch removed. Review evidence retained.
No other containers, live installation/configs, preview tooling or Nacre checkout touched. STOPPED before step 3; lead review required.
