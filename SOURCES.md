# Sources (trust tags: [P] primary, [S] secondary, [V] vendor, [I] inference, [E] estimate, [U] unverified)

## Discovery notes (research output, re-verified where built on)

- `../../../zero-to-one-discovery/reports/SDF open issues build gaps.md`: A2 (storage-schema SEP, emitter, decoder), B4 revised verdict, section 6 row B4/P017 [S]
- `../../../zero-to-one-discovery/reports/Stellar aggregator devtools infra ideas.md`: master table row 6 (B4) [S]
- `../../../zero-to-one-discovery/research_notes/SDF open issues build gaps/soroban_protocol.md`: TH1, covered-by list (soroban-sdk-tools, ShippedLabs safeguard) [S]
- `../../../zero-to-one-discovery/research_notes/Stellar aggregator devtools infra ideas/devtools_build_test.md`: C4 (partial prior art: SaboLabs sdkt, safeguard empirical mode) [S]

## Issues and PRs (read via `gh api`, 2026-11-03) [P]

- rs-soroban-sdk#1818 (10 comments; Leigh 2026-04-27 and 2026-04-28; mootz12 2026-04-24 and 2026-04-27): https://github.com/stellar/rs-soroban-sdk/issues/1818
- rs-soroban-sdk#1762 (4 comments; `#[contracttype(storage)]` sketch; Leigh 2026-09-25 cross-links #2088 and #1818): https://github.com/stellar/rs-soroban-sdk/issues/1762
- rs-soroban-sdk PR #2127 (draft, dirty, no reviews; 13 files): https://github.com/stellar/rs-soroban-sdk/pull/2127
- rs-soroban-sdk#2088 (key collisions; Leigh 2026-09-25): https://github.com/stellar/rs-soroban-sdk/issues/2088
- laboratory#2175 (open, updated 2026-08-12): https://github.com/stellar/laboratory/issues/2175
- discussions #1991 and #1877: https://github.com/orgs/stellar/discussions/1991 , https://github.com/orgs/stellar/discussions/1877
- Not re-read this session (cited from discovery notes) [S]: stellar-cli#2041, wallet-backend#610, stellar-etl#354

## Code read (2026-11-03) [P]

- soroban-sdk 28.0.0 (cargo registry): `src/spec_shaking.rs`, `src/_migrating/v27_export.rs` (shaking always on in v28; storage-only types removed post-build); `soroban-sdk-macros-28.0.0/src/derive_struct.rs`, `derive_enum.rs` (`pub const fn spec_xdr()`); `soroban-sdk-macros-28.0.0/src/shaking.rs` (marker scheme). `Cargo.toml`: `stellar-xdr = "=28.0.0"`
- soroban-sdk-macros 25.3.2 `derive_struct.rs`: `spec_xdr()` present when the type is exported
- stellar-xdr 28.0.0: `ScVal` includes `ExecutableTag`; `ContractExecutable::ExternalRef`; `ScAddress` arms `MuxedAccount`, `ClaimableBalance`, `LiquidityPool`
- soroban-env-host 28.0.2 `src/builtin_contracts/stellar_asset_contract/{storage_types,metadata,public_types,admin,asset_info}.rs`: SAC keys and value types (basis of `sidecars/stellar-asset-contract.storage.json`)
- soroban-examples @ 03d42aa6b973dcf3a453a99d0c6a6e8d25a196e2 (2026-10-06): `token/src/*`, `increment/src/lib.rs`, sdk 28.0.0, Apache-2.0
- OpenZeppelin stellar-contracts tag v0.8.0 @ ddc327eb92d63709436da3a8455f641c2d5476c5 (2026-10-20), MIT: `examples/fungible-pausable/src/contract.rs`; `packages/tokens/src/fungible/storage.rs`; `packages/tokens/src/fungible/extensions/total_supply/storage.rs` (lines 157-159: old and new TotalSupply keys are the same ScVal); `packages/contract-utils/src/pausable/storage.rs`; `packages/access/src/ownable/storage.rs`
- soroban-token-sdk 28.0.0 metadata key: `METADATA` in instance storage. Checked by the token e2e test, which decodes the host-written entry under that key (SSC-009)
- crates.io (API, 2026-11-03): stellar-tokens / stellar-contract-utils / stellar-macros 0.8.0 (MIT); soroban-token-sdk 28.0.0 (Apache-2.0; latest 30.0.0-rc.1)

## Prior-art repos

See `PRIOR-ART.md` (all [P] reads on 2026-11-03).

## Network

- Mainnet RPC: `https://mainnet.sorobanrpc.com` (the endpoint other kit projects use). Read-only calls only. Capture details in `fixtures/mainnet/provenance.json`.
- CORS on `https://mainnet.sorobanrpc.com` (2026-11-08, one OPTIONS request): `access-control-allow-origin: *`, methods GET/POST/HEAD/OPTIONS, headers `*` [P]. The site's live mode relies on it.
- Live playground reads on 2026-11-08 (from a local browser, read-only): USDC SAC instance + one balance at ledger 65,366,504; holder contract instance + wasm at ledger 65,366,515 [P]
- USDC SAC contract id `CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75` [I: well-known id; confirmed by the captured instance's AssetInfo (code USDC) in the decode test]

## Blocked or not checked

- stellar-cli shaking/optimise behaviour on unknown custom sections (RISKS R-05): not checked; no newer CLI installed.
- Axelar `#[contractstorage]`: not re-read [S: coordinator brief].
