<!-- SPDX-License-Identifier: Apache-2.0 -->
# sik-storage-schema-core

The `contractstoragev0` format for Soroban contracts: model, XDR codec, sidecar JSON, Wasm custom sections, decoder and lint. Off-chain only; the decoder says `unknown`, `ambiguous` or `value_mismatch` instead of guessing.

Default feature `rpc` adds a read-only `getLedgerEntries` client. Unaudited; the parsers read untrusted Wasm and XDR.

Docs, spec and playground: <https://storage-schema.vercel.app/>. Source: <https://github.com/sorobase/storage-schema>.
