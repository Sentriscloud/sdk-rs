//! gRPC door — tonic client over the chain's `sentrix.v1.Sentrix`
//! service. Mirror of `@sentrix/chain/grpc` on the TypeScript side.
//!
//! Two consumption paths:
//!
//! - [`SentrixGrpcClient`] — thin wrapper that hides the proto-loader
//!   plumbing + endpoint resolution. Convenience for the common
//!   `getLatestBlock` / `getBalance` / `streamEvents` calls.
//! - [`pb`] — the raw prost types + tonic stub if you need richer
//!   request shapes (custom `at_height`, multi-filter streams, …).
//!
//! The proto types are pre-generated + committed (`pb.rs` is checked
//! in) so consumers don't need `protoc` installed. Regenerate via
//! `cargo run --bin gen-grpc` if the upstream chain bumps the proto
//! (planned tooling — not shipped in alpha.0).
//!
//! Available calls (chain v0.4+):
//!   - `get_latest_block()` / `get_block_by_height(h)`
//!   - `get_balance(addr)`                       — 20-byte address
//!   - `get_validator_set(at_height: Option<u64>)`
//!   - `get_supply(at_height: Option<u64>)`
//!   - `get_mempool(limit: u32)`
//!   - `subscribe_events(filters: Vec<EventFilter>)` — server-stream

/// Re-export of the upstream [`sentrix_proto`] crate so existing
/// `sentrix_chain::grpc::pb::*` paths keep compiling without churn.
/// The proto schema is the source-of-truth in `sentrix-labs/sentrix`
/// and ships to crates.io as the standalone `sentrix-proto` crate;
/// this module is a stable alias.
#[allow(missing_docs)]
pub mod pb {
    pub use sentrix_proto::*;
}

use crate::network::{get_spec, Network};
use pb::sentrix_client::SentrixClient as InnerClient;

use tonic::transport::{Channel, ClientTlsConfig, Endpoint};

/// Errors surfaced by the gRPC module.
#[derive(Debug, thiserror::Error)]
pub enum GrpcError {
    /// Endpoint URL didn't parse.
    #[error("invalid endpoint: {0}")]
    Endpoint(String),
    /// Channel construction failed.
    #[error("transport: {0}")]
    Transport(#[from] tonic::transport::Error),
    /// RPC call failed at the server side.
    #[error("rpc: {0}")]
    Rpc(#[from] tonic::Status),
    /// Recipient address wasn't 20 bytes.
    #[error("invalid address: {0}")]
    InvalidAddress(String),
}

/// Thin wrapper around the generated tonic client. Holds the channel
/// + the typed stub. Cheap to clone (channel does internal connection
/// pooling); spin one up once per process and share.
#[derive(Clone)]
pub struct SentrixGrpcClient {
    inner: InnerClient<Channel>,
}

impl SentrixGrpcClient {
    /// Build a client targeting the given network's public gRPC
    /// endpoint (`grpc.sentrixchain.com:443` mainnet,
    /// `grpc-testnet.sentrixchain.com:443` testnet). TLS-by-default
    /// — Caddy serves the public endpoint with a Let's Encrypt cert
    /// that the system root store accepts via the `tls-roots`
    /// feature.
    pub async fn connect(network: Network) -> Result<Self, GrpcError> {
        let spec = get_spec(network);
        Self::connect_url(spec.grpc_url).await
    }

    /// Build a client against a custom URL. Use for dev sidecars,
    /// staging endpoints, or operator-side mTLS-protected hosts.
    /// Plain `http://…` for plaintext, `https://…` for TLS — when
    /// the URL is https we attach a system-roots TLS config
    /// automatically so consumers don't need to know the
    /// `ClientTlsConfig` builder exists.
    pub async fn connect_url(url: &str) -> Result<Self, GrpcError> {
        let endpoint =
            Endpoint::try_from(url.to_string()).map_err(|e| GrpcError::Endpoint(e.to_string()))?;
        let endpoint = if url.starts_with("https://") {
            endpoint.tls_config(ClientTlsConfig::new().with_native_roots())?
        } else {
            endpoint
        };
        let channel = endpoint.connect().await?;
        Ok(Self {
            inner: InnerClient::new(channel),
        })
    }

    /// `GetBlock { latest: true }` — latest finalised block.
    pub async fn get_latest_block(&mut self) -> Result<pb::Block, GrpcError> {
        let req = pb::GetBlockRequest {
            selector: Some(pb::get_block_request::Selector::Latest(true)),
        };
        Ok(self.inner.get_block(req).await?.into_inner())
    }

    /// `GetBlock { height }` — block at a specific height. Returns a
    /// `tonic::Status::NotFound`-equivalent error if the chain pruned
    /// it.
    pub async fn get_block_by_height(&mut self, height: u64) -> Result<pb::Block, GrpcError> {
        let req = pb::GetBlockRequest {
            selector: Some(pb::get_block_request::Selector::Height(pb::BlockHeight {
                value: height,
            })),
        };
        Ok(self.inner.get_block(req).await?.into_inner())
    }

    /// `GetBalance` — current native + EVM balance for a 20-byte
    /// address. Accepts hex string (with or without `0x`) or raw
    /// bytes.
    pub async fn get_balance(&mut self, address: &[u8]) -> Result<pb::Account, GrpcError> {
        if address.len() != 20 {
            return Err(GrpcError::InvalidAddress(format!(
                "want 20 bytes, got {}",
                address.len()
            )));
        }
        let req = pb::GetBalanceRequest {
            address: Some(pb::Address {
                value: address.to_vec(),
            }),
            at_height: None,
        };
        Ok(self.inner.get_balance(req).await?.into_inner())
    }

    /// v0.4+ — `GetValidatorSet` with full active set + jail flags.
    pub async fn get_validator_set(
        &mut self,
        at_height: Option<u64>,
    ) -> Result<pb::ValidatorSet, GrpcError> {
        let req = pb::GetValidatorSetRequest {
            at_height: at_height.map(|v| pb::BlockHeight { value: v }),
        };
        Ok(self.inner.get_validator_set(req).await?.into_inner())
    }

    /// v0.4+ — `GetSupply` minted/burned/circulating snapshot.
    pub async fn get_supply(&mut self, at_height: Option<u64>) -> Result<pb::Supply, GrpcError> {
        let req = pb::GetSupplyRequest {
            at_height: at_height.map(|v| pb::BlockHeight { value: v }),
        };
        Ok(self.inner.get_supply(req).await?.into_inner())
    }

    /// v0.4+ — `GetMempool` pending-tx size + capped header window.
    /// `limit = 0` ⇒ server default (100).
    pub async fn get_mempool(&mut self, limit: u32) -> Result<pb::Mempool, GrpcError> {
        let req = pb::GetMempoolRequest { limit };
        Ok(self.inner.get_mempool(req).await?.into_inner())
    }

    /// v0.4+ — server-streaming `StreamEvents`. Empty filter list =
    /// subscribe-all. Caller drains via `.message().await` (or
    /// `tokio_stream` adapters).
    pub async fn subscribe_events(
        &mut self,
        filters: Vec<i32>,
    ) -> Result<tonic::Streaming<pb::ChainEvent>, GrpcError> {
        let req = pb::StreamEventsRequest {
            filters,
            from_sequence: 0,
        };
        Ok(self.inner.stream_events(req).await?.into_inner())
    }
}

/// Convenience — short-form hex of a 32-byte hash for UI rendering.
pub fn hash_short(h: &pb::Hash) -> String {
    if h.value.len() != 32 {
        return "—".into();
    }
    let hex_str = ::hex::encode(&h.value);
    format!("{}…{}", &hex_str[..6], &hex_str[hex_str.len() - 4..])
}
