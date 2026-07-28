use crate::config::PollConfig;
use crate::error::{Result, SorobanUtilsError};
use soroban_client::Server;
use soroban_client::soroban_rpc::{GetTransactionResponse, TransactionStatus};

/// Fixed-interval poll of `getTransaction` until it leaves `NotFound`.
///
/// Direct port of the `while (getResponse.status === NOT_FOUND ...)` loop
/// that's repeated in `uploadWasm`, `createContractInstance`, and
/// `invokeContract` in the TS source. Kept as one function so none of the
/// higher-level modules duplicate the loop.
///
/// NOTE: fixed interval, not exponential backoff -- confirmed intentional,
/// since the dominant latency in custodial-signer flows is signer approval
/// time, not RPC load.
pub async fn poll_transaction_status(
    server: &Server,
    hash: &str,
    cfg: PollConfig,
) -> Result<GetTransactionResponse> {
    let deadline = tokio::time::Instant::now() + cfg.timeout;
    let mut response = server.get_transaction(hash).await?;

    while matches!(response.status, TransactionStatus::NotFound) {
        if tokio::time::Instant::now() >= deadline {
            return Err(SorobanUtilsError::Timeout {
                hash: hash.to_string(),
            });
        }
        tokio::time::sleep(cfg.interval).await;
        response = server.get_transaction(hash).await?;
    }

    match &response.status {
        TransactionStatus::Success => Ok(response),
        other => Err(SorobanUtilsError::TransactionFailed {
            hash: hash.to_string(),
            status: format!("{other:?}"),
        }),
    }
}
