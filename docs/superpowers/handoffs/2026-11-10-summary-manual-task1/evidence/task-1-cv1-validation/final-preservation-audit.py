#!/usr/bin/env python3
"""Final preservation audit with the sole expressly attributed Root append."""
from __future__ import annotations

import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess

WORKTREE = Path("/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation")
SESSION = WORKTREE / ".superpowers/sdd/2026-11-09-session-summary-continuation"
VALIDATION = SESSION / "task-1-cv1-validation"
SOURCE = VALIDATION / "source"
PROGRESS = ".superpowers/sdd/2026-11-09-session-summary-continuation/progress.md"
GIT_ENV = {**os.environ, "GIT_OPTIONAL_LOCKS": "0"}


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def file_sha(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def load(name: str) -> dict:
    return json.loads((VALIDATION / name).read_text())


def git(*args: str) -> bytes:
    return subprocess.check_output(["git", *args], cwd=WORKTREE, env=GIT_ENV)


def identity(path: Path) -> dict:
    if path.is_symlink():
        data = os.readlink(path).encode()
        return {"kind": "symlink", "bytes": len(data), "sha256": sha(data)}
    return {"kind": "file", "bytes": path.stat().st_size, "sha256": file_sha(path)}


def main() -> None:
    output_path = VALIDATION / "final-preservation-audit.json"
    assert not output_path.exists(), "preserve the first final preservation result"
    strict_paths = ["post-validation-audit.json", "post-validation-audit.log", "sqlite-fixture-audit.json", "cargo-test.log", "measurement.json", "run-result.json"]
    strict_before = {name: identity(VALIDATION / name) for name in strict_paths}
    original = load("original-baseline.json")
    initial = load("post-validation-audit.json")
    assert initial["original_mismatches"] == [PROGRESS]
    assert initial["database_checks_pass"] and initial["exact_one_measurement_raw_json_match"] and initial["actual_owned_cleanup_checked"]
    expected_progress = {
        "kind": "file", "bytes": 50002,
        "sha256": "744959b33638ac321848f0b3fdabb50557b6c1be796c49e211ba712b9f006b4e",
    }
    progress_data = (WORKTREE / PROGRESS).read_bytes()
    base_progress = original["files"][PROGRESS]
    progress_identity = identity(WORKTREE / PROGRESS)
    append_verified = progress_identity == expected_progress and base_progress["bytes"] == 49554 and sha(progress_data[:base_progress["bytes"]]) == base_progress["sha256"]
    unexpected = []
    exact_unchanged = 0
    for relative, expected in original["files"].items():
        if relative == PROGRESS:
            continue
        path = WORKTREE / relative
        if not path.exists() or identity(path) != expected:
            unexpected.append(relative)
        else:
            exact_unchanged += 1
    state = {
        "head": git("rev-parse", "HEAD").decode().strip(),
        "staged_entries_sha256": sha(git("ls-files", "--stage", "-z")),
        "staged_diff_sha256": sha(git("diff", "--cached", "--binary")),
        "refs_sha256": sha(git("show-ref")),
        "protected_gitignore_sha256": sha(git("show", ":.gitignore")),
    }
    state_matches = all(state[key] == original["state"][key] for key in state)
    copied = load("source-at-cargo-start.json")
    copy_changes = [relative for relative, expected in copied.items() if identity(SOURCE / relative) != expected]
    before_fixture = load("source-before-fixture.json")
    recovery = (SOURCE / "src-tauri/src/summary/recovery.rs").read_bytes()
    base_recovery = before_fixture["src-tauri/src/summary/recovery.rs"]
    exact_function_prefix = sha(recovery[:base_recovery["bytes"]]) == base_recovery["sha256"] and recovery[base_recovery["bytes"]:] == b'\n#[cfg(test)]\n#[path = "recovery/cv1_validation.rs"]\nmod cv1_validation;\n'
    run = load("run-result.json")
    measure = load("measurement.json")
    live_seen = [pid for pid in run["observed_cargo_descendant_pids"] if Path(f"/proc/{pid}").exists()]
    associated = []
    for path in Path("/proc").iterdir():
        if not path.name.isdigit():
            continue
        try:
            cwd = (path / "cwd").resolve(strict=True)
            if cwd == SOURCE or SOURCE in cwd.parents:
                associated.append({"pid": int(path.name), "cwd": str(cwd)})
        except (OSError, RuntimeError):
            continue
    strict_after = {name: identity(VALIDATION / name) for name in strict_paths}
    strict_unchanged = strict_before == strict_after
    database_matches = identity(VALIDATION / "data/vox-bridge.sqlite") == initial["data_artifacts"]["data/vox-bridge.sqlite"]
    binary = load("test-binary-identity.json")
    preserved_binary_matches = file_sha(Path(binary["preserved_copy"])) == binary["sha256"]
    result = {
        "completed_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "original_baseline_files": len(original["files"]), "unchanged_original_files": exact_unchanged,
        "unexpected_original_mismatches": unexpected,
        "sole_controller_bookkeeping_exception": {
            "path": PROGRESS, "owner": "/root", "validation_author_modified_original": False,
            "baseline": base_progress, "current": progress_identity,
            "original_prefix_byte_identical": append_verified,
            "appended_bytes": len(progress_data) - base_progress["bytes"],
            "parent_attribution": "controller-progress-authorization.txt",
            "parent_attribution_sha256": file_sha(VALIDATION / "controller-progress-authorization.txt"),
        },
        "original_input_index_ref_state": state, "original_input_index_ref_state_unchanged": state_matches,
        "protected_worktree_gitignore": identity(WORKTREE / ".gitignore"),
        "copied_frozen_plus_fixture_input_files": len(copied), "copied_input_changes": copy_changes,
        "copied_original_recovery_function_bytes_unchanged": exact_function_prefix,
        "strict_failed_audit_and_measurement_artifacts_unchanged": strict_unchanged,
        "strict_failed_audit_original_exit_code": 1, "preserved_artifacts": strict_after,
        "database_unchanged_after_coherence_audit": database_matches,
        "preserved_test_binary_hash_matches": preserved_binary_matches,
        "actual_result": measure["returned_code"], "elapsed_ns": measure["elapsed_ns"],
        "provider_calls": measure["provider_calls"], "availability_established": measure["availability_established"],
        "actual_cleanup": measure["cleanup"],
        "live_observed_cargo_or_child_pids": live_seen, "processes_with_source_copy_cwd": associated,
        "no_recovery_cargo_or_fixture_sql_audit_rerun": True,
    }
    result["preservation_with_exact_root_bookkeeping_exception_pass"] = append_verified and exact_unchanged == len(original["files"]) - 1 and not unexpected and state_matches and not copy_changes and exact_function_prefix and strict_unchanged and database_matches and preserved_binary_matches and not live_seen and not associated
    output_path.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps(result, indent=2, sort_keys=True))
    assert result["preservation_with_exact_root_bookkeeping_exception_pass"], "see the preserved final audit result"


if __name__ == "__main__":
    main()
