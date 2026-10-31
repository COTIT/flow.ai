//! Passwords: secret newtype, Laravel-equivalent strength policy, argon2id hash/verify off async runtime.

use std::{fmt, sync::LazyLock};

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use serde::Deserialize;

use crate::{
    error::AppError,
    http::json::{Rules, human},
};

const MIN_CHARS: usize = 8;
/// Caps argon2 input per request. Laravel had none (bcrypt silently cut at 72 bytes).
pub const MAX_CHARS: usize = 255;

/// Plaintext password from a request. Debug masked, no Display.
/// Never trimmed: Laravel TrimStrings skips password fields too.
#[derive(Deserialize)]
#[serde(transparent)]
pub struct Password(String);

impl Password {
    /// A password the server made (an officer-issued temporary one).
    pub(crate) fn generated(plain: String) -> Self {
        Self(plain)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(***)")
    }
}

// Overwrite plaintext before the allocation is freed, so it can't linger in a heap dump.
impl Drop for Password {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.0);
    }
}

/// Laravel `Password::min(8)->mixedCase()->letters()->numbers()->symbols()`, plus max length.
/// Lists every failed rule, same as Laravel.
pub fn violations(field: &str, password: &str) -> Vec<String> {
    let f = human(field);
    let has = |pred: fn(char) -> bool| password.chars().any(pred);
    let len = password.chars().count();
    let mut out = Vec::new();

    if len < MIN_CHARS {
        out.push(format!("The {f} must be at least {MIN_CHARS} characters."));
    }
    if len > MAX_CHARS {
        out.push(format!(
            "The {f} must not be greater than {MAX_CHARS} characters."
        ));
    }
    if !(has(char::is_uppercase) && has(char::is_lowercase)) {
        out.push(format!(
            "The {f} must contain at least one uppercase and one lowercase letter."
        ));
    }
    if !has(char::is_alphabetic) {
        out.push(format!("The {f} must contain at least one letter."));
    }
    if !has(char::is_numeric) {
        out.push(format!("The {f} must contain at least one number."));
    }
    // Laravel symbol = \p{Z}|\p{S}|\p{P}: anything printable that isn't a letter or digit, space included.
    if !has(|c| !c.is_alphanumeric() && !c.is_control()) {
        out.push(format!("The {f} must contain at least one symbol."));
    }
    out
}

/// Required + strength policy for a request field. Some when present, even if policy failed
/// (failures recorded in `r`, which blocks the request anyway).
pub fn check(r: &mut Rules, field: &str, value: Option<Password>) -> Option<Password> {
    match value {
        Some(password) if !password.0.is_empty() => {
            for message in violations(field, &password.0) {
                r.add(field, message);
            }
            Some(password)
        }
        _ => {
            r.missing(field);
            None
        }
    }
}

/// argon2id PHC string (OWASP default params). Blocking pool: costs tens of ms CPU.
pub async fn hash(password: Password) -> Result<String, AppError> {
    blocking(move || {
        Argon2::default()
            .hash_password(password.0.as_bytes())
            .map(|h| h.to_string())
            .map_err(|e| AppError::Internal(format!("argon2 hash: {e}")))
    })
    .await
}

/// `hash` for a borrowed password. Copies the secret into the blocking task.
pub async fn hash_ref(password: &Password) -> Result<String, AppError> {
    hash(Password(password.0.clone())).await
}

/// True when password matches stored hash. Unparseable stored hash = false + warn (bad data, not user error).
pub async fn verify(password: Password, stored: String) -> Result<bool, AppError> {
    blocking(move || Ok(matches(&password, &stored))).await
}

static DUMMY_HASH: LazyLock<Option<String>> = LazyLock::new(|| {
    Argon2::default()
        .hash_password(b"timing-equalizer")
        .map(|h| h.to_string())
        .ok()
});

/// Build the dummy hash at boot. Built lazily, the first unknown-email login would also pay
/// for a hash and answer measurably slower than later ones.
pub async fn warm_up() -> Result<(), AppError> {
    blocking(|| {
        LazyLock::force(&DUMMY_HASH);
        Ok(())
    })
    .await
}

/// Same CPU cost as `verify`, always false. Login runs it for unknown emails so response time
/// doesn't reveal which emails exist.
pub async fn verify_dummy(password: Password) -> Result<bool, AppError> {
    blocking(move || {
        if let Some(stored) = DUMMY_HASH.as_deref() {
            matches(&password, stored);
        }
        Ok(false)
    })
    .await
}

fn matches(password: &Password, stored: &str) -> bool {
    match PasswordHash::new(stored) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.0.as_bytes(), &parsed)
            .is_ok(),
        Err(e) => {
            tracing::warn!(error = %e, "stored password hash unparseable");
            false
        }
    }
}

/// At most one argon2 run per CPU at a time. Each run takes ~19 MiB; unbounded, a flood from
/// many IPs (past the per-IP limit) could fill the 512-thread blocking pool and GBs of RAM.
/// Extra callers wait in line instead.
static ARGON2_SLOTS: LazyLock<tokio::sync::Semaphore> = LazyLock::new(|| {
    tokio::sync::Semaphore::new(std::thread::available_parallelism().map_or(2, |n| n.get()))
});

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    let _slot = ARGON2_SLOTS
        .acquire()
        .await
        .map_err(|e| AppError::Internal(format!("argon2 semaphore: {e}")))?;
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| AppError::Internal(format!("blocking task: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;

    const STRONG: &str = "Str0ng!pass";

    #[test]
    fn strong_password_passes() {
        assert!(violations("password", STRONG).is_empty());
    }

    #[test]
    fn each_rule_has_laravel_message() {
        let cases = [
            ("Sh0rt!x", "The password must be at least 8 characters."),
            (
                "all-lower-1!",
                "The password must contain at least one uppercase and one lowercase letter.",
            ),
            (
                "12345678!",
                "The password must contain at least one letter.",
            ),
            (
                "NoNumbers!",
                "The password must contain at least one number.",
            ),
            (
                "NoSymb0ls",
                "The password must contain at least one symbol.",
            ),
        ];
        for (pw, msg) in cases {
            assert!(
                violations("password", pw).contains(&msg.to_string()),
                "{pw}: {:?}",
                violations("password", pw)
            );
        }
    }

    #[test]
    fn reports_every_failed_rule() {
        assert_eq!(violations("password", "a").len(), 4); // min, mixed, number, symbol
    }

    #[test]
    fn length_boundaries() {
        assert!(
            violations("password", "Ab1!abcd").is_empty(),
            "8 chars must pass"
        );
        let max = format!("Ab1!{}", "a".repeat(MAX_CHARS - 4));
        assert!(
            violations("password", &max).is_empty(),
            "{MAX_CHARS} chars must pass"
        );
        let over = format!("{max}a");
        assert_eq!(
            violations("password", &over),
            vec![format!(
                "The password must not be greater than {MAX_CHARS} characters."
            )]
        );
    }

    #[test]
    fn unicode_letters_and_symbols_count() {
        assert!(violations("password", "Ünïcødé1€").is_empty());
    }

    #[test]
    fn check_requires_non_empty() {
        for value in [None, Some(Password(String::new()))] {
            let mut r = Rules::default();
            assert!(check(&mut r, "password", value).is_none());
            assert_eq!(
                r.into_result().unwrap_err().to_string(),
                "The password field is required."
            );
        }
    }

    #[test]
    fn check_records_policy_failures_but_returns_value() {
        let mut r = Rules::default();

        assert!(check(&mut r, "password", Some(Password("weak".into()))).is_some());
        assert!(r.into_result().is_err());
    }

    #[test]
    fn check_strong_passes_clean() {
        let mut r = Rules::default();

        assert!(check(&mut r, "password", Some(Password(STRONG.into()))).is_some());
        assert!(r.into_result().is_ok());
    }

    #[test]
    fn debug_hides_secret() {
        let pw = Password(STRONG.to_owned());

        assert_eq!(format!("{pw:?}"), "Password(***)");
    }

    #[tokio::test]
    async fn hash_then_verify() {
        let hash = hash(Password(STRONG.to_owned())).await.unwrap();

        assert!(hash.starts_with("$argon2id$"), "{hash}");
        assert!(
            verify(Password(STRONG.to_owned()), hash.clone())
                .await
                .unwrap()
        );
        assert!(
            !verify(Password("Wr0ng!pass".to_owned()), hash)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn same_password_gets_different_salt() {
        let a = hash(Password(STRONG.to_owned())).await.unwrap();
        let b = hash(Password(STRONG.to_owned())).await.unwrap();

        assert_ne!(a, b);
    }

    #[tokio::test]
    async fn corrupt_stored_hash_is_mismatch_not_panic() {
        assert!(
            !verify(Password(STRONG.to_owned()), "not-a-phc-string".into())
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn warm_up_then_dummy_still_never_matches() {
        warm_up().await.unwrap();

        assert!(
            DUMMY_HASH
                .as_deref()
                .is_some_and(|h| h.starts_with("$argon2id$"))
        );
        assert!(!verify_dummy(Password(STRONG.to_owned())).await.unwrap());
    }

    #[tokio::test]
    async fn dummy_verify_never_matches() {
        assert!(!verify_dummy(Password(STRONG.to_owned())).await.unwrap());
    }
}
