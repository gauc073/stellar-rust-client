//! soroban-utils
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
//! use soroban_utils::{Client, NetworkConfig, LocalSigner};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let signer = LocalSigner::from_secret("S...")?;
//!     let client = Client::new(NetworkConfig::testnet(), Box::new(signer))?;
//!
//!     let wasm = soroban_utils::wasm::read_wasm_file("./target/wasm32-unknown-unknown/release/my_contract.wasm")?;
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
pub mod invoke;
pub mod poll;
pub mod read;
pub mod signer;
pub mod txbuilder;
pub mod wasm;

pub use client::Client;
pub use config::{NetworkConfig, PollConfig};
pub use error::{Result, SorobanUtilsError};
pub use signer::{LocalSigner, Signer};

// Re-export the pieces of soroban-client that callers will need to
// construct arguments (ScVal) and interpret results, so downstream crates
// don't have to separately depend on soroban-client just for these types.
pub use soroban_client::transaction::ScVal;
