#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Negative controls for the framework/product boundary guard."""

import pathlib
import importlib.util
import sys
import tempfile
import unittest

GUARD_PATH = pathlib.Path(__file__).resolve().parents[1] / "check-framework-boundary.py"
SPEC = importlib.util.spec_from_file_location("check_framework_boundary", GUARD_PATH)
boundary = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(boundary)


class FrameworkBoundaryTests(unittest.TestCase):
    def test_catches_nacre_dependency_hidden_by_alias_and_transitive_wrapper(self):
        metadata = {
            "workspace_members": ["path+file:///repo#bead@0.1.0"],
            "packages": [
                {
                    "id": "path+file:///repo#bead@0.1.0",
                    "name": "bead",
                    "source": None,
                    "dependencies": [
                        {"name": "wire", "package": "neutral-wrapper"},
                    ],
                },
                {
                    "id": "registry+wrapper",
                    "name": "neutral-wrapper",
                    "source": "registry+https://github.com/rust-lang/crates.io-index",
                    "dependencies": [],
                },
                {
                    "id": "git+nacre#protocol",
                    "name": "nacre-protocol",
                    "source": "git+https://github.com/nacre-desktop/nacre?rev=abc#abc",
                    "dependencies": [],
                },
            ],
            "resolve": {
                "nodes": [
                    {
                        "id": "path+file:///repo#bead@0.1.0",
                        "deps": [
                            {"pkg": "registry+wrapper", "name": "wire", "dep_kinds": []},
                        ],
                    },
                    {
                        "id": "registry+wrapper",
                        "deps": [
                            {"pkg": "git+nacre#protocol", "name": "protocol", "dep_kinds": []},
                        ],
                    },
                    {"id": "git+nacre#protocol", "deps": []},
                ]
            },
        }
        errors = boundary.dependency_violations(metadata)
        self.assertTrue(any("nacre-protocol" in error for error in errors), errors)
        self.assertTrue(any("dependency source" in error for error in errors), errors)

    def test_catches_product_path_and_nacrectl_literals(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            source = root / "src" / "lib.rs"
            source.parent.mkdir()
            source.write_text(
                'const CONFIG: &str = "~/.config/nacre";\n'
                'const TOOL: &str = "nacrectl";\n',
                encoding="utf-8",
            )
            errors = boundary.source_violations((root,), root)
        self.assertEqual(len(errors), 2, errors)
        self.assertTrue(any("~/.config/nacre" in error for error in errors), errors)
        self.assertTrue(any("nacrectl" in error for error in errors), errors)


if __name__ == "__main__":
    unittest.main()
