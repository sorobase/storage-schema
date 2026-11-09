# Tasks (prefix SSC)

Status values: `TODO`, `IN-PROGRESS(date)`, `DONE(date, note)`, `BLOCKED(reason)`.

| ID | Title | Deps | Acceptance | Status |
|---|---|---|---|---|
| SSC-001 | Prior-art re-check | - | `PRIOR-ART.md` with dates and verdict | DONE(2026-11-03, partial overlap; scope narrowed to the off-chain half; not a stop) |
| SSC-002 | Format spec | 001 | SPEC 3-4 incl. answer to Leigh's question and hard cases | DONE(2026-11-03) |
| SSC-003 | Core model + XDR codec | 002 | round-trip + pinned wire layout tests | DONE(2026-11-03) |
| SSC-004 | Wasm section read/inject/strip | 003 | append/strip round-trip; code digest unchanged | DONE(2026-11-03) |
| SSC-005 | Sidecar JSON + JSON Schema | 003 | parse/emit round-trip; `schemas/storage-schema.v0.schema.json` | DONE(2026-11-03) |
| SSC-006 | Decoder (matching, precedence, statuses, rendering) | 003 | unit tests for each status and the hard cases | DONE(2026-11-03) |
| SSC-007 | Lint (collisions, undefined types) | 006 | #2088 collision found | DONE(2026-11-03) |
| SSC-008 | Macros: `#[storage_schema]`, `#[storage_key]`, `storage_schema_extern!` | 003 | demos compile; section present; type entries via `spec_xdr()` | DONE(2026-11-03; `storage_schema_extern!` demo user added 2026-11-08 in SSC-013) |
| SSC-009 | Demos: token + increment (macro), OZ pausable (inject) on sdk 28.0.0 | 008 | e2e: every host-written entry decodes `matched` | DONE(2026-11-03) |
| SSC-010 | CLI | 004-007 | inspect/inject/strip/lint/decode/key/rpc/size + tests | DONE(2026-11-03) |
| SSC-011 | Size measurement [M] and mainnet decode demo | 009, 010 | `fixtures/size-report.json`; USDC SAC fixtures decode `matched` | DONE(2026-11-03, token 885 B / increment 72 B / OZ 1,273 B, code identical; found and fixed the `#[used]` data-copy bug; mainnet 8/8 matched) |
| SSC-012 | Section survives `stellar contract build`/`optimize` with a shaking-v2 CLI | - | test with a newer stellar-cli; document order | IN-PROGRESS(2026-11-03, CLI 25.2.0: kept by build and `--optimize`; newer CLIs unchecked) |
| SSC-013 | Cross-crate emission: a library annotates its own enum (or uses `storage_schema_extern!`) and the section reaches the contract | 008 | demo library crate + e2e test | DONE(2026-11-08, `demos/oz-keys` declares OZ 0.8.0 keys with `storage_schema_extern!`; `demos/oz-extern` links it; e2e: 6/6 entries matched, no warnings. Finding [M]: without `use ssc_demo_oz_keys as _;` the build succeeds and the section has 1 key instead of 5; private OZ `Metadata` needs a mirror type) |
| SSC-014 | Fuzz `decode_stream`, `wasm::sections`, sidecar parser | 003-005 | cargo-fuzz targets; 1 h without panic | TODO |
| SSC-015 | Golden decoder-output files | 006 | snapshot tests | DONE(2026-11-08, `fixtures/scenarios.json` (9 scenarios) -> `scripts/golden.sh` -> `fixtures/golden/`; CLI test `golden_outputs_match` asserts equality and that all four statuses occur; drift caught when a golden was edited by hand) |
| SSC-016 | Draft PR text for soroban-sdk-tools `#[contractstorage]` to emit the section | 008 | text in `drafts/` (user posts) | TODO |
| SSC-017 | Indexer-facing decode example (stellar-ledger-data-indexer shape) | 006 | JSON contract + example; no UI | TODO |
| SSC-018 | Integer-discriminant key enums in the macro (emit `EXACT(U32(n))`) | 008 | test | TODO |
| SSC-019 | Read #2127 `derive_contract_data.rs` and map its value encoding | - | SPEC note; optional `#[storage_schema]` on `#[contractdata]` structs | TODO |
| SSC-020 | History-aware schema selection (wasm active at `last_modified_ledger`) | holders-at-ledger state-at-ledger | decode of pre-upgrade entries | TODO |
| SSC-021 | TypeScript decoder `@sik/storage-schema` | 006 | parity tests against the Rust decoder on the same fixtures | TODO |
| SSC-022 | Importers for safeguard and soroban-upgrades schema files | 005 | convert their example files | TODO |
| SSC-023 | Re-check #1818/#1762/#2127 and prior art right before the user posts | - | PRIOR-ART.md dated update | TODO |
| SSC-024 | Test-host captures: entries each demo contract writes in the soroban test host, as getLedgerEntries JSON | 009 | `scripts/capture-testhost.sh`, `fixtures/testhost/` + provenance; core `ledger::to_get_ledger_entries_json` round-trip test | DONE(2026-11-08, 4 captures; two independent runs byte-identical) |
| SSC-025 | Shared in-memory API (`core::api`) for CLI and browser engine | 006, 010 | CLI uses it for decoder building, inspect JSON, pattern text; unit tests | DONE(2026-11-08, 6 tests incl. value_mismatch on real SAC balances and code-hash check) |
| SSC-026 | Browser engine `crates/web` (C ABI, no wasm-bindgen) + `scripts/build-engine.sh` | 025 | native parity test against goldens; built wasm + provenance in `site/public/engine/` | DONE(2026-11-08, 1,100,577 bytes; `cargo test -p sik-storage-schema-web` 3 tests) |
| SSC-027 | Docs site + playground (`site/`, Astro + Starlight, kit theme) | 015, 024, 026 | builds offline from committed data; `pnpm test` engine == goldens; scenarios, your files, live mainnet read-only | DONE(2026-11-08, 13 pages; 23 engine checks; live mode checked against mainnet 2026-11-08; light/dark, 1440 and 390 px looked at) |
| SSC-028 | MVP docs: MVP-PLAN, DEMO, PITCH, DEPLOY, README status | 027 | files exist; numbers sourced | DONE(2026-11-08) |
| SSC-029 | Publish crates (core, macros, cli) and host the site | 027 | `DEPLOY.md` checklist executed by the project owner | TODO (later phase; agents do not publish) |
| SSC-030 | Lint or CI check that warns when a dependency's schema declarations did not reach the wasm (the SSC-013 linking pitfall) | 013 | e.g. `inspect --expect-lib stellar-tokens@0.8.0` exits 2 when absent | TODO |
| SSC-031 | Shrink the browser engine (1.1 MB): drop `syn` from the decode path or split sidecar parsing | 026 | engine < 500 KB with the same goldens | TODO |
| SSC-032 | Testnet deployment of a demo contract (runtime throwaway key) so live mode can read a schema from on-chain wasm | 027 | script + recorded contract id; TESTNET-ONLY labels | TODO (stretch, not done in the MVP pass) |
| SSC-033 | Dependency advisory check with network access: `pnpm audit --prod` for `site/` (npm registry timed out from the build machine on 2026-11-08) and `cargo audit` with the yanked check (it stalls offline) | 027 | both exit 0, or every advisory triaged in RISKS | TODO (partial: offline `cargo audit --no-yanked` done, see RISKS R-13) |

## Cross-project requests

1. **Coordinator (root files):** add attribution rows to `NOTICE` and `docs/THIRD_PARTY.md` for code copied into `projects/storage-schema/demos/`:
   - stellar/soroban-examples @03d42aa6b973dcf3a453a99d0c6a6e8d25a196e2, `token/src/*` and `increment/src/*`, Apache-2.0 (token and increment annotated; changes marked in comments);
   - OpenZeppelin/stellar-contracts v0.8.0 @ddc327eb92d63709436da3a8455f641c2d5476c5, `examples/fungible-pausable/src/*`, MIT, unmodified.
2. **wasm-catalog (#8):** could show `contractstoragev0` next to spec-derived docs (`storage-schema inspect --format json`). Optional.
3. **holders-at-ledger (#9):** SSC-020 needs "wasm hash active at ledger N" for a contract.
