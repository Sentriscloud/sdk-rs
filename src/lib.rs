//! Official Rust SDK for Sentrix Chain.
//!
//! Surface (mirrors `@sentrix/chain` on the TypeScript side):
//!
//! - [`network`] — chain spec types + mainnet/testnet constants. Always
//!   compiled in; zero runtime deps.
//! - [`native`] — typed REST client for the Sentrix-shaped endpoints
//!   (`/chain/info`, `/staking/validators`, `/epoch/current`, …). Behind
//!   the `native` feature (default). Uses `reqwest` + `tokio`.
//! - [`wallet`] — secp256k1 keypair + Ethereum-style address derivation
//!   + Sentrix-native tx signing. Behind the `wallet` feature.
//! - `evm` (planned) — alloy-based EVM client.
//! - `grpc` (planned) — tonic client over the chain's `sentrix.v1.Sentrix`
//!   service.
//!
//! Status: `0.1.0-alpha.0`. Network spec + native REST are usable;
//! wallet signing scaffolded; EVM and gRPC are doors-only stubs.

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod network;

#[cfg(feature = "native")]
pub mod native;

#[cfg(feature = "wallet")]
pub mod wallet;

// Re-export the most-used types at the crate root for ergonomic use.
pub use network::{get_spec, sentrix_mainnet, sentrix_testnet, Network, SentrixChainSpec};

#[cfg(feature = "native")]
pub use native::NativeClient;

#[cfg(feature = "wallet")]
pub use wallet::SentrixWallet;
