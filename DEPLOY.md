<!-- SPDX-License-Identifier: Apache-2.0 -->
# storage-schema: deploy and publish checklist (later phase)

Agents do not publish, deploy or push (programme rules); the project owner runs these steps. Written 2026-11-08. Ticket: SSC-029.

**Status (2026-11-09):** the docs site is hosted on Vercel at <https://storage-schema.vercel.app/> (Astro preset, root directory `site`, `pnpm install --frozen-lockfile`, `pnpm build`, output `dist`, Node 22.x). Source: <https://github.com/sorobase/storage-schema>. The crates are **not** published yet (see "Before publishing crates").

## What gets published

| Artefact | Where | Build | Notes |
|---|---|---|---|
| Docs site + playground (**live**) | Vercel; any static host (GitHub Pages, Cloudflare Pages, Netlify, S3+CDN) | `cd site && pnpm install --frozen-lockfile && pnpm build` -> `site/dist/` | no server, no secrets, no env vars. Serve `.wasm` as `application/wasm` (most hosts do) |
| `sik-storage-schema-core` | crates.io | `cargo publish -p sik-storage-schema-core` | default feature `rpc` pulls `ureq`; the macro and engine use `default-features = false` |
| `sik-storage-schema-macros` | crates.io | `cargo publish -p sik-storage-schema-macros` | depends on core; publish core first |
| `sik-storage-schema-cli` (binary `storage-schema`) | crates.io (`cargo install`) and optional release binaries | `cargo publish -p sik-storage-schema-cli` | |
| `sik-storage-schema-web` | not published | `bash scripts/build-engine.sh` | its output is committed into the site |
| Demo contracts (`demos/`) | **never** | | DEMO-ONLY test subjects |
| Testnet contract ids | none | | this project deploys no contract; it has no on-chain component |

## Before publishing crates

1. Set `publish = true` (workspace `Cargo.toml` has `publish = false`) and add `repository`, `homepage`, `readme`, `keywords`, `categories` to each crate.
2. Replace path dependencies with versions (`sik-storage-schema-core = { version = "0.1.0", ... }` in macros, cli, web).
3. Check the crate names are free on crates.io and decide on the `sik-` prefix (kit naming convention).
4. `cargo package -p <crate> --list` and `cargo publish --dry-run -p <crate>` for each, in order core, macros, cli.
5. Run fuzzing first (SSC-014): the parsers read untrusted wasm and XDR.
6. Root `NOTICE` / `docs/THIRD_PARTY.md` attribution rows for the copied demo code (cross-project request 1 in `TASKS.md`); demos are not published, but the repo is.

## Before hosting the site

1. Set `site` (and `base` if served under a sub-path) in `site/astro.config.mjs`. Internal links in MDX pages are root-relative (`/playground/`): if `base` is not `/`, prefix them or use `import.meta.env.BASE_URL` (the components already do).
2. `cd site && pnpm test && pnpm build`. Re-run `pnpm demo:data` first if any fixture, golden or the engine changed (it refuses an engine whose sha256 differs from its provenance file).
3. Content Security Policy, if the host adds one: the playground needs `connect-src https://mainnet.sorobanrpc.com` for live mode and `script-src 'wasm-unsafe-eval'` (or `unsafe-eval` on older browsers) to instantiate the engine.
4. Optional: a second public RPC as fallback for live mode; check its CORS first.

## Environment variables

None. The site has no build-time or runtime secrets. The CLI takes `--rpc URL` on the command line. `scripts/capture-mainnet.sh` reads optional `RPC` and `USDC_SAC`.

## What must never go to mainnet (G-VALUE)

- No mainnet writes of any kind: the CLI and the site only call `getLedgerEntries` / `getLatestLedger` / `getEvents`; there is no signing code in this project.
- The demo contracts (`demos/token`, `increment`, `oz-pausable`, `oz-extern`) must not be deployed with value anywhere. They are copies or thin wrappers of examples, unaudited.

## After publishing

- Update `README.md` install instructions (`cargo install sik-storage-schema-cli`) and the site's Quickstart.
- Posting the SEP draft and the #1818 comment is a separate decision for the project owner (G-OUTREACH); re-run the prior-art check first (SSC-023).
