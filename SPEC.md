# storage-schema SPEC (v0, first version, 2026-11-03)

This document is normative for the code in this project. The SEP draft in `drafts/SEP-storage-schema.md` restates sections 3 to 6 in SEP form. MUST, SHOULD and MAY are used as in RFC 2119.

## 1. Scope

A Soroban contract MAY describe its storage layout in a Wasm custom section named `contractstoragev0`. The section is **informational**: the host never reads it, it does not change contract behaviour, and it is not part of the callable interface in `contractspecv0`. Its readers are off-chain: indexers, explorers, Lab, wallets, upgrade checkers and debuggers. It answers one question: for a contract-data ledger entry, which declared storage slot is it, and what are its key and value types?

It does not mandate any storage layout (see RISKS R-03 and stellar discussion #1991). A contract with no section is valid. A contract with a partial section is valid, and its undeclared entries decode as `unknown`.

Pinned versions (checked 2026-11-03):

| Component | Version | Why |
|---|---|---|
| `stellar-xdr` (core crate) | `=28.0.0` | the version `soroban-sdk 28.0.0` pins; decodes protocol-29 mainnet entries [P: cargo registry `soroban-sdk-28.0.0/Cargo.toml`] |
| `soroban-sdk` (demos only) | `=28.0.0` | what both upstream demos use: soroban-examples @03d42aa and OZ stellar-contracts v0.8.0 [P: their `Cargo.toml`, 2026-11-03] |
| rustc | 1.97.1 (`rust-toolchain.toml`) | programme pin |
| Format crates | no `soroban-sdk` dependency | the format and the macro are SDK-version independent; demos are a separate workspace, so no 25-vs-29 lockfile conflict arises |

## 2. Terms

- **Key ScVal**: the `key` of a `ContractDataEntry`, or the key of one element of an instance's `ScContractInstance.storage` map.
- **Durability**: `temporary`, `persistent` (from `ContractDataDurability`) or `instance` (an element of the instance storage map).
- **Schema entry**: one `SCStorageKeyEntryV0`, describing a family of keys (for example every `Balance(Address)`).
- **Type entry**: a UDT definition (`SCSpecEntry`) carried with the schema.

## 3. Section format

### 3.1 Placement

- Name: `contractstoragev0`. The version is in the name, as with `contractspecv0`. An incompatible change gets a new name (`contractstoragev1`). Readers ignore names they do not know.
- Content: zero or more `SCStorageEntry` XDR values, concatenated with no outer length or header. The stream ends at the end of the section.
- A module MAY contain several `contractstoragev0` sections. Readers MUST concatenate them in module order before decoding. This lets the linker merge statics from several crates, and lets a post-build `inject` append to a macro-emitted section.
- A reader that fails to decode the stream MUST report the section as malformed. It MUST NOT silently decode a prefix.

### 3.2 XDR

```
// Reuses SCSpecTypeDef, SCSpecEntry and SCVal from Stellar-contract-spec.x / Stellar-contract.x.

enum SCStorageEntryKind
{
    SC_STORAGE_ENTRY_KEY_V0  = 0,
    SC_STORAGE_ENTRY_TYPE_V0 = 1
};

// Extends ContractDataDurability (TEMPORARY = 0, PERSISTENT = 1).
enum SCStorageDurability
{
    SC_STORAGE_TEMPORARY  = 0,
    SC_STORAGE_PERSISTENT = 1,
    SC_STORAGE_INSTANCE   = 2
};

enum SCStorageKeyKind
{
    SC_STORAGE_KEY_EXACT  = 0,   // key == value
    SC_STORAGE_KEY_TAGGED = 1,   // key == Vec[Symbol(tag), f1 .. fn]
    SC_STORAGE_KEY_TYPED  = 2    // key is any value of the type
};

struct SCStorageKeyFieldV0
{
    string name<30>;            // "" for tuple-variant fields
    SCSpecTypeDef type;
};

struct SCStorageKeyTaggedV0
{
    string tag<32>;             // a valid Symbol
    SCStorageKeyFieldV0 fields<>;
};

union SCStorageKey switch (SCStorageKeyKind kind)
{
case SC_STORAGE_KEY_EXACT:
    SCVal exact;
case SC_STORAGE_KEY_TAGGED:
    SCStorageKeyTaggedV0 tagged;
case SC_STORAGE_KEY_TYPED:
    SCSpecTypeDef typed;
};

struct SCStorageKeyEntryV0
{
    string doc<1024>;
    string lib<80>;             // writer of the entry: "" = the contract; else crate[@version]
    string name<60>;            // display name, e.g. "DataKey::Balance"
    SCStorageDurability durability;
    SCStorageKey key;
    SCSpecTypeDef value;
};

union SCStorageEntry switch (SCStorageEntryKind kind)
{
case SC_STORAGE_ENTRY_KEY_V0:
    SCStorageKeyEntryV0 keyV0;
case SC_STORAGE_ENTRY_TYPE_V0:
    SCSpecEntry typeV0;         // UdtStructV0 / UdtUnionV0 / UdtEnumV0 / UdtErrorEnumV0
};
```

The wire layout is pinned by the test `model::tests::wire_layout_is_stable`.

### 3.3 Why type entries exist

Since soroban-sdk 28, spec shaking is always on. The macros emit a spec entry plus a marker for every `#[contracttype]`, and post-build tooling removes entries that are not reachable from a function parameter, a return value or an event [P: `soroban-sdk-28.0.0/src/spec_shaking.rs` and `src/_migrating/v27_export.rs`, read 2026-11-03]. Storage-only types (for example `AllowanceValue`) are therefore absent from a shaken `contractspecv0`. `SC_STORAGE_ENTRY_TYPE_V0` carries them with the schema. The bytes are exactly the `contractspecv0` encoding: the macro takes them from the type's own `spec_xdr()` const fn (SPEC 8.3). Whether stellar-cli's shaking pass keeps unknown custom sections is not yet checked (RISKS R-05).

### 3.4 De-duplication and conflicts

Readers MUST drop exact duplicate key entries and exact duplicate type entries, since the same crate's statics may be linked or injected twice. Two type entries with the same name and different shapes are a conflict. The reader keeps the first one in precedence order (6.2) and reports a warning.

## 4. Mapping a ledger entry to a schema entry

This section answers rs-soroban-sdk#1818 (leighmcculloch, 2026-04-28): *"how does an off-chain consumer looking at a contract storage ledger entry determine which contract type in the storage schema should apply?"*

### 4.1 Inputs

A contract-data ledger entry gives `(contract, key ScVal, durability, val ScVal)`. Then:

1. If `key` is `LedgerKeyContractInstance` and `val` is `ContractInstance`, the entry is the instance. Each element `(k, v)` of `instance.storage` is decoded as `(k, v)` with durability `instance`. The instance's executable (wasm hash, `stellar_asset` or `external_ref`) is reported alongside.
2. Otherwise durability is the entry's `ContractDataDurability` (`persistent` or `temporary`).

The schema comes from the contract's current wasm (the code the instance points at). For a built-in Stellar Asset Contract, or for any contract without a section, it comes from a sidecar (section 5). Readers MAY merge both.

### 4.2 Pattern match

A key `K` matches schema entry `E` only if `E.durability` equals the entry's durability and:

| `E.key` | Matches when | Produced by |
|---|---|---|
| `EXACT(v)` | `K == v` (XDR-equal) | raw symbols (`symbol_short!("COUNTER")`, `Symbol::new(..)`), integer keys, any literal |
| `TAGGED(tag, f1..fn)` | `K` is `Vec` of length `n+1`, `K[0] == Symbol(tag)`, and `K[i]` conforms to `fi.type` for every `i` | `#[contracttype] enum` variants (a unit variant is `Vec[Symbol(tag)]`), sdk#1762 `#[contracttype(storage)]`, sdk#2127 `namespace` keys `(Symbol("NS"), k1, ..)` (named fields) |
| `TYPED(t)` | `K` conforms to `t` | catch-all keys (for example any `Address`, any `u32`) |

Conformance of a value to a `SCSpecTypeDef` follows the soroban encoding:

- Scalars match their `ScVal` arm. `BytesN<n>` needs `Bytes` of length `n`. `Option<T>` is `Void` or `T`. `Vec<T>` is a `Vec` of `T`. `Map<K,V>` is a `Map`. A tuple is a `Vec` of exactly that arity. `Val` is anything.
- A named struct is a `Map` whose symbol keys are exactly the field names. A tuple struct (fields named `0`, `1`, ..) is a `Vec`.
- A union is `Vec[Symbol(case), ..]` with the case's arity.
- An integer enum is `U32` with a declared value.
- A UDT with no definition available conforms to anything. The decoder adds a warning (`type X is not defined`), so the gap is visible instead of guessed over.

### 4.3 Precedence and status

1. Collect every entry that matches (4.2).
2. Keep only those with the highest specificity: `EXACT` (3) > `TAGGED` (2) > `TYPED` (1).
3. Then:
   - **0 left: `unknown`.** The reason says either that the contract has no schema, or that no entry of this durability matches the key shape (for example `[Symbol("Foo"), Address]`). If an entry of another durability matches, the reason names it, which catches wrong durability declarations.
   - **1 left:** render the value against `E.value`. If it conforms, the status is **`matched`**. If not, it is **`value_mismatch`**, with the path, expected type and found shape.
   - **More than 1 left: `ambiguous`.** Each candidate is listed with `value_conforms` and its rendering. The decoder does not pick one, even when exactly one candidate's value type conforms. That evidence is reported, and the choice is left to the consumer.

### 4.4 Hard cases

| Case | What the ledger holds | What this format does |
|---|---|---|
| Raw Symbol keys (`symbol_short!("COUNTER")`) | `Symbol("COUNTER")` | `EXACT(Symbol("COUNTER"))`. The `#[storage_key]` macro reads it from the `symbol_short!` initialiser. A `TYPED(Symbol)` catch-all MAY coexist and loses by precedence |
| Instance storage | one ledger entry whose value holds an `ScMap` | each map element is decoded as an `instance` entry (4.1). Its TTL is the instance's TTL. `source.ledger_entry = "instance_storage"` |
| Keys written by imported crates (OZ `stellar-tokens`, `soroban-token-sdk`) | indistinguishable from the contract's own keys | the crate MAY emit its own section. If it does not, the contract declares them with `storage_schema_extern!`, `#[storage_key(lib = ..)]` or a sidecar. `lib` records who writes them. The demos cover both routes |
| Same key, different durability | separate ledger entries (durability is part of the ledger key) | durability partitions matching. OZ v0.8.0 depends on this: `FungibleStorageKey::TotalSupply` (old, instance) and `TotalSupplyStorageKey::TotalSupply` (new, persistent) are the same key ScVal [P: OZ v0.8.0 `packages/tokens/src/fungible/extensions/total_supply/storage.rs` lines 157-159] |
| **#2088 collisions**: two enums in different crates both have `Count` or `Balance(Address)` | **one** ledger entry; the chain cannot tell which writer meant it | `ambiguous` with both candidates and value evidence. `lint` reports the overlap before deploy as `collision` (same type) or `possible_collision` (types may overlap). This is a property of the encoding, not of the schema, so no off-chain format can resolve it after the fact; it can only make it visible |
| Integer-discriminant key enums | `U32(n)` | `EXACT(U32(n))` per variant (sidecar). The enum macro rejects them with a message |
| Struct keys (`AllowanceDataKey`) | `Map` | `TAGGED` with a UDT field, or `TYPED(Udt)`. The struct definition comes from type entries or the spec |
| Data written by older code (after an upgrade) | old shapes | `value_mismatch` or `unknown`. The schema describes the current wasm only. History needs the wasm active at the entry's `last_modified_ledger` (not in v0; TASKS SSC-020) |
| Entries the schema does not declare | anything | `unknown`, with `key_shape` and the raw XDR, so a schema author can see what to add |

### 4.5 Decoders MUST NOT

- assign a name to an entry that matched no pattern;
- pick one candidate of an ambiguous match without saying so;
- drop raw XDR (every result carries `raw.key_xdr` and `raw.val_xdr`).

## 5. Sidecar JSON

The sidecar is the same model as a JSON file, for contracts that will never adopt the macro (including the SAC, which has no wasm). JSON Schema: `schemas/storage-schema.v0.schema.json`. Examples: `sidecars/*.json`.

```json
{
  "storage_schema": 0,
  "description": "free text",
  "keys": [
    { "name": "DataKey::Balance", "lib": "", "doc": "", "durability": "persistent",
      "key": { "tagged": { "tag": "Balance", "fields": ["Address"] } }, "value": "i128" },
    { "name": "COUNTER", "durability": "instance", "key": { "symbol": "COUNTER" }, "value": "u32" },
    { "name": "Seven", "durability": "temporary", "key": { "exact": { "u32": 7 } }, "value": "bool" },
    { "name": "Deposit", "durability": "persistent",
      "key": { "tagged": { "tag": "USER_DEPOSIT", "fields": [{ "name": "user", "type": "Address" }] } }, "value": "(u32, u32)" },
    { "name": "ByAddr", "durability": "temporary", "key": { "typed": "Address" }, "value": "Val" }
  ],
  "types": [
    "pub struct BalanceValue { pub amount: i128, pub authorized: bool, pub clawback: bool }",
    { "udt_struct_v0": { "doc": "", "lib": "", "name": "X", "fields": [] } }
  ]
}
```

- Types are written in Rust syntax (the same mapping the macro uses) or as stellar-xdr JSON of an `SCSpecEntry`.
- `exact` takes stellar-xdr JSON of an `ScVal`. `symbol` is sugar for `exact` with a Symbol.
- `inject` then `inspect --format sidecar` round-trips (test `sidecar::tests::parse_and_roundtrip`, CLI test `inject_extract_roundtrip`).
- Unknown fields are rejected. `storage_schema` MUST be `0`.

## 6. Decoder

### 6.1 Output (`decode` JSON)

```json
{
  "latest_ledger": 60123456,
  "instances": [{ "contract": "C..", "executable": "wasm|stellar_asset|external_ref", "wasm_hash": "..", "storage_entries": 3, "live_until_ledger": 1 }],
  "summary": { "total": 7, "by_status": { "matched": 6, "ambiguous": 0, "value_mismatch": 0, "unknown": 1 } },
  "entries": [{
    "durability": "persistent", "status": "matched",
    "name": "DataKey::Balance", "lib": "soroban-env-host@28.0.2", "schema_source": "sidecar:stellar-asset-contract.storage.json",
    "key": { "tag": "Balance", "fields": ["C.."] },
    "value": { "amount": "123", "authorized": true, "clawback": false },
    "value_type": "BalanceValue",
    "key_shape": "[Symbol(\"Balance\"), Address]",
    "candidates": [], "reason": null, "warnings": [],
    "source": { "contract": "C..", "ledger_entry": "contract_data", "last_modified_ledger": 1, "live_until_ledger": 2 },
    "raw": { "key_xdr": "base64", "val_xdr": "base64" }
  }],
  "notes": []
}
```

Rendering: `u32`/`i32` are JSON numbers. 64-bit and wider integers, `Timepoint` and `Duration` are decimal strings. `U256`/`I256` are `0x` hex strings (two's complement for `I256`). Bytes are lowercase hex. Addresses are strkeys. A struct is an object. A union is `"Case"` (void case) or `{"Case": [..]}`. An enum is its case name. A map with string-rendered keys is an object, otherwise `[{key, value}]`. `Val` and any unmatched key or value use stellar-xdr JSON (the form `stellar xdr decode` prints). Empty and null fields are omitted.

### 6.2 Type registry precedence

1. `contractstoragev0` type entries, then
2. sidecar type entries (in the order given), then
3. `contractspecv0` UDTs.

The first definition of a name wins. A differing later definition is a reported conflict.

## 7. Lint

`lint` checks every pair of entries with the same durability:

| Code | Severity | Meaning |
|---|---|---|
| `collision` | error | patterns can address the same entry and the field types certainly overlap (sdk#2088) |
| `possible_collision` | warning | same tag and arity; field types may overlap (for example two different UDTs) |
| `shadowed` | info | overlap across specificity levels, resolved by precedence |
| `undefined_type` | warning | a referenced UDT has no definition anywhere |
| `type_conflict` | warning | two definitions of a type name differ |
| `untyped_value` | info | value type is `Val` |

`lint` exits 2 on an error, or on a warning with `--deny-warnings`.

## 8. Macros (`sik-storage-schema-macros`)

### 8.1 `#[storage_schema]` on a key enum

```rust
#[storage_schema(lib = "", default = persistent, types(Nested), embed_types = true)]  // all optional
#[contracttype]
pub enum DataKey {
    #[storage(temporary, value = AllowanceValue)] Allowance(AllowanceDataKey),
    #[storage(persistent, value = i128)]          Balance(Address),
    #[storage(skip)]                              State(Address),
    #[storage(instance, value = Address, doc = "admin")] Admin,
}
```

- It MUST be placed above `#[contracttype]`, so it runs first and removes its `#[storage]` helper attributes.
- Every variant needs `#[storage(..)]` or `#[storage(skip)]`. A missing one is a compile error naming the variant.
- Name: `Enum::Variant`. Doc: `doc = ".."` or the variant's doc comments.
- The per-variant shape `#[storage(<durability>, value = T)]` is the one sketched in sdk#1762, so an enum annotated for that proposal is annotated for this one.

### 8.2 Other entry points

- `#[storage_key(instance, value = u32)] const COUNTER: Symbol = symbol_short!("COUNTER");` declares an `EXACT` Symbol key. Options: `symbol = ".."` (when the initialiser is not `symbol_short!`), `name`, `lib`, `doc`.
- `storage_schema_extern! { #[storage_schema(lib = "crate@ver")] enum TheirKey { #[storage(..)] V(..), .. } }` declares keys written by a dependency. Only the section bytes are emitted.
- Cross-crate emission (checked 2026-11-08 with stellar-cli 25.2.0, rustc 1.97.1, SSC-013): statics emitted in a library crate reach the contract's wasm only if the contract crate references the library (for example `use the_keys_crate as _;`). Without a reference the library is not linked, the build succeeds, and its declarations are absent. Tooling SHOULD check the result (`inspect`), not assume it.

### 8.3 Emission

The macro emits, inside `const _: () = { .. };`:

- one `#[cfg_attr(target_family = "wasm", link_section = "contractstoragev0")] static [u8; N]` with the encoded key entries. There is no `#[used]`: with it, rustc 1.97.1 also placed a copy of the bytes in the linear-memory data segment. Measured: the delta doubled; see TASKS SSC-011;
- for each UDT used in a value or key field, and each type in `types(..)`, a `#[repr(C)]` pair `([0,0,0,1], <T>::spec_xdr())`. That is an `SC_STORAGE_ENTRY_TYPE_V0` header followed by the type's own spec bytes, read at compile time from `#[contracttype]`'s `pub const fn spec_xdr()` (present in soroban-sdk 25.3.2 and 28.0.0 [P: `soroban-sdk-macros-*/src/derive_struct.rs`]). Types nested inside those UDTs are not followed; list them in `types(..)`. Use `embed_types = false` for value types that are not `#[contracttype]`s.

It emits no code and no data outside the custom section. With `--cfg sik_storage_schema_off`, nothing is emitted. That build is the size-measurement baseline.

## 9. CLI (`storage-schema`)

| Command | Purpose |
|---|---|
| `inspect <wasm> [--format summary\|json\|sidecar]` | show (or extract as sidecar) the embedded schema and a spec summary |
| `inject <wasm> --schema <sidecar> -o <out> [--replace]` | post-build: append (or replace) a `contractstoragev0` section |
| `strip <wasm> -o <out>` | remove the section |
| `lint [--wasm W] [--schema S].. [--deny-warnings]` | section 7 |
| `decode [--wasm W] [--schema S].. [--entries getLedgerEntries.json].. [--contract C --rpc URL [--fetch NAME=args]..] [--format json\|text] [--strict]` | section 6. With `--contract`, the instance (and wasm, if not given) is fetched read-only. `--strict` exits 2 unless every entry is `matched` |
| `key --contract C --entry NAME [--wasm W\|--schema S] args..` | base64 `LedgerKey` for a schema entry (scalar arguments only) |
| `rpc entries --rpc URL --key-xdr B64..` / `rpc wasm --rpc URL --contract C [-o out]` | read-only capture helpers |
| `size <wasm> [--baseline W]` | section bytes, share of module, code digest, and against a baseline: delta, `code_identical`, `identical_after_strip` |

Exit codes: 0 ok, 1 input/IO/RPC error, 2 policy failure (`lint`, `decode --strict`).

## 10. Library (`sik-storage-schema-core`)

`model` (types, `encode_entry`, `decode_stream`, `Schema::{from_section_bytes,to_section_bytes,merge}`), `sidecar::{parse,to_json}`, `wasm::{sections,read_custom,append_custom,strip_custom,code_digest}`, `load_module`, `inject`, `decode::{Decoder,render,match_pattern,shape}`, `ledger::{parse_get_ledger_entries,decode_entries,build_ledger_key,raw_from_snapshot}`, `lint::lint`, `ledger::to_get_ledger_entries_json` (write entries in the RPC response shape), `api` (in-memory entry points shared by the CLI and the browser engine: `build_decoder`, `decode`, `inspect`, `lint`, `executable`, `code` (SHA-256 checked), `instance_key`, `code_key`, `entry_key`), and `rpc::Client` (feature `rpc`: `getLedgerEntries` only, spaced at 600 ms or more).

The browser engine (`crates/web`) wraps `api` in a JSON C ABI (`ssc_alloc`, `ssc_free`, `ssc_call`, `ssc_out_ptr`); its outputs MUST equal the CLI's for the same inputs (tests: `crates/web/tests/parity.rs`, `site/scripts/verify-engine.mjs`).

## 11. Non-goals (v0)

- No on-chain enforcement and no host change.
- No mandated layouts.
- No storage enumeration: RPC cannot list a contract's keys, so input entries come from known keys, the instance map, fixtures, or an index.
- No history-aware schema selection (SSC-020).
- No TypeScript decoder yet (SSC-021).
