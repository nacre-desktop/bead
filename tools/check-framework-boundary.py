#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Reject product references and dependencies from the MIT framework."""

from __future__ import annotations

import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
FORBIDDEN = re.compile(r"nacre", re.IGNORECASE)


def run(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True, stderr=subprocess.STDOUT)


def main() -> int:
    errors: list[str] = []

    # Cargo's resolved all-feature graph catches renamed direct dependencies and
    # transitive product crates, including aliases that do not spell "nacre".
    metadata = json.loads(run("cargo", "metadata", "--locked", "--all-features", "--format-version", "1"))
    workspace_ids = set(metadata["workspace_members"])
    packages = {package["id"]: package for package in metadata["packages"]}
    for package_id in sorted(workspace_ids):
        package = packages[package_id]
        for dependency in package["dependencies"]:
            if dependency["name"].lower().startswith("nacre"):
                errors.append(f"{package['name']}: dependency {dependency['name']}")

    # cargo metadata --no-deps is intentionally not used here: inspect resolved
    # nodes so aliases and transitive crates are covered too.
    resolve = metadata.get("resolve") or {}
    for node in resolve.get("nodes", []):
        if node["id"] not in workspace_ids:
            continue
        package = packages[node["id"]]
        for dep in node["deps"]:
            target = packages[dep["pkg"]]
            if target["name"].lower().startswith("nacre"):
                errors.append(f"{package['name']}: resolved dependency {target['name']}")

    tree = run("cargo", "tree", "--workspace", "--all-features", "--locked", "--prefix", "none")
    for line in tree.splitlines():
        name = line.split()[0] if line.split() else ""
        if name.lower().startswith("nacre"):
            errors.append(f"resolved workspace graph contains {name}")

    # The framework must not contain product crates, product path literals, or
    # product tool references. Historical evidence belongs with the product.
    for base in (ROOT / "crates", ROOT / "examples" / "minimal-shell"):
        for path in sorted(base.rglob("*")):
            if not path.is_file() or path.is_symlink():
                continue
            try:
                content = path.read_text(encoding="utf-8")
            except (UnicodeDecodeError, OSError):
                continue
            for line_number, line in enumerate(content.splitlines(), 1):
                if FORBIDDEN.search(line):
                    rel = path.relative_to(ROOT)
                    errors.append(f"{rel}:{line_number}: product reference: {line.strip()[:120]}")

    if errors:
        print("FRAMEWORK_BOUNDARY_FAILED", file=sys.stderr)
        print("\n".join(sorted(set(errors))), file=sys.stderr)
        return 1
    print("FRAMEWORK_BOUNDARY_OK: workspace dependencies and framework sources are product-free")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
