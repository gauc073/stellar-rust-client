//! Shared setup for the testnet integration tests in this directory.
//!
//! Mirrors the env-var handling in the TS `access-control` script:
//! `NETWORK`, `SOURCE_SECRET`, plus one `CONTRACT_ADDRESS_<NAME>` per
//! contract under test (since this crate doesn't own a deployments.json --
//! see design doc §7.2 / BUILD_NOTES.md).
//!
//! These tests hit live testnet RPC + a real signer, so they're marked
//! `#[ignore]` in the test files and must be run explicitly:
//!
//! ```sh
//! cargo test --test read_has_role -- --ignored --nocapture
//! cargo test --test write_register_contract -- --ignored --nocapture
//! ```
//!
//! Required `.env` (see `.env.example`):
//! ```text
//! NETWORK=testnet
//! SOURCE_SECRET=S...
//! CONTRACT_ADDRESS_USSTTOKEN=C...
//! CONTRACT_ADDRESS_REGISTERCONTRACT=C...
//! CONTRACT_ADDRESS_CORECONTRACT=C...
//! ```

use secrecy::{ExposeSecret, SecretString};
use stellar_rust_client::{Client, NetworkConfig, SignerConfig};

pub fn network_config() -> NetworkConfig {
    match std::env::var("NETWORK").as_deref() {
        Ok("testnet") => NetworkConfig::testnet(),
        Ok("mainnet") => {
            let rpc_url = std::env::var("RPC_URL")
                .expect("RPC_URL must be set explicitly for mainnet (no default, unlike testnet)");
            NetworkConfig::mainnet(rpc_url)
        }
        other => {
            panic!("Please set NETWORK=testnet or NETWORK=mainnet in your .env (got {other:?})")
        }
    }
}

/// `Client::new` is now async (resolving a custodial signer's public key
/// may need a network round trip), so this is async too -- call sites need
/// `.await`.
pub async fn client() -> Client {
    let data_b64 =
        std::env::var("data").expect("Please set data in your .env (deployer/caller secret key)");
    let password_b64 = std::env::var("password")
        .expect("Please set data in your .env (deployer/caller secret key)");
    let values = data_security::decrypt_export_to_map(&data_b64, &password_b64).unwrap();
    let key_name = "SOURCE_SECRET".to_string();
    let secret: &str = values.get(&key_name).unwrap().expose_secret();
    let signer_config = SignerConfig::Secret(SecretString::from(secret.to_string()));
    Client::new(network_config(), signer_config)
        .await
        .expect("failed to build Client")
}

/// Reads `CONTRACT_ADDRESS_<NAME>` (name upper-cased, matching the TS
/// script's contract name strings like "UsstToken" -> `CONTRACT_ADDRESS_USSTTOKEN`).
pub fn contract_address(name: &str) -> String {
    let var = format!("CONTRACT_ADDRESS_{}", name.to_uppercase());
    std::env::var(&var).unwrap_or_else(|_| panic!("Please set {var} in your .env"))
}

pub fn load_env() {
    // Ignore the error: fine if there's no .env file and vars are already
    // set in the environment (e.g. in CI).
    let _ = dotenvy::dotenv();
}
