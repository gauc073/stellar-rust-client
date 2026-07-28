use thiserror::Error;

/// Single error type for every operation in this crate.
///
/// The goal is that callers never have to match on soroban-client's or
/// stellar-rpc-client's internal error enums directly; everything relevant
/// gets normalized into one of the variants below.
#[derive(Debug, Error)]
pub enum SorobanUtilsError {
    #[error("RPC call failed: {0}")]
    Rpc(String),

    #[error("transaction simulation failed: {0}")]
    Simulation(String),

    #[error("transaction {hash} failed on-chain with status {status}")]
    TransactionFailed { hash: String, status: String },

    #[error("timed out waiting for transaction {hash} to finalize")]
    Timeout { hash: String },

    #[error("signer error: {0}")]
    Signer(String),

    #[error("failed to read wasm file at {path}: {source}")]
    WasmRead {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("no return value present in transaction/simulation response")]
    MissingReturnValue,

    #[error("xdr encode/decode error: {0}")]
    Xdr(String),

    #[error("invalid address or contract id: {0}")]
    InvalidAddress(String),
}

pub type Result<T> = std::result::Result<T, SorobanUtilsError>;

// soroban_client::error::Error -> SorobanUtilsError
impl From<soroban_client::error::Error> for SorobanUtilsError {
    fn from(e: soroban_client::error::Error) -> Self {
        SorobanUtilsError::Rpc(e.to_string())
    }
}
