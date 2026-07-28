use crate::config::PollConfig;
use crate::error::{Result, SorobanUtilsError};
use crate::signer::Signer;
use crate::txbuilder::{build_and_simulate, prepare_and_send};
use soroban_client::Server;
use soroban_client::operation::Operation;
use soroban_client::transaction::ScVal;

const DEFAULT_FEE: u32 = 100;

/// Invoke a state-changing contract function and wait for confirmation.
///
/// Port of `invokeContract`. Like the TS version, this simulates first and
/// skips the on-chain send if the simulation reports no state change
/// (`simulateResponse.stateChanges == undefined` in the TS code) -- avoids
/// paying for a submission that wouldn't do anything.
pub async fn invoke_contract(
    server: &Server,
    network_passphrase: &str,
    signer: &dyn Signer,
    contract_address: &str,
    function_name: &str,
    args: Vec<ScVal>,
    poll_cfg: PollConfig,
) -> Result<()> {
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

    match simulation.error {
        Some(err) => {
            println!("Error in Simulation {:?}", err);
            return Ok(());
        }
        None => {}
    }
    if simulation.to_state_changes().is_empty() {
        println!("Simulation complete, No State Changes, Skipping execution.");
        return Ok(());
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

    Ok(())
}
