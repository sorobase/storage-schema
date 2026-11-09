<!-- SPDX-License-Identifier: Apache-2.0 -->
# storage-schema: demo

How to run the demo from a clean checkout, and a 3-5 minute investor script. Written 2026-11-08.

## 1. Run it

Requirements: Node >= 22.18 and pnpm 10.22.0 for the site. Rust is **not** needed to show the site: the browser engine (`site/public/engine/storage-schema-engine.wasm`) and all site data are committed.

```bash
cd projects/storage-schema/site
pnpm install --frozen-lockfile
pnpm test            # browser engine == CLI goldens (23 checks), offline
pnpm build           # static site -> site/dist/ (13 pages), offline
pnpm preview         # http://localhost:4332
```

Live mode in the playground needs internet access to `https://mainnet.sorobanrpc.com` (read-only `getLedgerEntries`, CORS open, checked 2026-11-08). Everything else works offline.

### Optional: the terminal part (needs Rust)

`rust-toolchain.toml` pins rustc 1.97.1.

```bash
cd projects/storage-schema
cargo build -p sik-storage-schema-cli
CLI=target/debug/storage-schema
$CLI inspect fixtures/wasm/ssc_demo_token.wasm
$CLI decode --schema sidecars/stellar-asset-contract.storage.json \
     --entries fixtures/mainnet/usdc-sac.instance.json --entries fixtures/mainnet/usdc-sac.balances.json --format text
$CLI lint --wasm fixtures/wasm/ssc_demo_token.wasm --schema sidecars/oz-fungible-pausable-v0.8.0.storage.json; echo "exit $?"   # exit 2
```

### Regenerating the data (only if inputs change)

| Step | Command | Needs |
|---|---|---|
| Demo wasm + size report | `bash scripts/build-demos.sh` | Rust, stellar-cli 25.2.0; several minutes |
| Test-host captures | `bash scripts/capture-testhost.sh` | Rust (runs the demo e2e tests) |
| Mainnet captures | `bash scripts/capture-mainnet.sh` | network, read-only; do not re-run casually (fixtures are dated) |
| Golden CLI outputs | `bash scripts/golden.sh` | Rust |
| Browser engine | `bash scripts/build-engine.sh` | Rust + `wasm32-unknown-unknown` target; about 3.5 minutes |
| Site data | `cd site && pnpm demo:data` | Node |
| Full gate | `bash scripts/check.sh` | Rust + Node |

## 2. Investor script (3-5 minutes)

Open `http://localhost:4332/`.

**0:00 The problem (landing page, 45 s).**
- Point at the hero figure: "8 of 8 matched". Say: "Every Soroban contract stores its state as untyped key-value pairs. An explorer sees `[Symbol("Balance"), Address] -> I128` and has to guess what it is. Since soroban-sdk 28 the contract's own interface file drops the storage types too."
- Point at the chip under the figure: "Recorded mainnet capture, ledger 65,292,086, 2026-11-03. Every number on this site says where it came from."
- Scroll to "The gap": the SDK maintainers' open question in rs-soroban-sdk#1818 (April 2026) is exactly how to map a ledger entry to a type. This is a worked answer.

**0:45 What a contract carries (15 s).**
- Scroll to the code block: "The whole change to a token is one attribute per key. The schema then travels inside the wasm, 885 bytes, code byte-identical." Click the Fixture chip if asked for the method.

**1:00 Playground: real mainnet data (60 s).** Click **Open playground**.
- The USDC scenario is open. "These are USDC's real storage entries from mainnet. The built-in asset contract has no wasm, so it is described by a sidecar file. Every entry: named, typed, `matched`." Open the `DataKey::Balance` row: show the named value (`amount`, `authorized`, `clawback`), the declared type and the raw XDR.
- Point at the green line: "Identical to the native CLI output. The decoder is the Rust code compiled to WebAssembly, running in this tab."
- Click **A mainnet contract whose wasm carries no schema**: "This is every contract today. 37 entries, all `unknown`. The decoder refuses to guess."

**2:00 The honest statuses (60 s).**
- Click **Deliberate: USDC balances read with the wrong schema**: "Same key shape, different value type. You get `value_mismatch` with the reason, not a wrong number." (The "Deliberate" tag is on screen: these are real inputs combined on purpose.)
- Click **Deliberate: two crates both claim Balance(Address)**: "This is rs-soroban-sdk#2088: two libraries using the same key. The chain holds one entry. The decoder says `ambiguous`, lists both writers, and does not pick." Scroll to **Lint, before deploy**: "and the lint catches it before deployment."

**3:00 Works with code you do not control (30 s).**
- Click **Unmodified OpenZeppelin token, schema injected after the build**, then **OpenZeppelin keys declared in source with storage_schema_extern!**: "No change to OpenZeppelin's code: either write the schema into the built wasm afterwards, or declare their keys from a small library crate. `lib` says who writes each key."

**3:30 Live (45 s).** Click the **Live mainnet** tab, then **Read mainnet** (USDC is prefilled).
- "This page just called a public mainnet RPC, read-only." Point at the chip: "Live read, with the ledger the RPC reported." Point at **Calls this page made**.
- Optional: in **Also read one key** pick `DataKey::Balance (persistent)`, paste `CA6PUJLBYKZKUEKLZJMKBZLEKP2OTHANDEOWSFF44FTSYLKQPIICCJBE`, click **Read mainnet** again. If you then read a wasm contract, the SAC sidecar is cleared for you and the chip says `no sidecar`; the result is all `unknown`.

**4:15 Close (30 s).** Open **Status and limits** in the sidebar.
- "What works, what is not built, the gates. Read-only on mainnet, unaudited, demo contracts are demo-only. The SEP draft is written and not submitted: posting it is the project owner's decision." Mention the **Size cost** page if asked about chain fees.

### If something fails during the demo

- Live read fails (network, RPC rate limit): say so; the error box states nothing was decoded. Every scenario still works offline.
- The playground shows "Loading the decoder" for a few seconds on first load: the engine is 1.1 MB.
