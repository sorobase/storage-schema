<!-- SPDX-License-Identifier: Apache-2.0 -->
# sik-storage-schema-macros

`#[storage_schema]` and `#[storage_key]` attribute macros. They add no code to a contract: they write a `contractstoragev0` Wasm custom section describing the storage keys of `#[contracttype]` enums.

Checked with soroban-sdk 28.0.0. Unaudited. Guide: <https://storage-schema.vercel.app/guides/annotate/>. Source: <https://github.com/sorobase/storage-schema>.
