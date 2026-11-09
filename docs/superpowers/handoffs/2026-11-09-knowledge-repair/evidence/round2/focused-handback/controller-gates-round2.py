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
EVIDENCE = AUTHOR_EVIDENCE / "final-round2"
PREVIOUS_TREE = "da23738987e178f23b1f5bda34c9c8250d5f4f4e"
BASE = "b72556091fc533dafb0a336907bf077fec377615"
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
        raise RuntimeError("HEAD changed since the repair BASE")
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
                                  "frozen-review.index", "complete-repair-review.diff")
                if (EVIDENCE / name).exists()]
    if existing:
        raise RuntimeError("preserve the prior freeze before another round: " + ", ".join(existing))
    report = AUTHOR_EVIDENCE / "task-4-completion-report.md"
    repair_report = AUTHOR_EVIDENCE / "round2-repair-report.md"
    if not repair_report.is_file():
        raise RuntimeError("new explicit round2 handback report is required")
    if not report.is_file():
        raise RuntimeError("explicit frozen source/Cargo handback report is required")
    changed = set(git("diff", "HEAD", "--name-only", "--no-renames").splitlines())
    changed.update(git("ls-files", "--others", "--exclude-standard").splitlines())
    source = sorted(p for p in changed if p.startswith("src-tauri/"))
    if not source:
        raise RuntimeError("no repaired source to freeze")
    allowed = lambda p: (p.startswith("src-tauri/src/knowledge/")
                         or p == "src-tauri/src/intelligence/jobs.rs"
                         or p.startswith("src-tauri/src/intelligence/jobs/")
                         or p == "src-tauri/src/app/documents/tests.rs")
    if any(not allowed(p) for p in source):
        raise RuntimeError("source outside knowledge and the audited actual-reap job dependency needs an explicit audit")
    hashes = {p: digest(ROOT / p) for p in source}
    inputs = [
        "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "package.json", "package-lock.json",
        "src/lib/knowledge-wire.ts", "src/lib/knowledge-wire.test.ts",
        "docs/superpowers/plans/2026-11-09-knowledge-review-fixes.md",
        "docs/superpowers/reviews/2026-11-09-task5c.md",
        "docs/superpowers/reviews/2026-11-09-delete-proof-ruling.md",
        str(report.relative_to(ROOT)),
        str((AUTHOR_EVIDENCE / "task-4-completion-brief.md").relative_to(ROOT)),
        str(repair_report.relative_to(ROOT)),
        str((AUTHOR_EVIDENCE / "round2-repair-brief.md").relative_to(ROOT)),
        "docs/superpowers/reviews/2026-11-09-task5c-repair.md",
        str((AUTHOR_EVIDENCE / "final-gate-repair-report.md").relative_to(ROOT)),
        str((AUTHOR_EVIDENCE / "final-round1/source-freeze.json").relative_to(ROOT)),
        str((AUTHOR_EVIDENCE / "final-round1/final-verification.json").relative_to(ROOT)),
        str((AUTHOR_EVIDENCE / "complete-repair-round1-review-brief.md").relative_to(ROOT)),
        str((AUTHOR_EVIDENCE / "complete-repair-round2-review-brief.md").relative_to(ROOT)),
        str(Path(__file__).resolve().relative_to(ROOT)),
    ]
    input_hashes = {p: digest(ROOT / p) for p in inputs if (ROOT / p).is_file()}
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
    package = EVIDENCE / "complete-repair-review.diff"
    with package.open("wb") as out:
        out.write(("Task5C complete repair; immutable tree before final gates\n"
                   + "BASE " + BASE + "\nTREE " + tree + "\n\n").encode())
        subprocess.run(["git", "diff", "--stat", BASE, tree], cwd=ROOT,
                       stdout=out, check=True)
        out.write(b"\n")
        subprocess.run(["git", "diff", "--binary", "-U10", BASE, tree], cwd=ROOT,
                       stdout=out, check=True)
    scoped = EVIDENCE / "scoped-round2-review.diff"
    with scoped.open("wb") as out:
        out.write(("Task5C repair round2; scoped immutable source delta\n"
                   + "PREVIOUS_TREE " + PREVIOUS_TREE + "\nTREE " + tree + "\n\n").encode())
        subprocess.run(["git", "diff", "--stat", PREVIOUS_TREE, tree], cwd=ROOT,
                       stdout=out, check=True)
        out.write(b"\n")
        subprocess.run(["git", "diff", "--binary", "-U10", PREVIOUS_TREE, tree], cwd=ROOT,
                       stdout=out, check=True)
    result = {"created_utc": stamp(), "base": BASE, "tree": tree,
              "previous_tree": PREVIOUS_TREE,
              "scoped_package_sha256": digest(scoped), "scoped_package_bytes": scoped.stat().st_size,
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
    task_env.update({
        "CARGO_HOME": str(ROOT / ".superpowers/toolchain/cargo-writable-20261106"),
        "CARGO_TARGET_DIR": str(ROOT / ".superpowers/toolchain/cargo-target-intelligence-20261107"),
        "TMPDIR": "/dev/shm/voxbridge-intelligence-20261109.q7xaof9u",
        "PKG_CONFIG_PATH": str(ROOT / ".superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu/pkgconfig"),
        "PKG_CONFIG_SYSROOT_DIR": str(ROOT / ".superpowers/toolchain/alsa/sysroot"),
        "LIBRARY_PATH": str(ROOT / ".superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu"),
    })
    if not Path(task_env["TMPDIR"]).is_dir():
        task_env["TMPDIR"] = tempfile.mkdtemp(prefix="voxbridge-repair-gates-", dir="/dev/shm")
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
        raise SystemExit("usage: controller-gates.py freeze|verify")
    EVIDENCE.mkdir(exist_ok=True)
    (freeze if sys.argv[1] == "freeze" else verify)()
