#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# SSC-026: build the browser engine (crates/web: the unchanged core decoder
# behind a C ABI) for wasm32-unknown-unknown and copy it into the docs site
# with a provenance file. The built .wasm is kept in site/public/engine/ so
# the site builds offline without Rust.
#
#   bash scripts/build-engine.sh           build and copy
#   bash scripts/build-engine.sh --verify  rebuild and fail if the committed wasm differs
#
# Needs the pinned toolchain (rust-toolchain.toml) with the
# wasm32-unknown-unknown target (`rustup target add wasm32-unknown-unknown`).
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=site/public/engine
mkdir -p "$OUT"
CARGO_INCREMENTAL=0 cargo build -p sik-storage-schema-web --target wasm32-unknown-unknown --profile engine
if [ "${1:-}" = "--verify" ]; then
  if cmp -s target/wasm32-unknown-unknown/engine/sik_storage_schema_web.wasm "$OUT/storage-schema-engine.wasm"; then
    echo "engine: committed wasm equals a fresh build of the current sources"
    exit 0
  fi
  echo "engine: committed wasm differs from a fresh build; run bash scripts/build-engine.sh and (cd site && pnpm demo:data)" >&2
  exit 1
fi
cp target/wasm32-unknown-unknown/engine/sik_storage_schema_web.wasm "$OUT/storage-schema-engine.wasm"
SHA=$(sha256sum "$OUT/storage-schema-engine.wasm" | cut -d' ' -f1)
SIZE=$(stat -c %s "$OUT/storage-schema-engine.wasm")
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cat > "$OUT/storage-schema-engine.provenance.json" <<JSON
{
  "format": "sik.storage-schema.engine-provenance",
  "built_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "crate": "sik-storage-schema-web $VERSION (links sik-storage-schema-core $VERSION, feature rpc off)",
  "rustc": "$(rustc --version)",
  "target": "wasm32-unknown-unknown",
  "profile": "engine (opt-level z, lto, codegen-units 1, panic abort, strip)",
  "command": "bash scripts/build-engine.sh",
  "sha256": "$SHA",
  "size": $SIZE
}
JSON
echo "engine: $OUT/storage-schema-engine.wasm ($SIZE bytes, sha256 $SHA)"
