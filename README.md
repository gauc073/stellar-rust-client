# stellar-rust-client

Reusable Rust utilities for Soroban contract execution: upload WASM, deploy a contract instance,
invoke a state-changing function, and read a value via simulation. Ported from a working
TypeScript `MultiContractDeployer` implementation, built on top of
[`soroban-client`](https://crates.io/crates/soroban-client).

The point of this crate isn't new RPC capability — `soroban-client` already provides that. It's
the workflow layer on top: every Soroban call needs the same
`build transaction → simulate → assemble → sign → send → poll until confirmed` sequence, and this
crate centralizes that sequence once instead of re-implementing it at every call site.

## Install

```toml
[dependencies]
stellar-rust-client = "0.2.0"
```

Requires Rust 1.85+ (edition 2024, inherited from this crate's `Cargo.toml`).

## Quick start

```rust,no_run
use secrecy::SecretString;
use stellar_rust_client::{Client, InvokeOutcome, NetworkConfig, SignerConfig, utils};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let signer_config = SignerConfig::Secret(SecretString::from("S...".to_string()));
    let client = Client::new(NetworkConfig::testnet(), signer_config).await?;

    // Deploy
    let wasm = stellar_rust_client::wasm::read_wasm_file("./my_contract.wasm")?;
    let (contract_address, _wasm_hash) = client.deploy_contract(&wasm, vec![]).await?;
    println!("deployed at {contract_address}");

    // Read (simulation only, no transaction submitted)
    let result = client
        .read_contract(&contract_address, "get_balance", vec![])
        .await?;
    println!("balance: {result:?}");

    // Write (submits + polls until confirmed)
    let args = vec![utils::address("GABC...")?, utils::i128_val(1_000_000)];
    match client.invoke_contract(&contract_address, "set_balance", args).await? {
        InvokeOutcome::Executed { tx_hash } => println!("confirmed: {tx_hash}"),
        InvokeOutcome::SkippedNoStateChange(msg) => println!("no-op: {msg}"),
    }

    Ok(())
}
```

## What's in the box

| Module | Responsibility |
|---|---|
| `client` | `Client` — the main entry point: holds the RPC connection, network config, and signer. |
| `deploy` | `upload_wasm`, `create_contract_instance`, `deploy_contract` (upload + create in one call). |
| `invoke` | `invoke_contract` — simulates first and skips the on-chain send if simulation reports no state change. |
| `read` | `read_contract` — simulation-only, never submits a transaction. Returns the raw `ScVal`; native-type conversion is left to the caller. |
| `txbuilder` | The shared `build → prepare (simulate+assemble) → sign → send` and `build → simulate` helpers every other module calls into, so that sequence exists exactly once. Returns `(tx_hash, response)`. |
| `poll` | Fixed-interval polling of `getTransaction` until it leaves `NotFound`. |
| `fee` | `recommended_inclusion_fee` — asks the network's fee-stats endpoint for a realistic inclusion fee instead of always bidding the 100-stroop floor. Used automatically by `deploy` and `invoke`. |
| `utils` | `ScVal` construction/extraction helpers for common primitives (address, string, symbol, bytes, bool, i32/u32/i64/u64/i128/u128) — namespaced as `utils::` rather than re-exported at the crate root. |
| `signer` | `Signer`, `SignerFactory`, `SignerConfig`, `LocalSigner`. See below. |
| `wasm` | `read_wasm_file` — sync file read. |
| `error` | `SorobanUtilsError` — one error enum for the whole crate. |
| `config` | `NetworkConfig` (testnet/futurenet/mainnet/custom) and `PollConfig` (interval + timeout). |

## Signing

`Client::new` takes a `SignerConfig`, not a signer directly:

```rust
pub enum SignerConfig {
    /// A plain secret key, wrapped in `secrecy::SecretString`.
    Secret(secrecy::SecretString),
    /// Anything else -- a custodial signer (Fireblocks, Turnkey, an HSM, a
    /// remote signing service), implemented downstream and passed in here.
    Custom(Box<dyn SignerFactory>),
}
```

`Client` stores the resulting `Box<dyn SignerFactory>`, not a ready-to-use `Signer`. Every write
method (`upload_wasm`, `create_contract_instance`, `invoke_contract`) calls
`signer_factory.build_signer().await` immediately before signing and lets the result drop
immediately after -- so whatever secret material a `Signer` holds is only in memory for the
duration of one signing operation, not the `Client`'s whole lifetime. That matters more than it
might sound: in a long-lived process like a Lambda execution environment that gets frozen and
thawed between invocations, "whole lifetime" can be a long time.

```rust
#[async_trait::async_trait]
pub trait SignerFactory: Send + Sync {
    async fn public_key(&self) -> Result<String>;         // resolved once, at Client::new
    async fn build_signer(&self) -> Result<Box<dyn Signer>>;  // called fresh per transaction
}

#[async_trait::async_trait]
pub trait Signer: Send + Sync {
    fn public_key(&self) -> &str;
    async fn sign(&self, tx: Transaction, network_passphrase: &str) -> Result<Transaction>;
}
```

`SignerConfig::Secret` covers the plain-keypair case out of the box (backed internally by
`LocalSigner`, still exported if you want to build one directly). A custodial/remote signer is
implemented downstream, in whatever crate holds the vendor SDK and credentials, by implementing
`SignerFactory` and passing it in as `SignerConfig::Custom(...)`. Deliberately not a named variant
per vendor (no `Fireblocks` in this crate's public API) — this is a crate meant to be published
and reused broadly, and adding a new signing backend later should never require a new release of
it.

**On the security guarantee, stated plainly:** `secrecy::SecretString` guarantees the *stored*
secret zeroizes on drop and never accidentally prints via `{:?}`. The transient
`soroban_client::Keypair` rebuilt fresh inside `build_signer` for each sign call, however, is a
foreign type this crate doesn't control — it doesn't implement `Zeroize`, so its raw key bytes
aren't guaranteed to be scrubbed the instant it's dropped, only deallocated. What this design
does guarantee is a much smaller exposure window (one sign call instead of the `Client`'s whole
lifetime) and that the long-lived copy is handled properly. Full memory-scrubbing of the
ephemeral `Keypair` itself would need an upstream change in `soroban-client`.

## Polling

Fixed-interval, not exponential backoff — intentional. In custodial-signer flows the dominant
latency is signer approval time, not RPC load, so backing off doesn't help. Default interval is
2s with a 60-minute ceiling; override via `Client::with_poll_config` / `PollConfig`.

## Testing

Correctness is exercised via the integration tests in `tests/`, which run against live testnet
and are `#[ignore]`d by default:

```sh
cargo test --test read_has_role -- --ignored --nocapture
cargo test --test write_register_contract -- --ignored --nocapture
```

They read `NETWORK` and one `CONTRACT_ADDRESS_<NAME>` per contract under test from your
environment or a local `.env` (see `.env.example`). **`.env` is gitignored — never commit real
secret keys.** `tests/common/mod.rs` currently sources the signer's secret key through an
internal encrypted-export helper (`data-security` + `secrecy`, declared under
`[dev-dependencies]` so they aren't forced on downstream consumers of the published crate); swap
that block for a plain `SOURCE_SECRET` env var if you don't have that tooling available in
another repo.

## Scope

Deliberately out of scope, so this crate stays a thin execution layer rather than growing into a
deployment framework: deployment-log bookkeeping (tracking what's deployed where, the
`deployments.json` pattern), contract upgrade orchestration beyond what `invoke_contract` already
gives you, and native ScVal ⇄ Rust-type conversion helpers (use `soroban-client`'s own XDR types
for that). See `soroban-utils-design.md` for the full design rationale and the original
TypeScript-to-Rust mapping this crate was ported from.

## `invoke_contract` return value

`Client::invoke_contract` returns `Result<InvokeOutcome>`, not `Result<()>`:

```rust
pub enum InvokeOutcome {
    /// Transaction was submitted and confirmed on-chain. Carries the
    /// hex-encoded transaction hash so callers can log it or link straight
    /// to an explorer without re-deriving it.
    Executed { tx_hash: String },
    /// Simulation reported no state change; nothing was submitted. Not
    /// necessarily a bug (e.g. an idempotent call that's already in the
    /// desired state) -- inspect the message and decide per call site.
    SkippedNoStateChange(String),
}
```

A simulation *error* (bad args, auth failure, contract panic, etc.) is always an `Err`, matching
the original TS version's `throw` on `rpc.Api.isSimulationError(...)`. A no-state-change
simulation is `Ok(InvokeOutcome::SkippedNoStateChange(..))` rather than a silent `Ok(())` — match
on it explicitly if a no-op should be treated as a test/caller failure, the way
`tests/write_register_contract.rs` does.

`deploy::upload_wasm` / `create_contract_instance` / `deploy_contract` do *not* currently expose
their transaction hash the same way (they still return `[u8; 32]` / `String` / `(String, [u8;
32])` unchanged) — only `invoke_contract`'s return type changed, since that's the call site this
was requested against. Say the word if you want the hash threaded through the deploy path too;
it's the same shape of change.

## Fee estimation

`deploy::upload_wasm`, `deploy::create_contract_instance`, and `invoke::invoke_contract` now call
`fee::recommended_inclusion_fee` before submitting, instead of always bidding the 100-stroop
floor. This asks the RPC node's `getFeeStats` for the 90th-percentile fee actually being paid on
recent Soroban transactions (`soroban_inclusion_fee`, which is the fee market Soroban transactions
compete in — separate from the classic per-operation fee market). This is what was causing
transactions to be accepted by `sendTransaction` but sit in the mempool instead of confirming: the
resource fee (what the contract execution itself costs) was already being estimated correctly by
`prepare_transaction`, but the inclusion fee (the bid to get picked up by the next ledger close)
was always the bare minimum, which loses to any other traffic during congestion.

If the fee-stats call itself fails for some reason, it falls back to the 100-stroop floor rather
than failing the whole operation — worth keeping an eye on `fee::recommended_inclusion_fee`'s
result in your Lambda's logs (or call it directly) if you want visibility into which path is
being taken.

## `utils` helpers

`ScVal` construction and extraction for the common primitive types, extracted from what used to
be copy-pasted `sc_address`/`sc_string` helpers in the test files:

```rust
use stellar_rust_client::utils;

let args = vec![
    utils::address("GABC...")?,
    utils::string("some string")?,
    utils::symbol("SOME_ROLE")?,       // max 32 chars, protocol-enforced
    utils::i128_val(1_000_000_000),
    utils::u128_val(140),
    utils::boolean(true),
];

// and the reverse direction:
let addr: String = utils::to_address(&result)?;
let amount: i128 = utils::to_i128(&result)?;
```

Namespaced under `utils::` rather than re-exported at the crate root, since names like
`i128`/`u128`/`bool` would otherwise shadow Rust's own primitive types. This deliberately only
covers primitives — `Vec`/`Map`/struct-shaped `ScVal`s are still on you via `soroban_client::xdr`
directly, since a generic converter for those would just be a worse copy of what `soroban-client`
already does.

## Known rough edges to check before relying on this in production

- Contract-address decoding (`ScVal::Address` → `C...` strkey in `deploy::create_contract_instance`
  and `utils::to_address`) relies on `Address`'s `Display`/`to_string()`. You've tested read and
  write against live contracts already, so this is presumably fine — flagging only because
  strkey-encoding mistakes are the kind of bug that's easy to miss until compared against Stellar
  Explorer.
- `utils::to_bytes` / `utils::to_string_val` / `utils::to_symbol` assume the underlying XDR
  wrapper types (`BytesM`, `StringM`) support `.to_vec()` via a slice `Deref`. Wasn't able to
  compile-check this session (no Rust toolchain in this sandbox) — if `cargo build` flags it,
  the fix is a one-line `.clone().into()` swap, same pattern already proven working in
  `deploy::upload_wasm`.
- `Client::new` is now `async` and takes `SignerConfig` instead of `Box<dyn Signer>` — this is a
  breaking change from what you already integrated into your Lambda. Update that call site before
  redeploying: `SignerConfig::Secret(SecretString::from(your_secret))` replaces
  `Box::new(LocalSigner::from_secret(...)?)`, and the call becomes `.await`-ed.
- Wasn't able to compile-check the `secrecy` API surface this session either (no crates.io access
  in this sandbox) — `SecretString`/`ExposeSecret` are the right names as of `secrecy` 0.10, but
  worth a glance at that crate's changelog if `cargo build` disagrees.
- Before running `cargo publish`, double check the `stellar-rust-client` name is still available
  on crates.io (didn't get a chance to verify this session) and that `cargo package --list` /
  `cargo publish --dry-run` doesn't pick up anything from `.env`, `BUILD_NOTES.md`'s reference
  test contract addresses, or other local-only files you don't want in the published tarball.

## License

MIT
