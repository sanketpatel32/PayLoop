#![cfg(test)]

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::{MockAuth, MockAuthInvoke},
    IntoVal,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env, String,
};

/// Deploys a token (Stellar Asset Contract) we control, mints `amount` to
/// `to`, and returns the token address plus its admin client for minting.
fn create_token<'a>(env: &Env, admin: &Address) -> (Address, token::StellarAssetClient<'a>) {
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_admin = token::StellarAssetClient::new(env, &sac.address());
    (sac.address(), token_admin)
}

struct Setup<'a> {
    env: Env,
    contract: InvoiceContractClient<'a>,
    token: Address,
    token_admin: token::StellarAssetClient<'a>,
    admin: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let (token, token_admin) = create_token(&env, &admin);

    let contract_id = env.register(InvoiceContract, ());
    let contract = InvoiceContractClient::new(&env, &contract_id);
    contract.initialize(&admin, &token);

    Setup {
        env,
        contract,
        token,
        token_admin,
        admin,
    }
}

#[test]
fn create_then_fund_pays_freelancer() {
    let s = setup();
    let freelancer = Address::generate(&s.env);
    let client = Address::generate(&s.env);

    // Fund the client with 200 USDC (7-decimal token units).
    s.token_admin.mint(&client, &2_000_000_000i128);

    let id = s.contract.create_invoice(
        &freelancer,
        &client,
        &2_000_000_000i128,
        &String::from_str(&s.env, "Landing page copy"),
        &0u64,
    );
    assert_eq!(id, 1);

    let inv = s.contract.get_invoice(&id);
    assert_eq!(inv.status, InvoiceStatus::Pending);
    assert_eq!(inv.amount, 2_000_000_000i128);

    let token_client = token::Client::new(&s.env, &s.token);
    assert_eq!(token_client.balance(&client), 2_000_000_000i128);
    assert_eq!(token_client.balance(&freelancer), 0);

    s.contract.fund_invoice(&id);

    // Instant release: funds moved client -> freelancer.
    assert_eq!(token_client.balance(&client), 0);
    assert_eq!(token_client.balance(&freelancer), 2_000_000_000i128);

    let inv = s.contract.get_invoice(&id);
    assert_eq!(inv.status, InvoiceStatus::Paid);
    assert!(inv.paid_at >= inv.created_at);
}

#[test]
fn history_lists_freelancer_invoices_in_order() {
    let s = setup();
    let freelancer = Address::generate(&s.env);
    let client = Address::generate(&s.env);

    for i in 0..3 {
        s.contract.create_invoice(
            &freelancer,
            &client,
            &(1_000_000i128 * (i + 1)),
            &String::from_str(&s.env, "job"),
            &0u64,
        );
    }

    let history = s.contract.get_invoice_history(&freelancer);
    assert_eq!(history.len(), 3);
    assert_eq!(history.get(0).unwrap().id, 1);
    assert_eq!(history.get(2).unwrap().id, 3);
    assert_eq!(history.get(2).unwrap().amount, 3_000_000i128);
}

#[test]
fn double_funding_is_rejected() {
    let s = setup();
    let freelancer = Address::generate(&s.env);
    let client = Address::generate(&s.env);
    s.token_admin.mint(&client, &10_000_000i128);

    let id = s.contract.create_invoice(
        &freelancer,
        &client,
        &5_000_000i128,
        &String::from_str(&s.env, "job"),
        &0u64,
    );
    s.contract.fund_invoice(&id);

    let res = s.contract.try_fund_invoice(&id);
    assert_eq!(res, Err(Ok(Error::AlreadyPaid)));
}

#[test]
fn funding_missing_invoice_errors() {
    let s = setup();
    let res = s.contract.try_fund_invoice(&999u64);
    assert_eq!(res, Err(Ok(Error::InvoiceNotFound)));
}

#[test]
fn rejects_non_positive_amount() {
    let s = setup();
    let freelancer = Address::generate(&s.env);
    let client = Address::generate(&s.env);
    let res = s.contract.try_create_invoice(
        &freelancer,
        &client,
        &0i128,
        &String::from_str(&s.env, "job"),
        &0u64,
    );
    assert_eq!(res, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn cannot_initialize_twice() {
    let s = setup();
    let res = s.contract.try_initialize(&s.admin, &s.token);
    assert_eq!(res, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn timestamps_are_recorded() {
    let s = setup();
    s.env.ledger().with_mut(|l| l.timestamp = 1_700_000_000);

    let freelancer = Address::generate(&s.env);
    let client = Address::generate(&s.env);
    s.token_admin.mint(&client, &10_000_000i128);

    let id = s.contract.create_invoice(
        &freelancer,
        &client,
        &5_000_000i128,
        &String::from_str(&s.env, "job"),
        &1_700_100_000u64,
    );
    let inv = s.contract.get_invoice(&id);
    assert_eq!(inv.created_at, 1_700_000_000);

    s.env.ledger().with_mut(|l| l.timestamp = 1_700_050_000);
    s.contract.fund_invoice(&id);
    let inv = s.contract.get_invoice(&id);
    assert_eq!(inv.paid_at, 1_700_050_000);
}

// --- Authorization tests ---------------------------------------------------
//
// These do NOT use mock_all_auths: each test grants auth for specific parties
// via MockAuth, so they actually verify that the contract calls require_auth
// on the right address (and nothing else).

struct AuthEnv {
    env: Env,
    contract: InvoiceContractClient<'static>,
    contract_address: Address,
    token: Address,
    admin: Address,
    freelancer: Address,
    client: Address,
}

fn auth_setup() -> AuthEnv {
    let env = Env::default();
    let admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token = sac.address();
    let contract_address = env.register(InvoiceContract, ());
    let contract = InvoiceContractClient::new(&env, &contract_address);
    let freelancer = Address::generate(&env);
    let client = Address::generate(&env);

    // initialize requires the admin's auth; grant exactly that invoke.
    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract_address,
            fn_name: "initialize",
            args: (admin.clone(), token.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    contract.initialize(&admin, &token);

    AuthEnv {
        env,
        contract,
        contract_address,
        token,
        admin,
        freelancer,
        client,
    }
}

#[test]
fn create_invoice_succeeds_with_freelancer_auth() {
    let a = auth_setup();
    a.env.mock_auths(&[MockAuth {
        address: &a.freelancer,
        invoke: &MockAuthInvoke {
            contract: &a.contract_address,
            fn_name: "create_invoice",
            args: (
                a.freelancer.clone(),
                a.client.clone(),
                1_000_000i128,
                String::from_str(&a.env, "job"),
                0u64,
            )
                .into_val(&a.env),
            sub_invokes: &[],
        },
    }]);

    let id = a.contract.create_invoice(
        &a.freelancer,
        &a.client,
        &1_000_000i128,
        &String::from_str(&a.env, "job"),
        &0u64,
    );
    assert_eq!(id, 1);
}

#[test]
fn create_invoice_fails_without_freelancer_auth() {
    let a = auth_setup();
    // No auth at all: the freelancer's require_auth must fail the call.
    let res = a.contract.try_create_invoice(
        &a.freelancer,
        &a.client,
        &1_000_000i128,
        &String::from_str(&a.env, "job"),
        &0u64,
    );
    assert!(res.is_err());
}

#[test]
fn create_invoice_rejects_impostor_auth() {
    let a = auth_setup();
    let impostor = Address::generate(&a.env);
    // The impostor signs a create_invoice invoke, but the contract requires
    // the *freelancer's* auth — a different address — so it must fail.
    a.env.mock_auths(&[MockAuth {
        address: &impostor,
        invoke: &MockAuthInvoke {
            contract: &a.contract_address,
            fn_name: "create_invoice",
            args: (
                a.freelancer.clone(),
                a.client.clone(),
                1_000_000i128,
                String::from_str(&a.env, "job"),
                0u64,
            )
                .into_val(&a.env),
            sub_invokes: &[],
        },
    }]);
    let res = a.contract.try_create_invoice(
        &a.freelancer,
        &a.client,
        &1_000_000i128,
        &String::from_str(&a.env, "job"),
        &0u64,
    );
    assert!(res.is_err());
}

fn mint_with_admin_auth(a: &AuthEnv, to: &Address, amount: i128) {
    a.env.mock_auths(&[MockAuth {
        address: &a.admin,
        invoke: &MockAuthInvoke {
            contract: &a.token,
            fn_name: "mint",
            args: (to.clone(), amount).into_val(&a.env),
            sub_invokes: &[],
        },
    }]);
    token::StellarAssetClient::new(&a.env, &a.token).mint(to, &amount);
}

#[test]
fn fund_invoice_succeeds_with_client_auth() {
    let a = auth_setup();
    mint_with_admin_auth(&a, &a.client, 1_000_000);

    // Create is not under test here, so notarize it via freelancer MockAuth.
    a.env.mock_auths(&[MockAuth {
        address: &a.freelancer,
        invoke: &MockAuthInvoke {
            contract: &a.contract_address,
            fn_name: "create_invoice",
            args: (
                a.freelancer.clone(),
                a.client.clone(),
                1_000_000i128,
                String::from_str(&a.env, "job"),
                0u64,
            )
                .into_val(&a.env),
            sub_invokes: &[],
        },
    }]);
    let id = a.contract.create_invoice(
        &a.freelancer,
        &a.client,
        &1_000_000i128,
        &String::from_str(&a.env, "job"),
        &0u64,
    );

    // fund_invoice requires the client's auth — once for the contract call and,
    // because fund_invoice itself calls token.transfer (client → freelancer),
    // again for the token transfer. mock_all_auths can't distinguish those;
    // MockAuth with a sub_invoke expresses exactly what was authorized.
    a.env.mock_auths(&[MockAuth {
        address: &a.client,
        invoke: &MockAuthInvoke {
            contract: &a.contract_address,
            fn_name: "fund_invoice",
            args: (id,).into_val(&a.env),
            sub_invokes: &[MockAuthInvoke {
                contract: &a.token,
                fn_name: "transfer",
                args: (a.client.clone(), a.freelancer.clone(), 1_000_000i128).into_val(&a.env),
                sub_invokes: &[],
            }],
        },
    }]);
    a.contract.fund_invoice(&id);

    let token_client = token::Client::new(&a.env, &a.token);
    assert_eq!(token_client.balance(&a.freelancer), 1_000_000i128);
    assert_eq!(token_client.balance(&a.client), 0i128);
}

#[test]
fn fund_invoice_fails_without_client_auth() {
    let a = auth_setup();
    mint_with_admin_auth(&a, &a.client, 1_000_000);

    a.env.mock_auths(&[MockAuth {
        address: &a.freelancer,
        invoke: &MockAuthInvoke {
            contract: &a.contract_address,
            fn_name: "create_invoice",
            args: (
                a.freelancer.clone(),
                a.client.clone(),
                1_000_000i128,
                String::from_str(&a.env, "job"),
                0u64,
            )
                .into_val(&a.env),
            sub_invokes: &[],
        },
    }]);
    let id = a.contract.create_invoice(
        &a.freelancer,
        &a.client,
        &1_000_000i128,
        &String::from_str(&a.env, "job"),
        &0u64,
    );

    // No client auth granted: fund must fail.
    let res = a.contract.try_fund_invoice(&id);
    assert!(res.is_err());

    // The invoice must remain Pending and unfunded.
    let inv = a.contract.get_invoice(&id);
    assert_eq!(inv.status, InvoiceStatus::Pending);
}

// --- Edge case tests -------------------------------------------------------

fn invoice_with_id(s: &Setup, freelancer: &Address, client: &Address, amount: i128) -> u64 {
    s.contract.create_invoice(
        freelancer,
        client,
        &amount,
        &String::from_str(&s.env, "job"),
        &0u64,
    )
}

#[test]
fn create_invoice_ids_increment() {
    let s = setup();
    let freelancer = Address::generate(&s.env);
    let client = Address::generate(&s.env);
    let a = invoice_with_id(&s, &freelancer, &client, 1_000_000);
    let b = invoice_with_id(&s, &freelancer, &client, 1_000_000);
    let c = invoice_with_id(&s, &freelancer, &client, 1_000_000);
    assert_eq!((a, b, c), (1, 2, 3));
}

#[test]
fn rejects_self_directed_invoice() {
    let s = setup();
    let freelancer = Address::generate(&s.env);
    let res = s.contract.try_create_invoice(
        &freelancer,
        &freelancer,
        &1_000_000i128,
        &String::from_str(&s.env, "job"),
        &0u64,
    );
    assert_eq!(res, Err(Ok(Error::InvalidParties)));
}

#[test]
fn rejects_empty_description() {
    let s = setup();
    let freelancer = Address::generate(&s.env);
    let client = Address::generate(&s.env);
    let res = s.contract.try_create_invoice(
        &freelancer,
        &client,
        &1_000_000i128,
        &String::from_str(&s.env, ""),
        &0u64,
    );
    assert_eq!(res, Err(Ok(Error::InvalidDescription)));
}

#[test]
fn funding_whole_balance_succeeds_exactly() {
    let s = setup();
    let freelancer = Address::generate(&s.env);
    let client = Address::generate(&s.env);
    s.token_admin.mint(&client, &1_000_000i128);
    let id = invoice_with_id(&s, &freelancer, &client, 1_000_000);
    s.contract.fund_invoice(&id);
    let token_client = token::Client::new(&s.env, &s.token);
    assert_eq!(token_client.balance(&freelancer), 1_000_000i128);
    assert_eq!(token_client.balance(&client), 0i128);
}

#[test]
fn funding_short_balance_errors() {
    let s = setup();
    let freelancer = Address::generate(&s.env);
    let client = Address::generate(&s.env);
    s.token_admin.mint(&client, &500_000i128);
    let id = invoice_with_id(&s, &freelancer, &client, 1_000_000);
    let res = s.contract.try_fund_invoice(&id);
    assert_eq!(res, Err(Ok(Error::InsufficientBalance)));
    // Unchanged state after the failed attempt.
    let inv = s.contract.get_invoice(&id);
    assert_eq!(inv.status, InvoiceStatus::Pending);
    assert_eq!(s.contract.get_invoice_history(&freelancer).len(), 1);
}

#[test]
fn history_for_unknown_freelancer_is_empty() {
    let s = setup();
    let freelancer = Address::generate(&s.env);
    let client = Address::generate(&s.env);
    invoice_with_id(&s, &freelancer, &client, 1_000_000);
    let stranger = Address::generate(&s.env);
    let history = s.contract.get_invoice_history(&stranger);
    assert_eq!(history.len(), 0);
}

// --- Pagination and counter tests -------------------------------------------

#[test]
fn invoice_count_returns_created_total() {
    let s = setup();
    let freelancer = Address::generate(&s.env);
    let client = Address::generate(&s.env);

    // After initialize, no invoices exist yet: count is 0.
    let count = s.contract.invoice_count();
    assert_eq!(count, 0);

    invoice_with_id(&s, &freelancer, &client, 1_000_000);
    let count = s.contract.invoice_count();
    assert_eq!(count, 1);

    invoice_with_id(&s, &freelancer, &client, 2_000_000);
    let count = s.contract.invoice_count();
    assert_eq!(count, 2);
}

#[test]
fn get_invoice_range_respects_bounds() {
    let s = setup();
    let freelancer = Address::generate(&s.env);
    let client = Address::generate(&s.env);

    // Create 5 invoices for the freelancer.
    for i in 1..=5 {
        invoice_with_id(&s, &freelancer, &client, 1_000_000i128 * i);
    }

    // Get first 2 invoices (start=0, len=2).
    let range = s.contract.get_invoice_range(&freelancer, &0, &2);
    assert_eq!(range.len(), 2);
    assert_eq!(range.get(0).unwrap().id, 1);
    assert_eq!(range.get(1).unwrap().id, 2);

    // Get next 2 invoices (start=2, len=2).
    let range = s.contract.get_invoice_range(&freelancer, &2, &2);
    assert_eq!(range.len(), 2);
    assert_eq!(range.get(0).unwrap().id, 3);
    assert_eq!(range.get(1).unwrap().id, 4);

    // Get from start=4, len=2 (only 1 invoice left).
    let range = s.contract.get_invoice_range(&freelancer, &4, &2);
    assert_eq!(range.len(), 1);
    assert_eq!(range.get(0).unwrap().id, 5);

    // Get from start beyond end (empty).
    let range = s.contract.get_invoice_range(&freelancer, &5, &2);
    assert_eq!(range.len(), 0);
}

#[test]
fn get_invoice_range_handles_extreme_inputs() {
    let s = setup();
    let freelancer = Address::generate(&s.env);
    let client = Address::generate(&s.env);

    for i in 1..=3 {
        invoice_with_id(&s, &freelancer, &client, 1_000_000i128 * i);
    }

    // len = 0 returns an empty page.
    let range = s.contract.get_invoice_range(&freelancer, &0, &0);
    assert_eq!(range.len(), 0);

    // start beyond the end returns an empty page.
    let range = s.contract.get_invoice_range(&freelancer, &3, &10);
    assert_eq!(range.len(), 0);

    // start = u32::MAX must not trap (saturating arithmetic on wasm32) and
    // simply yields an empty page.
    let range = s.contract.get_invoice_range(&freelancer, &u32::MAX, &10);
    assert_eq!(range.len(), 0);

    // A huge len is capped at MAX_PAGE_LEN and returns everything available.
    let range = s.contract.get_invoice_range(&freelancer, &0, &u32::MAX);
    assert_eq!(range.len(), 3);
    assert_eq!(range.get(0).unwrap().id, 1);
}

#[test]
fn get_invoice_range_caps_page_at_max_len() {
    let s = setup();
    let freelancer = Address::generate(&s.env);
    let client = Address::generate(&s.env);

    // MAX_PAGE_LEN + 2 invoices prove the page is capped, not just bounded by
    // the history length.
    for _ in 0..27 {
        invoice_with_id(&s, &freelancer, &client, 1_000_000);
    }

    let range = s.contract.get_invoice_range(&freelancer, &0, &u32::MAX);
    assert_eq!(range.len(), 25);

    // The remainder is reachable on the next page.
    let range = s.contract.get_invoice_range(&freelancer, &25, &10);
    assert_eq!(range.len(), 2);
}

#[test]
fn version_reports_semver_symbol() {
    let s = setup();
    // Policy: MINOR bump for wire-compatible ABI additions since v1.
    assert_eq!(s.contract.version(), symbol_short!("v1_1_0"));
}

#[test]
fn get_invoice_range_for_unknown_freelancer_is_empty() {
    let s = setup();
    let freelancer = Address::generate(&s.env);
    let client = Address::generate(&s.env);
    let stranger = Address::generate(&s.env);

    invoice_with_id(&s, &freelancer, &client, 1_000_000);

    // Stranger has no invoices.
    let range = s.contract.get_invoice_range(&stranger, &0, &10);
    assert_eq!(range.len(), 0);
}

#[test]
fn get_client_history_tracks_invoices_owed() {
    let s = setup();
    let freelancer1 = Address::generate(&s.env);
    let freelancer2 = Address::generate(&s.env);
    let client = Address::generate(&s.env);

    // Client owes freelancer1 for 2 invoices.
    invoice_with_id(&s, &freelancer1, &client, 1_000_000);
    invoice_with_id(&s, &freelancer1, &client, 2_000_000);

    // Client owes freelancer2 for 1 invoice.
    invoice_with_id(&s, &freelancer2, &client, 3_000_000);

    // Get client's history.
    let history = s.contract.get_client_history(&client);
    assert_eq!(history.len(), 3);
    assert_eq!(history.get(0).unwrap().id, 1);
    assert_eq!(history.get(1).unwrap().id, 2);
    assert_eq!(history.get(2).unwrap().id, 3);
}

#[test]
fn get_client_history_for_unknown_client_is_empty() {
    let s = setup();
    let freelancer = Address::generate(&s.env);
    let client = Address::generate(&s.env);
    let stranger = Address::generate(&s.env);

    invoice_with_id(&s, &freelancer, &client, 1_000_000);

    // Stranger hasn't been charged for anything.
    let history = s.contract.get_client_history(&stranger);
    assert_eq!(history.len(), 0);
}
