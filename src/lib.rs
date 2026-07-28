//! stellar-rust-client
//!
//! Reusable utilities for Soroban contract execution: upload WASM, deploy a
//! contract instance, invoke a state-changing function, and read a value via
//! simulation -- ported from a working TypeScript `MultiContractDeployer`.
//!
//! See `soroban-utils-design.md` in the repo root for the full design
//! rationale, the TS-to-Rust workflow mapping, and open follow-ups.
//!
//! Quick start:
//! ```ignore
//! use secrecy::SecretString;
//! use stellar_rust_client::{Client, NetworkConfig, SignerConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let signer_config = SignerConfig::Secret(SecretString::from("S...".to_string()));
//!     let client = Client::new(NetworkConfig::testnet(), signer_config).await?;
//!
//!     let wasm = stellar_rust_client::wasm::read_wasm_file("./target/wasm32-unknown-unknown/release/my_contract.wasm")?;
//!     let (contract_address, _wasm_hash) = client.deploy_contract(&wasm, vec![]).await?;
//!
//!     println!("deployed at {contract_address}");
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod config;
pub mod deploy;
pub mod error;
pub mod fee;
pub mod invoke;
pub mod poll;
pub mod read;
pub mod signer;
pub mod txbuilder;
pub mod utils;
pub mod wasm;

pub use client::Client;
pub use config::{NetworkConfig, PollConfig};
pub use error::{Result, SorobanUtilsError};
pub use invoke::InvokeOutcome;
pub use signer::{LocalSigner, Signer, SignerConfig, SignerFactory};

// Re-export the pieces of soroban-client that callers will need to
// construct arguments (ScVal) and interpret results, so downstream crates
// don't have to separately depend on soroban-client just for these types.
pub use soroban_client::transaction::ScVal;
