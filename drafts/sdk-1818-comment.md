<!--
DRAFT for the user to post on https://github.com/stellar/rs-soroban-sdk/issues/1818 .
Not posted (gate G-OUTREACH). Before posting:
  1. re-read #1818, #1762, #2127 for activity after 2026-11-03;
  2. replace REPO_URL with the public link to the code, or drop the links;
  3. keep it short; the SEP draft is linked, not pasted.
-->

Following up on @leighmcculloch's question from April ("how does an off-chain consumer looking at a contract storage ledger entry determine which contract type in the storage schema should apply?"), I prototyped one answer end to end, as a separate custom section. Nothing goes into the spec.

**Section.** `contractstoragev0` holds a stream of XDR entries, concatenated like `contractspecv0`. Each key entry is `(name, lib, durability, key pattern, value: SCSpecTypeDef)`. A second entry kind carries `SCSpecEntry` UDT definitions, because since sdk 28 spec shaking removes storage-only types from `contractspecv0`.

**Mapping a ledger entry to a schema entry.**

1. Durability comes from the ledger key. Instance entries are the elements of the instance's storage map.
2. Key patterns:
   - `EXACT(ScVal)` for raw symbols such as `symbol_short!("COUNTER")`;
   - `TAGGED(tag, fields)`, meaning `Vec[Symbol(tag), f1..fn]` with the fields type-checked. This is exactly how `#[contracttype]` enum variants encode, and also #2127's `(Symbol("NS"), key..)`;
   - `TYPED(t)` as a catch-all.
3. Precedence is exact > tagged > typed. The result is one of:
   - `matched`;
   - `value_mismatch` (the key matched but the value doesn't conform: drift, an upgrade, or a collision);
   - `ambiguous` (several declarations claim the same key shape, which is the #2088 case; the ledger genuinely can't distinguish them, so the decoder lists the candidates and which ones the value conforms to, and doesn't pick);
   - `unknown`, which includes the key's shape and raw XDR.

**What I ran:**

- soroban-examples `token` and `increment` (sdk 28.0.0), annotated with the #1762-style `#[storage(persistent, value = i128)]` attributes. Every entry they write in the test host decodes as `matched`, including soroban-token-sdk's `METADATA` key, declared with `lib = "soroban-token-sdk@28.0.0"`.
- OZ stellar-contracts v0.8.0 `fungible-pausable`, unmodified. The schema is injected post-build from a JSON sidecar. Keys come from three crates. OZ's total-supply migration is a nice real case: `FungibleStorageKey::TotalSupply` (instance) and `TotalSupplyStorageKey::TotalSupply` (persistent) are the same key ScVal, and durability is what separates them.
- Mainnet USDC SAC instance and contract-holder balances, decoded through a sidecar (the SAC has no wasm).
- A lint flags #2088-style overlaps before deploy. For example, the soroban-examples token's `DataKey::Balance(Address)` and OZ's `FungibleStorageKey::Balance(Address)` in one contract give one ledger entry with two value types.

**On size (@mootz12's concern):** the section is data only. The code sections are byte-identical with and without it, and it is opt-in. Measured on the demos: 885 bytes for the soroban-examples token (4 keys plus the 3 struct definitions it needs, about 10% of an 8.9 KB contract), 72 bytes for `increment`, 1,273 bytes for the OZ example. `stellar contract build` (CLI 25.2.0, spec shaking) and `--optimize` both keep the section; shaking had removed every storage struct from `contractspecv0`, which is why the section carries its own type definitions. A contract that wants zero on-chain cost can ship the same schema as a sidecar instead.

Draft SEP text and code: REPO_URL. Would a separate informational section along these lines be something the SDK could emit from the #1762 design (it already has the variant to value-type mapping), or would you rather it live in tooling such as stellar-cli?
