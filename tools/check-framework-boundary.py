#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Reject product references and dependencies from the MIT framework."""

from __future__ import annotations

import json
import os
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
FORBIDDEN = re.compile(r"nacre", re.IGNORECASE)


def run(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True, stderr=subprocess.STDOUT)


def dependency_violations(metadata: dict) -> list[str]:
    workspace_ids = set(metadata["workspace_members"])
    packages = {package["id"]: package for package in metadata["packages"]}
    errors: list[str] = []
    for package_id in sorted(workspace_ids):
        package = packages[package_id]
        for dependency in package["dependencies"]:
            package_name = dependency.get("package", dependency["name"])
            if package_name.lower().startswith("nacre"):
                errors.append(f"{package['name']}: dependency {package_name}")

    resolve = metadata.get("resolve") or {}
    nodes = {node["id"]: node for node in resolve.get("nodes", [])}
    for workspace_id in sorted(workspace_ids):
        root_package = packages[workspace_id]
        pending = [workspace_id]
        visited: set[str] = set()
        while pending:
            package_id = pending.pop()
            if package_id in visited:
                continue
            visited.add(package_id)
            package = packages[package_id]
            source = package.get("source") or ""
            if "github.com/nacre-desktop/nacre" in source.lower():
                errors.append(f"{root_package['name']}: dependency source {source}")
            node = nodes.get(package_id)
            if node is None:
                continue
            for dep in node["deps"]:
                target = packages[dep["pkg"]]
                pending.append(dep["pkg"])
                if target["name"].lower().startswith("nacre"):
                    errors.append(f"{root_package['name']}: resolved dependency {target['name']}")
    return errors


def source_violations(bases: tuple[pathlib.Path, ...], root: pathlib.Path) -> list[str]:
    errors: list[str] = []
    for base in bases:
        paths = (base,) if base.is_file() else tuple(sorted(base.rglob("*")))
        for path in paths:
            if path.is_symlink():
                target = os.readlink(path)
                if FORBIDDEN.search(target):
                    rel = path.relative_to(root)
                    errors.append(f"{rel}: symlink product path: {target}")
                continue
            if not path.is_file():
                continue
            try:
                content = path.read_text(encoding="utf-8")
            except (UnicodeDecodeError, OSError):
                continue
            for line_number, line in enumerate(content.splitlines(), 1):
                if FORBIDDEN.search(line):
                    rel = path.relative_to(root)
                    errors.append(f"{rel}:{line_number}: product reference: {line.strip()[:120]}")
    return errors


def main() -> int:
    errors = []

    # Cargo's resolved all-feature graph catches renamed direct dependencies and
    # transitive product crates, including aliases that do not spell "nacre".
    metadata = json.loads(run("cargo", "metadata", "--locked", "--all-features", "--format-version", "1"))
    errors.extend(dependency_violations(metadata))

    tree = run("cargo", "tree", "--workspace", "--all-features", "--locked", "--prefix", "none")
    for line in tree.splitlines():
        name = line.split()[0] if line.split() else ""
        if name.lower().startswith("nacre"):
            errors.append(f"resolved workspace graph contains {name}")

    # Product path/tool literals in framework source and examples also fail;
    # historical evidence belongs with the product.
    errors.extend(
        source_violations(
            (ROOT / "Cargo.toml", ROOT / "crates", ROOT / "examples" / "minimal-shell"),
            ROOT,
        )
    )

    if errors:
        print("FRAMEWORK_BOUNDARY_FAILED", file=sys.stderr)
        print("\n".join(sorted(set(errors))), file=sys.stderr)
        return 1
    print("FRAMEWORK_BOUNDARY_OK: workspace dependencies and framework sources are product-free")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
