mod local;
pub use local::LocalSigner;

use crate::error::{Result, SorobanUtilsError};
use soroban_client::transaction::Transaction;

/// Anything that can produce a signature for a prepared (simulated +
/// assembled) transaction.
///
/// `LocalSigner` (this module) covers the plain-keypair case. A
/// custodial/remote signer (Fireblocks or similar) should be implemented as
/// a separate type -- possibly in a separate crate that holds the vendor SDK
/// and API credentials -- and just needs to satisfy this trait to plug into
/// `Client`. See `inject_signature` below for the piece a remote signer
/// needs: turning a raw signature blob back into a `DecoratedSignature` on
/// the transaction, same as `sendTransactionViaFireblocks` does in the TS
/// source.
#[async_trait::async_trait]
pub trait Signer: Send + Sync {
    /// The G... (or M...) address this signer signs on behalf of.
    fn public_key(&self) -> &str;

    /// Sign `tx` and return it ready to hand to `Server::send_transaction`.
    async fn sign(&self, tx: Transaction, network_passphrase: &str) -> Result<Transaction>;
}

/// Error type specific to signer implementations, wrapped into
/// `SorobanUtilsError::Signer` at the trait boundary.
#[derive(Debug, thiserror::Error)]
pub enum SignerError {
    #[error("{0}")]
    Message(String),
}

impl From<SignerError> for SorobanUtilsError {
    fn from(e: SignerError) -> Self {
        SorobanUtilsError::Signer(e.to_string())
    }
}
