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
stellar-rust-client = "0.1"
```

Requires Rust 1.85+ (edition 2024, inherited from this crate's `Cargo.toml`).

## Quick start

```rust,no_run
use stellar_rust_client::{Client, LocalSigner, NetworkConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let signer = LocalSigner::from_secret("S...")?;
    let client = Client::new(NetworkConfig::testnet(), Box::new(signer))?;

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
    client
        .invoke_contract(&contract_address, "set_balance", vec![/* ScVal args */])
        .await?;

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
| `txbuilder` | The shared `build → prepare (simulate+assemble) → sign → send` and `build → simulate` helpers every other module calls into, so that sequence exists exactly once. |
| `poll` | Fixed-interval polling of `getTransaction` until it leaves `NotFound`. |
| `signer` | `Signer` trait + `LocalSigner` (plain keypair). See below. |
| `wasm` | `read_wasm_file` — sync file read. |
| `error` | `SorobanUtilsError` — one error enum for the whole crate. |
| `config` | `NetworkConfig` (testnet/futurenet/mainnet/custom) and `PollConfig` (interval + timeout). |

## Signing

`Client` takes a `Box<dyn Signer>`:

```rust
#[async_trait::async_trait]
pub trait Signer: Send + Sync {
    fn public_key(&self) -> &str;
    async fn sign(&self, tx: Transaction, network_passphrase: &str) -> Result<Transaction>;
}
```

`LocalSigner` (plain `Keypair`, from a secret key or generated at random) is the only
implementation shipped in this crate. A custodial/remote signer (Fireblocks or similar) is meant
to be implemented downstream — in whatever crate holds the vendor SDK and API credentials — and
just needs to satisfy `Signer` to plug into `Client`. Keeping that out of this crate is
deliberate: it avoids pulling custodial vendor SDKs and credentials into a crate meant to be
published and reused elsewhere.

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
    /// Transaction was submitted and confirmed on-chain.
    Executed,
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

## Known rough edges to check before relying on this in production

- Contract-address decoding (`ScVal::Address` → `C...` strkey in `deploy::create_contract_instance`)
  relies on `Address`'s `Display`/`to_string()`. You've tested read and write against live
  contracts already, so this is presumably fine — flagging only because strkey-encoding mistakes
  are the kind of bug that's easy to miss until compared against Stellar Explorer.
- Before running `cargo publish`, double check the `stellar-rust-client` name is still available
  on crates.io (didn't get a chance to verify this session) and that `cargo package --list` /
  `cargo publish --dry-run` doesn't pick up anything from `.env`, `BUILD_NOTES.md`'s reference
  test contract addresses, or other local-only files you don't want in the published tarball.

## License

MIT
