//! Extend the time-to-live (TTL) of a Soroban contract's storage entries.
//!
//! Soroban entries expire (get archived) after their TTL runs out; extending
//! it is a distinct operation from `invoke_contract` -- `ExtendFootprintTTL`
//! -- and, unlike every other operation this crate builds, its footprint
//! can't be derived from simulating a contract call (there's no function
//! being invoked, so nothing for the simulator to trace). The caller has to
//! hand the RPC the exact ledger key(s) to extend, up front, via
//! `SorobanTransactionData.resources.footprint.read_only`. That's the one
//! real wrinkle in this module versus `invoke.rs`/`read.rs`: everything else
//! (build -> prepare -> sign -> send -> poll) is the same shape.
//!
//! The storage-key convention assumed here -- `ScVal::Vec([Symbol(mapping),
//! ...details])` -- is what `#[contracttype] enum DataKey { Balance(Address)
//! }`-style keys serialize to in the Soroban Rust SDK (the enum's variant
//! name becomes the leading symbol, its payload becomes the trailing
//! elements). A key that isn't a compound enum variant -- a single top-level
//! symbol, e.g. a fixed-key config entry -- should go through `extend_ttl`
//! directly with a bare `ScVal::Symbol`.
//!
//! ## Instance storage and Wasm code -- different ledger keys entirely
//!
//! `extend_ttl` (and `mapping_entry_key`) is specifically for entries
//! written via `env.storage().persistent()/.temporary()` -- each such entry
//! is its own `LedgerKey::ContractData` with its own independent TTL, which
//! is why it needs a `storage_key` + `durability` to identify.
//!
//! Two other kinds of TTL don't fit that shape at all, and get their own
//! functions:
//!
//! - **Instance storage** (`env.storage().instance()`) isn't one entry per
//!   key -- every value set via `.instance()` lives inside a *single*
//!   `LedgerKey::ContractData` entry (the contract instance itself, keyed by
//!   the sentinel `ScVal::LedgerKeyContractInstance`, always `Persistent`).
//!   That's exactly the "all will have the same TTL" instinct: there's
//!   architecturally only one TTL to extend, because there's only one
//!   ledger entry, no matter how many keys you've `.instance().set()`. Use
//!   `extend_instance_ttl` -- no `storage_key`/`durability` needed, since
//!   both are fixed.
//! - **Wasm code** is a *third*, separate ledger entry per unique wasm hash
//!   (`LedgerKey::ContractCode`, not `ContractData` at all -- no contract
//!   address or durability concept, just the hash). Multiple contract
//!   instances that share the same uploaded wasm (e.g. deployed from the
//!   same `wasm_hash` via `create_contract_instance`) share this one code
//!   entry and its one TTL. Use `extend_wasm_ttl` with the 32-byte wasm
//!   hash `upload_wasm` returned.
//!
//! So a contract can have up to three independently-expiring TTL surfaces:
//! the wasm code entry (shared across every instance of that wasm), the one
//! instance-storage entry (shared across every `.instance()` key on that
//! specific contract), and however many persistent/temporary `ContractData`
//! entries the contract has written via `.persistent()`/`.temporary()` (each
//! with its own independent TTL). All three funnel through the same
//! `extend_ledger_key_ttl` core -- they just differ in which `LedgerKey`
//! variant gets built.

use crate::config::PollConfig;
use crate::error::{Result, SorobanUtilsError};
use crate::invoke::InvokeOutcome;
use crate::signer::Signer;
use crate::utils;
use soroban_client::Server;
use soroban_client::address::{Address, AddressTrait};
use soroban_client::error::Error as RpcError;
use soroban_client::operation::Operation;
use soroban_client::transaction::{
    Transaction, TransactionBehavior, TransactionBuilder, TransactionBuilderBehavior,
};
use soroban_client::xdr::{
    ContractDataDurability, Hash, LedgerFootprint, LedgerKey, LedgerKeyContractCode,
    LedgerKeyContractData, SorobanResources, SorobanTransactionData, SorobanTransactionDataExt,
};

pub use soroban_client::transaction::ScVal;
pub use soroban_client::xdr::ContractDataDurability as Durability;

/// The sentinel key for a contract's instance-storage entry -- every value
/// set via `env.storage().instance()` lives in the one `ContractData` entry
/// keyed by this, not a per-key entry like `.persistent()`/`.temporary()`.
pub fn instance_key() -> ScVal {
    ScVal::LedgerKeyContractInstance
}

/// Build the storage key for a `#[contracttype] enum DataKey { Mapping(K1,
/// K2, ...) }`-style compound entry: `ScVal::Vec([Symbol(mapping_name),
/// ...details])`. `details` may be empty, in which case this is just the
/// bare symbol key (equivalent to a unit enum variant).
pub fn mapping_entry_key(mapping_name: &str, details: Vec<ScVal>) -> Result<ScVal> {
    let mut elements = vec![utils::symbol(mapping_name)?];
    elements.extend(details);
    utils::vec_val(elements)
}

/// Extend the TTL of a single contract storage entry so it stays live at
/// least until ledger `extend_to`.
///
/// If the entry has already expired and been archived, `ExtendFootprintTTL`
/// simulation reports `RestorationRequired` instead of extending anything --
/// archived entries have to be restored (`RestoreFootprint`) before their TTL
/// can be pushed forward. This function handles that transparently: on
/// `RestorationRequired`, it submits the restore transaction (using the
/// exact preamble the simulation handed back), waits for it to confirm, then
/// retries the extend. Callers never see the distinction -- an archived
/// entry costs one extra on-chain transaction and a bit more latency, not a
/// different error to handle.
///
/// `storage_key` is the raw `ScVal` the contract stored the value under --
/// use `mapping_entry_key` to build the common "map/enum key" shape, or pass
/// a bare `ScVal` (e.g. `utils::symbol("Config")`) for a single fixed-key
/// entry. `durability` must match how the contract wrote the entry
/// (`Persistent` for `env.storage().persistent()`, `Temporary` for
/// `env.storage().temporary()`) -- extending the wrong durability's footprint
/// entry will fail simulation, since that ledger key won't exist. Note only
/// `Persistent` entries can be restored -- an expired `Temporary` entry is
/// gone for good, and this will surface that as a plain simulation error
/// rather than attempting (and failing) a restore.
///
/// Note the protocol restriction called out in the underlying operation's
/// docs: a Soroban transaction may only contain *one* operation, so this
/// submits its own transaction for the extend (and, when needed, a second
/// one for the restore) rather than batching with an invoke.
pub async fn extend_ttl(
    server: &Server,
    network_passphrase: &str,
    signer: &dyn Signer,
    contract_address: &str,
    storage_key: ScVal,
    durability: ContractDataDurability,
    extend_to: u32,
    poll_cfg: PollConfig,
) -> Result<InvokeOutcome> {
    let contract = Address::new(contract_address)
        .map_err(|e| SorobanUtilsError::InvalidAddress(format!("{contract_address}: {e}")))?
        .to_sc_address()
        .map_err(|e| SorobanUtilsError::Xdr(format!("failed to encode contract address: {e:?}")))?;

    let ledger_key = LedgerKey::ContractData(LedgerKeyContractData {
        contract,
        key: storage_key,
        durability,
    });

    extend_ledger_key_ttl(
        server,
        network_passphrase,
        signer,
        ledger_key,
        extend_to,
        poll_cfg,
    )
    .await
}

/// Extend the TTL of a contract's **instance storage** -- the single ledger
/// entry backing every `env.storage().instance()` key on that contract (see
/// the module docs above for why this is one TTL, not one per key). Always
/// `Persistent`, so unlike `extend_ttl` there's no `durability` to pass.
pub async fn extend_instance_ttl(
    server: &Server,
    network_passphrase: &str,
    signer: &dyn Signer,
    contract_address: &str,
    extend_to: u32,
    poll_cfg: PollConfig,
) -> Result<InvokeOutcome> {
    let contract = Address::new(contract_address)
        .map_err(|e| SorobanUtilsError::InvalidAddress(format!("{contract_address}: {e}")))?
        .to_sc_address()
        .map_err(|e| SorobanUtilsError::Xdr(format!("failed to encode contract address: {e:?}")))?;

    let ledger_key = LedgerKey::ContractData(LedgerKeyContractData {
        contract,
        key: instance_key(),
        durability: ContractDataDurability::Persistent,
    });

    extend_ledger_key_ttl(
        server,
        network_passphrase,
        signer,
        ledger_key,
        extend_to,
        poll_cfg,
    )
    .await
}

/// Extend the TTL of an uploaded Wasm **code** entry -- a separate ledger
/// entry from any contract instance, keyed by the 32-byte wasm hash
/// `Client::upload_wasm`/`upload_wasm` returned. Every contract instance
/// deployed from the same wasm shares this one entry and its one TTL, so
/// this takes the hash directly rather than a contract address.
pub async fn extend_wasm_ttl(
    server: &Server,
    network_passphrase: &str,
    signer: &dyn Signer,
    wasm_hash: [u8; 32],
    extend_to: u32,
    poll_cfg: PollConfig,
) -> Result<InvokeOutcome> {
    let ledger_key = LedgerKey::ContractCode(LedgerKeyContractCode {
        hash: Hash(wasm_hash),
    });

    extend_ledger_key_ttl(
        server,
        network_passphrase,
        signer,
        ledger_key,
        extend_to,
        poll_cfg,
    )
    .await
}

/// Shared core behind `extend_ttl`, `extend_instance_ttl`, and
/// `extend_wasm_ttl` -- everything past "here is the one `LedgerKey` to
/// extend" is identical regardless of which of the three ledger-entry kinds
/// it is (`ContractData` for a persistent/temporary entry or the instance,
/// `ContractCode` for wasm).
async fn extend_ledger_key_ttl(
    server: &Server,
    network_passphrase: &str,
    signer: &dyn Signer,
    ledger_key: LedgerKey,
    extend_to: u32,
    poll_cfg: PollConfig,
) -> Result<InvokeOutcome> {
    let extend_soroban_data = extend_footprint_data(ledger_key)?;

    // Fresh account fetch immediately before every build that might
    // actually get submitted -- a failed `prepare_transaction` call never
    // touches the chain, so re-fetching here (rather than reusing one
    // `Account` across the restore-then-retry path) keeps the local
    // sequence number honest even if the first attempt below fails.
    let mut source_account = server.get_account(signer.public_key()).await?;
    let transaction = build_transaction(
        &mut source_account,
        network_passphrase,
        Operation::new()
            .extend_footprint_ttl(extend_to)
            .map_err(|e| SorobanUtilsError::Xdr(format!("{e:?}")))?,
        crate::fee::MIN_BASE_FEE,
        extend_soroban_data.clone(),
    );

    match server.prepare_transaction(&transaction).await {
        Ok(prepared) => submit(server, signer, network_passphrase, prepared, poll_cfg).await,

        Err(RpcError::RestorationRequired(resource_fee, restore_data)) => {
            restore_entry(
                server,
                network_passphrase,
                signer,
                resource_fee,
                restore_data,
                poll_cfg,
            )
            .await?;

            // Restored -- rebuild the extend tx against a fresh account
            // (the restore just consumed a sequence number) and retry.
            // This second attempt is not itself wrapped in another
            // restore-retry: if it still needs restoring, something more
            // is wrong than "the entry was archived" and that should
            // surface as a real error rather than looping.
            let mut source_account = server.get_account(signer.public_key()).await?;
            let transaction = build_transaction(
                &mut source_account,
                network_passphrase,
                Operation::new()
                    .extend_footprint_ttl(extend_to)
                    .map_err(|e| SorobanUtilsError::Xdr(format!("{e:?}")))?,
                crate::fee::MIN_BASE_FEE,
                extend_soroban_data,
            );
            let prepared = server
                .prepare_transaction(&transaction)
                .await
                .map_err(|e| SorobanUtilsError::Simulation(e.to_string()))?;
            submit(server, signer, network_passphrase, prepared, poll_cfg).await
        }

        Err(e) => Err(SorobanUtilsError::Simulation(e.to_string())),
    }
}

/// Submit a transaction that's already been simulated + assembled by
/// `Server::prepare_transaction`, and wait for confirmation. Shared by the
/// extend and restore legs of `extend_ttl` -- both end the same way.
async fn submit(
    server: &Server,
    signer: &dyn Signer,
    network_passphrase: &str,
    prepared: Transaction,
    poll_cfg: PollConfig,
) -> Result<InvokeOutcome> {
    let signed = signer.sign(prepared, network_passphrase).await?;
    let hash = hex::encode(<Transaction as TransactionBehavior>::hash(&signed));

    server.send_transaction(signed).await?;

    // Errors (including on-chain failure) already surfaced as `Err` by
    // `poll_transaction_status`; neither leg here has a meaningful return
    // value to report beyond "it landed".
    crate::poll::poll_transaction_status(server, &hash, poll_cfg).await?;

    Ok(InvokeOutcome::Executed { tx_hash: hash })
}

/// Submit the `RestoreFootprint` transaction for an archived entry, using
/// the exact `(resource_fee, SorobanTransactionData)` preamble RPC handed
/// back in `Error::RestorationRequired`. That preamble is already the final,
/// simulated resource footprint for the restore -- no separate
/// `prepare_transaction` round trip needed, just sign and send.
async fn restore_entry(
    server: &Server,
    network_passphrase: &str,
    signer: &dyn Signer,
    resource_fee: i64,
    restore_data: SorobanTransactionData,
    poll_cfg: PollConfig,
) -> Result<()> {
    let restore_op = Operation::new()
        .restore_footprint()
        .map_err(|e| SorobanUtilsError::Xdr(format!("{e:?}")))?;

    // Inclusion fee (bids for a ledger slot) plus the resource fee the
    // preamble says restoring will cost -- the same two components
    // `prepare_transaction` would combine into `transaction.fee` itself,
    // done by hand here since we're deliberately skipping that call for
    // this leg.
    let resource_fee_u32 = u32::try_from(resource_fee.max(0)).unwrap_or(u32::MAX);
    let total_fee = crate::fee::MIN_BASE_FEE.saturating_add(resource_fee_u32);

    let mut source_account = server.get_account(signer.public_key()).await?;
    let transaction = build_transaction(
        &mut source_account,
        network_passphrase,
        restore_op,
        total_fee,
        restore_data,
    );

    submit(server, signer, network_passphrase, transaction, poll_cfg).await?;
    Ok(())
}

/// `SorobanTransactionData` for extending exactly one ledger key, with
/// resources/fee left at zero -- `prepare_transaction` fills in the real
/// instruction count, read bytes, and resource fee from the footprint we
/// supplied here. Only the footprint itself has to be correct going in.
fn extend_footprint_data(ledger_key: LedgerKey) -> Result<SorobanTransactionData> {
    Ok(SorobanTransactionData {
        ext: SorobanTransactionDataExt::V0,
        resources: SorobanResources {
            footprint: LedgerFootprint {
                read_only: vec![ledger_key]
                    .try_into()
                    .map_err(|e| SorobanUtilsError::Xdr(format!("{e:?}")))?,
                read_write: Vec::new()
                    .try_into()
                    .map_err(|e| SorobanUtilsError::Xdr(format!("{e:?}")))?,
            },
            instructions: 0,
            disk_read_bytes: 0,
            write_bytes: 0,
        },
        resource_fee: 0,
    })
}

fn build_transaction(
    source_account: &mut soroban_client::transaction::Account,
    network_passphrase: &str,
    op: soroban_client::xdr::Operation,
    fee: u32,
    soroban_data: SorobanTransactionData,
) -> Transaction {
    let mut builder = TransactionBuilder::new(source_account, network_passphrase, None);
    builder
        .fee(fee)
        .add_operation(op)
        .set_soroban_data(soroban_data);
    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `mapping_entry_key` must encode to exactly what a Rust-SDK
    /// `#[contracttype] enum DataKey { Balance(Address) }` produces on the
    /// contract side: `Vec[Symbol("Balance"), Address(...)]`. Get this
    /// wrong and `extend_ttl` builds a footprint for a ledger key that
    /// doesn't exist -- simulation fails, but only against live RPC, so
    /// this is worth pinning down as a fast, no-network unit test rather
    /// than only ever catching it in the `#[ignore]`d integration test.
    #[test]
    fn compound_key_matches_symbol_plus_details_shape() {
        let address = "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJPH7YE22RBTIALX7YZOAAWX2VZUZ";
        let details = vec![utils::address(address).expect("valid test address")];

        let key = mapping_entry_key("Balance", details.clone()).expect("key should build");

        let ScVal::Vec(Some(elements)) = &key else {
            panic!("expected ScVal::Vec, got {key:?}");
        };
        assert_eq!(elements.0.len(), 1 + details.len());
        assert_eq!(
            elements.0[0],
            utils::symbol("Balance").expect("valid symbol")
        );
        assert_eq!(elements.0[1], details[0]);
    }

    /// A unit-variant-style key (no payload, e.g. `DataKey::Config`) should
    /// collapse to a bare one-element vec, not error out on empty details.
    #[test]
    fn empty_details_yields_bare_symbol_vec() {
        let key = mapping_entry_key("Config", vec![]).expect("key should build");

        let ScVal::Vec(Some(elements)) = &key else {
            panic!("expected ScVal::Vec, got {key:?}");
        };
        assert_eq!(elements.0.len(), 1);
        assert_eq!(
            elements.0[0],
            utils::symbol("Config").expect("valid symbol")
        );
    }

    /// `instance_key` must stay the sentinel unit variant, not something
    /// that looks like it (e.g. accidentally wrapping it in a `Vec` the way
    /// `mapping_entry_key` does) -- get this wrong and `extend_instance_ttl`
    /// builds a footprint for a `ContractData` entry that doesn't exist.
    #[test]
    fn instance_key_is_the_sentinel_variant() {
        assert_eq!(instance_key(), ScVal::LedgerKeyContractInstance);
    }

    /// Symbols are protocol-limited to 32 characters -- confirm the length
    /// check surfaces as an `Err` here rather than panicking deep inside
    /// XDR encoding once this hits `extend_ttl`.
    #[test]
    fn mapping_name_over_32_chars_is_rejected() {
        let too_long = "a".repeat(33);
        let result = mapping_entry_key(&too_long, vec![]);
        assert!(
            result.is_err(),
            "expected an error for an over-length symbol"
        );
    }
}
