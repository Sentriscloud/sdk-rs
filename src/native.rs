//! Native Sentrix REST client. Wraps `reqwest` to call the
//! Sentrix-shaped endpoints (`/chain/info`, `/staking/validators`,
//! `/epoch/current`, `/sentrix_status`, …) — the same surface
//! `@sentrix/chain/native` exposes on the TypeScript side.
//!
//! Why a dedicated module instead of letting callers use `reqwest`
//! directly: typed response structs, single endpoint resolution,
//! consistent error handling, and a place to attach retry / backoff
//! when the chain LB returns 5xx during binary swaps.
//!
//! Native transfer amounts and fees are sentri (8-decimal SRX). The
//! `/chain/info` supply fields exposed by [`ChainInfo`] are display SRX
//! values because the REST endpoint returns them that way.

use serde::{Deserialize, Serialize};

use crate::network::{get_spec, Network, SentrixChainSpec};

/// Errors surfaced by the native client.
#[derive(Debug, thiserror::Error)]
pub enum NativeError {
    /// Underlying HTTP transport failed.
    #[error("transport: {0}")]
    Transport(#[from] reqwest::Error),
    /// Server returned a non-2xx status.
    #[error("http {status}: {body}")]
    Status {
        /// HTTP status code.
        status: u16,
        /// Response body (truncated to 1 KB) for diagnostic.
        body: String,
    },
    /// Response body didn't deserialise into the expected shape.
    #[error("decode: {0}")]
    Decode(#[from] serde_json::Error),
}

/// Top-level chain stats — what `/chain/info` returns. u64-backed
/// fields stay as `u64` because Rust handles them natively (vs. the
/// TypeScript SDK that has to drop to `bigint` to avoid Number
/// overflow above 2^53).
#[derive(Debug, Clone, Deserialize)]
pub struct ChainInfo {
    /// Tip block height.
    pub height: u64,
    /// Total blocks ever produced (= height + 1 for a healthy chain).
    pub total_blocks: u64,
    /// Total minted SRX, as returned by `/chain/info`.
    pub total_minted_srx: f64,
    /// Total burned SRX (50% of every fee), as returned by `/chain/info`.
    pub total_burned_srx: f64,
    /// Configured max supply (315 M post-tokenomics-v2 fork).
    pub max_supply_srx: f64,
    /// Active validators in the BFT set right now.
    pub active_validators: u32,
    /// Pending tx count in the mempool.
    pub mempool_size: u64,
    /// Reward paid for the next block in SRX display units.
    pub next_block_reward_srx: f64,
}

/// One validator's stake + jail snapshot. Returned by
/// `/staking/validators`.
#[derive(Debug, Clone, Deserialize)]
pub struct Validator {
    /// 0x-prefixed lowercased address.
    pub address: String,
    /// Display name set at RegisterValidator (may be `None`).
    #[serde(default)]
    pub name: Option<String>,
    /// True if currently in the BFT active set.
    pub is_active: bool,
    /// True if jailed (excluded from voting until unjail).
    #[serde(default)]
    pub is_jailed: bool,
    /// Self-bond. u128 wire (chain returns string for safety).
    pub self_stake: String,
    /// Sum of delegations from non-validator accounts.
    pub total_delegated: String,
}

/// Wrapper for native REST responses that always wrap data in
/// `{ "validators": [...] }` style envelopes.
#[derive(Debug, Clone, Deserialize)]
struct ValidatorsEnvelope {
    validators: Vec<Validator>,
}

/// Outgoing-tx envelope for `POST /transactions`. Each field is the
/// signed-payload form the chain accepts. Use the `wallet` module
/// helper to construct + sign one before broadcast.
#[derive(Debug, Clone, Serialize)]
pub struct SignedTransaction {
    /// 0x-prefixed lowercased sender.
    pub from_address: String,
    /// 0x-prefixed lowercased recipient.
    pub to_address: String,
    /// Sentri (8-decimal). u64 string.
    pub amount: String,
    /// Sentri.
    pub fee: String,
    /// Per-sender nonce.
    pub nonce: u64,
    /// 65-byte secp256k1 signature, hex.
    pub signature: String,
    /// 33-byte compressed public key, hex.
    pub public_key: String,
    /// Optional 0x-prefixed call data.
    #[serde(default)]
    pub data: Option<String>,
    /// Chain id (7119 / 7120).
    pub chain_id: u32,
}

/// Response envelope from `POST /transactions`. The chain returns
/// `{ "txid": "0x..." }` on success.
#[derive(Debug, Clone, Deserialize)]
struct BroadcastResponse {
    txid: String,
}

/// REST client. Reuses the underlying `reqwest::Client` connection
/// pool across calls — instantiate once per process.
#[derive(Debug, Clone)]
pub struct NativeClient {
    spec: SentrixChainSpec,
    http: reqwest::Client,
}

impl NativeClient {
    /// Build a client for the given network using the public REST
    /// endpoint. Use [`NativeClient::with_base_url`] to point at a
    /// loopback / private validator URL during dev.
    pub fn new(network: Network) -> Self {
        Self {
            spec: get_spec(network),
            http: reqwest::Client::new(),
        }
    }

    /// Build a client with a custom REST base URL — handy when
    /// pointing at an internal endpoint during dev or against a
    /// staging chain.
    pub fn with_base_url(network: Network, base_url: &'static str) -> Self {
        let mut spec = get_spec(network);
        spec.rest_url = base_url;
        Self {
            spec,
            http: reqwest::Client::new(),
        }
    }

    /// `GET /chain/info` — top-level stats.
    pub async fn chain_info(&self) -> Result<ChainInfo, NativeError> {
        self.get_json("/chain/info").await
    }

    /// `GET /staking/validators` — current validator set with stake +
    /// jail flags.
    pub async fn validators(&self) -> Result<Vec<Validator>, NativeError> {
        let env: ValidatorsEnvelope = self.get_json("/staking/validators").await?;
        Ok(env.validators)
    }

    /// `GET /accounts/<addr>/nonce` — next valid nonce for an
    /// address. Pre-broadcast use to set `tx.nonce` correctly.
    pub async fn next_nonce(&self, address: &str) -> Result<u64, NativeError> {
        #[derive(Deserialize)]
        struct NonceResp {
            nonce: u64,
        }
        let url = format!(
            "{}/accounts/{}/nonce",
            self.spec.rest_url,
            address.trim_start_matches("0x")
        );
        let resp = self.http.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(self.status_err(resp).await);
        }
        let n: NonceResp = resp.json().await?;
        Ok(n.nonce)
    }

    /// `POST /transactions` — submit a signed native tx. Returns the
    /// chain-assigned txid (32-byte hex). The chain admits to the
    /// mempool synchronously; finalisation happens in a later block,
    /// listen on `sentrix_finalized` via `@sentrix/chain/bft` (or the
    /// gRPC `streamEvents` once the gRPC SDK lands here) for the
    /// confirmation push.
    pub async fn broadcast(&self, tx: &SignedTransaction) -> Result<String, NativeError> {
        let url = format!("{}/transactions", self.spec.rest_url);
        let resp = self.http.post(&url).json(tx).send().await?;
        if !resp.status().is_success() {
            return Err(self.status_err(resp).await);
        }
        let r: BroadcastResponse = resp.json().await?;
        Ok(r.txid)
    }

    /// Read the chain spec this client was constructed with. Useful
    /// when threading the spec into other modules without re-resolving
    /// from `Network`.
    pub fn spec(&self) -> &SentrixChainSpec {
        &self.spec
    }

    // ── internals ────────────────────────────────────────────────
    async fn get_json<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, NativeError> {
        let url = format!("{}{}", self.spec.rest_url, path);
        let resp = self.http.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(self.status_err(resp).await);
        }
        Ok(resp.json().await?)
    }

    async fn status_err(&self, resp: reqwest::Response) -> NativeError {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        NativeError::Status {
            status,
            body: body.chars().take(1024).collect(),
        }
    }
}
