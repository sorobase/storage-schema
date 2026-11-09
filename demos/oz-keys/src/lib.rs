// SPDX-License-Identifier: Apache-2.0
//! DEMO-ONLY (SSC-013). Keys written by OpenZeppelin stellar-contracts 0.8.0
//! crates, declared with `storage_schema_extern!` from a library crate. The
//! OZ crates are not modified and emit no schema themselves; any contract
//! that links this crate carries these
//! declarations in its `contractstoragev0` section.
//!
//! Shapes are from the OZ v0.8.0 sources (tag v0.8.0, commit ddc327e):
//! `packages/tokens/src/fungible/storage.rs` (FungibleStorageKey, AllowanceKey,
//! AllowanceData, Metadata) and `packages/contract-utils/src/pausable/storage.rs`
//! (PausableStorageKey).
#![no_std]

use sik_storage_schema_macros::storage_schema_extern;
use soroban_sdk::{contracttype, String};
use stellar_tokens::fungible::{AllowanceData, AllowanceKey};

/// Mirror of `stellar_tokens::fungible::storage::Metadata`, which OZ keeps
/// in a private module, so its `spec_xdr()` cannot be named from here. Same
/// name and fields; it exists only to give the schema the type definition.
/// It is never stored by this crate.
#[contracttype]
pub struct Metadata {
    pub decimals: u32,
    pub name: String,
    pub symbol: String,
}

storage_schema_extern! {
    #[storage_schema(lib = "stellar-tokens@0.8.0")]
    enum FungibleStorageKey {
        /// Token metadata (decimals, name, symbol).
        #[storage(instance, value = Metadata)]
        Meta,
        /// Balance of an account.
        #[storage(persistent, value = i128)]
        Balance(Address),
        /// Allowance of a spender over an owner's balance.
        #[storage(temporary, value = AllowanceData)]
        Allowance(AllowanceKey),
    }
}

storage_schema_extern! {
    #[storage_schema(lib = "stellar-contract-utils@0.8.0")]
    enum PausableStorageKey {
        /// Whether the contract is paused.
        #[storage(instance, value = bool)]
        Paused,
    }
}
