import json, os, subprocess, sys, time, hashlib
from pathlib import Path
root = Path(__file__).resolve().parents[4]
evidence = Path(__file__).resolve().parent
accepted = json.loads((evidence.parent / "accepted-environment.json").read_text())
env = os.environ.copy()
env.update(accepted["environment"])
label = sys.argv[1]
command = ["cargo", "test", "--offline", "--locked", "--manifest-path", "src-tauri/Cargo.toml", "--lib", *sys.argv[2:]]
log = evidence / (label + ".log")
if log.exists():
    raise SystemExit("refusing to overwrite evidence")
sources = {}
for base in [root / "src-tauri/src", root / "src-tauri/migrations"]:
    for path in sorted(base.rglob("*")):
        if path.is_file(): sources[str(path.relative_to(root))] = hashlib.sha256(path.read_bytes()).hexdigest()
source_log = evidence / (label + ".sources.json")
source_log.write_text(json.dumps(sources, sort_keys=True, indent=2)+"\n")
started = time.time()
with log.open("wb") as output:
    result = subprocess.run(command, cwd=root, env=env, stdout=output, stderr=subprocess.STDOUT)
record = {"label": label, "command": command, "environment": accepted["environment"], "cwd": str(root), "exit_code": result.returncode, "elapsed_seconds": time.time()-started, "sources": str(source_log.relative_to(root)), "sources_sha256": hashlib.sha256(source_log.read_bytes()).hexdigest(), "log": str(log.relative_to(root)), "sha256": hashlib.sha256(log.read_bytes()).hexdigest()}
with (evidence / "runs.jsonl").open("a") as ledger:
    ledger.write(json.dumps(record, ensure_ascii=False)+"\n")
print(json.dumps(record, ensure_ascii=False), flush=True)
print(log.read_text(errors="replace")[-16000:], flush=True)
sys.exit(result.returncode)
