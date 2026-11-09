#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# SSC-015: golden decoder outputs. Runs the native CLI over every scenario in
# fixtures/scenarios.json and writes fixtures/golden/<id>.decode.json, plus
# <wasm>.inspect.json for each demo wasm and lint-token-oz.json. The
# CLI test `golden_outputs_match` and the browser-engine check
# (site: pnpm test) compare against these files. Offline.
#
#   bash scripts/golden.sh
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -p sik-storage-schema-cli
CLI=$PWD/target/debug/storage-schema
mkdir -p fixtures/golden
python3 - "$CLI" <<'PY'
import json, subprocess, sys
cli = sys.argv[1]
sc = json.load(open("fixtures/scenarios.json"))
for s in sc["scenarios"]:
    args = [cli, "decode", "--format", "json"]
    if s["wasm"]:
        args += ["--wasm", s["wasm"]]
    for p in s["sidecars"]:
        args += ["--schema", p]
    for p in s["entries"]:
        args += ["--entries", p]
    out = subprocess.run(args, check=True, capture_output=True, text=True).stdout
    json.loads(out)
    open(f"fixtures/golden/{s['id']}.decode.json", "w").write(out)
    summary = json.loads(out)["summary"]
    print(s["id"], summary["total"], summary["by_status"])
# inspect --format json for every demo wasm (the playground's "what it carries")
import glob, os
for w in sorted(glob.glob("fixtures/wasm/ssc_demo_*.wasm")):
    if w.endswith((".raw.wasm", ".noschema.wasm")):
        continue
    out = subprocess.run([cli, "inspect", w, "--format", "json"], check=True, capture_output=True, text=True).stdout
    name = os.path.basename(w)[:-5]
    open(f"fixtures/golden/{name}.inspect.json", "w").write(out)
    print("inspect", name)
# lint of the #2088 collision scenario (exit 2 = error findings, expected)
r = subprocess.run([cli, "lint", "--wasm", "fixtures/wasm/ssc_demo_token.wasm", "--schema", "sidecars/oz-fungible-pausable-v0.8.0.storage.json"], capture_output=True, text=True)
assert r.returncode == 2, r.returncode
open("fixtures/golden/lint-token-oz.json", "w").write(r.stdout)
print("lint-token-oz exit", r.returncode)
PY
