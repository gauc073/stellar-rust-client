use crate::config::{NetworkConfig, PollConfig};
use crate::error::Result;
use crate::signer::{SignerConfig, SignerFactory};
use soroban_client::transaction::ScVal;
use soroban_client::{Options, Server};

/// Top-level entry point. Holds the RPC connection, network config, and a
/// `SignerFactory` -- not a `Signer` directly. Roughly equivalent to
/// `MultiContractDeployer` in the TS source, minus the deployment-log
/// bookkeeping (out of scope -- see design doc §7.2).
///
/// The signer is deliberately *not* held as a ready-to-use `Signer` for the
/// `Client`'s whole lifetime. Every write method below builds a transient
/// `Signer` via `signer_factory.build_signer()` immediately before signing
/// and lets it drop immediately after -- see `signer::SignerFactory` for
/// why that matters.
pub struct Client {
    server: Server,
    network: NetworkConfig,
    signer_factory: Box<dyn SignerFactory>,
    public_key: String,
    poll_cfg: PollConfig,
}

impl Client {
    pub async fn new(network: NetworkConfig, signer_config: SignerConfig) -> Result<Self> {
        Self::with_poll_config(network, signer_config, PollConfig::default()).await
    }

    pub async fn with_poll_config(
        network: NetworkConfig,
        signer_config: SignerConfig,
        poll_cfg: PollConfig,
    ) -> Result<Self> {
        let server = Server::new(&network.rpc_url, Options::default())?;
        let signer_factory = signer_config.into_factory()?;
        // Resolved once here, not re-derived on every call -- a public key
        // isn't secret, and for a custodial signer this may cost a network
        // round trip (e.g. resolving a Fireblocks vault's address).
        let public_key = signer_factory.public_key().await?;
        Ok(Self {
            server,
            network,
            signer_factory,
            public_key,
            poll_cfg,
        })
    }

    pub fn public_key(&self) -> &str {
        &self.public_key
    }

    pub async fn upload_wasm(&self, wasm: &[u8]) -> Result<[u8; 32]> {
        let signer = self.signer_factory.build_signer().await?;
        crate::deploy::upload_wasm(
            &self.server,
            &self.network.network_passphrase,
            signer.as_ref(),
            wasm,
            self.poll_cfg,
        )
        .await
        // `signer` drops here, right after use.
    }

    pub async fn create_contract_instance(
        &self,
        wasm_hash: [u8; 32],
        constructor_args: Vec<ScVal>,
    ) -> Result<String> {
        let signer = self.signer_factory.build_signer().await?;
        crate::deploy::create_contract_instance(
            &self.server,
            &self.network.network_passphrase,
            signer.as_ref(),
            wasm_hash,
            constructor_args,
            self.poll_cfg,
        )
        .await
    }

    pub async fn deploy_contract(
        &self,
        wasm: &[u8],
        constructor_args: Vec<ScVal>,
    ) -> Result<(String, [u8; 32])> {
        // Two separate transient signers (one per submitted transaction),
        // not one shared across both steps -- consistent with "build fresh,
        // use once, drop" everywhere else.
        let wasm_hash = self.upload_wasm(wasm).await?;
        let contract_address = self
            .create_contract_instance(wasm_hash, constructor_args)
            .await?;
        Ok((contract_address, wasm_hash))
    }

    pub async fn invoke_contract(
        &self,
        contract_address: &str,
        function_name: &str,
        args: Vec<ScVal>,
    ) -> Result<crate::invoke::InvokeOutcome> {
        let signer = self.signer_factory.build_signer().await?;
        crate::invoke::invoke_contract(
            &self.server,
            &self.network.network_passphrase,
            signer.as_ref(),
            contract_address,
            function_name,
            args,
            self.poll_cfg,
        )
        .await
    }

    /// Extend the TTL of a single contract storage entry so it stays live
    /// at least until ledger `extend_to`.
    ///
    /// `mapping_name` + `details` build the storage key the same way a
    /// `#[contracttype] enum DataKey { <mapping_name>(..details) }` variant
    /// would serialize (see `ttl::mapping_entry_key`) -- pass an empty
    /// `details` for a bare-symbol key. `durability` must match how the
    /// contract wrote the entry (`Persistent` vs `Temporary`). For a raw
    /// `ScVal` key that doesn't fit that compound shape, call
    /// `ttl::extend_ttl` directly instead of going through this wrapper.
    pub async fn extend_ttl(
        &self,
        contract_address: &str,
        mapping_name: &str,
        details: Vec<ScVal>,
        durability: crate::ttl::Durability,
        extend_to: u32,
    ) -> Result<crate::invoke::InvokeOutcome> {
        let signer = self.signer_factory.build_signer().await?;
        let storage_key = crate::ttl::mapping_entry_key(mapping_name, details)?;
        crate::ttl::extend_ttl(
            &self.server,
            &self.network.network_passphrase,
            signer.as_ref(),
            contract_address,
            storage_key,
            durability,
            extend_to,
            self.poll_cfg,
        )
        .await
    }

    /// Extend the TTL of a contract's **instance storage** -- the single
    /// entry backing every `env.storage().instance()` key on that contract.
    /// There's only one such entry per contract (all `.instance()` values
    /// live inside it), so unlike `extend_ttl` there's no mapping
    /// name/details/durability to pass -- see the `ttl` module docs for how
    /// this differs from per-key persistent/temporary entries.
    pub async fn extend_instance_ttl(
        &self,
        contract_address: &str,
        extend_to: u32,
    ) -> Result<crate::invoke::InvokeOutcome> {
        let signer = self.signer_factory.build_signer().await?;
        crate::ttl::extend_instance_ttl(
            &self.server,
            &self.network.network_passphrase,
            signer.as_ref(),
            contract_address,
            extend_to,
            self.poll_cfg,
        )
        .await
    }

    /// Extend the TTL of an uploaded Wasm **code** entry -- keyed by the
    /// 32-byte hash `upload_wasm`/`deploy_contract` returned, not a contract
    /// address. Every contract instance deployed from the same wasm shares
    /// this one entry and its one TTL.
    pub async fn extend_wasm_ttl(
        &self,
        wasm_hash: [u8; 32],
        extend_to: u32,
    ) -> Result<crate::invoke::InvokeOutcome> {
        let signer = self.signer_factory.build_signer().await?;
        crate::ttl::extend_wasm_ttl(
            &self.server,
            &self.network.network_passphrase,
            signer.as_ref(),
            wasm_hash,
            extend_to,
            self.poll_cfg,
        )
        .await
    }

    /// Look up the wasm hash a deployed contract instance runs, by reading
    /// its instance-storage entry. `Ok(None)` means the contract has no
    /// instance entry (doesn't exist / archived) or isn't backed by a
    /// separately-expiring wasm entry at all. Read-only -- no signer needed.
    pub async fn wasm_hash_of(&self, contract_address: &str) -> Result<Option<[u8; 32]>> {
        crate::ttl::wasm_hash_of(&self.server, contract_address).await
    }

    /// Current TTL (`live_until_ledger_seq`) of a wasm code entry. `Ok(None)`
    /// means the entry doesn't exist -- never uploaded, or expired and
    /// archived (`extend_wasm_ttl` handles the archived case automatically
    /// if you go on to extend it). Read-only -- no signer needed.
    pub async fn wasm_ttl(&self, wasm_hash: [u8; 32]) -> Result<Option<u32>> {
        crate::ttl::wasm_ttl(&self.server, wasm_hash).await
    }

    pub async fn read_contract(
        &self,
        contract_address: &str,
        function_name: &str,
        args: Vec<ScVal>,
    ) -> Result<ScVal> {
        // Read-only: no signer needed at all, just the public key as the
        // simulation source account.
        crate::read::read_contract(
            &self.server,
            &self.network.network_passphrase,
            &self.public_key,
            contract_address,
            function_name,
            args,
        )
        .await
    }
}
