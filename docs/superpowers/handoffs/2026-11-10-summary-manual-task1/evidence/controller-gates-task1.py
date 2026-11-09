"""Capture an isolated review tree, then run the final serialized Cargo gates.

Task-local recovery tooling only. Does not touch the live index or user files.
Run only after the implementer's explicit frozen source/Cargo handback.
"""
from pathlib import Path
import datetime
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[3]
AUTHOR_EVIDENCE = Path(__file__).resolve().parent
EVIDENCE = AUTHOR_EVIDENCE / "final-manual-task1"
BASE = "73760d609a050eaddcd8002cec572fbc73c65164"
PROTECTED = "1e708ede61f6522d88e4bea58f5c55e70831f404fd00f7a6710d424446e57a3a"


def stamp():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def git(*args, env=None):
    return subprocess.check_output(["git", *args], cwd=ROOT, env=env).decode().strip()


def write_json(name, value):
    (EVIDENCE / name).write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def guard():
    if git("rev-parse", "HEAD") != BASE:
        raise RuntimeError("HEAD changed since the manual-summary BASE")
    if digest(ROOT / ".gitignore") != PROTECTED:
        raise RuntimeError("protected .gitignore content changed")
    indexed = subprocess.check_output(["git", "show", ":.gitignore"], cwd=ROOT)
    if hashlib.sha256(indexed).hexdigest() != PROTECTED:
        raise RuntimeError("protected .gitignore index content changed")
    if git("diff", "--cached", "--name-only") != ".gitignore":
        raise RuntimeError("unexpected live staging before isolated source freeze")


def freeze():
    guard()
    existing = [name for name in ("source-freeze.json", "frozen-source-paths.txt",
                                  "frozen-review.index", "manual-task1-review.diff")
                if (EVIDENCE / name).exists()]
    if existing:
        raise RuntimeError("preserve the prior freeze before another round: " + ", ".join(existing))
    report = AUTHOR_EVIDENCE / "task-1-report.md"
    acceptance = AUTHOR_EVIDENCE / "task-1-handback-acceptance.json"
    paths = AUTHOR_EVIDENCE / "source-owned-paths.txt"
    if not all(p.is_file() for p in (report, acceptance, paths)):
        raise RuntimeError("Root-audited author report, handback and exact paths are required")
    accepted = json.loads(acceptance.read_text())
    if (accepted.get("base") != BASE
            or accepted.get("source_focused_cargo_returned") is not True
            or accepted.get("report_sha256") != digest(report)):
        raise RuntimeError("explicit current manual source/Cargo handback is required")
    source = paths.read_text().splitlines()
    if not source or len(source) != len(set(source)):
        raise RuntimeError("source allowance must be complete, nonempty and unique")
    for p in source:
        parts = Path(p).parts
        if (not p.startswith("src-tauri/") or Path(p).is_absolute()
                or ".." in parts or "\\" in p or (ROOT / p).is_symlink()
                or not (ROOT / p).is_file()):
            raise RuntimeError("unaudited or noncanonical backend source path: " + p)
    source = sorted(source)
    changed = set(git("diff", "HEAD", "--name-only", "--no-renames").splitlines())
    changed.update(git("ls-files", "--others", "--exclude-standard").splitlines())
    actual = {p for p in changed if p.startswith("src-tauri/")}
    if actual != set(source):
        raise RuntimeError("source delta differs from the Root-audited exact allowance")
    hashes = {p: digest(ROOT / p) for p in source}
    if hashes != accepted.get("source_sha256"):
        raise RuntimeError("source changed since explicit handback acceptance")
    required = [
        "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "package.json",
        "docs/superpowers/plans/2026-11-09-session-summary-continuation.md",
        "docs/superpowers/specs/2026-11-04-live-translation-workbench-design.md",
        "docs/superpowers/handoffs/2026-11-09-knowledge-repair/evidence/task-6-architecture-checkpoint.md",
        "docs/superpowers/handoffs/2026-11-09-knowledge-repair/evidence/task-6-interface-amendments.md",
        str(report.relative_to(ROOT)), str(acceptance.relative_to(ROOT)),
        str(paths.relative_to(ROOT)),
        str((AUTHOR_EVIDENCE / "task-1-brief.md").relative_to(ROOT)),
        str((AUTHOR_EVIDENCE / "task-1-execution-context.md").relative_to(ROOT)),
        str((AUTHOR_EVIDENCE / "global-constraints.md").relative_to(ROOT)),
        str((AUTHOR_EVIDENCE / "accepted-environment.json").relative_to(ROOT)),
        str((AUTHOR_EVIDENCE / "task-1-review-brief.md").relative_to(ROOT)),
        str(Path(__file__).resolve().relative_to(ROOT)),
    ]
    optional = ["package-lock.json", "pnpm-lock.yaml", "bun.lock", "yarn.lock",
                "src/lib/intelligence-wire.ts", "src/lib/intelligence-wire.test.ts"]
    inputs = required + [p for p in optional if (ROOT / p).is_file()]
    input_hashes = {p: digest(ROOT / p) for p in inputs}
    path_file = EVIDENCE / "frozen-source-paths.txt"
    path_file.write_text("\n".join(source) + "\n")
    # A new separate index binds untracked files without disturbing live staging.
    temp_index = EVIDENCE / "frozen-review.index"
    if temp_index.exists():
        raise RuntimeError("review index already exists; preserve the prior freeze")
    build_env = dict(os.environ, GIT_INDEX_FILE=str(temp_index))
    subprocess.run(["git", "read-tree", BASE], cwd=ROOT, env=build_env, check=True)
    subprocess.run(["git", "add", "--pathspec-from-file=" + str(path_file)],
                   cwd=ROOT, env=build_env, check=True)
    tree = git("write-tree", env=build_env)
    package = EVIDENCE / "manual-task1-review.diff"
    with package.open("wb") as out:
        out.write(("Task6 manual Task1; immutable complete source before final gates\n"
                   + "BASE " + BASE + "\nTREE " + tree + "\n\n").encode())
        subprocess.run(["git", "diff", "--stat", BASE, tree], cwd=ROOT,
                       stdout=out, check=True)
        out.write(b"\n")
        subprocess.run(["git", "diff", "--binary", "-U10", BASE, tree], cwd=ROOT,
                       stdout=out, check=True)
    result = {"created_utc": stamp(), "base": BASE, "tree": tree,
              "source_sha256": hashes, "input_sha256": input_hashes,
              "package_sha256": digest(package), "package_bytes": package.stat().st_size,
              "protected_gitignore": PROTECTED, "live_index_untouched": True}
    write_json("source-freeze.json", result)
    guard()
    print(json.dumps({"freeze": tree, "source_paths": len(source),
                      "package_bytes": result["package_bytes"]}), flush=True)


def check_frozen(data):
    guard()
    for path, expected in {**data["source_sha256"], **data["input_sha256"]}.items():
        if digest(ROOT / path) != expected:
            raise RuntimeError("frozen path changed: " + path)


def verify():
    data = json.loads((EVIDENCE / "source-freeze.json").read_text())
    check_frozen(data)
    previous = sorted(path.name for path in EVIDENCE.glob("final-*.log"))
    if (EVIDENCE / "final-verification.json").exists():
        previous.append("final-verification.json")
    if previous:
        raise RuntimeError("preserve prior final gate evidence before a new run: " + ", ".join(previous))
    if os.environ.get("RUST_TEST_THREADS") is not None:
        raise RuntimeError("final covering gates require default test concurrency")
    task_env = dict(os.environ)
    recorded = json.loads((AUTHOR_EVIDENCE / "accepted-environment.json").read_text())
    task_env.update(recorded["environment"])
    if not Path(task_env["TMPDIR"]).is_dir():
        raise RuntimeError("recorded task-local TMPDIR is absent; Root must reconcile it")
    common = ["--offline", "--locked", "--manifest-path", "src-tauri/Cargo.toml"]
    commands = [
        ("all-targets", ["cargo", "test", *common, "--all-targets"]),
        ("production-check", ["cargo", "check", *common, "--lib"]),
        ("desktop-build", ["cargo", "build", *common, "--bin", "vox-bridge"]),
        ("rustfmt", ["cargo", "fmt", "--manifest-path", "src-tauri/Cargo.toml", "--", "--check"]),
        ("frozen-diff-check", ["git", "diff", "--check", data["base"], data["tree"]]),
    ]
    results = {"started_utc": stamp(), "tree": data["tree"],
               "environment": {k: task_env[k] for k in ("CARGO_HOME", "CARGO_TARGET_DIR", "TMPDIR", "PKG_CONFIG_PATH", "PKG_CONFIG_SYSROOT_DIR", "LIBRARY_PATH")},
               "gates": []}
    for name, command in commands:
        check_frozen(data)
        log = EVIDENCE / ("final-" + name + ".log")
        print("START " + name + " " + stamp(), flush=True)
        began = time.monotonic()
        with log.open("wb") as out:
            completed = subprocess.run(command, cwd=ROOT, env=task_env,
                                       stdout=out, stderr=subprocess.STDOUT)
        check_frozen(data)
        content = log.read_text(errors="replace")
        gate = {"name": name, "command": command, "exit_code": completed.returncode,
                "duration_seconds": round(time.monotonic() - began, 2), "log": log.name,
                "sha256": digest(log), "bytes": log.stat().st_size,
                "test_results": re.findall(r"^test result: .*$", content, re.MULTILINE),
                "warnings": re.findall(r"^warning:.*$", content, re.MULTILINE)}
        results["gates"].append(gate)
        write_json("final-verification.json", results)
        print(json.dumps(gate), flush=True)
        if completed.returncode != 0:
            print(content[-12000:], flush=True)
            sys.exit(completed.returncode)
    results["finished_utc"] = stamp()
    results["all_required_gates_exit_zero"] = True
    write_json("final-verification.json", results)


if __name__ == "__main__":
    if len(sys.argv) != 2 or sys.argv[1] not in {"freeze", "verify"}:
        raise SystemExit("usage: controller-gates-task1.py freeze|verify")
    EVIDENCE.mkdir(exist_ok=True)
    (freeze if sys.argv[1] == "freeze" else verify)()
