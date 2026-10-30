//! Sealing of stored secrets (integration credentials): XChaCha20-Poly1305 under the server
//! key (`CREDENTIALS_KEY`), each value bound to its workspace and path.

use std::fmt;

use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{error::AppError, modules::auth::token::PlainSecret};

const NONCE_LEN: usize = 24;

/// The 32-byte server key (`CREDENTIALS_KEY`). Debug prints `***`; the cipher zeroizes its
/// key on drop.
pub struct CredentialKey {
    cipher: XChaCha20Poly1305,
    id: String,
}

impl fmt::Debug for CredentialKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CredentialKey(***)")
    }
}

impl CredentialKey {
    /// From 64 hex characters of random bytes. The errors never echo the input.
    pub fn from_hex(hex: &str) -> Result<Self, String> {
        let mut bytes = [0u8; 32];
        let parsed = hex::decode_to_slice(hex.trim(), &mut bytes);
        // 32 random bytes hold about 30 distinct values; a pattern or a zero key holds few.
        let distinct = bytes.iter().collect::<std::collections::HashSet<_>>().len();
        let key = parsed.map(|()| Self::from_bytes(&bytes));
        zeroize::Zeroize::zeroize(&mut bytes);
        match key {
            Err(_) => Err("CREDENTIALS_KEY must be 64 hex characters (32 bytes)".into()),
            Ok(_) if distinct < 16 => {
                Err("CREDENTIALS_KEY isn't random: generate one with `openssl rand -hex 32`".into())
            }
            Ok(key) => Ok(key),
        }
    }

    fn from_bytes(bytes: &[u8; 32]) -> Self {
        let digest = Sha256::new()
            .chain_update(b"genesy credentials key id\0")
            .chain_update(bytes)
            .finalize();
        Self {
            cipher: XChaCha20Poly1305::new(bytes.into()),
            id: hex::encode(&digest[..8]),
        }
    }

    /// 16 hex chars naming this key (a hash, not the key), stored with every sealed value so
    /// a key change shows as such.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Random nonce + ciphertext + tag of `plain`, bound to `workspace` and `path`.
    pub fn seal(&self, workspace: Uuid, path: &str, plain: &str) -> Result<Vec<u8>, AppError> {
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce).map_err(|e| AppError::Internal(format!("os rng: {e}")))?;
        let aad = aad(workspace, path);
        let sealed = self
            .cipher
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: plain.as_bytes(),
                    aad: &aad,
                },
            )
            .map_err(|_| AppError::Internal("credential seal failed".into()))?;
        Ok([nonce.as_slice(), &sealed].concat())
    }

    /// Plaintext of a `seal` output; fails for another key, workspace or path, or any
    /// changed byte.
    pub fn open(
        &self,
        workspace: Uuid,
        path: &str,
        sealed: &[u8],
    ) -> Result<PlainSecret, AppError> {
        let failed = || AppError::Internal(format!("credential {path} failed to open"));
        let (nonce, body) = sealed.split_first_chunk::<NONCE_LEN>().ok_or_else(failed)?;
        let aad = aad(workspace, path);
        let plain = self
            .cipher
            .decrypt(
                &XNonce::from(*nonce),
                Payload {
                    msg: body,
                    aad: &aad,
                },
            )
            .map_err(|_| failed())?;
        String::from_utf8(plain).map(PlainSecret::new).map_err(|e| {
            // Authentic but not text: wipe the plaintext before the buffer is freed.
            zeroize::Zeroize::zeroize(&mut e.into_bytes());
            failed()
        })
    }
}

/// Workspace uuid (fixed 16 bytes) then path: a sealed value copied to another workspace or
/// key fails to open.
fn aad(workspace: Uuid, path: &str) -> Vec<u8> {
    [workspace.as_bytes().as_slice(), path.as_bytes()].concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEX: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
    const PATH: &str = "integrations.payment.payment_providers.mollie.api_key";

    fn key() -> CredentialKey {
        CredentialKey::from_hex(HEX).unwrap()
    }

    #[test]
    fn seal_then_open_round_trips() {
        let ws = Uuid::from_u128(7);
        let sealed = key().seal(ws, PATH, "test_abc").unwrap();
        assert_eq!(sealed.len(), NONCE_LEN + "test_abc".len() + 16);
        assert!(!sealed.windows(8).any(|w| w == b"test_abc"));
        assert_eq!(key().open(ws, PATH, &sealed).unwrap().expose(), "test_abc");
        let empty = key().seal(ws, PATH, "").unwrap();
        assert_eq!(key().open(ws, PATH, &empty).unwrap().expose(), "");
    }

    #[test]
    fn every_seal_takes_a_fresh_nonce() {
        let ws = Uuid::from_u128(7);
        let a = key().seal(ws, PATH, "same").unwrap();
        let b = key().seal(ws, PATH, "same").unwrap();
        assert_ne!(a[..NONCE_LEN], b[..NONCE_LEN]);
        assert_ne!(a, b);
    }

    #[test]
    fn opens_only_for_its_workspace_path_and_key() {
        let ws = Uuid::from_u128(7);
        let sealed = key().seal(ws, PATH, "test_abc").unwrap();
        assert!(key().open(Uuid::from_u128(8), PATH, &sealed).is_err());
        assert!(key().open(ws, "smtp.password", &sealed).is_err());
        let other = CredentialKey::from_hex(&HEX.replace("1f", "ff")).unwrap();
        assert!(other.open(ws, PATH, &sealed).is_err());
        for i in [0, NONCE_LEN, sealed.len() - 1] {
            let mut bad = sealed.clone();
            bad[i] ^= 1;
            assert!(key().open(ws, PATH, &bad).is_err(), "byte {i}");
        }
        assert!(key().open(ws, PATH, &sealed[..NONCE_LEN + 15]).is_err());
        assert!(key().open(ws, PATH, &[]).is_err());
    }

    #[test]
    fn non_text_plaintext_fails_to_open() {
        let ws = Uuid::from_u128(7);
        let nonce = [9u8; NONCE_LEN];
        let body = key()
            .cipher
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &[0xff, 0xfe],
                    aad: &aad(ws, PATH),
                },
            )
            .unwrap();
        let sealed = [nonce.as_slice(), &body].concat();
        assert!(key().open(ws, PATH, &sealed).is_err());
    }

    #[test]
    fn key_parsing_and_printing_never_show_it() {
        for bad in [
            "",
            "abc",
            &HEX[..62],
            &format!("{HEX}00"),
            &HEX.replace('0', "g"),
        ] {
            let err = CredentialKey::from_hex(bad).unwrap_err();
            assert_eq!(err, "CREDENTIALS_KEY must be 64 hex characters (32 bytes)");
        }
        assert!(CredentialKey::from_hex(&format!("{HEX}\n")).is_ok());
        for weak in [
            "00".repeat(32),
            "0102".repeat(16),
            "0001020304050607".repeat(4),
        ] {
            let err = CredentialKey::from_hex(&weak).unwrap_err();
            assert!(err.starts_with("CREDENTIALS_KEY isn't random"), "{err}");
        }
        assert_eq!(format!("{:?}", key()), "CredentialKey(***)");
        assert_eq!(key().id().len(), 16);
        assert_eq!(key().id(), key().id());
        assert!(!HEX.contains(key().id()));
        let other = CredentialKey::from_hex(&HEX.replace("1f", "ff")).unwrap();
        assert_ne!(other.id(), key().id());
    }
}
