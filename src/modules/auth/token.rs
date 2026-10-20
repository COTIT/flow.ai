//! Random secrets (bearer tokens, invite codes): 32 OS-random bytes, hex for transport, sha256 for storage.

use std::fmt;

use sha2::{Digest, Sha256};

use crate::error::AppError;

/// Plaintext secret, shown once (response body or email link). Debug masked, no Display.
pub struct PlainSecret(String);

impl PlainSecret {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PlainSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PlainSecret(***)")
    }
}

/// New secret + sha256 to store. 256 bits of OS randomness, so plain sha256 (no salt/stretch) is enough.
pub fn generate() -> Result<(PlainSecret, [u8; 32]), AppError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| AppError::Internal(format!("os rng: {e}")))?;
    let plain = hex::encode(bytes);
    let digest = hash(&plain);
    Ok((PlainSecret(plain), digest))
}

/// sha256 of secret exactly as client sends it (hex text), so lookup needs no decoding.
pub fn hash(plain: &str) -> [u8; 32] {
    Sha256::digest(plain.as_bytes()).into()
}

/// Cheap shape check before DB lookup: every real secret is 64 hex chars.
pub fn looks_valid(plain: &str) -> bool {
    plain.len() == 64 && plain.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn generate_gives_64_hex_chars_and_matching_hash() {
        let (plain, hash) = generate().unwrap();

        assert_eq!(plain.expose().len(), 64);
        assert!(plain.expose().chars().all(|c| c.is_ascii_hexdigit()));
        let expect: [u8; 32] = Sha256::digest(plain.expose().as_bytes()).into();
        assert_eq!(hash, expect);
    }

    #[test]
    fn generate_never_repeats() {
        let (a, _) = generate().unwrap();
        let (b, _) = generate().unwrap();

        assert_ne!(a.expose(), b.expose());
    }

    #[test]
    fn hash_is_deterministic() {
        assert_eq!(hash("abc"), hash("abc"));
        assert_ne!(hash("abc"), hash("abd"));
    }

    #[test]
    fn debug_hides_secret() {
        let (plain, _) = generate().unwrap();

        assert_eq!(format!("{plain:?}"), "PlainSecret(***)");
    }
}
