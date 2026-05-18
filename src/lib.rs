//! Official Rust SDK for Sentrix Chain.
//!
//! Alpha surface (mirrors `@sentrix/chain` on the TypeScript side):
//!
//! - [`network`] — chain spec types + mainnet/testnet constants. Always
//!   compiled in; zero runtime deps.
//! - [`native`] — alpha typed REST client for `/chain/info`,
//!   `/staking/validators`, `/accounts/<addr>/nonce`, and
//!   `POST /transactions`. Behind the `native` feature (default).
//! - [`wallet`] — alpha secp256k1 keypair, Ethereum-style address
//!   derivation, and Sentrix-native transfer signing. Behind `wallet`.
//! - [`evm`] — alpha Alloy HTTP provider factory for Sentrix EVM RPC.
//!   Behind `evm`.
//! - [`grpc`] — alpha tonic client over the chain's `sentrix.v1.Sentrix`
//!   service via the `sentrix-proto` crate. Behind `grpc`.
//! - [`bft`] — alpha WebSocket subscription manager for Sentrix/EVM
//!   subscription channels. Behind `bft`.
//!
//! Status: `0.1.0-alpha.1`. APIs compile and are intended for external
//! integration testing, but the crate is not a 1.0-stable production
//! interface yet.
//!
//! Unit warning: native REST/native ledger amounts use `sentri`
//! (8-decimal SRX). EVM JSON-RPC uses wei-style 18-decimal units for
//! Ethereum tooling compatibility. Do not mix native and EVM amounts
//! without explicit conversion.

#![deny(unsafe_code)]
#![warn(missing_docs)]
// Doc comments intentionally use multi-line continuations without
// extra indentation — the prose reads cleaner that way and rustfmt
// preserves it. Clippy's lazy-continuation rule wants every wrap
// double-indented; we'd rather not.
#![allow(clippy::doc_lazy_continuation)]

pub mod network;

#[cfg(feature = "native")]
pub mod native;

#[cfg(feature = "wallet")]
pub mod wallet;

#[cfg(feature = "evm")]
pub mod evm;

#[cfg(feature = "grpc")]
pub mod grpc;

#[cfg(feature = "bft")]
pub mod bft;

// Re-export the most-used types at the crate root for ergonomic use.
pub use network::{get_spec, sentrix_mainnet, sentrix_testnet, Network, SentrixChainSpec};

#[cfg(feature = "native")]
pub use native::NativeClient;

#[cfg(feature = "wallet")]
pub use wallet::SentrixWallet;
