Status: shipped to `main` with acceptance failures recorded; B0162 does not meet all interaction gates.
Harness: `./tools/daily-acceptance/run.sh` on the TEST desktop; complete three-round + idle + repeat arms ran at 2880×1800/1.5.
Baseline `900369fb`: FAIL; 84/96 legacy timing samples p50/p99 80.781/81.352 ms, outside-close/idle layer count 7 vs 6, Pss_Anon +9.074 MiB, UI_ERROR 0.
Main-before `24709327`: FAIL; 42 marker samples p50/p99 246.056/278.685 ms; Escape retained Updates; 24 UI errors; Pss_Anon +18.578 MiB.
Fixed-main candidate: FAIL; 35 marker samples p50/p99 245.732/270.954 ms, 72/96 close screenshots differ; UI_ERROR 0, drag commits and resource gates pass, Pss_Anon +18.195 MiB.
Root cause: `bda20168` enabled text-only focus for bar popouts, preventing Escape dismissal; widget canvas defaults removed candidate UI errors. Slow-open first bad commit and exact black-square pixel remain unidentified.
Remote full CI on `5a39635d`: exit 0, `BEAD_CI_AND_RELEASE_ARMS_PASS`; later `df6c8b4e` changes only harness routing and passed py_compile + full capture. GitHub Actions run `40531337840` passed for pushed HEAD `37596135` in 6m40s.
Fixes and harness are committed and pushed by safe-push; no live laptop install or configuration was changed. Private capture artifacts remain uncommitted.
