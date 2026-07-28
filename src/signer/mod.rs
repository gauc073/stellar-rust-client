mod local;
pub use local::LocalSigner;

use crate::error::{Result, SorobanUtilsError};
use secrecy::{ExposeSecret, SecretString};
use soroban_client::keypair::{Keypair, KeypairBehavior};
use soroban_client::transaction::Transaction;

/// Anything that can produce a signature for a prepared (simulated +
/// assembled) transaction.
///
/// `LocalSigner` (this module) covers the plain-keypair case. A
/// custodial/remote signer (Fireblocks or similar) should be implemented as
/// a separate type -- possibly in a separate crate that holds the vendor SDK
/// and API credentials -- and just needs to satisfy this trait to plug into
/// `Client` via `SignerConfig::Custom`.
///
/// Note this trait itself doesn't dictate how long the secret material
/// behind a `Signer` lives -- that's `SignerFactory`'s job (see below).
/// `Signer` instances built via `SignerFactory::build_signer` are meant to
/// be used once and dropped.
#[async_trait::async_trait]
pub trait Signer: Send + Sync {
    /// The G... (or M...) address this signer signs on behalf of.
    fn public_key(&self) -> &str;

    /// Sign `tx` and return it ready to hand to `Server::send_transaction`.
    async fn sign(&self, tx: Transaction, network_passphrase: &str) -> Result<Transaction>;
}

/// Builds a `Signer` on demand, rather than `Client` holding one signer
/// instance for its whole lifetime.
///
/// The point of this split: `Client` stores a `SignerFactory`, not a
/// `Signer`. Every write operation (`upload_wasm`, `create_contract_instance`,
/// `invoke_contract`) calls `build_signer` immediately before signing and
/// drops the result immediately after -- so whatever secret material a
/// `Signer` implementation holds is only in memory for the duration of one
/// signing operation, not the lifetime of the whole `Client` (which, in a
/// long-lived process like a Lambda execution environment that gets frozen
/// and thawed between invocations, could otherwise be a long time).
///
/// `public_key` is separate and resolved once, at `Client` construction --
/// it's async because a custodial signer may need a network round trip to
/// resolve its address (e.g. the original TS source's
/// `getSignerAddress(fireblocksAdmin, vaultId, assetId)`), but a public key
/// isn't secret, so there's no reason to re-resolve it on every call.
#[async_trait::async_trait]
pub trait SignerFactory: Send + Sync {
    /// The G... (or M...) address this factory's signers will produce
    /// signatures for.
    async fn public_key(&self) -> Result<String>;

    /// Build a `Signer` for exactly one signing operation. Call this fresh
    /// for every transaction and let the result drop as soon as signing is
    /// done -- don't cache it.
    async fn build_signer(&self) -> Result<Box<dyn Signer>>;
}

/// How `Client` should sign transactions. One of these, picked once at
/// construction:
///
/// - `Secret`: a plain secret key, wrapped in `secrecy::SecretString` so it
///   zeroizes on drop and never prints in a `{:?}` by accident. `Client`
///   never holds a `Keypair` built from it directly -- only the
///   `SecretString`, plus a transient `Keypair` rebuilt fresh for each
///   individual sign call.
/// - `Custom`: anything else (a custodial signer like Fireblocks or Turnkey,
///   an HSM, a remote signing service). Deliberately not a named variant per
///   vendor -- implement `SignerFactory` downstream, in whatever crate holds
///   the vendor SDK and credentials, and pass it in here. Adding support for
///   a new signing backend later never requires a change to this crate.
pub enum SignerConfig {
    Secret(SecretString),
    Custom(Box<dyn SignerFactory>),
}

impl SignerConfig {
    pub(crate) fn into_factory(self) -> Result<Box<dyn SignerFactory>> {
        match self {
            SignerConfig::Secret(secret) => {
                Ok(Box::new(SecretSignerFactory::new(secret)?) as Box<dyn SignerFactory>)
            }
            SignerConfig::Custom(factory) => Ok(factory),
        }
    }
}

/// `SignerFactory` for `SignerConfig::Secret`.
///
/// Caveat worth being direct about: `secrecy::SecretString` guarantees the
/// *stored* secret is zeroized on drop and redacted in `Debug`. The
/// transient `soroban_client::Keypair` built fresh inside `build_signer`,
/// however, is a foreign type this crate doesn't control -- it doesn't
/// implement `Zeroize`, so its internal copy of the raw key bytes isn't
/// guaranteed to be scrubbed the instant it's dropped, only deallocated.
/// What this design *does* guarantee is a much smaller exposure window
/// (one sign call instead of the `Client`'s whole lifetime) and that the
/// long-lived copy (the `SecretString` on this struct) is handled properly.
/// Full memory-scrubbing of the ephemeral `Keypair` would require an
/// upstream change in `soroban-client` itself.
struct SecretSignerFactory {
    secret: SecretString,
    public_key: String,
}

impl SecretSignerFactory {
    fn new(secret: SecretString) -> Result<Self> {
        // Public keys aren't secret -- derive it once now so `public_key()`
        // doesn't need to touch the secret again later.
        let keypair = Keypair::from_secret(secret.expose_secret())
            .map_err(|e| SorobanUtilsError::Signer(e.to_string()))?;
        let public_key = keypair.public_key();
        Ok(Self { secret, public_key })
    }
}

#[async_trait::async_trait]
impl SignerFactory for SecretSignerFactory {
    async fn public_key(&self) -> Result<String> {
        Ok(self.public_key.clone())
    }

    async fn build_signer(&self) -> Result<Box<dyn Signer>> {
        let keypair = Keypair::from_secret(self.secret.expose_secret())
            .map_err(|e| SorobanUtilsError::Signer(e.to_string()))?;
        Ok(Box::new(LocalSigner::from_keypair(keypair)))
    }
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
