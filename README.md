# sentrix-chain (sdk-rs)

[![CI](https://github.com/Sentriscloud/sdk-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/Sentriscloud/sdk-rs/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![crates.io](https://img.shields.io/crates/v/sentrix-chain.svg)](https://crates.io/crates/sentrix-chain)

Official Rust SDK for **Sentrix Chain** (chain ID `7119` mainnet, `7120` testnet).

Mirror of [`@sentrix/chain`](https://github.com/Sentriscloud/sdk-ts) on the TypeScript side: same network spec, same canonical addresses, same tx signing semantics. Use this crate for Rust services, indexers, bridges, and monitoring agents that need to talk to Sentrix without spinning up a Node process.

The `0.1.x` line is alpha. APIs are intended for integration testing and early developer use, but may still change before 1.0.

## Surface

| Module | Feature flag | Status | What it does |
|---|---|---|---|
| `network` | _always on_ | alpha, low churn | Chain spec types + `MAINNET_SPEC` / `TESTNET_SPEC` constants. Single source of truth for chain ID, RPC / REST / WS / gRPC URLs, explorer, faucet. |
| `native` | `native` (default) | alpha | Typed REST client over `reqwest` for `/chain/info`, `/staking/validators`, `/accounts/<addr>/nonce`, `POST /transactions`. |
| `wallet` | `wallet` | alpha | secp256k1 keypair + Ethereum-style address derivation + native tx signing. Applications remain responsible for secret storage. |
| `evm` | `evm` | alpha | alloy-based EVM HTTP provider factory using Sentrix mainnet/testnet RPC config. Reach for alloy directly for signing / contract bindings / event filters. |
| `grpc` | `grpc` | alpha | tonic client over `sentrix.v1.Sentrix` — getBlock / getBalance / getValidatorSet / getSupply / getMempool / streamEvents. Proto types come from the published [`sentrix-proto`](https://crates.io/crates/sentrix-proto) crate. Consumers building from source may need `protoc` installed (`apt install protobuf-compiler` or equivalent). |
| `bft` | `bft` | alpha | WebSocket subscription manager for EVM and Sentrix-specific subscription channels over tokio-tungstenite. Runtime behavior depends on the configured WS endpoint. |

Trim what you actually use:

```toml
[dependencies]
sentrix-chain = { version = "0.1.0-alpha.1", default-features = false, features = ["native", "wallet"] }
```

## Quick start

## Unit warning

Native REST/native ledger amounts use **sentri**, the 8-decimal SRX
unit: `1 SRX = 100_000_000 sentri`.

EVM JSON-RPC uses **wei-style 18-decimal units** for Ethereum tooling
compatibility. Do not mix native and EVM amounts directly; convert
explicitly at API boundaries.

## Examples

Run examples against mainnet by default, or set `SENTRIX_NETWORK=testnet`.

```bash
cargo run --example chain_info
cargo run --no-default-features --features evm --example evm_block_number
cargo run --no-default-features --features grpc --example grpc_latest_block
cargo run --no-default-features --features bft --example websocket_subscribe
cargo run --no-default-features --features wallet --example sign_native_transfer
```

`sign_native_transfer` reads secrets from the environment and only
prints the signed transaction envelope; it does not broadcast.

A native balance example is not included in this alpha because
`NativeClient` does not yet expose a documented native balance endpoint.
Use the `grpc` surface for balance reads when that endpoint is available
for your deployment.

### Read chain stats

```rust
use sentrix_chain::{Network, NativeClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = NativeClient::new(Network::Mainnet);
    let info = client.chain_info().await?;
    println!(
        "height={} validators={} mempool={} burned={}",
        info.height, info.active_validators, info.mempool_size, info.total_burned_srx
    );

    for v in client.validators().await? {
        println!("{} active={} self_stake={}", v.address, v.is_active, v.self_stake);
    }
    Ok(())
}
```

### Sign + broadcast a native transfer

```rust
use sentrix_chain::{Network, NativeClient, SentrixWallet};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let w = SentrixWallet::from_private_key_hex(&std::env::var("PRIVATE_KEY")?)?;
    let client = NativeClient::new(Network::Mainnet);

    let nonce = client.next_nonce(&w.address).await?;
    let tx = w.build_and_sign_transfer(
        "0x0804a00f53fde72d46abd1db7ee3e97cbfd0a107",
        100_000_000, // 1 SRX in sentri
        10_000,      // 0.0001 SRX fee
        nonce,
        7119,
    )?;
    let txid = client.broadcast(&tx).await?;
    println!("broadcast: {}", txid);
    Ok(())
}
```

### EVM read via alloy

```rust
use sentrix_chain::{Network, evm};
use alloy::providers::Provider;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let p = evm::http_provider(Network::Mainnet)?;
    let block = p.get_block_number().await?;
    println!("tip: {block}");
    Ok(())
}
```

Reach for `alloy` directly for tx signing, contract bindings, event filters — `http_provider()` returns alloy's standard `RootProvider` so the rest of the alloy ecosystem works unchanged.

### gRPC read

```rust
use sentrix_chain::{Network, grpc::SentrixGrpcClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut c = SentrixGrpcClient::connect(Network::Mainnet).await?;
    let block = c.get_latest_block().await?;
    println!("tip: {} ({} txs)", block.index, block.transactions.len());

    // Server-stream chain events
    let mut stream = c.subscribe_events(vec![]).await?;
    while let Some(ev) = stream.message().await? {
        println!("event: {ev:?}");
    }
    Ok(())
}
```

`connect()` resolves the public TLS endpoint (`grpc.sentrixchain.com:443`) and attaches system-roots automatically. For dev sidecars use `connect_url("http://localhost:50051")`.

### Subscribe via WebSocket (BFT door)

```rust
use sentrix_chain::{Network, bft::{SubscriptionManager, Channel}};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mgr = SubscriptionManager::new(Network::Mainnet);

    let mut heads = mgr.subscribe(Channel::NewHeads).await?;
    let mut jail = mgr.subscribe(Channel::SentrixJail).await?;

    tokio::select! {
        Some(h) = heads.recv() => println!("new head: {h}"),
        Some(j) = jail.recv() => println!("jail event: {j}"),
    }
    Ok(())
}
```

Same socket carries both subscriptions; reconnects automatically with exponential backoff and re-subscribes everything. Pings every 30 s so middleboxes (Caddy `idle_timeout`, NAT, AWS ALB) don't drop quiet connections; if no frame arrives within 90 s the manager force-closes + reconnects (half-open guard).

### Network spec — const-accessible

```rust
use sentrix_chain::network::{Network, get_spec};

const MAINNET: sentrix_chain::SentrixChainSpec = get_spec(Network::Mainnet);
println!("{}: {}", MAINNET.name, MAINNET.rpc_url);
// "Sentrix Chain: https://rpc.sentrixchain.com"
```

## Status

`v0.1.0-alpha.1` is the release candidate prepared for feature-flag hardening, examples, docs, and publish readiness. All six surfaces (`network`, `native`, `wallet`, `evm`, `grpc`, `bft`) are intended to compile behind their feature flags. Live endpoint compatibility is still alpha, so expect breaking changes before 1.0 stabilises.

## Roadmap

All six surfaces are present in v0.1.0-alpha.1:

- [x] `network` — chain spec, mainnet + testnet constants
- [x] `native` — REST read + tx broadcast
- [x] `wallet` — secp256k1 keypair + tx signing
- [x] `evm` — alloy-based provider (read; write via alloy direct)
- [x] `grpc` — tonic client over `sentrix.v1.Sentrix` (consumes [`sentrix-proto`](https://crates.io/crates/sentrix-proto) for the schema)
- [x] `bft` — WebSocket subscription manager (multiplex + keepalive ping + auto-reconnect, port of `@sentrix/chain/bft`)

Next: surface stabilisation toward 1.0 — naming review, error-type cleanup, native balance support if the REST endpoint is documented, and optional `EvmClient` wrappers around alloy.

## License

MIT — see [LICENSE](./LICENSE).
