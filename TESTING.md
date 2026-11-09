# Testing

All default tests are offline and use committed fixtures. Run `bash scripts/check.sh` (fmt, clippy `-D warnings`, workspace tests, demo tests).

## Layers

| Layer | Where | What it proves |
|---|---|---|
| Unit: codec | `crates/core/src/model.rs` | round-trip; concatenated streams de-duplicate; trailing garbage rejected; **byte layout pinned** (`wire_layout_is_stable`) |
| Unit: wasm | `crates/core/src/wasm.rs` | append/read/strip round-trip; multi-section concatenation; LEB sizes; cost formula; malformed input rejected |
| Unit: types | `crates/core/src/rusttype.rs` | Rust type syntax <-> spec types round-trip; UDT path capture; struct/tuple/union/enum definitions round-trip |
| Unit: sidecar | `crates/core/src/sidecar.rs` | parse -> JSON -> parse -> XDR -> parse is identity; errors name the key; version and enum checks |
| Unit: decoder | `crates/core/src/decode.rs` | tagged match and rendering; exact beats typed; durability partition (with hint); value_mismatch; struct key and value; #2088 ambiguity with value evidence; no schema -> unknown; missing UDT -> warning |
| Unit: lint | `crates/core/src/lint.rs` | #2088 collision found; durability/arity/field-type differences are fine; shadowing is info; undefined types warn |
| Unit: ledger | `crates/core/src/ledger.rs` | key building for the mainnet SAC; RPC response shapes |
| **Demo e2e** | `demos/*/tests/schema_e2e.rs` | the *built wasm* runs in the soroban test host; every contract-data entry it writes decodes as `matched` using only the schema read from that wasm (token, increment); the OZ wasm with an injected sidecar decodes fully, and without a schema everything is `unknown` |
| Unit: api | `crates/core/src/api.rs` | inspect output; SAC sidecar on the mainnet capture; wrong schema gives value_mismatch; lint collision; live helpers (instance key equals the captured key); code SHA-256 mismatch refused |
| Golden outputs (SSC-015) | CLI `golden_outputs_match` | every scenario in `fixtures/scenarios.json` decodes exactly to `fixtures/golden/<id>.decode.json`; the scenarios cover all four statuses |
| Engine parity | `crates/web/tests/parity.rs`; `site/scripts/verify-engine.mjs` (also in `check.sh`) | the engine (native, and the built `.wasm` through the page's own glue) equals the goldens for decode, inspect and lint; key builders match the recorded RPC keys; bad input is an error, not a trap |
| Demo e2e: extern (SSC-013) | `demos/oz-extern/tests/schema_e2e.rs` | keys declared with `storage_schema_extern!` in a library crate reach the wasm; every OZ-written entry decodes `matched` without warnings |
| CLI | `crates/cli/tests/cli.rs` | inspect; inject determinism and sidecar round-trip; strip restores the original bytes; size cost equals section bytes and code is identical; lint finds the token-vs-OZ `Balance(Address)` collision (exit 2); mainnet SAC decode (all matched, USDC asset info, balances); mainnet holder without schema -> all unknown (exit 2 under `--strict`); key builder and bad input |

## Fixtures

| Fixture | Produced by | Provenance |
|---|---|---|
| `fixtures/wasm/ssc_demo_*.wasm` (`stellar contract build` output), `*.raw.wasm` / `*.noschema.wasm` (raw size pair), `*.injected.wasm` | `scripts/build-demos.sh` | `fixtures/wasm/provenance.json` (rustc, sdk, upstream commits, sha256) |
| `fixtures/size-report.json` | `scripts/build-demos.sh` | method field in the file |
| `fixtures/mainnet/usdc-sac.instance.json`, `usdc-sac.balances.json`, `holder.instance.json` | `scripts/capture-mainnet.sh` | `fixtures/mainnet/provenance.json` (RPC URL, ledger, date, holders, holder wasm hash/sections) |
| `fixtures/testhost/*.entries.json` | `scripts/capture-testhost.sh` (runs the demo e2e tests with `SSC_CAPTURE_DIR`) | `fixtures/testhost/provenance.json` (calls made, sha256). Deterministic: two runs on 2026-11-08 were byte-identical |
| `fixtures/scenarios.json`, `fixtures/golden/*` | hand-written scenario list; goldens by `scripts/golden.sh` (native CLI) | each scenario names its inputs and their provenance file |
| `site/public/engine/storage-schema-engine.wasm` | `scripts/build-engine.sh` | `storage-schema-engine.provenance.json` (rustc, profile, sha256); `pnpm demo:data` refuses a mismatch |

To capture new mainnet fixtures:

```
bash scripts/build-demos.sh      # builds the CLI too
bash scripts/capture-mainnet.sh  # read-only
```

Then re-run the tests. Holders and balances change over time; the tests assert shapes and statuses, not amounts.

Wasm fixtures are rebuilt only when the demos or the macro change. Builds are deterministic for a fixed toolchain [I]: the `inject_extract_roundtrip` test compares bytes with the committed injected wasm, so a toolchain change will show up as a test failure.

## Mutation checks (run by hand, 2026-11-03; all caught)

| Mutation | Caught by |
|---|---|
| `Decoder::decode` ignores durability (`if k.durability == durability` -> `if true`) | `decode::tests::durability_partitions_and_reports` and others |
| ambiguous matches resolved to the first candidate (`top.len()` -> `top.len().min(1)`) | `decode::tests::collisions_are_ambiguous_with_value_evidence` and others |
| instance storage map not expanded (`ledger::decode_entries`) | CLI `decode_mainnet_usdc_sac_via_sidecar`, `decode_mainnet_without_schema_reports_unknown` |

Re-run them by hand after changing the decoder.
