#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Build the demo contracts and regenerate fixtures/wasm/* plus the size
# measurement in fixtures/size-report.json. Offline after the first fetch
# of crates. Writes provenance next to the fixtures.
#
#   1. stellar contract build (CLI 25.2.0, spec shaking v2) -> *.wasm
#      (token, increment, oz-pausable, oz-extern; oz-keys and capture are libraries)
#   2. raw `cargo rustc` of token/increment with and without
#      `--cfg sik_storage_schema_off` (identical source) -> *.raw.wasm / *.noschema.wasm
#   3. inject sidecars/oz-fungible-pausable-v0.8.0.storage.json into the
#      unmodified OZ wasm -> ssc_demo_oz_pausable.injected.wasm
#   4. storage-schema size for each pair -> fixtures/size-report.json
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
OUT=$ROOT/fixtures/wasm
mkdir -p "$OUT"

# Debug profile: the CLI is a host tool; release adds nothing to the fixtures.
cargo build -p sik-storage-schema-cli
CLI=$ROOT/target/debug/storage-schema

cd "$ROOT/demos"
# soroban-sdk 28 refuses plain `cargo build` for wasm: its build script needs
# SOROBAN_SDK_BUILD_SYSTEM_SUPPORTS_SPEC_SHAKING_V2, set by `stellar contract
# build` (CLI >= 25.2.0), which also runs the spec-shaking post-pass. The
# fixtures are the CLI's (shaken) output: the real-world path.
stellar contract build --out-dir "$OUT"
# Size baseline: the same final crate with the macro output cfg'd away
# (deps reused; flag on the final crate only), and the same crate with the
# schema, both built by the identical raw cargo command (no shaking pass).
# Same CARGO_BUILD_RUSTFLAGS as stellar-cli 25.2.0 uses, so dependencies are reused.
RAWFLAGS="--remap-path-prefix=$HOME/.cargo/registry/src="
for pkg in token increment; do
  n=ssc_demo_$pkg
  CARGO_BUILD_RUSTFLAGS="$RAWFLAGS" SOROBAN_SDK_BUILD_SYSTEM_SUPPORTS_SPEC_SHAKING_V2=1 cargo rustc --manifest-path=$pkg/Cargo.toml --crate-type=cdylib \
    --target=wasm32v1-none --release
  cp "target/wasm32v1-none/release/$n.wasm" "$OUT/$n.raw.wasm"
  CARGO_BUILD_RUSTFLAGS="$RAWFLAGS" SOROBAN_SDK_BUILD_SYSTEM_SUPPORTS_SPEC_SHAKING_V2=1 cargo rustc --manifest-path=$pkg/Cargo.toml --crate-type=cdylib \
    --target=wasm32v1-none --release -- --cfg sik_storage_schema_off
  cp "target/wasm32v1-none/release/$n.wasm" "$OUT/$n.noschema.wasm"
done
cd "$ROOT"

"$CLI" inject "$OUT/ssc_demo_oz_pausable.wasm" --schema sidecars/oz-fungible-pausable-v0.8.0.storage.json \
  --replace -o "$OUT/ssc_demo_oz_pausable.injected.wasm"

{
  echo '{'
  echo '  "measured": "'"$(date -u +%Y-%m-%dT%H:%M:%SZ)"'",'
  echo '  "method": "raw pairs: identical source and cargo rustc command, baseline adds --cfg sik_storage_schema_off to the final crate so the macro emits nothing; OZ pair: stellar contract build output vs post-build inject; unpaired rows: stellar contract build (shaken) outputs",'
  echo '  "reports": ['
  "$CLI" size "$OUT/ssc_demo_token.raw.wasm" --baseline "$OUT/ssc_demo_token.noschema.wasm"; echo ','
  "$CLI" size "$OUT/ssc_demo_increment.raw.wasm" --baseline "$OUT/ssc_demo_increment.noschema.wasm"; echo ','
  "$CLI" size "$OUT/ssc_demo_token.wasm"; echo ','
  "$CLI" size "$OUT/ssc_demo_increment.wasm"; echo ','
  "$CLI" size "$OUT/ssc_demo_oz_extern.wasm"; echo ','
  "$CLI" size "$OUT/ssc_demo_oz_pausable.injected.wasm" --baseline "$OUT/ssc_demo_oz_pausable.wasm"
  echo '  ]'
  echo '}'
} > "$ROOT/fixtures/size-report.json"
python3 -m json.tool "$ROOT/fixtures/size-report.json" > /dev/null

# Provenance.
{
  echo '{'
  echo '  "captured": "'"$(date -u +%Y-%m-%dT%H:%M:%SZ)"'",'
  echo '  "script": "scripts/build-demos.sh",'
  echo '  "rustc": "'"$(rustc --version)"'",'
  echo '  "target": "wasm32v1-none, profile release (opt-level z, lto, codegen-units 1, strip symbols); built by stellar contract build (stellar-cli 25.2.0)",'
  echo '  "soroban_sdk": "=28.0.0",'
  echo '  "upstreams": {'
  echo '    "soroban-examples": "https://github.com/stellar/soroban-examples @ 03d42aa6b973dcf3a453a99d0c6a6e8d25a196e2 (Apache-2.0), token/ and increment/",'
  echo '    "oz-stellar-contracts": "https://github.com/OpenZeppelin/stellar-contracts tag v0.8.0 @ ddc327eb92d63709436da3a8455f641c2d5476c5 (MIT), examples/fungible-pausable/, crates stellar-tokens/stellar-contract-utils/stellar-macros =0.8.0"'
  echo '  },'
  echo '  "files": {'
  first=1
  for f in "$OUT"/*.wasm; do
    [ $first -eq 1 ] || echo ','
    first=0
    printf '    "%s": {"bytes": %s, "sha256": "%s"}' "$(basename "$f")" "$(stat -c %s "$f")" "$(sha256sum "$f" | cut -d' ' -f1)"
  done
  echo
  echo '  }'
  echo '}'
} > "$OUT/provenance.json"
python3 -m json.tool "$OUT/provenance.json" > /dev/null
echo "fixtures/wasm and fixtures/size-report.json regenerated"
