<!-- SPDX-License-Identifier: Apache-2.0 -->
# sik-storage-schema-cli

Installs the `storage-schema` binary: `inspect`, `inject`, `strip`, `lint`, `decode`, `key` and read-only `rpc`.

```bash
cargo install sik-storage-schema-cli
storage-schema inspect contract.wasm
```

Network use is read-only (`getLedgerEntries`, `getLatestLedger`, `getEvents`). No keys, no transactions. Unaudited. Docs: <https://storage-schema.vercel.app/>. Source: <https://github.com/sorobase/storage-schema>.
