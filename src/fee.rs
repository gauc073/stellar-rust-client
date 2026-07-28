use crate::error::{Result, SorobanUtilsError};
use soroban_client::Server;

/// Stellar's protocol-defined minimum base fee, in stroops (`BASE_FEE` in the
/// JS/TS SDK). Never bid below this regardless of what the network's fee
/// stats report.
pub const MIN_BASE_FEE: u32 = 100;

/// Ask the network for a recommended inclusion fee instead of hardcoding the
/// 100-stroop minimum.
///
/// The inclusion fee (this) and the Soroban resource fee are two different
/// things: `Server::prepare_transaction` already estimates and adds the
/// resource fee from simulation, so that part was never the problem. The
/// inclusion fee is the *bidding* fee that determines whether your
/// transaction gets picked up by the next ledger close or sits in the
/// mempool behind higher bidders during congestion -- passing the bare
/// minimum here is exactly what causes transactions to be accepted
/// (`sendTransaction` returns success) but never actually confirmed.
///
/// Soroban transactions have their own fee market (`soroban_inclusion_fee`),
/// separate from classic Stellar operations (`inclusion_fee`) -- this uses
/// the Soroban one since every transaction built by this crate is a Soroban
/// `invoke_host_function`. Returns the 90th-percentile fee paid over the
/// recent ledger window: aggressive enough to clear congestion without
/// bidding the max.
///
/// Falls back to `MIN_BASE_FEE` if the network reports something lower or
/// unparsable (can happen on quiet networks like Futurenet, where the stat
/// comes back as `"0"`).
pub async fn recommended_inclusion_fee(server: &Server) -> Result<u32> {
    let stats = server.get_fee_stats().await?;

    let p90: u32 = stats
        .soroban_inclusion_fee
        .p90
        .parse()
        .map_err(|e| SorobanUtilsError::Rpc(format!("failed to parse fee stats p90: {e}")))?;

    Ok(p90.max(MIN_BASE_FEE))
}
