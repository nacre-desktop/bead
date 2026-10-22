#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Package the frozen control and current Bead UI for the laptop capture."""
import argparse
import io
import hashlib
import json
import pathlib
import shutil
import subprocess
import tarfile


FROZEN_FIXTURE_GIT_TREE = "51dbf56ff49bf55656d7f04257f532dde73ece47"


def digest(path: pathlib.Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def tree_digest(root: pathlib.Path) -> str:
    value = hashlib.sha256()
    for path in sorted(item for item in root.rglob("*") if item.is_file()):
        value.update(path.relative_to(root).as_posix().encode())
        value.update(b"\0")
        value.update(bytes.fromhex(digest(path)))
    return value.hexdigest()


def git_tree(root: pathlib.Path, revision: str) -> str:
    repository = subprocess.check_output(["git", "-C", str(root), "rev-parse", "--show-toplevel"], text=True).strip()
    return subprocess.check_output(
        ["git", "-C", repository, "rev-parse", f"{revision}:shell/tests/fixtures/dashboard"],
        text=True).strip()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--control", type=pathlib.Path, required=True)
    parser.add_argument("--candidate", type=pathlib.Path, required=True)
    parser.add_argument("--fixtures", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--candidate-revision", required=True)
    parser.add_argument("--control-revision", default="1cff9d17d61fb05451546bb0ea13c826c93cb4c9")
    parser.add_argument("--control-kind", choices=("original-frozen", "frozen-source-rebuild"), default="original-frozen")
    args = parser.parse_args()
    for path in (args.control, args.candidate):
        if not path.is_file():
            parser.error(f"artifact is not a file: {path}")
    if not args.fixtures.is_dir() or not any(args.fixtures.rglob("*")):
        parser.error(f"fixture tree is missing or empty: {args.fixtures}")
    try:
        frozen_tree = git_tree(args.fixtures, args.control_revision)
        candidate_tree = git_tree(args.fixtures, args.candidate_revision)
    except subprocess.CalledProcessError as error:
        parser.error(f"could not prove dashboard fixture source revisions: {error}")
    if frozen_tree != FROZEN_FIXTURE_GIT_TREE:
        parser.error(f"frozen dashboard fixture tree changed: {frozen_tree}")
    if candidate_tree != frozen_tree:
        parser.error(f"current main dashboard fixtures no longer match the frozen control: {candidate_tree}")
    output = args.output.resolve()
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    artifacts = {}
    for key, source, revision in (("control", args.control, args.control_revision),
                                  ("candidate", args.candidate, args.candidate_revision)):
        destination = output / key / "nacre-shell"
        destination.parent.mkdir(mode=0o700)
        shutil.copy2(source, destination)
        destination.chmod(0o700)
        artifacts[key] = {"path": f"{key}/nacre-shell", "sha256": digest(destination),
                          "source_revision": revision}
    repository = subprocess.check_output(
        ["git", "-C", str(args.fixtures), "rev-parse", "--show-toplevel"], text=True).strip()
    frozen_shell = subprocess.check_output(
        ["git", "-C", repository, "archive", args.control_revision,
         "shell/ui", "shell/tests/icon.svg", "shell/tests/fixtures/dashboard"])
    bundle_root = output / "control-bundle"
    bundle_root.mkdir(mode=0o700)
    with tarfile.open(fileobj=io.BytesIO(frozen_shell), mode="r:") as archive:
        members = archive.getmembers()
        if any(pathlib.PurePosixPath(member.name).is_absolute()
               or ".." in pathlib.PurePosixPath(member.name).parts for member in members):
            parser.error("frozen control bundle contains an unsafe archive path")
        archive.extractall(bundle_root)
    frozen_fixture_hash = tree_digest(bundle_root / "shell/tests/fixtures/dashboard")
    if frozen_fixture_hash != tree_digest(args.fixtures.resolve()):
        parser.error("frozen control bundle and current main dashboard fixtures differ")
    original_sha = "b01dfccaac148b984a6e1089f7a54d41bced473455b3d41890d15ec074a8038b"
    original_identity = artifacts["control"]["sha256"] == original_sha
    if args.control_kind == "original-frozen" and not original_identity:
        parser.error(f"original frozen artifact hash mismatch: expected {original_sha}, got {artifacts['control']['sha256']}")
    artifacts["control"].update({
        "artifact_kind": args.control_kind,
        "original_frozen_binary_sha256": original_sha,
        "exact_original_binary_identity": original_identity,
    })
    manifest = {"schema_version": 1, **artifacts,
                "control_bundle_root": "control-bundle",
                "control_bundle_sha256": tree_digest(bundle_root),
                "dashboard_fixtures_sha256": tree_digest(args.fixtures.resolve()),
                "dashboard_fixtures_git_tree": candidate_tree,
                "toolchain": "pinned Bead CI Rust 1.99.0; release profile",
                "notes": ([] if original_identity else
                          ["The original frozen executable was not used.",
                           "Control is a hash-pinned rebuild from frozen source 1cff9d17."])}
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"output": str(output), "control_sha256": artifacts["control"]["sha256"],
                      "candidate_sha256": artifacts["candidate"]["sha256"],
                      "control_exact_original": artifacts["control"]["exact_original_binary_identity"],
                      "control_bundle_sha256": manifest["control_bundle_sha256"],
                      "fixtures_sha256": manifest["dashboard_fixtures_sha256"]}, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
