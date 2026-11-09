<!-- SPDX-License-Identifier: Apache-2.0 -->
# storage-schema: investor brief

One page, 2026-11-08. Tags: [M] measured in this project, [P] primary source read, [S] secondary, [I] inferred, [E] estimate.

## Problem

Soroban contracts keep their state as untyped key-value ledger entries. A contract-data entry is a key `SCVal`, a durability and a value `SCVal`; it carries no name and no type. Tools that read contract state (explorers, indexers, wallets, Stellar Lab, upgrade checkers) cannot say what an entry is without the contract's source.

- The SDK maintainers' open question in rs-soroban-sdk#1818 (2026-04-28): "how does an off-chain consumer looking at a contract storage ledger entry determine which contract type in the storage schema should apply?" Unanswered in the thread as of 2026-11-03 [P].
- Since soroban-sdk 28, spec shaking removes storage-only types from the contract's interface section, so even the type definitions are gone from deployed code [P: soroban-sdk 28.0.0 `spec_shaking.rs`; M: the token demo's shaken spec keeps 0 of its 3 storage types].
- Key collisions between crates (rs-soroban-sdk#2088) silently share one ledger entry [P].
- Symptoms cited in Stellar repos: Lab's storage tab (laboratory#2175 [P]); `stellar contract read --key` symbols only (stellar-cli#2041 [S]); indexers special-casing tokens (wallet-backend#610, stellar-etl#354 [S]).

## What it does

An informational Wasm custom section, `contractstoragev0`, that lists the keys a contract writes, their durability and value types, and carries the type definitions spec shaking removed. A decoder maps each ledger entry to a declared slot with one of four answers (matched, ambiguous, value mismatch, unknown) and never guesses. Delivered as attribute macros, a sidecar JSON for contracts that never adopt them (including the built-in Stellar Asset Contract), post-build inject/strip, a collision lint, a CLI with read-only RPC, a SEP draft, and the decoder compiled to WebAssembly for a browser playground.

## Who it is for

Explorer and indexer teams (decode any contract's state with names and types), Stellar Lab and wallets (show contract storage to users), contract authors and auditors (lint key collisions before deploy; review what a contract stores), and the SDK team (a concrete, tested candidate answer to #1818 that is compatible with the #1762 attribute shape).

## Working today (all real, all reproducible)

| Figure | Value | Source |
|---|---|---|
| USDC (mainnet Stellar Asset Contract) entries decoded `matched` via a sidecar | 8 of 8 | [M] recorded mainnet capture, ledger 65,292,086, 2026-11-03; re-read live in the playground 2026-11-08 (ledger 65,366,504, 4 of 4 for instance + one balance) |
| A mainnet wasm contract without a schema | 37 of 37 entries `unknown` | [M] recorded capture, ledger 65,292,097 |
| Section cost, token (4 keys, 3 types) | 885 bytes, code byte-identical; +6.91% over the same build without it (11,915 to 12,800 B); 10.0% of the 8,885 B shaken wasm that carries it | [M] `fixtures/size-report.json`, 2026-11-08 |
| Section cost, OpenZeppelin token injected after build (7 keys, 3 types) | 1,273 bytes; strip restores the original file | [M] same |
| Scenarios with golden outputs; all four statuses on real entries | 9 scenarios | [M] CLI test `golden_outputs_match` |
| Browser engine equals the CLI | 23 of 23 checks | [M] `pnpm test`, 2026-11-08 |
| Tests | see README status table | `bash scripts/check.sh` |

## Why Stellar

The SDK already ships a custom-section convention (`contractspecv0`) and per-type spec bytes the macros reuse; the open #1818 thread suggests a separate custom section; and spec shaking in sdk 28 makes the gap concrete. A schema inside the wasm travels with the code hash, so any reader that can fetch code can find it, with no registry. Ethereum's nearest analogue (solc `storageLayout`) is a build artefact that is not deployed [S: PRIOR-ART.md].

## Prior art (G-NOVELTY)

Supported claim only: no public repo or crate found on 2026-11-03 emits or consumes an off-chain storage schema in a Wasm custom section (`PRIOR-ART.md`). Typed in-contract storage (sdk#1762, PR #2127, soroban-sdk-tools, Axelar), sidecar schemas for upgrade checks (soroban-upgrade-safeguard, soroban-upgrades) and single-entry decoders (Sorobscope, sdkt) exist and are not claimed.

## Roadmap

1. Adoption path: the user posts the #1818 comment and the SEP draft (G-OUTREACH: agents never post); a PR draft for soroban-sdk-tools to emit the section (SSC-016).
2. Hardening: newer stellar-cli check (SSC-012), fuzzing (SSC-014), integer-key enums (SSC-018), #2127 value encoding (SSC-019).
3. Reach: TypeScript decoder for JS tooling (SSC-021), importers for existing schema files (SSC-022), an indexer-facing example (SSC-017).
4. History: decode entries written before an upgrade with the wasm active at that ledger (SSC-020; needs holders-at-ledger).

## Risks and gates

- SDF may ship a different emission design under #1762 (R-01). Mitigation: same attribute shape; the section format is independent of how it is emitted; adoption of any compatible design counts.
- Opposition in principle to internal types in external artefacts (R-04). Mitigation: separate section, opt-in, measured cost, zero-cost sidecar option.
- A library's declarations are silently absent unless the contract references the crate [M: SSC-013, 2026-11-08]. Mitigation: documented; `inspect`/`decode --strict` in CI catch it.
- Gates: G-VALUE (mainnet read-only; no writes ever), G-OUTREACH (drafts unposted), G-NOVELTY (claim above only). Code unaudited; demo contracts DEMO-ONLY.

## What launch needs

Publishing `sik-storage-schema-core`, `-macros` and `-cli` to crates.io (names and licence check); a security review of the parsers (they read untrusted wasm and XDR) with fuzzing first; the SDF conversation on #1818. See `DEPLOY.md`.
