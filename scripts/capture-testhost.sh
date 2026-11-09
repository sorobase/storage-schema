#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# SSC-024: capture the contract-data entries each demo contract writes when
# its built wasm (fixtures/wasm/*, from scripts/build-demos.sh) runs in the
# soroban test host (soroban-sdk =28.0.0, testutils). The e2e tests write
# them through demos/capture when SSC_CAPTURE_DIR is set. Output has the
# getLedgerEntries response shape, so the CLI, the browser engine and the
# docs site read test-host captures exactly like RPC captures.
#
# Offline. No network, no keys. The test host's address generator and
# ledger are deterministic, so re-running yields the same bytes [I: checked
# by running twice, see TESTING.md].
#
#   bash scripts/capture-testhost.sh
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
OUT=$ROOT/fixtures/testhost
mkdir -p "$OUT"
(cd demos && SSC_CAPTURE_DIR="$OUT" CARGO_INCREMENTAL=0 cargo test --test schema_e2e)
{
  echo '{'
  echo '  "captured": "'"$(date -u +%Y-%m-%dT%H:%M:%SZ)"'",'
  echo '  "script": "scripts/capture-testhost.sh",'
  echo '  "source": "soroban test host (soroban-sdk =28.0.0 testutils), running the built wasm from fixtures/wasm/ in demos/*/tests/schema_e2e.rs; not a network",'
  echo '  "rustc": "'"$(rustc --version)"'",'
  echo '  "calls": {'
  echo '    "token.entries.json": "ssc_demo_token.wasm: __constructor(admin, 7, \"Demo\", \"DEMO\"); mint(alice, 1000); transfer(alice, bob, 250); approve(alice, bob, 40, 200)",'
  echo '    "increment.entries.json": "ssc_demo_increment.wasm: increment() x3",'
  echo '    "oz-pausable.entries.json": "ssc_demo_oz_pausable.injected.wasm (same code as ssc_demo_oz_pausable.wasm): __constructor(\"OZ Demo\", \"OZD\", owner, 5000); transfer(owner, bob, 1200); approve(owner, bob, 7, 500); pause(owner)",'
  echo '    "oz-extern.entries.json": "ssc_demo_oz_extern.wasm: __constructor(owner, 9000); transfer(owner, bob, 400); approve(owner, bob, 25, 300); pause(owner)"'
  echo '  },'
  echo '  "files": {'
  first=1
  for f in "$OUT"/*.entries.json; do
    [ $first -eq 1 ] || echo ','
    first=0
    printf '    "%s": {"bytes": %s, "sha256": "%s"}' "$(basename "$f")" "$(stat -c %s "$f")" "$(sha256sum "$f" | cut -d' ' -f1)"
  done
  echo
  echo '  }'
  echo '}'
} > "$OUT/provenance.json"
python3 -m json.tool "$OUT/provenance.json" > /dev/null
echo "fixtures/testhost regenerated"
