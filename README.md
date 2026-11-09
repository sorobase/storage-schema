# storage-schema (project #1, ticket prefix SSC)

An off-chain storage schema for Soroban contracts. A contract describes which keys it writes, with what durability and value type, in a Wasm custom section `contractstoragev0`, separate from the callable spec `contractspecv0`. Tools then turn raw contract-data ledger entries into named, typed records, and say `unknown`, `ambiguous` or `value_mismatch` when they cannot.

This answers the open question in [rs-soroban-sdk#1818](https://github.com/stellar/rs-soroban-sdk/issues/1818) (Leigh, 2026-04-28): *how does an off-chain consumer map a ledger entry to a schema type?* See SPEC section 4.

- `SPEC.md`: normative format, matching rules, CLI and library interfaces
- `drafts/SEP-storage-schema.md`: SEP draft (not submitted)
- `drafts/sdk-1818-comment.md`: comment draft for #1818 (not posted)

## Status (2026-11-08)

MVP pass on 2026-11-08: see `MVP-PLAN.md`. Demo: `DEMO.md`. Investor brief: `PITCH.md`. Publishing checklist: `DEPLOY.md`.

| Area | State | Evidence |
|---|---|---|
| Prior art (SSC-001) | partial overlap, **not a stop**; scope narrowed to the off-chain half | `PRIOR-ART.md` (2026-11-03) |
| Format (`contractstoragev0`, XDR) and SPEC | works; wire layout pinned by a test | SPEC 3-4; `model::tests` |
| Sidecar JSON + JSON Schema | works; round-trips with the section byte for byte | `sidecar::tests`, CLI `inject_extract_roundtrip` |
| Macros (`#[storage_schema]`, `#[storage_key]`) | work on soroban-sdk 28.0.0; UDT definitions embedded via `spec_xdr()` | demos build; `inspect` shows 4 keys and 3 types for the token |
| `storage_schema_extern!` (SSC-013) | **works from a library crate**, if the contract references that crate (`use lib as _;`). Without the reference the declarations are silently absent (measured: 5 keys vs 1) | `demos/oz-keys` + `demos/oz-extern`, e2e `library_declared_extern_keys_reach_the_wasm` |
| Decoder + statuses (matched / ambiguous / value_mismatch / unknown) | works; **all four on real entries** in 9 scenarios with golden outputs (SSC-015) | `fixtures/scenarios.json`, `fixtures/golden/`, CLI `golden_outputs_match` |
| Lint (#2088 collisions) | works; flags token `DataKey::Balance(Address)` vs OZ `FungibleStorageKey::Balance(Address)` | CLI test; `fixtures/golden/lint-token-oz.json` |
| Post-build inject / strip | works; strip restores the original bytes | CLI tests |
| Read-only RPC (`decode --contract --rpc`, `rpc entries`, `rpc wasm`) | works against mainnet | capture at ledger 65,292,081 (2026-11-03); live playground read 2026-11-08 |
| Mainnet demo | USDC SAC instance + 5 contract-holder balances: **8/8 matched** via sidecar; one holder contract (wasm, no section): 37/37 `unknown` | `fixtures/mainnet/*`, CLI tests |
| Test-host captures (SSC-024) | the entries each demo contract writes in the soroban test host, as `getLedgerEntries` JSON; deterministic (two runs byte-identical) | `scripts/capture-testhost.sh`, `fixtures/testhost/` |
| Browser engine (SSC-026) | core decoder compiled to `wasm32-unknown-unknown` (1,100,577 bytes), C ABI, equals the CLI on every scenario | `crates/web`, `site/public/engine/`, `pnpm test` (23 checks); `scripts/build-engine.sh --verify` (part of `check.sh`) fails if the committed wasm differs from a fresh build |
| Docs site + playground (SSC-027) | Astro + Starlight, kit theme; 13 pages; scenarios, your files, live mainnet (read-only) | `site/`, `pnpm build` |
| Section survives `stellar contract build` and `--optimize` (CLI 25.2.0, spec shaking v2) | **yes** (checked); shaking removed every storage struct from the token's `contractspecv0`, so the type entries are needed | `inspect` of `fixtures/wasm/ssc_demo_token.wasm` |
| Size cost [M] | section bytes only, code byte-identical: token 885 B (+6.91% over the identical build without it, 11,915 to 12,800 B raw pair; 10.0% of the 8,885 B shaken wasm that carries it), increment 72 B, OZ sidecar 1,273 B (9.2% of the injected wasm) | `fixtures/size-report.json` (re-measured 2026-11-08, same bytes); SEP table |
| Deterministic builds | re-running `build-demos.sh` on 2026-11-08 reproduced every committed wasm byte for byte (same sha256) | `fixtures/wasm/provenance.json` |
| Tests (2026-11-08, all pass) | core 33 unit; CLI 8 integration; engine 3; demos 6 e2e (token, increment, OZ injected, OZ without schema, OZ extern x2) + 16 upstream unit tests; browser engine 23 checks; clippy `-D warnings` clean (workspace and the project-written demo crates) | `bash scripts/check.sh` |
| Mutation checks | 3/3 caught (ignore durability; pick first candidate; skip instance expansion) | TESTING.md (2026-11-03) |
| Advisory check (2026-11-08) | `cargo audit --no-yanked` on the main lockfile: clean; on `demos/Cargo.lock` (test-only): crypto-bigint 0.5.5 RUSTSEC-2026-0387 via soroban-sdk, not shipped. `pnpm audit` and the yanked check could not reach the network | RISKS R-13, SSC-033 |
| Not checked / not built | newer stellar-cli versions (SSC-012); fuzzing of the XDR and wasm parsers (SSC-014, the real security gap); network advisory check (SSC-033); integer-key enums in the macro (SSC-018); #2127 value encoding (SSC-019); history-aware decoding (SSC-020); TS decoder (SSC-021); importers (SSC-022); crates.io / hosting (SSC-029) | TASKS |
| Drafts | SEP and #1818 comment written, **not posted** | `drafts/`; SEP rendered on the site as "draft, not submitted" |

Bug found by measuring: the first macro version marked its statics `#[used]`, and rustc 1.97.1 then *also* copied the bytes into the wasm data segment (the cost doubled and the code was not identical). It was removed. Without `#[used]` the section is still emitted and the code is byte-identical.

## Quick start

```bash
# toolchain: rust-toolchain.toml pins 1.97.1 with wasm32v1-none
bash scripts/build-demos.sh            # CLI + demo wasm (via `stellar contract build`, CLI 25.2.0) + size report; first run: many minutes
CLI=target/debug/storage-schema

# 1. What a macro-annotated contract carries
$CLI inspect fixtures/wasm/ssc_demo_token.wasm

# 2. Give a contract that never adopts the macro a schema, post-build
$CLI inject fixtures/wasm/ssc_demo_oz_pausable.wasm \
     --schema sidecars/oz-fungible-pausable-v0.8.0.storage.json --replace -o /tmp/oz.wasm

# 3. Decode real mainnet entries (USDC Stellar Asset Contract) through a sidecar, offline
$CLI decode --schema sidecars/stellar-asset-contract.storage.json \
     --entries fixtures/mainnet/usdc-sac.instance.json \
     --entries fixtures/mainnet/usdc-sac.balances.json --format text

# 4. Same, live and read-only (instance + one balance)
$CLI decode --rpc https://mainnet.sorobanrpc.com \
     --contract CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75 \
     --schema sidecars/stellar-asset-contract.storage.json \
     --fetch 'DataKey::Balance=<C... holder>' --format text

# 5. Catch sdk#2088 collisions before deploy
$CLI lint --wasm fixtures/wasm/ssc_demo_token.wasm --schema sidecars/oz-fungible-pausable-v0.8.0.storage.json

# 6. Size cost against an identical build without the schema
$CLI size fixtures/wasm/ssc_demo_token.wasm --baseline fixtures/wasm/ssc_demo_token.noschema.wasm
```

Annotating a contract (the whole diff for soroban-examples `token`):

```rust
#[storage_schema]                      // above #[contracttype]
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    #[storage(temporary, value = AllowanceValue)] Allowance(AllowanceDataKey),
    #[storage(persistent, value = i128)]          Balance(Address),
    #[storage(skip)]                              State(Address),
    #[storage(instance, value = Address)]         Admin,
}
#[storage_key(instance, value = TokenMetadata, symbol = "METADATA", name = "METADATA", lib = "soroban-token-sdk@28.0.0")]
const TOKEN_SDK_METADATA_KEY: &str = "METADATA";
```

## Commands

| Purpose | Command |
|---|---|
| Everything (DoD gate) | `bash scripts/check.sh` |
| Docs site | `cd site && pnpm install && pnpm test && pnpm build && pnpm preview` (see DEMO.md) |
| Golden outputs / test-host captures / browser engine | `bash scripts/golden.sh`, `bash scripts/capture-testhost.sh`, `bash scripts/build-engine.sh` |
| Format | `cargo fmt --all -- --check` (and in `demos/`) |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` |
| Tests | `cargo test --workspace`, then `(cd demos && cargo test)` |
| Rebuild wasm fixtures + size report | `bash scripts/build-demos.sh` |
| Re-capture mainnet fixtures (read-only) | `bash scripts/capture-mainnet.sh` |

## Layout

`crates/core` (format, decoder, lint, RPC, shared `api`), `crates/macros` (emitters), `crates/cli` (`storage-schema`), `crates/web` (browser engine), `demos/` (separate workspace, soroban-sdk =28.0.0), `site/` (docs + playground), `sidecars/`, `schemas/`, `fixtures/`, `drafts/`. See ARCHITECTURE.md.

## Versions

rustc 1.97.1; stellar-cli 25.2.0 (soroban-sdk 28 refuses plain `cargo build` for wasm and needs `stellar contract build`); stellar-xdr =28.0.0 (core); soroban-sdk =28.0.0, soroban-token-sdk =28.0.0, OZ stellar-tokens / stellar-contract-utils / stellar-macros =0.8.0 (demos only). The format crates do not depend on soroban-sdk.

DEMO-ONLY: the demo contracts are test subjects, not audited, and not for deployment. Mainnet use is read-only; nothing in this project signs or sends a transaction.
