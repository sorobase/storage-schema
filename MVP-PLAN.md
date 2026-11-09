<!-- SPDX-License-Identifier: Apache-2.0 -->
# storage-schema: MVP plan

Written 2026-11-08 at the start of the MVP pass. Tags: [M] measured here, [P] primary source read, [I] inferred, [E] estimate.

## 1. Current state (2026-11-08, before this pass)

- Format (`contractstoragev0`), SPEC, sidecar JSON + JSON Schema, macros, decoder with four statuses, lint, inject/strip, read-only RPC and CLI exist and are tested (README status table of 2026-11-03).
- Real data on disk: built demo wasm (`fixtures/wasm/`, provenance with sha256), a recorded mainnet capture of the USDC SAC and one holder contract at ledger 65,292,081 (`fixtures/mainnet/`), and a size report.
- No `todo!()` in the code. Open tickets: SSC-012..023.
- Gaps for an investor demo:
  - No visual surface. Everything is terminal output.
  - `storage_schema_extern!` has no user (SSC-013): one of the three documented routes for "keys written by a dependency" is untested.
  - The test-host runs of the demo contracts produce real ledger entries, but they only exist inside `cargo test`. There is no committed capture a UI could show.
  - The `ambiguous` and `value_mismatch` statuses are covered by unit tests on synthetic values only; no scenario shows them on real entries.
  - No golden decoder outputs (SSC-015).

## 2. MVP definition: what an investor sees in 3-5 minutes

1. **Docs site landing page** (`site/`): the problem in one sentence (a contract-data ledger entry carries no name and no type; rs-soroban-sdk#1818 asks how an off-chain reader maps it), gate badges, and one real figure with its chip: "USDC on mainnet: 8 of 8 entries decoded `matched` through a sidecar (recorded mainnet capture, ledger 65,292,081, 2026-11-03)".
2. **Playground** (`/playground/`): pick a scenario and the real decoder (the Rust core compiled to WebAssembly) runs in the browser and lists every entry with name, durability, typed key, typed value and status. Scenarios, all from real captures:
   - USDC SAC on mainnet + SAC sidecar: all `matched`.
   - A mainnet holder contract whose wasm has no section: all `unknown` (the status quo today).
   - USDC SAC balances read with the wrong schema (the soroban-examples token's `Balance -> i128`): `value_mismatch`, not a garbage decode.
   - The token, increment and OZ demo contracts run in the soroban test host: `matched` from the schema embedded in the wasm, or injected after the build.
   - The token's entries with the OZ fungible sidecar merged in: `ambiguous` (the sdk#2088 collision, with value evidence per candidate).
   - The new `storage_schema_extern!` demo (SSC-013): OZ-written keys declared in the contract source, `matched`.
   Every scenario shows its provenance chip and a "matches the CLI golden output" check computed in the page.
3. **Bring your own**: drop a `.wasm` (inspect what it carries), paste a sidecar or a `getLedgerEntries` response, decode.
4. **Live mode**: paste a mainnet contract id; the page makes read-only `getLedgerEntries` calls from the browser (instance, then wasm code), decodes the instance storage with whatever schema the wasm carries or the chosen sidecar, and labels the result "Live read, ledger N".
5. **Size cost** page: the measured section cost (token 885 B, increment 72 B, OZ 1,273 B; code byte-identical) as a chart with table view.
6. **SEP draft** page: the draft rendered in the site, marked "draft, not submitted".

## 3. Surface choice

Docs site plus in-browser playground in one Astro + Starlight site under `site/`, re-themed with the kit design (`design/astro`, accent `storage-schema` hue 328). Reason: the product is a format, a library and a CLI for developers, so docs are the product surface; the playground is the visual proof. Recommended surface followed; no deviation.

In-browser engine: `crates/web` (`sik-storage-schema-web`), a `cdylib` for `wasm32-unknown-unknown` that links `sik-storage-schema-core` (feature `rpc` off) and exposes a JSON-in/JSON-out C ABI. No wasm-bindgen (it needs a CLI binary matched to the crate version; the ABI is three functions). The built engine is committed under `site/public/engine/` with a provenance file, so the site builds offline without Rust. Faithfulness: a Node script runs every scenario through the built engine and compares with golden outputs produced by the native CLI; a Rust test does the same natively.

## 4. Tasks in this pass

| Id | Task |
|---|---|
| SSC-013 | `demos/oz-extern`: a contract (our code) built on OZ `stellar-tokens` 0.8.0 whose OZ keys are declared with `storage_schema_extern!` in a separate library crate, plus e2e test. Records whether statics from a dependency crate reach the final wasm |
| SSC-024 | Test-host captures: the demo e2e tests write the entries the contract wrote, as a `getLedgerEntries`-shaped JSON, to `fixtures/testhost/` (`scripts/capture-testhost.sh`, provenance file) |
| SSC-015 | Golden decoder outputs for every scenario (`scripts/golden.sh` -> `fixtures/golden/`), checked by a CLI test |
| SSC-025 | Shared request/response API in core (`core::api`): decode/inspect/lint/key from in-memory inputs, used by the CLI and the engine so both produce the same JSON |
| SSC-026 | `crates/web` engine + `scripts/build-engine.sh` + native parity test |
| SSC-027 | `site/`: Starlight docs, playground (scenarios, bring your own, live mainnet read), size page, SEP draft page; `pnpm demo:data` builds data from fixtures with provenance; `pnpm test` checks engine output against goldens |
| SSC-028 | DEMO.md, PITCH.md, DEPLOY.md, README status table, TASKS.md |

Stretch, only if the above is done: a testnet deployment of the demo token with a runtime throwaway key, so live mode can also show a contract whose schema is read from the on-chain wasm.

## 5. Left out, and why

- **Newer stellar-cli check** (SSC-012): needs a second stellar-cli build or download; the demo uses CLI 25.2.0, which is checked.
- **Fuzzing** (SSC-014): needs cargo-fuzz and a nightly toolchain and an hour of CPU on a shared 4-core machine.
- **#2127 value encoding, history-aware decoding, TS decoder, importers** (SSC-019..022): depth work, not on the demo path. The browser engine is the Rust decoder, so a TS port is not needed for the demo.
- **Posting the SEP draft or the #1818 comment** (G-OUTREACH): the user posts.
- **Hosting, crates/npm publishing**: later phase, listed in DEPLOY.md.

## 6. Outcome (2026-11-08, end of the pass)

Done: SSC-013, SSC-015, SSC-024 to SSC-028 (TASKS.md). `bash scripts/check.sh` passes: core 33 unit tests, CLI 8 integration tests (including the golden check over 9 scenarios), engine 3 parity tests, demos 6 e2e + 16 upstream unit tests, clippy `-D warnings`, and the built browser engine against the goldens (23 checks) [M]. The site builds offline (13 pages) and was looked at in light and dark at 1440 and 390 px (no horizontal scroll). Live mode was exercised against mainnet from the browser: USDC SAC instance + one balance (4/4 matched, ledger 65,366,504) and the holder contract via its wasm (37/37 unknown, code SHA-256 checked) [M].

Findings worth knowing:

- Cross-crate emission works only when the contract references the library crate; otherwise the declarations vanish without any error (SSC-013, measured; ticket SSC-030 for a check).
- Re-running `build-demos.sh` reproduced every committed wasm byte for byte; the test-host captures are deterministic too.
- The browser engine is 1.1 MB (SSC-031 to shrink).

Not done, as planned: SSC-012, SSC-014, SSC-018 to SSC-022, publishing and hosting (SSC-029). The stretch goal (a testnet deployment for live mode, SSC-032) was not attempted: the live mainnet path already shows a schema-less wasm contract and the SAC, and no contract on mainnet carries the section yet, so a testnet contract would be the only on-chain example; it stays a ticket.
