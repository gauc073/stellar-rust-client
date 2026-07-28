use crate::config::PollConfig;
use crate::error::{Result, SorobanUtilsError};
use crate::poll::poll_transaction_status;
use crate::signer::Signer;
use soroban_client::Server;
use soroban_client::soroban_rpc::GetTransactionResponse;
use soroban_client::transaction::{Transaction, TransactionBuilder, TransactionBuilderBehavior};
use soroban_client::xdr::Operation as XdrOperation;

/// The one place that does `build -> prepare (simulate+assemble) -> sign ->
/// send -> poll`.
///
/// This block is copy-pasted four times in the TS `MultiContractDeployer`
/// (`uploadWasm`, `createContractInstance`, `invokeContract`, and indirectly
/// in `upgradeContractInstance` via `invokeContract`). Centralizing it here
/// is the main point of this crate existing: every deploy/invoke module
/// below calls into this one function instead of re-implementing the loop.
///
/// `Server::prepare_transaction` performs both simulation and assembly in
/// one RPC round trip (equivalent to the TS
/// `simulateTransaction` + `rpc.assembleTransaction` pair).
pub async fn prepare_and_send(
    server: &Server,
    network_passphrase: &str,
    caller_address: &str,
    operation: XdrOperation,
    fee: u32,
    signer: &dyn Signer,
    poll_cfg: PollConfig,
) -> Result<GetTransactionResponse> {
    let mut source_account = server.get_account(caller_address).await?;

    let transaction: Transaction =
        TransactionBuilder::new(&mut source_account, network_passphrase, None)
            .fee(fee)
            .add_operation(operation)
            .build();

    let prepared = server
        .prepare_transaction(&transaction)
        .await
        .map_err(|e| SorobanUtilsError::Simulation(e.to_string()))?;

    let signed = signer.sign(prepared, network_passphrase).await?;

    let hash = hex::encode(
        <Transaction as soroban_client::transaction::TransactionBehavior>::hash(&signed),
    );

    server.send_transaction(signed).await?;

    poll_transaction_status(server, &hash, poll_cfg).await
}

/// Read-only path: build, simulate (no send), return the simulation
/// response. Equivalent to `readContract` in the TS source, which never
/// calls `sendTransaction`.
pub async fn build_and_simulate(
    server: &Server,
    network_passphrase: &str,
    caller_address: &str,
    operation: XdrOperation,
    fee: u32,
) -> Result<soroban_client::soroban_rpc::SimulateTransactionResponse> {
    let mut source_account = server.get_account(caller_address).await?;

    let transaction: Transaction =
        TransactionBuilder::new(&mut source_account, network_passphrase, None)
            .fee(fee)
            .add_operation(operation)
            .build();

    server
        .simulate_transaction(&transaction, None)
        .await
        .map_err(|e| SorobanUtilsError::Simulation(e.to_string()))
}
