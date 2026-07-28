/// Network connection details. Mirrors the `networkConfig` object loaded from
/// `loadConfig(config.network)` in the TS `MultiContractDeployer`.
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub rpc_url: String,
    pub network_passphrase: String,
}

impl NetworkConfig {
    pub fn testnet() -> Self {
        Self {
            rpc_url: "https://soroban-testnet.stellar.org".to_string(),
            network_passphrase: "Test SDF Network ; September 2015".to_string(),
        }
    }

    pub fn futurenet() -> Self {
        Self {
            rpc_url: "https://rpc-futurenet.stellar.org".to_string(),
            network_passphrase: "Test SDF Future Network ; October 2022".to_string(),
        }
    }

    pub fn mainnet(rpc_url: impl Into<String>) -> Self {
        Self {
            rpc_url: rpc_url.into(),
            network_passphrase: "Public Global Stellar Network ; September 2015".to_string(),
        }
    }

    pub fn custom(rpc_url: impl Into<String>, network_passphrase: impl Into<String>) -> Self {
        Self {
            rpc_url: rpc_url.into(),
            network_passphrase: network_passphrase.into(),
        }
    }
}

/// Fixed-interval polling config used by `poll::poll_transaction_status`.
///
/// Kept fixed-interval (not exponential backoff) on purpose: the slow leg in
/// custodial-signer flows (e.g. Fireblocks) is signer approval latency, not
/// RPC load, so backing off doesn't help and just adds latency to the common
/// case of a fast local-keypair signature.
#[derive(Debug, Clone, Copy)]
pub struct PollConfig {
    pub interval: std::time::Duration,
    pub timeout: std::time::Duration,
}

impl Default for PollConfig {
    fn default() -> Self {
        Self {
            interval: std::time::Duration::from_secs(2),
            timeout: std::time::Duration::from_secs(60 * 60),
        }
    }
}
