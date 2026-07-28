use crate::config::{NetworkConfig, PollConfig};
use crate::error::Result;
use crate::signer::Signer;
use soroban_client::transaction::ScVal;
use soroban_client::{Options, Server};

/// Top-level entry point. Holds the RPC connection, network config, and a
/// pluggable `Signer`. Roughly equivalent to `MultiContractDeployer` in the
/// TS source, minus the deployment-log bookkeeping (out of scope -- see
/// design doc §7.2) and with the Fireblocks-vs-local branching replaced by
/// the `Signer` trait object.
pub struct Client {
    server: Server,
    network: NetworkConfig,
    signer: Box<dyn Signer>,
    poll_cfg: PollConfig,
}

impl Client {
    pub fn new(network: NetworkConfig, signer: Box<dyn Signer>) -> Result<Self> {
        Self::with_poll_config(network, signer, PollConfig::default())
    }

    pub fn with_poll_config(
        network: NetworkConfig,
        signer: Box<dyn Signer>,
        poll_cfg: PollConfig,
    ) -> Result<Self> {
        let server = Server::new(&network.rpc_url, Options::default())?;
        Ok(Self {
            server,
            network,
            signer,
            poll_cfg,
        })
    }

    pub fn public_key(&self) -> &str {
        self.signer.public_key()
    }

    pub async fn upload_wasm(&self, wasm: &[u8]) -> Result<[u8; 32]> {
        crate::deploy::upload_wasm(
            &self.server,
            &self.network.network_passphrase,
            self.signer.as_ref(),
            wasm,
            self.poll_cfg,
        )
        .await
    }

    pub async fn create_contract_instance(
        &self,
        wasm_hash: [u8; 32],
        constructor_args: Vec<ScVal>,
    ) -> Result<String> {
        crate::deploy::create_contract_instance(
            &self.server,
            &self.network.network_passphrase,
            self.signer.as_ref(),
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
        crate::deploy::deploy_contract(
            &self.server,
            &self.network.network_passphrase,
            self.signer.as_ref(),
            wasm,
            constructor_args,
            self.poll_cfg,
        )
        .await
    }

    pub async fn invoke_contract(
        &self,
        contract_address: &str,
        function_name: &str,
        args: Vec<ScVal>,
    ) -> Result<crate::invoke::InvokeOutcome> {
        crate::invoke::invoke_contract(
            &self.server,
            &self.network.network_passphrase,
            self.signer.as_ref(),
            contract_address,
            function_name,
            args,
            self.poll_cfg,
        )
        .await
    }

    pub async fn read_contract(
        &self,
        contract_address: &str,
        function_name: &str,
        args: Vec<ScVal>,
    ) -> Result<ScVal> {
        crate::read::read_contract(
            &self.server,
            &self.network.network_passphrase,
            self.signer.public_key(),
            contract_address,
            function_name,
            args,
        )
        .await
    }
}
