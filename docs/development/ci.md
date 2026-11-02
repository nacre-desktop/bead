# Framework CI

The GitHub Actions workflow is the authoritative framework check. It covers formatting, warnings-denied Clippy, workspace tests, feature configurations, dependency/license policy for each framework root, and the minimal-shell check and proofs.

Run focused checks while developing, then run the same CI workflow before submitting. Do not require an application checkout: the framework workspace and minimal-shell example must build and prove independently.
