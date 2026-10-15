#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Fail if a headless host gains a UI, store, renderer or compositor dependency."""
import subprocess

for features in ([], ["--all-features"]):
    tree = subprocess.check_output(
        ["cargo", "tree", "-p", "bead-services", "--locked", "--prefix", "none", *features],
        text=True,
    )
    forbidden = {"bead", "bead-core", "bead-ui", "bead-render", "bead-wayland", "bead-backends", "smithay-client-toolkit"}
    bad = sorted({line.split()[0] for line in tree.splitlines() if line.split()[0] in forbidden or line.startswith("wayland-")})
    if bad:
        raise SystemExit("headless service boundary violated: " + ", ".join(bad))
print("HEADLESS_SERVICE_BOUNDARY_OK default+all-features")
