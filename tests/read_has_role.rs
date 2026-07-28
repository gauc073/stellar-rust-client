//! Testnet read-operation test, port of the TS "access control" script:
//! for each contract, for each role, call `has_role(account, role)` via
//! simulation and print whether the account holds it.
//!
//! Run with: `cargo test --test read_has_role -- --ignored --nocapture`
//! (ignored by default -- it hits live testnet RPC).

mod common;

use soroban_client::address::{Address, AddressTrait};
use soroban_client::xdr::ScString;
use stellar_rust_client::ScVal;

/// Build the `address` ScVal argument, equivalent to
/// `nativeToScVal(account, { type: "address" })` in the TS script.
fn sc_address(account: &str) -> ScVal {
    Address::new(account)
        .expect("invalid account address")
        .to_sc_val()
        .expect("failed to encode address as ScVal")
}

/// Build the `string` ScVal argument, equivalent to
/// `nativeToScVal(role, { type: "string" })` in the TS script.
fn sc_string(value: &str) -> ScVal {
    ScVal::String(ScString(
        value
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("role name too long for ScString"),
    ))
}

#[tokio::test]
#[ignore = "hits live testnet RPC; run with `cargo test -- --ignored`"]
async fn checks_has_role_across_contracts() {
    common::load_env();
    let client = common::client();

    // Please add contract names here -- must match CONTRACT_ADDRESS_<NAME> env vars.
    let contracts = ["CoreContract"];

    // User account whose role membership is being checked. Defaults to the
    // caller/deployer address, same as the TS script's `deployerAddress`.
    let account = client.public_key().to_string();

    // Role names to check.
    let roles = ["EXECUTOR_ROLE", "DEFAULT_ADMIN_ROLE"];

    for contract_name in contracts {
        let contract_address = "CC3LP4VY7P2TQGWTTQFSH3COS53FYVUBPXHK77TPPDIUQBVG7GMUWT7U";

        for role in roles {
            let args = vec![sc_address(&account), sc_string(role)];

            let result = client
                .read_contract(&contract_address, "has_role", args)
                .await
                .unwrap_or_else(|e| {
                    panic!("read_contract(has_role) failed for {contract_name}/{role}: {e}")
                });

            let has_role = matches!(result, ScVal::Bool(true));
            let status = if has_role {
                "Does Have"
            } else {
                "Does Not Have"
            };
            println!(
                "For Contract {contract_address} ({contract_name}), User {account} {status} Role {role}"
            );
        }
    }
}
