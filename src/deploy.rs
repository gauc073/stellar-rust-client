use crate::config::PollConfig;
use crate::error::{Result, SorobanUtilsError};
use crate::fee::{self, MIN_BASE_FEE};
use crate::signer::Signer;
use crate::txbuilder::prepare_and_send;
use soroban_client::Server;
use soroban_client::operation::Operation;
use soroban_client::transaction::ScVal;

/// Upload a compiled contract WASM blob to the network.
///
/// Port of `uploadWasm` in the TS source. Returns the 32-byte WASM hash
/// (used by `create_contract_instance` below).
pub async fn upload_wasm(
    server: &Server,
    network_passphrase: &str,
    signer: &dyn Signer,
    wasm: &[u8],
    poll_cfg: PollConfig,
) -> Result<[u8; 32]> {
    let op = Operation::new()
        .upload_wasm(wasm, None)
        .map_err(|e| SorobanUtilsError::Xdr(format!("{:?}", e)))?;

    // Ask the network for a realistic inclusion fee instead of always
    // bidding the floor -- avoids transactions sitting in the mempool
    // during congestion. Falls back to the floor if the fee-stats call
    // itself fails.
    let fee = fee::recommended_inclusion_fee(server)
        .await
        .unwrap_or(MIN_BASE_FEE);

    let (_tx_hash, response) = prepare_and_send(
        server,
        network_passphrase,
        signer.public_key(),
        op,
        fee,
        signer,
        poll_cfg,
    )
    .await?;

    let (_meta, return_value) = response
        .to_result_meta()
        .ok_or(SorobanUtilsError::MissingReturnValue)?;
    let return_value = return_value.ok_or(SorobanUtilsError::MissingReturnValue)?;

    match return_value {
        ScVal::Bytes(bytes) => {
            let bytes: Vec<u8> = bytes.into();
            bytes
                .try_into()
                .map_err(|_| SorobanUtilsError::Xdr("wasm hash was not 32 bytes".into()))
        }
        other => Err(SorobanUtilsError::Xdr(format!(
            "unexpected return value for upload_wasm: {other:?}"
        ))),
    }
}

/// Create a contract instance from an already-uploaded WASM hash.
///
/// Port of `createContractInstance`. `constructor_args` is passed straight
/// through to the contract's `__constructor` if it has one, same as the TS
/// version's `constructorArgs`.
pub async fn create_contract_instance(
    server: &Server,
    network_passphrase: &str,
    signer: &dyn Signer,
    wasm_hash: [u8; 32],
    constructor_args: Vec<ScVal>,
    poll_cfg: PollConfig,
) -> Result<String> {
    let deployer = signer.public_key();

    let op = Operation::new()
        .create_contract(deployer, wasm_hash, None, None, constructor_args)
        .map_err(|e| SorobanUtilsError::Xdr(format!("{:?}", e)))?;

    let fee = fee::recommended_inclusion_fee(server)
        .await
        .unwrap_or(MIN_BASE_FEE);

    let (_tx_hash, response) = prepare_and_send(
        server,
        network_passphrase,
        deployer,
        op,
        fee,
        signer,
        poll_cfg,
    )
    .await?;

    let (_meta, return_value) = response
        .to_result_meta()
        .ok_or(SorobanUtilsError::MissingReturnValue)?;
    let return_value = return_value.ok_or(SorobanUtilsError::MissingReturnValue)?;

    match return_value {
        ScVal::Address(addr) => Ok(addr.to_string()),
        other => Err(SorobanUtilsError::Xdr(format!(
            "unexpected return value for create_contract: {other:?}"
        ))),
    }
}

/// Convenience wrapper: upload + create in one call, matching the
/// two-step body of the TS `deployContract` (minus the deployment-log
/// bookkeeping, which is out of scope for this crate -- see design doc §7.2).
pub async fn deploy_contract(
    server: &Server,
    network_passphrase: &str,
    signer: &dyn Signer,
    wasm: &[u8],
    constructor_args: Vec<ScVal>,
    poll_cfg: PollConfig,
) -> Result<(String, [u8; 32])> {
    let wasm_hash = upload_wasm(server, network_passphrase, signer, wasm, poll_cfg).await?;
    let contract_address = create_contract_instance(
        server,
        network_passphrase,
        signer,
        wasm_hash,
        constructor_args,
        poll_cfg,
    )
    .await?;
    Ok((contract_address, wasm_hash))
}
