use crate::config::PollConfig;
use crate::error::{Result, SorobanUtilsError};
use crate::signer::Signer;
use crate::txbuilder::{build_and_simulate, prepare_and_send};
use soroban_client::Server;
use soroban_client::operation::Operation;
use soroban_client::transaction::ScVal;

const DEFAULT_FEE: u32 = 100;

/// Outcome of a call to `invoke_contract` that did *not* fail outright.
///
/// A no-op invoke (simulation reports no state change) is not necessarily a
/// bug -- e.g. an idempotent "set role" call that's already in the desired
/// state -- so it's surfaced as `Ok(InvokeOutcome::SkippedNoStateChange(..))`
/// rather than silently returning `Ok(())` the way an earlier version of
/// this function did. A simulation *error*, on the other hand, always means
/// something is actually wrong (bad args, auth failure, contract panic,
/// etc.) and is now propagated as `Err`, not swallowed.
#[derive(Debug, Clone)]
pub enum InvokeOutcome {
    /// Transaction was submitted and confirmed on-chain.
    Executed,
    /// Simulation reported no state change; nothing was submitted. The
    /// message is human-readable context for logs/callers, not a full
    /// simulation dump.
    SkippedNoStateChange(String),
}

/// Invoke a state-changing contract function and wait for confirmation.
///
/// Port of `invokeContract`. Like the TS version, this simulates first and
/// skips the on-chain send if the simulation reports no state change
/// (`simulateResponse.stateChanges == undefined` in the TS code) -- avoids
/// paying for a submission that wouldn't do anything. Unlike the earlier
/// version of this function, a simulation *error* is now returned as `Err`
/// instead of being logged and treated as success -- matching the TS
/// version, which throws on `rpc.Api.isSimulationError(simulateResponse)`.
pub async fn invoke_contract(
    server: &Server,
    network_passphrase: &str,
    signer: &dyn Signer,
    contract_address: &str,
    function_name: &str,
    args: Vec<ScVal>,
    poll_cfg: PollConfig,
) -> Result<InvokeOutcome> {
    let op = Operation::new()
        .invoke_contract(contract_address, function_name, args, None)
        .map_err(|e| SorobanUtilsError::Xdr(format!("{:?}", e)))?;

    // Simulate first so we can bail out early if there's no state change,
    // same short-circuit the TS version does.
    let simulation = build_and_simulate(
        server,
        network_passphrase,
        signer.public_key(),
        op.clone(),
        DEFAULT_FEE,
    )
    .await?;

    if let Some(err) = simulation.error {
        return Err(SorobanUtilsError::Simulation(format!(
            "invoke_contract({function_name}) on {contract_address}: {err:?}"
        )));
    }

    if simulation.to_state_changes().is_empty() {
        let message = format!(
            "invoke_contract({function_name}) on {contract_address}: simulation reported no state changes, transaction not submitted"
        );
        return Ok(InvokeOutcome::SkippedNoStateChange(message));
    }

    prepare_and_send(
        server,
        network_passphrase,
        signer.public_key(),
        op,
        DEFAULT_FEE,
        signer,
        poll_cfg,
    )
    .await?;

    Ok(InvokeOutcome::Executed)
}
