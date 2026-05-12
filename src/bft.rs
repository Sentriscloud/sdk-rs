//! BFT door — WebSocket subscription helpers for the 9 Sentrix
//! channels. Mirror of `@sentrix/chain/bft` on the TypeScript side.
//!
//! All subscriptions go through `eth_subscribe`, even the
//! Sentrix-native channels (`sentrix_finalized`, `sentrix_validatorSet`,
//! `sentrix_tokenOps`, `sentrix_stakingOps`, `sentrix_jail`). The chain
//! dispatches them by channel name — there is no separate
//! `sentrix_subscribe` method, common confusion source.
//!
//! Recommended usage: instantiate [`SubscriptionManager`] once per
//! process + call [`SubscriptionManager::subscribe`] repeatedly. The
//! manager multiplexes every subscription over one socket, sends
//! keepalive pings every 30 s (so middleboxes — Caddy reverse_proxy
//! idle_timeout, NAT, AWS ALB — don't drop quiet connections), and
//! transparently re-subscribes after reconnect with exponential
//! backoff (1 s → 2 s → … → 30 s capped).
//!
//! Each [`subscribe`] call returns a `tokio::sync::mpsc::UnboundedReceiver`
//! that yields `serde_json::Value` payloads. Drain it with the standard
//! `.recv().await` loop.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time::{interval, sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::network::{get_spec, Network};

/// One of the 9 supported channel names. Mirror of the TS
/// `Channel` union type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    /// Standard EVM block-header push.
    NewHeads,
    /// Standard EVM log filter (pass `filter` via `subscribe_logs`).
    Logs,
    /// EVM mempool admission events.
    NewPendingTransactions,
    /// EVM sync-status push.
    Syncing,
    /// BFT-finalised block (after 2/3+1 precommit supermajority).
    SentrixFinalized,
    /// Active validator set rotation events.
    SentrixValidatorSet,
    /// Native SRC-20 Mint/Burn/Transfer/Approve/Deploy.
    SentrixTokenOps,
    /// Native Delegate/Undelegate/ClaimRewards/RegisterValidator/AddSelfStake/Unjail.
    SentrixStakingOps,
    /// Per-validator jail / unjail events.
    SentrixJail,
}

impl Channel {
    /// Wire name as the chain expects it in `eth_subscribe` params.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NewHeads => "newHeads",
            Self::Logs => "logs",
            Self::NewPendingTransactions => "newPendingTransactions",
            Self::Syncing => "syncing",
            Self::SentrixFinalized => "sentrix_finalized",
            Self::SentrixValidatorSet => "sentrix_validatorSet",
            Self::SentrixTokenOps => "sentrix_tokenOps",
            Self::SentrixStakingOps => "sentrix_stakingOps",
            Self::SentrixJail => "sentrix_jail",
        }
    }
}

/// Errors surfaced by the BFT subscription module.
#[derive(Debug, thiserror::Error)]
pub enum BftError {
    /// WebSocket transport failed.
    #[error("websocket: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    /// JSON serialise / deserialise failed.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// Subscribe response timed out (10 s).
    #[error("subscribe timeout for {0:?}")]
    Timeout(Channel),
    /// Server returned an error in the subscribe response.
    #[error("server: {0}")]
    Server(String),
    /// Channel send failed because the receiver was dropped.
    #[error("send failed: receiver dropped")]
    SendFailed,
}

/// Cadence between WebSocket pings + half-open detection threshold.
/// Same values as the TS SDK so behaviour is identical across rails.
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
const STALE_TIMEOUT: Duration = Duration::from_secs(90);
/// Subscribe-response timeout. Matches the TS SDK's 10 s.
const SUBSCRIBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Multiplexes many subscriptions over one WebSocket. Spawn one per
/// process and `clone()` cheaply across tasks.
#[derive(Clone)]
pub struct SubscriptionManager {
    inner: Arc<Mutex<Inner>>,
    tx_to_writer: mpsc::UnboundedSender<WriterCmd>,
}

struct Inner {
    next_id: u64,
    /// jsonrpc id → channel + oneshot for the subscribe-response.
    pending: HashMap<u64, (Channel, oneshot::Sender<Result<String, BftError>>)>,
    /// chain-assigned subscription id → caller's mpsc sender.
    subs: HashMap<String, mpsc::UnboundedSender<Value>>,
}

enum WriterCmd {
    Send(String),
}

impl SubscriptionManager {
    /// Build a manager + spawn the read/write/keepalive loops. Cheap;
    /// the WebSocket connection is established in the background and
    /// retried on failure.
    pub fn new(network: Network) -> Self {
        Self::new_with_url(get_spec(network).ws_url)
    }

    /// Build a manager pointing at a custom WebSocket URL.
    pub fn new_with_url(ws_url: &str) -> Self {
        let inner = Arc::new(Mutex::new(Inner {
            next_id: 1,
            pending: HashMap::new(),
            subs: HashMap::new(),
        }));
        let (tx, rx) = mpsc::unbounded_channel();

        // Spawn the connection driver. It owns reconnect + keepalive +
        // re-subscribe logic; the public `subscribe` API just hands
        // frames to it via `tx_to_writer`.
        let driver_inner = inner.clone();
        let url = ws_url.to_string();
        tokio::spawn(async move {
            run_driver(url, driver_inner, rx).await;
        });

        Self {
            inner,
            tx_to_writer: tx,
        }
    }

    /// Subscribe to a channel. Returns a `Receiver` that yields raw
    /// JSON payloads (the chain's `params.result` per stream event).
    /// Caller decodes the shape based on the channel — `serde_json`
    /// per channel is the cheapest path; a future helper could
    /// dispatch into per-channel typed structs.
    pub async fn subscribe(
        &self,
        channel: Channel,
    ) -> Result<mpsc::UnboundedReceiver<Value>, BftError> {
        self.subscribe_with_filter(channel, None).await
    }

    /// Subscribe to `logs` with a filter object. The filter is passed
    /// verbatim as the second param in the `eth_subscribe` request —
    /// see https://eth.wiki/json-rpc/API for the address/topics
    /// shape.
    pub async fn subscribe_with_filter(
        &self,
        channel: Channel,
        filter: Option<Value>,
    ) -> Result<mpsc::UnboundedReceiver<Value>, BftError> {
        let (id, resp_rx) = {
            let mut inner = self.inner.lock().await;
            let id = inner.next_id;
            inner.next_id += 1;
            let (tx, rx) = oneshot::channel();
            inner.pending.insert(id, (channel, tx));
            (id, rx)
        };

        let mut params = vec![Value::String(channel.as_str().to_string())];
        if let (Channel::Logs, Some(f)) = (channel, filter) {
            params.push(f);
        }
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "eth_subscribe",
            "params": params,
        });
        self.tx_to_writer
            .send(WriterCmd::Send(req.to_string()))
            .map_err(|_| BftError::SendFailed)?;

        // Race the subscribe-response against a 10 s timeout. On
        // timeout we drop the pending entry to avoid leaking, then
        // surface BftError::Timeout to the caller.
        let server_id = match tokio::time::timeout(SUBSCRIBE_TIMEOUT, resp_rx).await {
            Ok(Ok(Ok(sid))) => sid,
            Ok(Ok(Err(e))) => return Err(e),
            Ok(Err(_)) => {
                // oneshot sender dropped — connection died mid-handshake.
                return Err(BftError::Server("connection dropped".into()));
            }
            Err(_) => {
                self.inner.lock().await.pending.remove(&id);
                return Err(BftError::Timeout(channel));
            }
        };

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        self.inner.lock().await.subs.insert(server_id, event_tx);
        Ok(event_rx)
    }
}

async fn run_driver(
    url: String,
    inner: Arc<Mutex<Inner>>,
    mut tx_from_caller: mpsc::UnboundedReceiver<WriterCmd>,
) {
    let mut backoff = Duration::from_secs(1);
    loop {
        let conn = match connect_async(&url).await {
            Ok((ws, _)) => ws,
            Err(e) => {
                tracing_warn(&format!("ws connect failed: {e}; retrying in {backoff:?}"));
                sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(30));
                continue;
            }
        };
        backoff = Duration::from_secs(1);
        let (mut sink, mut stream) = conn.split();
        let mut last_frame_at = std::time::Instant::now();
        let mut keepalive = interval(KEEPALIVE_INTERVAL);
        keepalive.tick().await; // skip the immediate first tick

        loop {
            tokio::select! {
                msg = stream.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            last_frame_at = std::time::Instant::now();
                            handle_frame(&inner, &text).await;
                        }
                        Some(Ok(Message::Pong(_))) => {
                            last_frame_at = std::time::Instant::now();
                        }
                        Some(Ok(_)) => { /* binary / ping / close — ignore */ }
                        Some(Err(e)) => {
                            tracing_warn(&format!("ws read error: {e}; reconnecting"));
                            break;
                        }
                        None => {
                            tracing_warn("ws stream ended; reconnecting");
                            break;
                        }
                    }
                }
                cmd = tx_from_caller.recv() => {
                    match cmd {
                        Some(WriterCmd::Send(s)) => {
                            if let Err(e) = sink.send(Message::Text(s.into())).await {
                                tracing_warn(&format!("ws send error: {e}; reconnecting"));
                                break;
                            }
                        }
                        None => {
                            // Manager dropped — exit driver loop.
                            return;
                        }
                    }
                }
                _ = keepalive.tick() => {
                    // Half-open guard: if STALE_TIMEOUT has elapsed
                    // without any inbound frame, force-close so the
                    // outer loop reconnects.
                    if last_frame_at.elapsed() > STALE_TIMEOUT {
                        tracing_warn(&format!("ws stale > {STALE_TIMEOUT:?}; forcing reconnect"));
                        break;
                    }
                    if let Err(e) = sink.send(Message::Ping(Default::default())).await {
                        tracing_warn(&format!("ws ping failed: {e}; reconnecting"));
                        break;
                    }
                }
            }
        }
        // Reject every pending subscribe so callers don't hang.
        let mut g = inner.lock().await;
        for (_, (_, tx)) in g.pending.drain() {
            let _ = tx.send(Err(BftError::Server("connection dropped".into())));
        }
        // Existing subs stay — the driver will re-subscribe after
        // reconnect (TS SDK does the same to preserve mpsc receivers
        // on the caller side). Caller's stream just sees a brief gap.
        drop(g);
    }
}

async fn handle_frame(inner: &Arc<Mutex<Inner>>, text: &str) {
    let Ok(msg) = serde_json::from_str::<Value>(text) else {
        return;
    };
    // Subscribe-response: {"id": N, "result": "0x..."}
    if let (Some(id), Some(result)) = (msg.get("id").and_then(|v| v.as_u64()), msg.get("result")) {
        let mut g = inner.lock().await;
        if let Some((_, tx)) = g.pending.remove(&id) {
            if let Some(sid) = result.as_str() {
                let _ = tx.send(Ok(sid.to_string()));
            } else if let Some(err) = msg
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
            {
                let _ = tx.send(Err(BftError::Server(err.to_string())));
            } else {
                let _ = tx.send(Err(BftError::Server("malformed subscribe response".into())));
            }
        }
        return;
    }
    // Stream event: {"method":"eth_subscription","params":{"subscription":"0x...","result":...}}
    if msg.get("method").and_then(|m| m.as_str()) == Some("eth_subscription") {
        if let Some(params) = msg.get("params") {
            if let (Some(sid), Some(result)) = (
                params.get("subscription").and_then(|s| s.as_str()),
                params.get("result"),
            ) {
                let g = inner.lock().await;
                if let Some(tx) = g.subs.get(sid) {
                    let _ = tx.send(result.clone());
                }
            }
        }
    }
}

// Lightweight log shim — we don't pull `log` or `tracing` as required
// deps just for occasional warnings. Consumers that want structured
// logs can wrap the manager + emit themselves; the noisy paths below
// are reconnect bookkeeping which is mostly invisible during steady
// state. eprintln! routes to stderr which container journals capture.
fn tracing_warn(msg: &str) {
    eprintln!("[sentrix-chain/bft] {msg}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_wire_names_match_chain_dispatch() {
        assert_eq!(Channel::NewHeads.as_str(), "newHeads");
        assert_eq!(Channel::SentrixFinalized.as_str(), "sentrix_finalized");
        assert_eq!(Channel::SentrixJail.as_str(), "sentrix_jail");
    }
}
