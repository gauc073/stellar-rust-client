//! Testnet write-operation test against `RegisterContract`.
//!
//! PLACEHOLDER: the function name and arguments below (`register`, single
//! address arg) are guesses since the contract isn't finalized yet. Update
//! `FUNCTION_NAME` and the `args` vec once `RegisterContract`'s real
//! interface is settled -- everything else (client setup, invoke + confirm)
//! should stay the same.
//!
//! Run with: `cargo test --test write_register_contract -- --ignored --nocapture`
//! (ignored by default -- it submits a real transaction on testnet).

mod common;

use stellar_rust_client::{InvokeOutcome, utils};

// TODO: replace with the real exported function name on RegisterContract.
const FUNCTION_NAME: &str = "update_price";

#[tokio::test]
#[ignore = "submits a real transaction on testnet; run with `cargo test -- --ignored`"]
async fn registers_account_on_register_contract() {
    common::load_env();
    let client = common::client().await;

    let contract_address = "CC3LP4VY7P2TQGWTTQFSH3COS53FYVUBPXHK77TPPDIUQBVG7GMUWT7U".to_string();

    // TODO: swap for the real argument list once RegisterContract's
    // constructor/function signature is finalized. Placeholder assumes a
    // single `address` argument (the account being registered), defaulting
    // to the caller's own address.
    let account_to_register = client.public_key().to_string();
    let args = vec![
        utils::address(&account_to_register).expect("invalid account address"),
        utils::i128_val(1_138_328_760_000_000_000),
    ];

    let outcome = client
        .invoke_contract(&contract_address, FUNCTION_NAME, args)
        .await
        .unwrap_or_else(|e| {
            panic!("invoke_contract({FUNCTION_NAME}) failed on {contract_address}: {e}")
        });

    match outcome {
        InvokeOutcome::Executed { tx_hash } => println!(
            "Invoked {FUNCTION_NAME}({account_to_register}) on RegisterContract ({contract_address}) -- confirmed, tx hash: {tx_hash}"
        ),
        // Simulation succeeded but reported no state change -- e.g. the
        // value being set already matches on-chain state. Not a crash, but
        // also not what this test is meant to verify, so fail loudly rather
        // than silently passing.
        InvokeOutcome::SkippedNoStateChange(message) => {
            panic!("expected {FUNCTION_NAME} to change state, but it didn't: {message}")
        }
    }

    // TODO: once RegisterContract exposes a read function to confirm
    // registration (e.g. `is_registered`), add a follow-up
    // `client.read_contract(...)` call here to assert the write actually
    // took effect, the same way the read test in `read_has_role.rs` checks
    // role membership after a grant.
}
