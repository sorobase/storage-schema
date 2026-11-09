use sik_storage_schema_macros::{storage_key, storage_schema};
use soroban_sdk::{contracttype, Address};
#[allow(unused_imports)] // only referenced by macro output, absent under --cfg sik_storage_schema_off
use soroban_token_sdk::metadata::TokenMetadata;

pub(crate) const DAY_IN_LEDGERS: u32 = 17280;
pub(crate) const INSTANCE_BUMP_AMOUNT: u32 = 7 * DAY_IN_LEDGERS;
pub(crate) const INSTANCE_LIFETIME_THRESHOLD: u32 = INSTANCE_BUMP_AMOUNT - DAY_IN_LEDGERS;

pub(crate) const BALANCE_BUMP_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
pub(crate) const BALANCE_LIFETIME_THRESHOLD: u32 = BALANCE_BUMP_AMOUNT - DAY_IN_LEDGERS;

#[derive(Clone)]
#[contracttype]
pub struct AllowanceDataKey {
    pub from: Address,
    pub spender: Address,
}

#[contracttype]
pub struct AllowanceValue {
    pub amount: i128,
    pub live_until_ledger: u32,
}

// storage-schema demo: the only change to upstream is the attributes below
// (#[storage_schema] above #[contracttype], one #[storage(..)] per variant)
// and the `storage_key` declaration for soroban-token-sdk's metadata key.
#[storage_schema]
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    #[storage(temporary, value = AllowanceValue)]
    Allowance(AllowanceDataKey),
    #[storage(persistent, value = i128)]
    Balance(Address),
    // Declared upstream but never written by this contract.
    #[storage(skip)]
    State(Address),
    #[storage(instance, value = Address)]
    Admin,
}

/// soroban-token-sdk 28.0.0 stores metadata in instance storage under the
/// raw symbol `METADATA` (verified in the crate source, see SOURCES.md). The token-sdk crate does not use this macro, so the
/// contract declares the key on its behalf (`lib` names the writer).
#[storage_key(instance, value = TokenMetadata, symbol = "METADATA", name = "METADATA", lib = "soroban-token-sdk@28.0.0")]
#[allow(dead_code)]
const TOKEN_SDK_METADATA_KEY: &str = "METADATA";
