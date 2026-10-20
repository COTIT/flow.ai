//! Env config. Read once at boot; missing or bad var fails boot with var name.

use std::{fmt, net::SocketAddr};

const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8080";
const DEFAULT_TOKEN_TTL_DAYS: i64 = 30;
const MAX_TOKEN_TTL_DAYS: i64 = 365;

/// Runtime settings. Dev loads `.env`, prod injects real env.
pub struct Config {
    pub database_url: String,
    pub bind_addr: SocketAddr,
    /// Bearer token lifetime. Laravel tokens never expired; ours do.
    pub token_ttl_days: i64,
    /// vimdesk-web origin for links in emails, no trailing slash. Laravel used request Origin (spoofable).
    pub frontend_url: String,
    pub smtp_url: String,
    pub mail_from: String,
    /// Public auth routes: requests allowed at once per client IP.
    pub rate_limit_burst: u32,
    /// Seconds until one more request is allowed.
    pub rate_limit_replenish_secs: u64,
    /// Key rate limit on X-Forwarded-For. Only behind Traefik.
    pub trust_proxy_headers: bool,
    /// Invites one user may create or re-send per hour.
    pub invites_per_hour: i64,
}

// Manual Debug: database_url carries DB password.
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("database_url", &"***")
            .field("bind_addr", &self.bind_addr)
            .field("token_ttl_days", &self.token_ttl_days)
            .field("frontend_url", &self.frontend_url)
            .field("smtp_url", &"***") // may carry SMTP credentials
            .field("mail_from", &self.mail_from)
            .field("rate_limit_burst", &self.rate_limit_burst)
            .field("rate_limit_replenish_secs", &self.rate_limit_replenish_secs)
            .field("trust_proxy_headers", &self.trust_proxy_headers)
            .field("invites_per_hour", &self.invites_per_hour)
            .finish()
    }
}

impl Config {
    /// Build from process env.
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    // Lookup fn instead of env::var so tests skip global env mutation (unsafe in edition 2024).
    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let database_url = get("DATABASE_URL").ok_or("DATABASE_URL not set")?;
        let bind_addr = get("BIND_ADDR")
            .unwrap_or_else(|| DEFAULT_BIND_ADDR.into())
            .parse()
            .map_err(|err| format!("BIND_ADDR invalid: {err}"))?;
        let token_ttl_days = match get("TOKEN_TTL_DAYS") {
            None => DEFAULT_TOKEN_TTL_DAYS,
            Some(raw) => raw
                .parse()
                .ok()
                .filter(|days| (1..=MAX_TOKEN_TTL_DAYS).contains(days))
                .ok_or(format!(
                    "TOKEN_TTL_DAYS must be 1..={MAX_TOKEN_TTL_DAYS}, got {raw:?}"
                ))?,
        };

        let frontend_url = get("FRONTEND_URL").ok_or("FRONTEND_URL not set")?;
        if !(frontend_url.starts_with("https://") || frontend_url.starts_with("http://")) {
            return Err(format!(
                "FRONTEND_URL must start with http(s)://, got {frontend_url:?}"
            ));
        }
        let frontend_url = frontend_url.trim_end_matches('/').to_owned();
        let smtp_url = get("SMTP_URL").ok_or("SMTP_URL not set")?;
        let mail_from = get("MAIL_FROM").ok_or("MAIL_FROM not set")?;
        // Default 10 at once, then 1 per 6s: ~10 attempts a minute per IP.
        let rate_limit_burst = positive(&get, "RATE_LIMIT_BURST", 10)?;
        let rate_limit_replenish_secs = positive(&get, "RATE_LIMIT_REPLENISH_SECS", 6)?;
        let trust_proxy_headers = match get("TRUST_PROXY_HEADERS").as_deref() {
            None | Some("false") => false,
            Some("true") => true,
            Some(other) => {
                return Err(format!(
                    "TRUST_PROXY_HEADERS must be true|false, got {other:?}"
                ));
            }
        };
        let invites_per_hour = positive(&get, "INVITES_PER_HOUR", 20)?;

        Ok(Self {
            database_url,
            bind_addr,
            token_ttl_days,
            frontend_url,
            smtp_url,
            mail_from,
            rate_limit_burst,
            rate_limit_replenish_secs,
            trust_proxy_headers,
            invites_per_hour,
        })
    }
}

/// Optional integer env var, must be > 0.
fn positive<T>(get: &impl Fn(&str) -> Option<String>, key: &str, default: T) -> Result<T, String>
where
    T: std::str::FromStr + PartialOrd + Default,
{
    match get(key) {
        None => Ok(default),
        Some(raw) => raw
            .parse::<T>()
            .ok()
            .filter(|v| *v > T::default())
            .ok_or(format!("{key} must be a positive integer, got {raw:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Required vars besides DATABASE_URL, so each test only spells out what it checks.
    const REQUIRED: [(&str, &str); 3] = [
        ("FRONTEND_URL", "https://app.test"),
        ("SMTP_URL", "smtp://127.0.0.1:1025"),
        ("MAIL_FROM", "Genesy <no-reply@genesy.test>"),
    ];

    /// `pairs` win over REQUIRED defaults; pair value "" means unset.
    fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .chain(REQUIRED.iter())
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
                .filter(|v| !v.is_empty())
        }
    }

    #[test]
    fn reads_all_vars() {
        let cfg = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://u:p@h/db"),
            ("BIND_ADDR", "127.0.0.1:9000"),
        ]))
        .unwrap();

        assert_eq!(cfg.database_url, "postgres://u:p@h/db");
        assert_eq!(cfg.bind_addr, "127.0.0.1:9000".parse().unwrap());
    }

    #[test]
    fn token_ttl_defaults_and_overrides() {
        let default = Config::from_lookup(lookup(&[("DATABASE_URL", "postgres://x")])).unwrap();
        let custom = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://x"),
            ("TOKEN_TTL_DAYS", "7"),
        ]))
        .unwrap();

        assert_eq!(default.token_ttl_days, 30);
        assert_eq!(custom.token_ttl_days, 7);
    }

    #[test]
    fn token_ttl_out_of_range_names_var() {
        for bad in ["0", "366", "-1", "abc"] {
            let err = Config::from_lookup(lookup(&[
                ("DATABASE_URL", "postgres://x"),
                ("TOKEN_TTL_DAYS", bad),
            ]))
            .unwrap_err();
            assert!(err.starts_with("TOKEN_TTL_DAYS"), "{bad}: {err}");
        }
    }

    #[test]
    fn debug_masks_database_url() {
        let cfg =
            Config::from_lookup(lookup(&[("DATABASE_URL", "postgres://u:secret@h/db")])).unwrap();

        assert!(!format!("{cfg:?}").contains("secret"));
    }

    #[test]
    fn bind_addr_defaults_when_unset() {
        let cfg = Config::from_lookup(lookup(&[("DATABASE_URL", "postgres://x")])).unwrap();

        assert_eq!(cfg.bind_addr, DEFAULT_BIND_ADDR.parse().unwrap());
    }

    #[test]
    fn mail_and_frontend_vars_required() {
        for var in ["FRONTEND_URL", "SMTP_URL", "MAIL_FROM"] {
            let err = Config::from_lookup(lookup(&[("DATABASE_URL", "postgres://x"), (var, "")]))
                .unwrap_err();
            assert_eq!(err, format!("{var} not set"));
        }
    }

    #[test]
    fn frontend_url_trailing_slash_trimmed_and_scheme_checked() {
        let cfg = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://x"),
            ("FRONTEND_URL", "https://app.genesy.app/"),
        ]))
        .unwrap();
        assert_eq!(cfg.frontend_url, "https://app.genesy.app");

        let err = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://x"),
            ("FRONTEND_URL", "app.genesy.app"),
        ]))
        .unwrap_err();
        assert!(err.starts_with("FRONTEND_URL must start"), "{err}");
    }

    #[test]
    fn debug_masks_smtp_url() {
        let cfg = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://x"),
            ("SMTP_URL", "smtp://user:smtpsecret@host:587"),
        ]))
        .unwrap();

        assert!(!format!("{cfg:?}").contains("smtpsecret"));
    }

    #[test]
    fn rate_limit_defaults_and_validation() {
        let cfg = Config::from_lookup(lookup(&[("DATABASE_URL", "postgres://x")])).unwrap();
        assert_eq!(
            (cfg.rate_limit_burst, cfg.rate_limit_replenish_secs),
            (10, 6)
        );
        assert!(!cfg.trust_proxy_headers);

        let cfg = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://x"),
            ("RATE_LIMIT_BURST", "3"),
            ("TRUST_PROXY_HEADERS", "true"),
        ]))
        .unwrap();
        assert_eq!(cfg.rate_limit_burst, 3);
        assert!(cfg.trust_proxy_headers);

        for (key, bad) in [
            ("RATE_LIMIT_BURST", "0"),
            ("RATE_LIMIT_REPLENISH_SECS", "-1"),
            ("RATE_LIMIT_BURST", "lots"),
            ("TRUST_PROXY_HEADERS", "yes"),
        ] {
            let err = Config::from_lookup(lookup(&[("DATABASE_URL", "postgres://x"), (key, bad)]))
                .unwrap_err();
            assert!(err.starts_with(key), "{key}={bad}: {err}");
        }
    }

    #[test]
    fn missing_database_url_names_var() {
        let err = Config::from_lookup(lookup(&[])).unwrap_err();

        assert_eq!(err, "DATABASE_URL not set");
    }

    #[test]
    fn bad_bind_addr_names_var() {
        let err = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://x"),
            ("BIND_ADDR", "not-an-addr"),
        ]))
        .unwrap_err();

        assert!(err.starts_with("BIND_ADDR invalid"), "{err}");
    }
}
