# Security policy

Report suspected vulnerabilities in Bead privately through GitHub's private vulnerability reporting: on this repository's **Security** tab, choose **Report a vulnerability**. Please do not open a public issue or pull request for a vulnerability.

Include the affected revision, steps to reproduce and the expected impact. Bead has no releases; fixes land on `main`, and applications pick them up by moving their pinned revision.

Luau sandboxing of plugins is a stability boundary, not a security boundary ([spec 0203](docs/specs/0203-plugins-v1.md)); hosts should enable only trusted plugins.
