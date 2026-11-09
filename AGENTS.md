# storage-schema: instructions for agents

Follow `docs/AGENT_GUIDE.md` (programme root) and `docs/SESSION_BUILD_2026-11-03.md`. Then read `README.md`, `SPEC.md`, `ARCHITECTURE.md`, `TASKS.md`, `TESTING.md`, `RISKS.md`, `PRIOR-ART.md` and `SOURCES.md`.

The user wants to build this project themselves. Keep changes small and explained, and prefer tickets over sweeping rewrites.

1. Work only inside `projects/storage-schema/`. Root docs and other projects are off limits; write cross-project needs in `TASKS.md`.
2. The wire format in SPEC 3.2 is pinned by `model::tests::wire_layout_is_stable`. Changing it means a new section name (`contractstoragev1`), a SPEC change and a SEP draft change, in that order.
3. Decoders never guess. Do not add heuristics that turn `ambiguous` or `unknown` into `matched`.
4. Fixtures are produced by `scripts/build-demos.sh` and `scripts/capture-mainnet.sh` only, with provenance. Never hand-edit them.
5. Network: read-only mainnet RPC (`getLatestLedger`, `getEvents`, `getLedgerEntries`) at 1 request per second or slower. No keys and no transactions.
6. Demo contracts are DEMO-ONLY test subjects. Upstream code keeps its licence (soroban-examples Apache-2.0, OZ MIT). The OZ example stays unmodified.
7. No posting. `drafts/` holds text for the user to send.
8. Stubs use `todo!("SSC-NNN")`, and every TODO needs a ticket id. Every source file carries the Apache-2.0 SPDX header.
9. Do not run cargo inside downloaded upstream checkouts: their `rust-toolchain.toml` can change the user's toolchains (this happened once, see RISKS R-10).
10. Before marking a task DONE, `bash scripts/check.sh` must pass.
