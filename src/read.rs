use crate::error::{Result, SorobanUtilsError};
use crate::fee::MIN_BASE_FEE;
use crate::txbuilder::build_and_simulate;
use soroban_client::Server;
use soroban_client::operation::Operation;
use soroban_client::transaction::ScVal;

/// Simulate a read-only contract call and return the raw `ScVal` result.
///
/// Port of `readContract`. Never sends a transaction on-chain -- simulation
/// only, same as the TS version's use of `simulateResponse.result.retval`.
/// Native-type conversion (the TS `scValToNative` step) is deliberately left
/// to the caller: XDR -> native mapping is application-specific once you get
/// past primitives, and baking a generic converter into this crate would
/// just be a worse copy of `stellar-xdr`/`soroban-sdk`'s own conversions.
pub async fn read_contract(
    server: &Server,
    network_passphrase: &str,
    caller_address: &str,
    contract_address: &str,
    function_name: &str,
    args: Vec<ScVal>,
) -> Result<ScVal> {
    let op = Operation::new()
        .invoke_contract(contract_address, function_name, args, None)
        .map_err(|e| SorobanUtilsError::Xdr(format!("{:?}", e)))?;

    let simulation =
        build_and_simulate(server, network_passphrase, caller_address, op, MIN_BASE_FEE).await?;

    simulation
        .to_result()
        .map(|r| r.0)
        .ok_or(SorobanUtilsError::MissingReturnValue)
}
