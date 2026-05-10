# sentrix-chain (sdk-rs)

[![CI](https://github.com/Sentriscloud/sdk-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/Sentriscloud/sdk-rs/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![crates.io](https://img.shields.io/crates/v/sentrix-chain.svg)](https://crates.io/crates/sentrix-chain)

Official Rust SDK for **Sentrix Chain** (chain ID `7119` mainnet, `7120` testnet).

Mirror of [`@sentrix/chain`](https://github.com/Sentriscloud/sdk-ts) on the TypeScript side — same network spec, same canonical addresses, same tx signing semantics. Use this crate for Rust services (validators, indexers, bridges, monitoring agents) that need to talk to Sentrix without spinning up a Node process.

## Surface

| Module | Feature flag | Status | What it does |
|---|---|---|---|
| `network` | _always on_ | ✅ stable | Chain spec types + `MAINNET_SPEC` / `TESTNET_SPEC` constants. Single source of truth for chain ID, RPC / REST / WS / gRPC URLs, explorer, faucet. |
| `native` | `native` (default) | ✅ alpha | Typed REST client over `reqwest` for `/chain/info`, `/staking/validators`, `/accounts/<addr>/nonce`, `POST /transactions`. |
| `wallet` | `wallet` | ✅ alpha | secp256k1 keypair + Ethereum-style address derivation + native tx signing. |
| `evm` | `evm` | ✅ alpha | alloy-based EVM JSON-RPC client (Provider factory; reach for alloy directly for signing / contract bindings / event filters). |
| `grpc` | `grpc` | 🟡 planned | tonic client over `sentrix.v1.Sentrix`. |

Trim what you actually use:

```toml
[dependencies]
sentrix-chain = { version = "0.1.0-alpha.0", default-features = false, features = ["native", "wallet"] }
```

## Quick start

### Read chain stats

```rust
use sentrix_chain::{Network, NativeClient};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
async fn main() -> anyhow::Result<()> {
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

### Network spec — const-accessible

```rust
use sentrix_chain::network::{Network, get_spec};

const MAINNET: sentrix_chain::SentrixChainSpec = get_spec(Network::Mainnet);
println!("{}: {}", MAINNET.name, MAINNET.rpc_url);
// "Sentrix Chain: https://rpc.sentrixchain.com"
```

## Status

`v0.1.0-alpha.0` — network spec + native REST + wallet signing are usable today. EVM (alloy) and gRPC (tonic) modules are doors-only stubs; track [the roadmap](#roadmap) for landing dates.

## Roadmap

- [x] `network` — chain spec, mainnet + testnet constants
- [x] `native` — REST read + tx broadcast
- [x] `wallet` — secp256k1 keypair + tx signing
- [x] `evm` — alloy-based provider (read; write via alloy direct)
- [ ] `grpc` — tonic client over `sentrix.v1.Sentrix` (`getBlock`, `getBalance`, `getValidatorSet`, `getSupply`, `getMempool`, `streamEvents`)
- [ ] `bft` — WebSocket subscription manager (port the keepalive + multiplex pattern from `@sentrix/chain/bft`)
- [ ] Published to crates.io once feature surface stabilises

## Decimals

Sentrix's underlying ledger is **8-decimal** native (1 SRX = 100,000,000 sentri). The EVM tooling sees an **18-decimal** view because `eth_getBalance` returns wei-scaled values for compatibility with MetaMask / ethers / viem. When you use `NativeClient::balance(...)` you get sentri (8-decimal); when the planned `EvmClient` ships you'll get wei (18-decimal). Don't mix the units across surfaces.

## License

MIT — see [LICENSE](./LICENSE).
