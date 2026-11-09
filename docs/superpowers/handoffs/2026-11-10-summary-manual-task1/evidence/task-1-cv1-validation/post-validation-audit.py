#!/usr/bin/env python3
"""Read-only persisted-fixture and original-preservation checks; no recovery rerun."""
from __future__ import annotations

import datetime
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import uuid

WORKTREE = Path("/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation")
SESSION = WORKTREE / ".superpowers/sdd/2026-11-09-session-summary-continuation"
VALIDATION = SESSION / "task-1-cv1-validation"
SOURCE = VALIDATION / "source"
DATABASE = VALIDATION / "data/vox-bridge.sqlite"
GIT_ENV = {**os.environ, "GIT_OPTIONAL_LOCKS": "0"}
ROWS = 200000


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


def save(name: str, value: object) -> None:
    (VALIDATION / name).write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def git(*args: str) -> bytes:
    return subprocess.check_output(["git", *args], cwd=WORKTREE, env=GIT_ENV)


def identity(path: Path) -> dict:
    if path.is_symlink():
        data = os.readlink(path).encode()
        return {"kind": "symlink", "bytes": len(data), "sha256": sha(data)}
    return {"kind": "file", "bytes": path.stat().st_size, "sha256": file_sha(path)}


def audit_database(seed: dict) -> dict:
    assert not Path(str(DATABASE) + "-wal").exists(), "immutable read requires a fully checkpointed database"
    before = identity(DATABASE)
    connection = sqlite3.connect(DATABASE.as_uri() + "?mode=ro&immutable=1", uri=True)
    connection.row_factory = sqlite3.Row
    scalar = lambda sql: connection.execute(sql).fetchone()[0]
    counts = {
        "native_summaries": scalar("SELECT count(*) FROM session_summaries WHERE origin='native'"),
        "all_summaries": scalar("SELECT count(*) FROM session_summaries"),
        "jobs": scalar("SELECT count(*) FROM intelligence_jobs"),
        "active_jobs": scalar("SELECT count(*) FROM intelligence_jobs WHERE status IN ('queued','running')"),
        "deleting_summaries": scalar("SELECT count(*) FROM session_summaries WHERE deleting<>0"),
        "deleting_sessions": scalar("SELECT count(*) FROM session_lifecycle WHERE summary_session_deleting<>0"),
        "pending_auto_delivery": scalar("SELECT count(*) FROM session_lifecycle WHERE summary_terminal_pending<>0"),
        "responses": scalar("SELECT count(*) FROM summary_responses"),
        "coverage": scalar("SELECT count(*) FROM summary_coverage"),
        "sessions": scalar("SELECT count(*) FROM sessions"),
        "completed_sessions": scalar("SELECT count(*) FROM session_lifecycle WHERE status='completed'"),
    }
    versions = dict(connection.execute("SELECT min(version) AS minimum,max(version) AS maximum,count(DISTINCT version) AS distinct_count FROM session_summaries").fetchone())
    versions["durable_counter"] = scalar("SELECT last_version FROM summary_version_counters")
    foreign_key_violations = [tuple(row) for row in connection.execute("PRAGMA foreign_key_check")]
    ended_at = scalar("SELECT ended_at FROM session_lifecycle")
    issues = []
    issue_count = 0
    payloads = hashlib.sha256()
    payload_rows = 0
    min_bytes = None
    max_bytes = 0
    total_bytes = 0
    query = """
        SELECT s.*,j.id AS actual_job_id,j.kind AS actual_kind,
               j.status AS actual_status,j.session_id AS actual_session,
               j.object_id AS actual_object,j.exclusion_key,j.permanent_key,
               j.payload_json,j.completed_units,j.total_units,
               j.error_code AS actual_error,j.created_at_ms AS actual_created,
               j.updated_at_ms AS actual_updated
        FROM session_summaries s LEFT JOIN intelligence_jobs j ON j.id=s.job_id
        ORDER BY s.version
    """
    for number, row in enumerate(connection.execute(query), 1):
        expected_summary = str(uuid.UUID(int=0x20000000000000000000000000000000 | number))
        expected_job = str(uuid.UUID(int=0x30000000000000000000000000000000 | number))
        checks = {
            "exact_contiguous_version": row["version"] == number,
            "canonical_disjoint_summary_id": row["id"] == expected_summary,
            "canonical_disjoint_job_id": row["job_id"] == expected_job == row["actual_job_id"],
            "exact_session": row["session_id"] == seed["session_id"] == row["actual_session"],
            "exact_object": row["actual_object"] == expected_summary,
            "exact_kind_exclusion_permanent": row["actual_kind"] == "summary" and row["exclusion_key"] == "summary:" + seed["session_id"] and row["permanent_key"] is None,
            "matching_interrupted_terminal": row["status"] == row["actual_status"] == "interrupted" and row["error_code"] == row["actual_error"] == "interrupted",
            "exact_profile_metadata": row["profile_id"] == seed["profile"]["id"] and row["model"] == seed["profile"]["model"],
            "native_not_deleting": row["origin"] == "native" and row["deleting"] == 0,
            "unentered_no_invented_model_output": row["structured_json"] == "null" and row["input_hash"] is None and row["input_final_count"] is None and row["snapshot_last_rowid"] is None and row["raw_response_hash"] is None and row["raw_response_bytes"] == 0 and row["completed_units"] == 0 and row["total_units"] is None,
            "coherent_seconds_milliseconds": row["created_at_ms"] == row["actual_created"] and row["created_at"] == row["created_at_ms"] // 1000 and row["actual_updated"] == row["actual_created"] + 1 and row["actual_created"] >= ended_at * 1000,
        }
        raw = row["payload_json"]
        if isinstance(raw, str):
            encoded = raw.encode()
            digest = hashlib.sha256(encoded).hexdigest()
            checks["actual_raw_sha256_witness"] = row["job_payload_hash"] == digest
            checks["bounded_raw_payload"] = len(encoded) <= 64 * 1024
            try:
                parsed = json.loads(raw)
                checks["strict_full_profile_payload"] = type(parsed) is dict and set(parsed) == {"version", "profiles", "input"} and type(parsed["version"]) is int and parsed["version"] == 1 and parsed["profiles"] == [seed["profile"]] and parsed["input"] == {"summary_id": expected_summary, "session_id": seed["session_id"]}
            except (ValueError, TypeError, KeyError):
                checks["strict_full_profile_payload"] = False
            payloads.update(len(encoded).to_bytes(8, "little"))
            payloads.update(encoded)
            payload_rows += 1
            min_bytes = len(encoded) if min_bytes is None else min(min_bytes, len(encoded))
            max_bytes = max(max_bytes, len(encoded))
            total_bytes += len(encoded)
        else:
            checks["payload_is_text"] = False
        failed = [name for name, ok in checks.items() if not ok]
        if failed:
            issue_count += 1
            if len(issues) < 5:
                issues.append({"version": number, "failed_checks": failed})
    connection.close()
    after = identity(DATABASE)
    aggregate_matches = payloads.hexdigest() == seed["payload_sequence_sha256"]
    expected_counts = {"native_summaries": ROWS, "all_summaries": ROWS, "jobs": ROWS, "active_jobs": 0, "deleting_summaries": 0, "deleting_sessions": 0, "pending_auto_delivery": 0, "responses": 0, "coverage": 0, "sessions": 1, "completed_sessions": 1}
    expected_versions = {"minimum": 1, "maximum": ROWS, "distinct_count": ROWS, "durable_counter": ROWS}
    result = {
        "database": str(DATABASE), "read_mode": "mode=ro&immutable=1",
        "counts": counts, "counts_match_expected": counts == expected_counts,
        "versions": versions, "versions_match_expected": versions == expected_versions,
        "foreign_key_violations": foreign_key_violations,
        "persisted_rows_validated": payload_rows, "rows_with_issues": issue_count, "first_issues": issues,
        "payload_bytes": {"minimum": min_bytes, "maximum": max_bytes, "total": total_bytes},
        "payload_sequence_sha256": payloads.hexdigest(), "payload_sequence_matches_actual_seed": aggregate_matches,
        "database_identity": after, "database_unchanged_by_read_only_audit": before == after,
        "no_recovery_or_provider_call_performed": True,
    }
    result["all_checks_pass"] = counts == expected_counts and versions == expected_versions and not foreign_key_violations and payload_rows == ROWS and issue_count == 0 and aggregate_matches and before == after
    save("sqlite-fixture-audit.json", result)
    return result


def main() -> None:
    assert not (VALIDATION / "post-validation-audit.json").exists(), "preserve the first completed audit"
    baseline = load("original-baseline.json")
    run = load("run-result.json")
    measure = load("measurement.json")
    raw = (VALIDATION / "cargo-test.log").read_bytes()
    lines = raw.decode().splitlines()
    raw_json = [json.loads(line.removeprefix("CV1_MEASUREMENT_JSON ")) for line in lines if line.startswith("CV1_MEASUREMENT_JSON ")]
    log_matches = len(raw_json) == 1 and raw_json[0] == measure and sum(line.startswith("CV1_MEASUREMENT_BEGIN ") for line in lines) == 1 and sum(line.startswith("CV1_MEASUREMENT_RETURN ") for line in lines) == 1 and sha(raw) == run["raw_log_sha256"]
    db = audit_database(load("seed.json"))
    original_mismatches = []
    for relative, expected in baseline["files"].items():
        path = WORKTREE / relative
        if not path.exists() or identity(path) != expected:
            original_mismatches.append(relative)
    state = {
        "head": git("rev-parse", "HEAD").decode().strip(),
        "staged_entries_sha256": sha(git("ls-files", "--stage", "-z")),
        "staged_diff_sha256": sha(git("diff", "--cached", "--binary")),
        "refs_sha256": sha(git("show-ref")),
        "protected_gitignore_sha256": sha(git("show", ":.gitignore")),
    }
    state_matches = all(state[key] == baseline["state"][key] for key in state)
    source_before = load("source-at-cargo-start.json")
    copied_source_mismatches = [relative for relative, expected in source_before.items() if identity(SOURCE / relative) != expected]
    expected_generated = ["src-tauri/gen/schemas/acl-manifests.json", "src-tauri/gen/schemas/capabilities.json", "src-tauri/gen/schemas/desktop-schema.json", "src-tauri/gen/schemas/linux-schema.json"]
    frozen = load("copy-verification.json")
    archive_matches = file_sha(VALIDATION / "frozen-tree.tar") == frozen["archive_sha256"]
    owner = measure["owner_release"]
    cleanup = measure["cleanup"]
    cleanup_checked = cleanup["actual_drain_joined"] and cleanup["actual_drain_within_15s"] and cleanup["scheduler_shutdown_ok"] and cleanup["owner_gone"] and cleanup["process_lock_reacquired"] and (cleanup["direct_slots"], cleanup["actual_slots"], cleanup["config_slots"]) == (16, 1, 2) and owner["file_closed"] and owner["direct_permit_released"] and (owner["direct_slots_before_release"], owner["direct_slots_after_release"], owner["actual_slots_before_owner_release"], owner["actual_slots_after_owner_release"]) == (15, 16, 0, 0)
    live_observed = [pid for pid in run["observed_cargo_descendant_pids"] if Path(f"/proc/{pid}").exists()]
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
    data_files = {str(path.relative_to(VALIDATION)): identity(path) for path in sorted((VALIDATION / "data").rglob("*")) if path.is_file()}
    result = {
        "audit_completed_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "actual_result": measure["returned_code"], "elapsed_ns": measure["elapsed_ns"],
        "cargo_exit_code": run["cargo_exit_code"], "single_fixture_result": run["single_fixture_result"],
        "exact_one_measurement_raw_json_match": log_matches,
        "warnings": [line for line in lines if line.startswith("warning")],
        "database_audit_sha256": file_sha(VALIDATION / "sqlite-fixture-audit.json"),
        "database_checks_pass": db["all_checks_pass"],
        "original_baseline_files_checked": len(baseline["files"]), "original_mismatches": original_mismatches,
        "original_git_state": state, "original_git_state_matches_baseline": state_matches,
        "copied_frozen_plus_fixture_files_checked": len(source_before), "copied_source_mismatches": copied_source_mismatches,
        "copy_build_generated_schema_files": run["source_after_cargo_added"],
        "only_expected_build_generated_schema_files_added": run["source_after_cargo_added"] == expected_generated,
        "frozen_archive_unchanged": archive_matches,
        "actual_owned_cleanup_checked": cleanup_checked,
        "provider_calls": measure["provider_calls"],
        "ready_at_return": measure["ready_at_return"], "ready_after_actual_drain": measure["ready_after_actual_drain"],
        "availability_established": measure["availability_established"],
        "live_observed_cargo_or_child_pids": live_observed, "processes_with_source_copy_cwd": associated,
        "data_artifacts": data_files,
        "original_report": identity(SESSION / "task-1-report.md"),
        "original_review": identity(WORKTREE / "docs/superpowers/reviews/2026-11-09-summary-manual-task1.md"),
    }
    result["validation_audit_pass"] = run["cargo_exit_code"] == 0 and run["single_fixture_result"] and log_matches and db["all_checks_pass"] and not original_mismatches and state_matches and not copied_source_mismatches and archive_matches and cleanup_checked and measure["provider_calls"] == 0 and not live_observed and not associated and measure["returned_code"] in {"Ready", "Timeout"}
    save("post-validation-audit.json", result)
    print(json.dumps(result, indent=2, sort_keys=True))
    assert result["validation_audit_pass"], "see the preserved actual audit result"


if __name__ == "__main__":
    main()
