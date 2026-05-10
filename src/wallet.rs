//! Native Sentrix wallet — secp256k1 keypair + Ethereum-style address
//! derivation. Mirror of `@sentrix/chain/wallet`.
//!
//! Sentrix derives addresses identically to Ethereum: take the
//! uncompressed secp256k1 public key (65 bytes, skip the 0x04 prefix),
//! keccak-256 the remaining 64 bytes, the last 20 bytes are the
//! address. So a MetaMask / EVM private key is also a Sentrix native
//! private key — same address on both rails.

use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tiny_keccak::{Hasher, Keccak};

use crate::native::SignedTransaction;

/// Errors surfaced by the wallet module.
#[derive(Debug, thiserror::Error)]
pub enum WalletError {
    /// Private key wasn't a valid secp256k1 scalar.
    #[error("invalid private key: {0}")]
    InvalidKey(String),
    /// Hex decoding failed.
    #[error("hex decode: {0}")]
    Hex(#[from] hex::FromHexError),
    /// Recipient address wasn't a 20-byte hex.
    #[error("invalid recipient address: {0}")]
    InvalidRecipient(String),
    /// JSON serialisation of the signing payload failed.
    #[error("serialise: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// secp256k1 wallet — keypair + derived 0x-address.
pub struct SentrixWallet {
    /// 32-byte secret. Held in memory; never serialised by this struct.
    secret: SecretKey,
    /// Compressed public key (33 bytes) for tx.public_key.
    public: PublicKey,
    /// 0x-prefixed lowercased 20-byte Ethereum-style address.
    pub address: String,
}

impl SentrixWallet {
    /// Build a wallet from a hex-encoded 32-byte private key (with or
    /// without the `0x` prefix).
    pub fn from_private_key_hex(hex: &str) -> Result<Self, WalletError> {
        let stripped = hex.trim_start_matches("0x");
        if stripped.len() != 64 {
            return Err(WalletError::InvalidKey(format!(
                "want 32 bytes (64 hex chars), got {}",
                stripped.len()
            )));
        }
        let bytes = ::hex::decode(stripped)?;
        let secret =
            SecretKey::from_slice(&bytes).map_err(|e| WalletError::InvalidKey(e.to_string()))?;
        let secp = Secp256k1::new();
        let public = PublicKey::from_secret_key(&secp, &secret);
        let address = derive_address(&public);
        Ok(Self {
            secret,
            public,
            address,
        })
    }

    /// Compressed pubkey hex (33 bytes → 66 hex chars).
    pub fn public_key_hex(&self) -> String {
        ::hex::encode(self.public.serialize())
    }

    /// Build + sign a native SRX transfer ready for
    /// [`crate::NativeClient::broadcast`]. `amount_sentri` and
    /// `fee_sentri` are sentri (10^-8 SRX) — convert from SRX with
    /// `srx * 100_000_000`.
    pub fn build_and_sign_transfer(
        &self,
        to: &str,
        amount_sentri: u64,
        fee_sentri: u64,
        nonce: u64,
        chain_id: u32,
    ) -> Result<SignedTransaction, WalletError> {
        let to_lower = to.trim_start_matches("0x").to_lowercase();
        if to_lower.len() != 40 {
            return Err(WalletError::InvalidRecipient(format!(
                "want 20 bytes (40 hex chars), got {}",
                to_lower.len()
            )));
        }
        let to_addr = format!("0x{}", to_lower);

        // Canonical signing payload — BTreeMap-ordered JSON of
        // (amount, chain_id, data, fee, from, nonce, timestamp, to)
        // matches what the chain hashes server-side. Empty data +
        // timestamp=0 for transfers that don't pin a wall-clock; the
        // chain accepts either pattern.
        #[derive(Serialize)]
        struct Payload<'a> {
            amount: u64,
            chain_id: u32,
            data: &'a str,
            fee: u64,
            from: &'a str,
            nonce: u64,
            timestamp: u64,
            to: &'a str,
        }
        let payload = Payload {
            amount: amount_sentri,
            chain_id,
            data: "",
            fee: fee_sentri,
            from: &self.address,
            nonce,
            timestamp: 0,
            to: &to_addr,
        };
        let payload_json = serde_json::to_vec(&payload)?;
        // Chain hashes the canonical JSON with sha256, then signs.
        let mut hasher = Sha256::new();
        hasher.update(&payload_json);
        let digest = hasher.finalize();
        let msg = Message::from_digest_slice(&digest)
            .map_err(|e| WalletError::InvalidKey(e.to_string()))?;
        let secp = Secp256k1::new();
        let sig = secp.sign_ecdsa(&msg, &self.secret);
        let sig_bytes = sig.serialize_compact();

        Ok(SignedTransaction {
            from_address: self.address.clone(),
            to_address: to_addr,
            amount: amount_sentri.to_string(),
            fee: fee_sentri.to_string(),
            nonce,
            signature: ::hex::encode(sig_bytes),
            public_key: self.public_key_hex(),
            data: None,
            chain_id,
        })
    }
}

/// Public-only address derivation — useful when you have an
/// uncompressed pubkey and want the Sentrix address without holding a
/// secret. Uses keccak-256 (Ethereum's hash, not SHA-3 NIST), last 20
/// bytes, lowercased + 0x-prefixed.
fn derive_address(pk: &PublicKey) -> String {
    let serialized = pk.serialize_uncompressed();
    // Skip the 0x04 prefix byte.
    let body = &serialized[1..];
    let mut keccak = Keccak::v256();
    keccak.update(body);
    let mut out = [0u8; 32];
    keccak.finalize(&mut out);
    format!("0x{}", ::hex::encode(&out[12..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_metamask_test_key_derives_correct_address() {
        // Vitalik's well-known test key — same address on Ethereum
        // and Sentrix (both rails use keccak-256 on the uncompressed
        // pubkey).
        let pk = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
        let w = SentrixWallet::from_private_key_hex(pk).unwrap();
        assert_eq!(w.address, "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266");
    }

    #[test]
    fn rejects_short_private_key() {
        let r = SentrixWallet::from_private_key_hex("deadbeef");
        assert!(r.is_err());
    }
}
