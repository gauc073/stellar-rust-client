//! Testnet integration test for `Client::extend_ttl` (`ExtendFootprintTTL`).
//!
//! PLACEHOLDER: `MAPPING_NAME` below assumes a `Balance(Address)`-style
//! storage key (the common Soroban token-contract shape). Update it to match
//! whatever entry the contract under test actually stores, the same way
//! `write_register_contract.rs` flags its own placeholder args.
//!
//! Unlike `write_register_contract.rs`, this doesn't just check
//! `InvokeOutcome::Executed` -- it also reads the entry's TTL back via
//! `Server::get_contract_data` before and after, and asserts the "after"
//! value actually increased. `ExtendFootprintTTL` simulation reports a state
//! change any time the requested ledger is higher than the current one, even
//! if something else about the footprint was subtly wrong, so confirming the
//! on-chain number itself moved is the real assertion here -- not just "the
//! transaction landed".
//!
//! Run with: `cargo test --test write_extend_ttl -- --ignored --nocapture`
//! (ignored by default -- it submits a real transaction on testnet).

mod common;

use soroban_client::{Options, Server};
use stellar_rust_client::{Durability, InvokeOutcome, ttl};

// TODO: replace with the real mapping name for the contract under test once
// its storage layout is confirmed.
const MAPPING_NAME: &str = "Balance";

#[tokio::test]
#[ignore = "submits a real transaction on testnet; run with `cargo test -- --ignored`"]
async fn extends_ttl_for_a_persistent_entry() {
    common::load_env();
    let client = common::client().await;
    let network = common::network_config();
    let rpc = Server::new(&network.rpc_url, Options::default()).expect("failed to build Server");

    let contract_address = "CDO7CKS3EZIPPM3FWZ35OBRIZ5TXSE6OUO3ICCXT2GGXLE5WEQLMJBLU";

    // Defaults to the caller's own address -- same convention as
    // `write_register_contract.rs`'s `account_to_register`.
    let account = "GAQ44YTNFVMG2N3LFIUJA2AKQTXGSJBC3XNCMARQGHDFHD2TYZW6FIN3";
    let details =
        vec![stellar_rust_client::utils::address(&account).expect("invalid account address")];

    let storage_key =
        ttl::mapping_entry_key(MAPPING_NAME, details.clone()).expect("failed to build storage key");

    let before = live_until_ledger(&rpc, &contract_address, storage_key.clone())
        .await
        .unwrap_or_else(|e| {
            panic!(
                "entry {MAPPING_NAME}({account}) not found on {contract_address} \
                 before extend -- can't measure the TTL change: {e}"
            )
        });

    // Ask for a healthy margin past whatever the current TTL is, so the
    // assertion below isn't flaky against a contract whose default TTL
    // already happens to be long.
    let extend_to = before + 5;

    let outcome = client
        .extend_ttl(
            &contract_address,
            MAPPING_NAME,
            details,
            Durability::Persistent,
            extend_to,
        )
        .await
        .unwrap_or_else(|e| panic!("extend_ttl({MAPPING_NAME}) failed on {contract_address}: {e}"));

    match outcome {
        InvokeOutcome::Executed { tx_hash } => println!(
            "Extended TTL for {MAPPING_NAME}({account}) on {contract_address} to ledger \
             {extend_to} -- confirmed, tx hash: {tx_hash}"
        ),
        InvokeOutcome::SkippedNoStateChange(message) => panic!(
            "expected extend_ttl to change the entry's live-until ledger, but it didn't: {message}"
        ),
    }

    let after = live_until_ledger(&rpc, &contract_address, storage_key)
        .await
        .expect("entry should still exist after extending its TTL");

    assert!(
        after > before,
        "expected live-until ledger to increase (before: {before}, after: {after})"
    );
    assert!(
        after >= extend_to,
        "expected live-until ledger to reach at least the requested {extend_to} (got {after})"
    );
}

/// Read the current `live_until_ledger_seq` for one storage entry via
/// `getLedgerEntries` (through `Server::get_contract_data`), so the test can
/// assert the extend actually moved the number rather than just trusting
/// that "no error" means "it worked".
async fn live_until_ledger(
    rpc: &Server,
    contract_address: &str,
    storage_key: stellar_rust_client::ScVal,
) -> Result<u32, String> {
    let entry = rpc
        .get_contract_data(
            contract_address,
            storage_key,
            soroban_client::Durability::Persistent,
        )
        .await
        .map_err(|e| e.to_string())?;

    entry
        .live_until_ledger_seq
        .ok_or_else(|| "entry has no live_until_ledger_seq (unexpected)".to_string())
}
