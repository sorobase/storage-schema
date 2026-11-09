#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Capture read-only mainnet fixtures for the decoder demo. No keys, no
# writes: getLatestLedger, getEvents and getLedgerEntries only, at most
# ~1 request per second.
#
# Output (fixtures/mainnet/):
#   usdc-sac.instance.json        getLedgerEntries response: USDC SAC instance
#   usdc-sac.balances.json        getLedgerEntries response: DataKey::Balance of contract holders
#   holder.instance.json          getLedgerEntries response: instance of one holder contract (wasm)
#   provenance.json               RPC URL, ledgers, date, keys, holder wasm hash and sections
#
# Requires: the CLI (scripts/build-demos.sh builds it), curl, python3, stellar (CLI 25.2.0, for `xdr decode`).
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
RPC=${RPC:-https://mainnet.sorobanrpc.com}
USDC_SAC=${USDC_SAC:-CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75}
SIDE=sidecars/stellar-asset-contract.storage.json
OUT=$ROOT/fixtures/mainnet
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$OUT"
CLI=$ROOT/target/debug/storage-schema
[ -x "$CLI" ] || cargo build -p sik-storage-schema-cli

rpc() { # method params-json
  sleep 1
  curl -sS --max-time 30 -H 'content-type: application/json' -A 'sik-storage-schema/0.1 (read-only)' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":$2}" "$RPC"
}

LATEST=$(rpc getLatestLedger '{}' | python3 -I -c 'import sys,json; print(json.load(sys.stdin)["result"]["sequence"])')
echo "latest ledger $LATEST"

# 1. Instance of the USDC SAC (the key builder returns the instance key for instance entries).
IKEY=$("$CLI" key --schema "$SIDE" --contract "$USDC_SAC" --entry InstanceDataKey::Admin)
sleep 1
"$CLI" rpc entries --rpc "$RPC" --key-xdr "$IKEY" > "$OUT/usdc-sac.instance.json"

# 2. Contract addresses that recently sent or received USDC (transfer event topics 1 and 2).
START=$((LATEST - 5000))
rpc getEvents "{\"startLedger\":$START,\"filters\":[{\"type\":\"contract\",\"contractIds\":[\"$USDC_SAC\"]}],\"pagination\":{\"limit\":200}}" > "$TMP/events.json"
python3 -I -c '
import sys, json
ev = json.load(open(sys.argv[1]))
for e in ev.get("result", {}).get("events", []):
    t = e.get("topic", [])
    if len(t) >= 3:
        print(t[1]); print(t[2])
' "$TMP/events.json" | sort -u > "$TMP/topics.txt"
: > "$TMP/holders.txt"
while read -r t; do
  a=$(echo "$t" | stellar xdr decode --type ScVal --input single-base64 --output json 2>/dev/null | python3 -I -c 'import sys,json; v=json.load(sys.stdin); print(v.get("address",""))' || true)
  case "$a" in C*) echo "$a" >> "$TMP/holders.txt" ;; esac
done < "$TMP/topics.txt"
sort -u "$TMP/holders.txt" | grep -v "^$USDC_SAC$" | head -5 > "$TMP/holders5.txt" || true
echo "contract holders: $(wc -l < "$TMP/holders5.txt")"
[ -s "$TMP/holders5.txt" ] || { echo "no contract holders found in the last 5000 ledgers" >&2; exit 1; }

# 3. Balance entries of those holders.
ARGS=()
while read -r h; do
  ARGS+=(--key-xdr "$("$CLI" key --schema "$SIDE" --contract "$USDC_SAC" --entry DataKey::Balance "$h")")
done < "$TMP/holders5.txt"
sleep 1
"$CLI" rpc entries --rpc "$RPC" "${ARGS[@]}" > "$OUT/usdc-sac.balances.json"

# 4. One holder contract that runs wasm: its instance, and whether its wasm has a storage schema.
HOLDER=""
HOLDER_WASM="{}"
while read -r h; do
  sleep 1
  info=$("$CLI" rpc wasm --rpc "$RPC" --contract "$h" -o "$TMP/holder.wasm" || true)
  if echo "$info" | grep -q '"executable":"wasm"'; then
    HOLDER=$h
    HOLDER_WASM=$("$CLI" inspect "$TMP/holder.wasm" --format json | python3 -I -c '
import sys, json
r = json.load(sys.stdin)
print(json.dumps({"bytes": r["wasm_bytes"], "has_contractstoragev0": r["contractstoragev0"] is not None,
                  "custom_sections": [s["name"] for s in r["custom_sections"]]}))')
    HOLDER_WASM=$(python3 -I -c 'import sys,json; a=json.loads(sys.argv[1]); a.update(json.loads(sys.argv[2])); print(json.dumps(a))' "$info" "$HOLDER_WASM")
    break
  fi
done < "$TMP/holders5.txt"
if [ -n "$HOLDER" ]; then
  HKEY=$("$CLI" key --schema "$SIDE" --contract "$HOLDER" --entry InstanceDataKey::Admin)
  sleep 1
  "$CLI" rpc entries --rpc "$RPC" --key-xdr "$HKEY" > "$OUT/holder.instance.json"
fi

python3 -I - "$OUT/provenance.json" "$RPC" "$LATEST" "$USDC_SAC" "$HOLDER" "$HOLDER_WASM" "$TMP/holders5.txt" <<'PY'
import sys, json, datetime
out, rpc, latest, sac, holder, hw, holders = sys.argv[1:8]
json.dump({
    "captured": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "script": "scripts/capture-mainnet.sh",
    "network": "mainnet (read-only)",
    "rpc": rpc,
    "latest_ledger_at_start": int(latest),
    "usdc_sac": sac,
    "events_window": f"getEvents from ledger {int(latest) - 5000}, limit 200, contract {sac}",
    "holders_queried": [l.strip() for l in open(holders) if l.strip()],
    "holder_contract": holder or None,
    "holder_wasm": json.loads(hw),
    "note": "The holder wasm itself is not stored; only its hash, size and section names.",
}, open(out, "w"), indent=2)
PY
echo "fixtures written to $OUT"
