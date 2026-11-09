# Build plan

Estimates are [E]: one developer, from this session's pace. They are not measured.

## M1: first working version (this session, 2026-11-03)

Acceptance:

- format spec (SPEC.md) and SEP draft;
- core library (codec, sidecar, wasm sections, decoder, lint, read-only RPC);
- macros (`#[storage_schema]`, `#[storage_key]`, `storage_schema_extern!`);
- CLI;
- three demo contracts built against soroban-sdk 28.0.0 that decode end to end in the test host;
- OZ demo via post-build inject;
- mainnet USDC SAC decode via sidecar;
- size measured [M];
- #1818 comment drafted;
- `scripts/check.sh` green.

Status: see README "Status".

## M2: hardening (about 1 week [E])

- SSC-012: test `stellar contract build`/`optimize` from a CLI that does spec shaking v2; prove the section survives, or document the inject-after-optimise order.
- SSC-013: cross-crate emission (a library crate annotates its own enum; check that the section reaches the final contract).
- SSC-014: fuzz `decode_stream` and `wasm::sections` (cargo-fuzz); bounded allocation on hostile input.
- SSC-015: golden JSON for decoder output (snapshot tests).

Acceptance: those tickets DONE; no panics under 1 h of fuzzing [E target].

## M3: adoption path (2-3 weeks [E], partly user-led)

- SSC-016: PR draft for BlaineHeffron/soroban-sdk-tools `#[contractstorage]` to emit the section (drafted in `drafts/`, posted by the user).
- SSC-017: decoder demo for Lab / stellar-ledger-data-indexer (JSON contract only; no UI without the taste-skill rule).
- SSC-021: TypeScript decoder (`@sik/storage-schema`) for the JS ecosystem.
- SSC-022: importers for ShippedLabs safeguard and bielcarpi/soroban-upgrades schema files.

Acceptance: at least one external tool reads the section, or the user records a decision not to pursue it.

## M4: history (later)

- SSC-020: select the schema by the wasm that was active at an entry's `last_modified_ledger` (needs `holders-at-ledger` state-at-ledger).
