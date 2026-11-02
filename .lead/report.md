Done: integrated B0216 (7 commits), B0215 (1), then B0217 (13), all cherry-picked with -x.
B0216 conflict: ported IconButton centring into crates/bead-kit/assets/kit.luau; retained injected fonts/API.
Review correction: icon-only PillButton also uses tokens.fonts.icon (cc945877); no restored/deleted-file duplicates.
Report conflict: accepted the B0216 report during replay, replaced by this integration report.
B0215 slider and B0217 scroll changes merged without code conflicts; B0214c structure retained.
Goldens: 229 inherited B0216 images personally reviewed in 29 before/after sheets; zero regenerated.
Goldens remain byte-identical to B0216's own files; workspace tests had no golden mismatches.
CI: every ci.yml step PASS, exit 0, Ubuntu 24.04 container ci-int-b0217, 8 CPUs / 16 GiB on dev-1.
Validated code head: cc94587739174520529cf70f76830ea7483310ba; this report is the only later change.
Evidence: .lead/ci-int/ci-int-b0217.log and ci-int-b0217.status (local, ignored).
Push target: lead/integrate-b0217-20261102 only; all commits use required identity and [no-publish].
No local builds/tests, prod use, live desktop access, main push, force-push or publication.
Cleanup: task container, remote runner/tmux and local review sheets removed; CI evidence retained.
