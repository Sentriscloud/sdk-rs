//! EVM door — alloy-based JSON-RPC client. Mirror of
//! `@sentrix/chain/evm` (which wraps viem).
//!
//! Sentrix exposes the standard `eth_*` JSON-RPC surface at the
//! network's RPC URL. Anything fluent in alloy can drop this into
//! existing Rust dApp code without learning a Sentrix-specific API —
//! same `Provider` trait, same `Address` / `B256` / `U256` types.
//!
//! Only the `provider` factory + chain-spec wiring lives here. For
//! more advanced flows (signing transactions, contract bindings,
//! event filters) reach for alloy directly via the returned
//! `RootProvider`.

use alloy::providers::{ProviderBuilder, RootProvider};
use alloy::transports::http::{Client, Http};
use url::Url;

use crate::network::{get_spec, Network};

/// Errors surfaced by the EVM module.
#[derive(Debug, thiserror::Error)]
pub enum EvmError {
    /// Endpoint URL didn't parse.
    #[error("invalid rpc url: {0}")]
    Url(#[from] url::ParseError),
}

/// Build an HTTP `RootProvider` for the chosen network. Cheap;
/// constructs a `reqwest::Client` wrapper but no network IO until
/// the first RPC call.
///
/// ```no_run
/// use sentrix_chain::{Network, evm};
/// use alloy::providers::Provider;
///
/// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
/// let provider = evm::http_provider(Network::Mainnet)?;
/// let block_number = provider.get_block_number().await?;
/// println!("tip: {block_number}");
/// # Ok(()) }
/// ```
pub fn http_provider(network: Network) -> Result<RootProvider<Http<Client>>, EvmError> {
    let spec = get_spec(network);
    let url: Url = spec.rpc_url.parse()?;
    Ok(ProviderBuilder::new().on_http(url))
}

/// Same as [`http_provider`] but with a custom RPC URL — handy when
/// pointing at an internal endpoint during dev or against a staging
/// chain.
pub fn http_provider_with_url(rpc_url: &str) -> Result<RootProvider<Http<Client>>, EvmError> {
    let url: Url = rpc_url.parse()?;
    Ok(ProviderBuilder::new().on_http(url))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_builds_for_mainnet() {
        let p = http_provider(Network::Mainnet);
        assert!(p.is_ok());
    }

    #[test]
    fn provider_builds_for_testnet() {
        let p = http_provider(Network::Testnet);
        assert!(p.is_ok());
    }

    #[test]
    fn provider_with_custom_url() {
        let p = http_provider_with_url("https://example.com/rpc");
        assert!(p.is_ok());
    }

    #[test]
    fn rejects_malformed_url() {
        let p = http_provider_with_url("not a url");
        assert!(p.is_err());
    }
}
