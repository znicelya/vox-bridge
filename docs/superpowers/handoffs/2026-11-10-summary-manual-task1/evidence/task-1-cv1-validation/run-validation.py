#!/usr/bin/env python3
"""One CV-1 Cargo invocation, with separate preserved command/source/raw evidence."""
from __future__ import annotations

import datetime
import difflib
import hashlib
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import time

WORKTREE = Path("/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation")
SESSION = WORKTREE / ".superpowers/sdd/2026-11-09-session-summary-continuation"
VALIDATION = SESSION / "task-1-cv1-validation"
SOURCE = VALIDATION / "source"
FIXTURE = "summary::recovery::cv1_validation::cv1_recovery_near_200000_native_history_one_measurement"
RECOVERY = "src-tauri/src/summary/recovery.rs"
TEST_FILE = "src-tauri/src/summary/recovery/cv1_validation.rs"
GIT_ENV = {**os.environ, "GIT_OPTIONAL_LOCKS": "0"}


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def save(name: str, value: object) -> None:
    (VALIDATION / name).write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def source_manifest() -> dict:
    result = {}
    for path in sorted(SOURCE.rglob("*")):
        if path.is_symlink():
            data = os.readlink(path).encode()
            kind = "symlink"
        elif path.is_file():
            data = path.read_bytes()
            kind = "file"
        else:
            continue
        result[str(path.relative_to(SOURCE))] = {"kind": kind, "bytes": len(data), "sha256": sha(data)}
    return result


def descendants(pid: int) -> set[int]:
    found = {pid}
    parents = {}
    for path in Path("/proc").iterdir():
        if not path.name.isdigit():
            continue
        try:
            # The comm field may contain spaces or parentheses; split after it.
            fields = (path / "stat").read_text().rsplit(")", 1)[1].split()
            parents[int(path.name)] = int(fields[1])
        except (OSError, ValueError, IndexError):
            continue
    while True:
        new = {child for child, parent in parents.items() if parent in found}
        if new <= found:
            return found
        found |= new


def main() -> None:
    if (VALIDATION / "command.json").exists() or (VALIDATION / "measurement.json").exists() or (VALIDATION / "data").exists():
        raise SystemExit("Refusing a repeated invocation or replacement of measurement data")
    before = json.loads((VALIDATION / "source-before-fixture.json").read_text())
    after = source_manifest()
    changed = [p for p, info in before.items() if after.get(p) != info]
    added = sorted(set(after) - set(before))
    assert changed == [RECOVERY], changed
    assert added == [TEST_FILE], added
    original = subprocess.check_output(
        ["git", "show", "a907fe5b9eb006cba8533ab57831c18cda485b7c:" + RECOVERY],
        cwd=WORKTREE, env=GIT_ENV,
    )
    wiring = b'\n#[cfg(test)]\n#[path = "recovery/cv1_validation.rs"]\nmod cv1_validation;\n'
    assert (SOURCE / RECOVERY).read_bytes() == original + wiring
    save("source-at-cargo-start.json", after)
    patch = "".join(difflib.unified_diff(
        original.decode().splitlines(keepends=True),
        (SOURCE / RECOVERY).read_text().splitlines(keepends=True),
        fromfile="a/" + RECOVERY, tofile="b/" + RECOVERY,
    ))
    patch += "".join(difflib.unified_diff(
        [], (SOURCE / TEST_FILE).read_text().splitlines(keepends=True),
        fromfile="/dev/null", tofile="b/" + TEST_FILE,
    ))
    (VALIDATION / "fixture.diff").write_text(patch)
    save("fixture-identities.json", {
        "changed_frozen_paths": changed,
        "added_fixture_paths": added,
        "production_function_bytes_preserved": True,
        "frozen_recovery_sha256": sha(original),
        "recovery_with_test_wiring": after[RECOVERY],
        "temporary_test": after[TEST_FILE],
        "fixture_diff_sha256": sha(patch.encode()),
        "source_at_cargo_start_sha256": sha((VALIDATION / "source-at-cargo-start.json").read_bytes()),
    })
    accepted_path = WORKTREE / "docs/superpowers/handoffs/2026-11-10-summary-manual-task1/evidence/accepted-environment.json"
    accepted = json.loads(accepted_path.read_text())
    overrides = accepted["environment"]
    assert len(overrides) == 6
    extra = {
        "VOXBRIDGE_CV1_DATA_DIR": str(VALIDATION / "data"),
        "VOXBRIDGE_CV1_ARTIFACT_DIR": str(VALIDATION),
    }
    env = {**os.environ, **overrides, **extra}
    assert "RUST_TEST_THREADS" not in env, "default libtest concurrency required"
    argv = ["cargo", "test", "--offline", "--locked", "--manifest-path", str(SOURCE / "src-tauri/Cargo.toml"), "--lib", FIXTURE, "--", "--exact", "--nocapture"]
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    command = {
        "started_utc": started,
        "cwd": str(SOURCE / "src-tauri"),
        "argv": argv,
        "environment_overrides": overrides,
        "fixture_environment": extra,
        "default_libtest_concurrency": True,
        "accepted_environment_sha256": sha(accepted_path.read_bytes()),
        "source_manifest_sha256": sha((VALIDATION / "source-at-cargo-start.json").read_bytes()),
        "rustc_version": subprocess.check_output(["rustc", "--version", "--verbose"], env=env, text=True),
        "inherited_build_environment": {key: os.environ.get(key) for key in ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC_WRAPPER", "CARGO_BUILD_JOBS", "CARGO_INCREMENTAL", "LD_LIBRARY_PATH"]},
        "shell_reproduction": shlex.join(["env", *[key + "=" + value for key, value in overrides.items()], *[key + "=" + value for key, value in extra.items()], *argv]),
        "runner_pid": os.getpid(),
    }
    with (VALIDATION / "command.json").open("x") as output:
        json.dump(command, output, indent=2, sort_keys=True)
        output.write("\n")
    print("CV1_COMMAND " + json.dumps({"cwd": command["cwd"], "argv": argv, "environment_overrides": overrides}), flush=True)
    log_path = VALIDATION / "cargo-test.log"
    clock = time.monotonic_ns()
    with log_path.open("xb") as log:
        process = subprocess.Popen(argv, cwd=command["cwd"], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        save("process-identities.json", {"cargo_pid": process.pid, "runner_pid": os.getpid()})
        seen = {process.pid}
        assert process.stdout is not None
        for line in iter(process.stdout.readline, b""):
            seen |= descendants(process.pid)
            log.write(line)
            log.flush()
            decoded = line.decode(errors="replace")
            if decoded.startswith(("CV1_", "error", "warning", "test result:", "test summary::")) or "Finished `test`" in decoded or "Running unittests" in decoded:
                print(decoded.rstrip(), flush=True)
        process.stdout.close()
        code = process.wait()
    final = source_manifest()
    save("source-after-cargo.json", final)
    body = log_path.read_bytes()
    result = {
        "started_utc": started,
        "finished_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "cargo_exit_code": code,
        "cargo_elapsed_ns": time.monotonic_ns() - clock,
        "raw_log_path": str(log_path), "raw_log_bytes": len(body), "raw_log_sha256": sha(body),
        "command_sha256": sha((VALIDATION / "command.json").read_bytes()),
        "source_after_cargo_sha256": sha((VALIDATION / "source-after-cargo.json").read_bytes()),
        "source_after_cargo_changes": [p for p in after if after[p] != final.get(p)],
        "source_after_cargo_added": sorted(set(final) - set(after)),
        "cargo_pid": process.pid, "observed_cargo_descendant_pids": sorted(seen),
        "measurement_present": (VALIDATION / "measurement.json").exists(),
        "single_fixture_result": "1 passed; 0 failed; 0 ignored;" in body.decode(errors="replace"),
    }
    save("run-result.json", result)
    print("CV1_RUN_RESULT " + json.dumps(result, sort_keys=True), flush=True)
    sys.exit(code)


if __name__ == "__main__":
    main()
