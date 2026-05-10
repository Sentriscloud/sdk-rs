//! Network identity + endpoint inventory. Single source of truth for the
//! SDK; every other module (`native`, future `evm`/`grpc`) consumes from
//! here, never hard-codes the chain id or RPC URL on its own.

/// Mainnet vs testnet selector. Mirrors the TypeScript SDK's
/// `SentrixNetwork` enum so a project that uses both rails stays
/// terminology-aligned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Network {
    /// Sentrix Chain mainnet — chain ID 7119.
    Mainnet,
    /// Sentrix Testnet — chain ID 7120.
    Testnet,
}

/// Full spec for a Sentrix network — name, chain ID, RPC + REST + WS
/// + explorer + verifier + faucet URLs. Const-constructed; no I/O
/// happens just because you reference it.
#[derive(Debug, Clone, Copy)]
pub struct SentrixChainSpec {
    /// Display name (matches the canonical chainlist registry entry).
    pub name: &'static str,
    /// EIP-155 chain id.
    pub chain_id: u32,
    /// Public HTTP JSON-RPC endpoint.
    pub rpc_url: &'static str,
    /// Public WebSocket JSON-RPC endpoint.
    pub ws_url: &'static str,
    /// Native REST API base (`/chain/info`, `/staking/validators`, …).
    pub rest_url: &'static str,
    /// Public gRPC-Web endpoint. Caddy transcodes between gRPC-Web ↔
    /// native gRPC HTTP/2 transparently so the same address serves
    /// both browser (`grpc-web`) and server (`@grpc/grpc-js` / `tonic`)
    /// consumers.
    pub grpc_url: &'static str,
    /// EIP-3091-compatible block explorer.
    pub explorer_url: &'static str,
    /// Self-hosted Sourcify verifier.
    pub verifier_url: &'static str,
    /// Faucet URL — testnet only; `None` on mainnet.
    pub faucet_url: Option<&'static str>,
}

/// Mainnet — chain ID 7119, real economic value, single-token (SRX only).
pub const MAINNET_SPEC: SentrixChainSpec = SentrixChainSpec {
    name: "Sentrix Chain",
    chain_id: 7119,
    rpc_url: "https://rpc.sentrixchain.com",
    ws_url: "wss://rpc.sentrixchain.com/ws",
    rest_url: "https://rpc.sentrixchain.com",
    grpc_url: "https://grpc.sentrixchain.com",
    explorer_url: "https://scan.sentrixchain.com",
    verifier_url: "https://verify.sentrixchain.com",
    faucet_url: None,
};

/// Testnet — chain ID 7120, freely-faucetable SRX, target activation
/// venue for new chain features before they reach mainnet.
pub const TESTNET_SPEC: SentrixChainSpec = SentrixChainSpec {
    name: "Sentrix Testnet",
    chain_id: 7120,
    rpc_url: "https://testnet-rpc.sentrixchain.com",
    ws_url: "wss://testnet-rpc.sentrixchain.com/ws",
    rest_url: "https://testnet-rpc.sentrixchain.com",
    grpc_url: "https://grpc-testnet.sentrixchain.com",
    explorer_url: "https://scan-testnet.sentrixchain.com",
    verifier_url: "https://verify.sentrixchain.com",
    faucet_url: Some("https://faucet.sentrixchain.com"),
};

/// Convenience accessor — usable in `const` contexts.
pub const fn sentrix_mainnet() -> SentrixChainSpec {
    MAINNET_SPEC
}

/// Convenience accessor — usable in `const` contexts.
pub const fn sentrix_testnet() -> SentrixChainSpec {
    TESTNET_SPEC
}

/// Resolve the spec for a given `Network`.
pub const fn get_spec(network: Network) -> SentrixChainSpec {
    match network {
        Network::Mainnet => MAINNET_SPEC,
        Network::Testnet => TESTNET_SPEC,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mainnet_spec_matches_chainlist_registry() {
        // These values are the canonical chainlist submission
        // (ethereum-lists/chains#8266) — any drift here is a real
        // bug, not a style choice.
        assert_eq!(MAINNET_SPEC.name, "Sentrix Chain");
        assert_eq!(MAINNET_SPEC.chain_id, 7119);
        assert_eq!(MAINNET_SPEC.rpc_url, "https://rpc.sentrixchain.com");
        assert_eq!(MAINNET_SPEC.explorer_url, "https://scan.sentrixchain.com");
    }

    #[test]
    fn testnet_uses_dedicated_explorer_host() {
        // Pre-2026-05-10 testnet pointed at scan.sentrixchain.com,
        // which sent every testnet tx deeplink to the mainnet view.
        // Dedicated scan-testnet.* host gives EIP-3091 a fighting chance.
        assert_eq!(
            TESTNET_SPEC.explorer_url,
            "https://scan-testnet.sentrixchain.com"
        );
        assert_eq!(
            TESTNET_SPEC.faucet_url,
            Some("https://faucet.sentrixchain.com")
        );
    }

    #[test]
    fn get_spec_dispatch() {
        assert_eq!(get_spec(Network::Mainnet).chain_id, 7119);
        assert_eq!(get_spec(Network::Testnet).chain_id, 7120);
    }
}
