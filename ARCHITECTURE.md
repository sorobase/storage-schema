# storage-schema architecture

## Modules

```
crates/core   (sik-storage-schema-core, lib)
  model.rs     SCStorageEntry model + XDR codec (normative wire format, SPEC 3)
  rusttype.rs  Rust type syntax <-> SCSpecTypeDef / SCSpecEntry (shared by macro and sidecar)
  sidecar.rs   JSON sidecar <-> model (SPEC 5)
  wasm.rs      section walker: list / read (concatenate) / append / strip / code digest
  decode.rs    TypeRegistry, typed rendering, key matching, statuses (SPEC 4, 6)
  ledger.rs    getLedgerEntries JSON, instance-map expansion, report, key building, test-snapshot adapter
  lint.rs      pairwise overlap analysis (SPEC 7)
  rpc.rs       read-only JSON-RPC client (feature "rpc", ureq)
  lib.rs       load_module(), inject(), read_spec()
crates/macros (sik-storage-schema-macros, proc-macro; depends on core without "rpc")
  #[storage_schema] on key enums, #[storage_key] on consts, storage_schema_extern!
  api.rs       in-memory entry points shared by the CLI and the browser engine (SSC-025)
crates/cli    (sik-storage-schema-cli, bin `storage-schema`)
crates/web    (sik-storage-schema-web, cdylib for wasm32-unknown-unknown): JSON C ABI over core::api for the site (SSC-026)
demos/        separate workspace, soroban-sdk =28.0.0; token, increment (macro), oz-pausable (unmodified, injected),
              oz-extern + oz-keys (storage_schema_extern! from a library crate, SSC-013), capture (test-host capture helper)
site/         Astro + Starlight docs and playground; data from `pnpm demo:data` (fixtures, goldens, engine provenance)
sidecars/     hand-written schemas: OZ fungible-pausable v0.8.0, Stellar Asset Contract
schemas/      JSON Schema 2020-12 of the sidecar
fixtures/     wasm/ (built demos + provenance + size report), mainnet/ (RPC captures + provenance),
              testhost/ (test-host captures + provenance), scenarios.json, golden/ (CLI outputs)
drafts/       SEP draft, sdk#1818 comment draft (not posted)
```

## Data flow

```
build time                                         read time
----------                                         ---------
#[storage_schema] enum ──encode_entry()──┐         wasm (file or RPC getLedgerEntries(code))
#[storage_key] const  ───────────────────┤                 │ wasm::read_custom (concat sections)
T::spec_xdr() (TYPE entries) ────────────┤                 ▼
                                         ▼          Schema ──┐   sidecar.json ──parse──► Schema
               link_section "contractstoragev0"              ▼                              │
               (linker merges same-name statics)      Decoder (keys + TypeRegistry ◄── contractspecv0 UDTs)
                                                             │
sidecar.json ──parse──► Schema ──inject()──► wasm            ▼
                                             ledger entries (fixtures / RPC / test snapshot)
                                                  │ instance map expanded to INSTANCE entries
                                                  ▼
                                       DecodedEntry {status, name, key, value, reason, raw}
```

## Decisions

| # | Decision | Why | Alternatives rejected |
|---|---|---|---|
| D1 | XDR in the section, JSON for sidecars | matches `contractspecv0` tooling and keeps the section compact; JSON is for hand-authoring; they are 1:1 and round-trip | JSON in the section (larger; a second parser for SDKs) |
| D2 | Reuse `SCSpecTypeDef`/`SCSpecEntry`/`SCVal` | no new type language; SDK macros already compute these bytes | a custom type grammar |
| D3 | Three pattern kinds with fixed precedence | covers every encoding the SDK produces (variant vec, raw literal) plus a catch-all; precedence is total, so results are deterministic | regex or path patterns (no SDK producer) |
| D4 | `ambiguous` instead of choosing by value conformance | #2088 collisions really are one entry; silently choosing hides bugs | "first declared wins" |
| D5 | Type entries embedded via `<T>::spec_xdr()` | spec shaking (sdk 28) strips storage-only types; reusing the SDK's own bytes avoids re-deriving struct layouts in our macro | macro re-parses the type's source (it cannot see it) |
| D6 | Macro is attribute-only and adds no code | answers the size objection; compatible with #1762 attribute syntax; coexists with any accessor generator | generating typed accessors (that is #1762/#2127/soroban-sdk-tools' job) |
| D7 | Rust syntax for sidecar types | the same mapping as the macro; authors copy types from source | stellar-xdr JSON only (still accepted, and used for lossless output) |
| D8 | Format crates do not depend on soroban-sdk | SDK-version independence; avoids the 25 vs 29 lockfile conflict (CONVENTIONS) | depending on soroban-spec |
| D9 | Hand-written Wasm section walker | only framing is touched, so code stays byte-identical; no parser-version churn | wasmparser / walrus |
| D10 | `--cfg sik_storage_schema_off` | measures cost against an identical source build | two copies of each contract |
| D11 | Browser decoder = the Rust core compiled to wasm behind a C ABI (2026-11-08) | one decoder, so the site cannot drift from the CLI; parity is tested against goldens | a TypeScript port for the demo (SSC-021 stays a separate ticket); wasm-bindgen (needs a CLI binary matched to the crate version) |

## Interfaces exposed to other projects

`wasm-catalog` (#8) and `export-gate` (#13) could read `contractstoragev0` through the core crate or the CLI JSON (`inspect --format json`, `decode`). No other project depends on this one yet. Proposed consumers are listed in TASKS "Cross-project requests".
