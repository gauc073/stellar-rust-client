use super::Signer;
use crate::error::Result;
use soroban_client::keypair::{Keypair, KeypairBehavior};
use soroban_client::transaction::{Transaction, TransactionBehavior};

/// Signs with a plain `Keypair` held in memory.
///
/// Direct port of the non-Fireblocks branch of the TS constructor
/// (`this.deployer = Keypair.fromSecret(...)`) and the
/// `preparedTransaction.sign(this.deployer)` call sites.
pub struct LocalSigner {
    keypair: Keypair,
    // Keypair::public_key() returns an owned String, so we cache it once at
    // construction to satisfy the `&str`-returning Signer::public_key.
    public_key: String,
}

impl LocalSigner {
    pub fn from_secret(secret: &str) -> Result<Self> {
        let keypair = Keypair::from_secret(secret)
            .map_err(|e| crate::error::SorobanUtilsError::Signer(e.to_string()))?;
        let public_key = keypair.public_key();
        Ok(Self {
            keypair,
            public_key,
        })
    }

    pub fn random() -> Result<Self> {
        let keypair = Keypair::random()
            .map_err(|e| crate::error::SorobanUtilsError::Signer(e.to_string()))?;
        let public_key = keypair.public_key();
        Ok(Self {
            keypair,
            public_key,
        })
    }
}

#[async_trait::async_trait]
impl Signer for LocalSigner {
    fn public_key(&self) -> &str {
        &self.public_key
    }

    async fn sign(&self, mut tx: Transaction, _network_passphrase: &str) -> Result<Transaction> {
        tx.sign(&[self.keypair.clone()]);
        Ok(tx)
    }
}
