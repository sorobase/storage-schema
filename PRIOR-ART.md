# Prior art (SSC-001, re-checked 2026-11-03)

Method: GitHub code, repo, issue and PR search through `gh` (7 s between search calls, 2 s between REST calls), crates.io API search, direct reads of the named repos' READMEs, trees and example files, and the cargo registry sources of soroban-sdk 25.3.2/28.0.0 and soroban-env-host 28.0.2. The search script was run from a scratch directory, and its raw output was summarised here, not committed.

## Verdict

**Partial overlap, not a stop (rule 1).** No project found emits or reads an *in-wasm* storage-schema section. No project found defines how a ledger key `ScVal` maps to a declared storage entry, including the ambiguous cases. No project found decodes contract-data ledger entries against a declared schema with honest unknown/ambiguous reporting.

Three neighbours already exist, so the scope is narrowed:

- **In-contract typed storage** (Axelar, soroban-sdk-tools, sdk#1762 design, PR #2127) is not rebuilt. Our macro only *annotates*, and it reuses the #1762 attribute shape.
- **Hand-declared sidecar schemas for upgrade diffing** exist in two tools (ShippedLabs safeguard, bielcarpi/soroban-upgrades). Our sidecar is a different shape, because it must carry key patterns and value types. Importers for their formats are a ticket (SSC-022), not a reason to stop.
- **Single-entry RPC decoders** exist (Sorobscope, SaboLabs sdkt). They decode raw `ScVal`s, optionally with the ABI. None maps entries to declared storage slots.

G-NOVELTY: this file supports the narrow claim "no public repo or crate found on 2026-11-03 emits or consumes an off-chain storage schema in a Wasm custom section". It does not support "first" for typed storage, sidecar schemas or storage decoding in general.

## Searches

| # | Query / target | Date | Result | Verdict |
|---|---|---|---|---|
| 1 | `gh search code contractstoragev0` | 2026-11-03 | hits only in `near/intents` and a fork (`contracts/defuse/src/contract/state/v0.rs`: fuzzy matches on "contract state v0", NEAR, unrelated) | none |
| 2 | `gh search code "link_section contractstorage"` | 2026-11-03 | 0 | none |
| 3 | `gh search code "soroban storage schema custom section"` | 2026-11-03 | 0 | none |
| 4 | repos "soroban storage schema" / "soroban storage layout" / "soroban storage decode" | 2026-11-03 | 0 / 0 / 0 | none |
| 5 | repos "soroban contract data", created > 2026-01-01 | 2026-11-03 | Stellar-Scan-app/stellar-scan-api (indexed contract data API), orbitfund-interface (frontend) | unrelated |
| 6 | repos "stellar contract storage", created > 2026-01-01 | 2026-11-03 | 0 | none |
| 7 | repos `sorobscope` | 2026-11-03 | Sorobscope/sorobscope (created 2026-09-07, 0 stars, Apache-2.0) | partial, see below |
| 8 | issues "storage schema", owner stellar | 2026-11-03 | sdk#1818 (open), sdk#1762 (open), stellar-docs#2193 (migration docs), wallet-backend#91 | context only |
| 9 | issues `contractstorage` | 2026-11-03 | sdk#1762; unrelated non-Stellar hits; Lumeeena/lumina-backend#9 "Index full Soroban contract storage state" (closed 2026-10-18) | none |
| 10 | PRs "storage" in rs-soroban-sdk, updated > 2026-09-01 | 2026-11-03 | #2127 (draft), #1908 (external read-only storage API), several closed docs PRs | see below |
| 11 | crates.io "soroban storage" | 2026-11-03 | soroban-chonk (chunked storage), soroban-macros (helpers) | unrelated |
| 12 | crates.io "contractstorage" | 2026-11-03 | soroban-sdk-tools 0.1.4 | partial, see below |
| 13 | crates.io "stellar storage schema" | 2026-11-03 | soroban-upgrades-core / -cli 1.0.7 (created 2026-08-25) | partial, see below |
| 14 | GraphQL discussions "storage schema repo:stellar/stellar-protocol" | 2026-11-03 | #1991 "Should SEPs define a storage model?", #1877 (became CAP-86) | context |

## Repos and designs examined

| Item | What it does (as read) | Overlap | Dates / tags |
|---|---|---|---|
| [rs-soroban-sdk#1818](https://github.com/stellar/rs-soroban-sdk/issues/1818) | Leigh: storage types do not belong in specs; "someplace to include storage types is interesting"; separate custom section suggested. His 2026-04-28 question is unanswered. mootz12 against (2026-04-24, 2026-04-27); tupui and teddav for. 10 comments, unassigned | the open question this project answers | [P] read 2026-11-03, updated 2026-09-25 |
| [rs-soroban-sdk#1762](https://github.com/stellar/rs-soroban-sdk/issues/1762) | `#[contracttype(storage)]` with `#[storage(persistent, value = i128)]` per variant, generating typed accessors. Leigh 2026-09-25: a solution should consider #2088 and #1818. No assignee, no milestone | in-contract half; we reuse its attribute shape and do not generate accessors | [P] 2026-11-03 |
| [rs-soroban-sdk PR #2127](https://github.com/stellar/rs-soroban-sdk/pull/2127) | draft `#[contractdata(storage=persistent, namespace="USER_DEPOSIT")]` struct with `#[key]` fields; key `(Symbol("USER_DEPOSIT"), user)`. 13 files (macros, `contract_data.rs`, tests, snapshots). No reviews, `mergeable_state: dirty`. No schema output in the file list | in-contract half. Its keys map to our `TAGGED` with named fields. Its *value* encoding was not read (RISKS R-08) | [P] 2026-11-03 |
| [rs-soroban-sdk#2088](https://github.com/stellar/rs-soroban-sdk/issues/2088) | enum keys collide across modules and crates; Leigh: think about it with #1762; module path in #1970 would not remove all cases | we detect it (lint) and report it (ambiguous); we do not fix the encoding | [P] 2026-11-03 |
| [BlaineHeffron/soroban-sdk-tools](https://github.com/BlaineHeffron/soroban-sdk-tools) | `#[contractstorage]` typed handles plus key shortening, compile-time uniqueness checks; v0.1.4 on crates.io (7,590 downloads), sdk 28 since 2026-09-28; Apache-2.0, 2 stars. Tree and README show no schema emission | in-contract macro; natural emitter partner (SSC-016) | [P] 2026-11-03 |
| Axelar `#[contractstorage]` (stellar-axelar-std-derive) | typed storage accessors | in-contract macro | [S: coordinator brief, 2026-11-03; not re-read] |
| [ShippedLabs/soroban-upgrade-safeguard](https://github.com/ShippedLabs/soroban-upgrade-safeguard) | upgrade diff; `--storage-schema` JSON/TOML of hand-declared *operations* (`{name, operation, durability, namespace, key_type}`, one per function touching a variant per its cookbook); infers storage access from wasm; `--empirical-file` validation. MIT, 5 stars, pushed 2026-10-29 | sidecar for upgrade checks, not a decoder; no in-wasm section | [P] fixtures `tests/fixtures/storage_durability/*.json`, `docs/storage-schema-cookbook.md`, 2026-11-03 |
| [bielcarpi/soroban-upgrades](https://github.com/bielcarpi/soroban-upgrades) | upgrade validation with `--from-schema/--to-schema/--schema-history`; schema JSON `{formatVersion, complete, schemaVersion, contractVersion, entries:[{key, durability, type}]}`. Apache-2.0, 5 stars, created 2026-08-07, v1.0.7 on crates.io | closest sidecar shape; key is a bare name (no pattern for parameterised keys); no in-wasm section; no ledger decoding | [P] `examples/schemas/counter-v1.schema.json`, README, 2026-11-03 |
| [Sorobscope/sorobscope](https://github.com/Sorobscope/sorobscope) | read-only RPC CLI; reads **one** entry whose key you supply; checks persistent, temporary and instance; lists "a schema-aware key builder for composite keys" as future work | decoder of raw values; no schema | [P] README, 2026-11-03 |
| [SaboLabs/soroban-devkit](https://github.com/SaboLabs/soroban-devkit) (`sdkt`) | `storage read --key-xdr`, `storage analyze` (classify instance/persistent/temporary), ABI-aware ScVal formatting. MIT, 11 stars, pushed 2026-10-22 | raw decoding with ABI; no storage schema | [P] README, 2026-11-03 |
| OpenZeppelin stellar-contracts v0.8.0 (released 2026-10-20) | no storage schema emission; library enums `FungibleStorageKey`, `PausableStorageKey`, `OwnableStorageKey`, `TotalSupplyStorageKey`. Its total-supply migration relies on two enums sharing a key | consumer and test subject, not prior art | [P] tag v0.8.0 @ddc327e |
| soroban-sdk 28 spec shaking | storage-only types are removed from `contractspecv0` after build | forces our type entries (SPEC 3.3) | [P] registry source 2026-11-03 |

## Analogues outside Stellar

solc `storageLayout` output (compiler artefact, not in bytecode) and Anchor IDL `accounts` are the familiar models [S: discovery note `devtools_build_test.md` C4]. Neither places the layout in the deployed artefact. Placing it in the wasm (as Leigh suggested) lets the layout travel with the code hash.

## Re-check triggers

Re-run before posting the #1818 comment or publishing the SEP: #1818, #1762 and #2127 activity; new repos matching `contractstorage`; any `contractstoragev0` or similar section name in rs-soroban-sdk or stellar-cli.
