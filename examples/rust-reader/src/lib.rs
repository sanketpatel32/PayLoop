//! Minimal cross-contract call example: a Soroban contract that reads a
//! PayLoop invoice and the PayLoop contract version through its client.
//!
//! Build:
//!   cargo build --target wasm32v1-none --release
//!
//! Deploy the wasm, then call `read_invoice(payloop_contract_id, invoice_id)`.
#![no_std]

use soroban_sdk::{contract, contractclient, contractimpl, contracttype, Address, Env, String, Symbol, Vec};

// Mirror of the PayLoop `Invoice` struct — the field order and types must
// match the deployed contract for the return value to deserialize.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Invoice {
    pub id: u64,
    pub freelancer: Address,
    pub client: Address,
    pub amount: i128,
    pub description: String,
    pub token: Address,
    pub status: Status,
    pub due_date: u64,
    pub created_at: u64,
    pub paid_at: u64,
}

#[contracttype]
#[derive(Clone, Debug)]
pub enum Status {
    Pending,
    Paid,
}

// Client generated against PayLoop's public read interface.
#[contractclient(name = "PayLoopClient")]
pub trait PayLoopInterface {
    fn version(env: Env) -> Symbol;
    fn get_invoice(env: Env, invoice_id: u64) -> Invoice;
    fn invoice_count(env: Env) -> u64;
    fn get_invoice_history(env: Env, freelancer: Address) -> Vec<Invoice>;
}

#[contract]
pub struct PayLoopReader;

#[contractimpl]
impl PayLoopReader {
    /// Read one invoice from a PayLoop deployment.
    pub fn read_invoice(env: Env, payloop: Address, invoice_id: u64) -> Invoice {
        PayLoopClient::new(&env, &payloop).get_invoice(&invoice_id)
    }

    /// Read the PayLoop contract's interface version (e.g. `v1_1_0`).
    pub fn read_version(env: Env, payloop: Address) -> Symbol {
        PayLoopClient::new(&env, &payloop).version()
    }
}
