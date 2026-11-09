#!/usr/bin/env python3
"""Read-only original identity capture and frozen-tree validation copy."""
from __future__ import annotations

import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile

WORKTREE = Path("/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation")
SESSION = WORKTREE / ".superpowers/sdd/2026-11-09-session-summary-continuation"
VALIDATION = SESSION / "task-1-cv1-validation"
FROZEN_TREE = "a907fe5b9eb006cba8533ab57831c18cda485b7c"
BASE = "73760d609a050eaddcd8002cec572fbc73c65164"
SOURCE = VALIDATION / "source"
GIT_ENV = {**os.environ, "GIT_OPTIONAL_LOCKS": "0"}


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def git(*args: str) -> bytes:
    return subprocess.check_output(["git", *args], cwd=WORKTREE, env=GIT_ENV)


def save(name: str, value: object) -> None:
    (VALIDATION / name).write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def identity(path: Path) -> dict:
    if path.is_symlink():
        data = os.readlink(path).encode()
        kind = "symlink"
    else:
        data = path.read_bytes()
        kind = "file"
    return {"kind": kind, "bytes": len(data), "sha256": sha(data)}


def main() -> None:
    if SOURCE.exists() or (VALIDATION / "original-baseline.json").exists():
        raise SystemExit("Refusing to replace an existing copy or baseline")
    VALIDATION.mkdir(parents=True, exist_ok=True)
    paths = {
        Path(os.fsdecode(p))
        for p in git("ls-files", "--cached", "--others", "--exclude-standard", "-z").split(b"\0")
        if p
    }
    # Existing ignored evidence is immutable too. Cargo caches/targets are the
    # explicitly authorized mutable environment and are deliberately excluded.
    superpowers = WORKTREE / ".superpowers"
    for directory, directories, filenames in os.walk(superpowers):
        d = Path(directory)
        directories[:] = [
            name
            for name in directories
            if d / name != superpowers / "toolchain" and d / name != VALIDATION
        ]
        for name in filenames:
            path = d / name
            if path == SESSION / "task-1-cv1-validation-report.md":
                continue
            paths.add(path.relative_to(WORKTREE))
    original = {str(p): identity(WORKTREE / p) for p in sorted(paths) if (WORKTREE / p).is_file() or (WORKTREE / p).is_symlink()}
    head = git("rev-parse", "HEAD").decode().strip()
    if head != BASE:
        raise SystemExit(f"Unexpected HEAD {head}")
    original_report = original[str((SESSION / "task-1-report.md").relative_to(WORKTREE))]
    assert original_report["bytes"] == 77047
    assert original_report["sha256"] == "342ee2c8adb81bea0f9feaa19a3e1d47dc906a1326e6ad5d2e3b921d27098937"
    gitignore_expected = "1e708ede61f6522d88e4bea58f5c55e70831f404fd00f7a6710d424446e57a3a"
    assert original[".gitignore"]["sha256"] == gitignore_expected
    assert sha(git("show", ":.gitignore")) == gitignore_expected
    state = {
        "head": head,
        "staged_entries_sha256": sha(git("ls-files", "--stage", "-z")),
        "staged_diff_sha256": sha(git("diff", "--cached", "--binary")),
        "refs_sha256": sha(git("show-ref")),
        "protected_gitignore_sha256": gitignore_expected,
        "reviewed_tree": FROZEN_TREE,
        "captured_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    }
    save("original-baseline.json", {"state": state, "files": original})
    archive = VALIDATION / "frozen-tree.tar"
    with archive.open("xb") as output:
        subprocess.run(["git", "archive", "--format=tar", FROZEN_TREE], cwd=WORKTREE, env=GIT_ENV, stdout=output, check=True)
    SOURCE.mkdir()
    with tarfile.open(archive) as bundle:
        bundle.extractall(SOURCE, filter="data")
    copied = {
        str(p.relative_to(SOURCE)): identity(p)
        for p in sorted(SOURCE.rglob("*"))
        if p.is_file() or p.is_symlink()
    }
    tree_entries = [entry for entry in git("ls-tree", "-r", "-z", FROZEN_TREE).split(b"\0") if entry]
    assert len(copied) == len(tree_entries) == 720
    manifest_path = SESSION / "task-1-evidence/run079-gate-fix-final-fmt-check.sources.json"
    assert sha(manifest_path.read_bytes()) == "565c56b5908332628c3a96aef6836ddbb92041394d35ce4746ffa988bed43c4e"
    author_manifest = json.loads(manifest_path.read_text())
    mismatches = [p for p, expected in author_manifest.items() if copied.get(p, {}).get("sha256") != expected]
    assert not mismatches, mismatches
    original_mismatches = [p for p, info in copied.items() if not (WORKTREE / p).exists() or identity(WORKTREE / p) != info]
    build_mismatches = [p for p in original_mismatches if p.startswith("src-tauri/") or p.startswith("tests/") or p in {"Cargo.toml", "Cargo.lock"}]
    assert not build_mismatches, build_mismatches
    save("source-before-fixture.json", copied)
    accepted_path = WORKTREE / "docs/superpowers/handoffs/2026-11-10-summary-manual-task1/evidence/accepted-environment.json"
    accepted = json.loads(accepted_path.read_text())
    assert len(accepted["environment"]) == 6
    for key, value in accepted["environment"].items():
        assert Path(value).is_dir(), (key, value)
    summary = {
        "reviewed_tree": FROZEN_TREE,
        "archive_sha256": sha(archive.read_bytes()),
        "source_files": len(copied),
        "source_bytes": sum(info["bytes"] for info in copied.values()),
        "author_manifest_paths_checked": len(author_manifest),
        "author_manifest_mismatches": mismatches,
        "original_copy_mismatches": original_mismatches,
        "build_input_mismatches": build_mismatches,
        "original_baseline_files": len(original),
        "original_baseline_sha256": sha((VALIDATION / "original-baseline.json").read_bytes()),
        "source_before_fixture_sha256": sha((VALIDATION / "source-before-fixture.json").read_bytes()),
        "accepted_environment_sha256": sha(accepted_path.read_bytes()),
        "environment": accepted["environment"],
        "disk_free_bytes": shutil.disk_usage(VALIDATION).free,
        "prepared_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    }
    save("copy-verification.json", summary)
    print(json.dumps(summary, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
