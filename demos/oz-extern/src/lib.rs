// SPDX-License-Identifier: Apache-2.0
//! DEMO-ONLY (SSC-013), not audited, not for deployment with value.
//!
//! A small fungible token with a pause switch, written for this project on
//! top of OpenZeppelin `stellar-tokens` and `stellar-contract-utils` 0.8.0
//! (MIT, used as dependencies, unmodified). The OZ crates write their own
//! storage keys and emit no schema. Instead of a sidecar injected after the
//! build (the `oz-pausable` demo), the keys are declared in source with
//! `storage_schema_extern!`, in the library crate `ssc-demo-oz-keys`. The
//! contract's own key uses `#[storage_key]`.
#![no_std]

use sik_storage_schema_macros::storage_key;
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, String, Symbol};
use stellar_contract_utils::pausable;
use stellar_tokens::fungible::Base;

// Links the OZ key declarations (their `contractstoragev0` statics).
use ssc_demo_oz_keys as _;

/// Owner, allowed to pause.
#[storage_key(instance, value = Address)]
pub const OWNER: Symbol = symbol_short!("OWNER");

#[contract]
pub struct ExternDemo;

#[contractimpl]
impl ExternDemo {
    pub fn __constructor(e: &Env, owner: Address, initial_supply: i128) {
        Base::set_metadata(e, 7, String::from_str(e, "Extern demo"), String::from_str(e, "XDM"));
        Base::mint(e, &owner, initial_supply);
        e.storage().instance().set(&OWNER, &owner);
    }

    pub fn balance(e: &Env, account: Address) -> i128 {
        Base::balance(e, &account)
    }

    pub fn transfer(e: &Env, from: Address, to: Address, amount: i128) {
        if pausable::paused(e) {
            panic!("paused");
        }
        Base::transfer(e, &from, &to.into(), amount);
    }

    pub fn approve(e: &Env, owner: Address, spender: Address, amount: i128, live_until_ledger: u32) {
        Base::approve(e, &owner, &spender, amount, live_until_ledger);
    }

    pub fn pause(e: &Env, caller: Address) {
        caller.require_auth();
        let owner: Address = e.storage().instance().get(&OWNER).expect("owner is set");
        if owner != caller {
            panic!("not the owner");
        }
        pausable::pause(e);
    }
}
