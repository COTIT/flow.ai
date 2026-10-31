//! App key secrets: `gsk_` + 64 hex (a `token::generate` secret), sha256 of the whole string stored.

use crate::{
    error::AppError,
    modules::auth::token::{self, PlainSecret},
};

/// Marks an app key, so it never passes for a user token (64 bare hex) or the other way round.
pub const PREFIX: &str = "gsk_";

/// Scopes a key may hold. Checked on create; the extractor checks them per route.
pub const SCOPES: &[&str] = &[CATALOG_READ, CHECKOUT_WRITE];
/// Read products, variants, prices and available stock (`/storefront`).
pub const CATALOG_READ: &str = "catalog:read";
/// Place web orders and start their payment (`/storefront/orders`).
pub const CHECKOUT_WRITE: &str = "checkout:write";

/// App kinds a key is issued to.
pub const KINDS: &[&str] = &["shop", "till", "field", "partner"];

/// New key, its sha256 and its 8-character display prefix.
pub fn generate() -> Result<(PlainSecret, [u8; 32], String), AppError> {
    let (raw, _) = token::generate()?;
    let plain = PlainSecret::new(format!("{PREFIX}{}", raw.expose()));
    let hash = token::hash(plain.expose());
    let prefix = plain.expose()[..8].to_owned();
    Ok((plain, hash, prefix))
}

/// Cheap shape check before the DB lookup.
pub fn looks_valid(plain: &str) -> bool {
    plain.strip_prefix(PREFIX).is_some_and(token::looks_valid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_gives_prefixed_key_matching_hash_and_prefix() {
        let (plain, hash, prefix) = generate().unwrap();

        assert!(looks_valid(plain.expose()), "{}", plain.expose().len());
        assert_eq!(hash, token::hash(plain.expose()));
        assert_eq!(prefix, plain.expose()[..8]);
        assert!(prefix.starts_with(PREFIX));
    }

    #[test]
    fn user_tokens_and_app_keys_never_pass_for_each_other() {
        let (user, _) = token::generate().unwrap();
        let (key, _, _) = generate().unwrap();

        assert!(!looks_valid(user.expose()));
        assert!(!token::looks_valid(key.expose()));
        assert!(!looks_valid("gsk_short"));
        assert!(!looks_valid(&format!("gsk_{}", "z".repeat(64))));
    }
}
