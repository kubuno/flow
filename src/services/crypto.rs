//! AES-256-GCM encryption for credential payloads.
//!
//! The key is derived from the core `internal_secret` (stable across restarts as
//! long as the secret is configured) — no extra key management. Each ciphertext
//! carries its own random 12-byte nonce, stored alongside it.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Derive the 32-byte encryption key from the internal secret.
pub fn derive_key(internal_secret: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(internal_secret.as_bytes());
    h.update(b"flow-credentials-v1");
    h.finalize().into()
}

/// Encrypt plaintext → (ciphertext, nonce).
pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher.encrypt(nonce, plaintext).map_err(|e| format!("encrypt: {e}"))?;
    Ok((ct, nonce_bytes.to_vec()))
}

/// Decrypt (ciphertext, nonce) → plaintext.
pub fn decrypt(key: &[u8; 32], ciphertext: &[u8], nonce: &[u8]) -> Result<Vec<u8>, String> {
    if nonce.len() != 12 {
        return Err("nonce invalide".into());
    }
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(nonce);
    cipher.decrypt(nonce, ciphertext).map_err(|e| format!("decrypt: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let key = derive_key("secret-xyz");
        let (ct, nonce) = encrypt(&key, b"hello world").unwrap();
        assert_ne!(ct, b"hello world");
        let pt = decrypt(&key, &ct, &nonce).unwrap();
        assert_eq!(pt, b"hello world");
    }

    #[test]
    fn wrong_key_fails() {
        let (ct, nonce) = encrypt(&derive_key("a"), b"data").unwrap();
        assert!(decrypt(&derive_key("b"), &ct, &nonce).is_err());
    }
}
