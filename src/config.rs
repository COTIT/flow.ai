//! Env config. Read once at boot; missing or bad var fails boot with var name.

use std::{fmt, net::SocketAddr};

use crate::{crypto::CredentialKey, storage::S3Config};

const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8080";
const DEFAULT_TOKEN_TTL_DAYS: i64 = 30;
const MAX_TOKEN_TTL_DAYS: i64 = 365;
/// Request pool size: sqlx's default 10 let one workspace's lock waiters starve every tenant.
const DEFAULT_DB_MAX_CONNECTIONS: u32 = 20;
/// Shared Postgres `max_connections` is 100.
const MAX_DB_MAX_CONNECTIONS: u32 = 100;

/// Runtime settings. Dev loads `.env`, prod injects real env.
pub struct Config {
    pub database_url: String,
    /// Request pool size (`DATABASE_MAX_CONNECTIONS`).
    pub db_max_connections: u32,
    pub bind_addr: SocketAddr,
    /// Bearer token lifetime. Laravel tokens never expired; ours do.
    pub token_ttl_days: i64,
    /// vimdesk-web origin for links in emails, no trailing slash. Laravel used request Origin (spoofable).
    pub frontend_url: String,
    /// Public origin of this API, for links that must hit the API (email verification).
    pub api_public_url: String,
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
    /// Authenticated routes: requests allowed at once per session (bearer token).
    pub private_rate_limit_burst: u32,
    /// Milliseconds until one more request is allowed per session.
    pub private_rate_limit_replenish_ms: u64,
    /// Authenticated routes: ceiling per client IP around the per-session limit.
    pub private_ip_rate_limit_burst: u32,
    /// Milliseconds until one more request is allowed per client IP.
    pub private_ip_rate_limit_replenish_ms: u64,
    /// Requests one workspace may have in flight; default half the request pool.
    pub workspace_max_inflight: u32,
    /// Requests one user may have in flight; default a quarter of the request pool, at least 2.
    pub user_max_inflight: u32,
    /// Seconds a `/v1` request may run before it answers 503.
    pub request_timeout_secs: u64,
    /// Seconds a request body may take to arrive before it answers 408.
    pub request_body_deadline_secs: u64,
    /// Seals integration credentials (`CREDENTIALS_KEY`, 64 hex). Optional: without it the API
    /// runs and refuses to store a credential.
    pub credentials_key: Option<CredentialKey>,
    /// Files bucket (`B2_UPLOADS_*`, the uploads bucket Laravel uses; keys under `genesy/`).
    /// Optional: without it file routes answer 503.
    pub storage: Option<S3Config>,
    /// Gotenberg sidecar base URL. Optional: without it PDF routes answer 503.
    pub gotenberg_url: Option<String>,
}

// Manual Debug: database_url carries DB password.
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("database_url", &"***")
            .field("db_max_connections", &self.db_max_connections)
            .field("bind_addr", &self.bind_addr)
            .field("token_ttl_days", &self.token_ttl_days)
            .field("frontend_url", &self.frontend_url)
            .field("api_public_url", &self.api_public_url)
            .field("smtp_url", &"***") // may carry SMTP credentials
            .field("mail_from", &self.mail_from)
            .field("rate_limit_burst", &self.rate_limit_burst)
            .field("rate_limit_replenish_secs", &self.rate_limit_replenish_secs)
            .field("trust_proxy_headers", &self.trust_proxy_headers)
            .field("invites_per_hour", &self.invites_per_hour)
            .field("private_rate_limit_burst", &self.private_rate_limit_burst)
            .field(
                "private_rate_limit_replenish_ms",
                &self.private_rate_limit_replenish_ms,
            )
            .field(
                "private_ip_rate_limit_burst",
                &self.private_ip_rate_limit_burst,
            )
            .field(
                "private_ip_rate_limit_replenish_ms",
                &self.private_ip_rate_limit_replenish_ms,
            )
            .field("workspace_max_inflight", &self.workspace_max_inflight)
            .field("user_max_inflight", &self.user_max_inflight)
            .field("request_timeout_secs", &self.request_timeout_secs)
            .field(
                "request_body_deadline_secs",
                &self.request_body_deadline_secs,
            )
            .field("credentials_key", &self.credentials_key)
            .field("storage", &self.storage.as_ref().map(|s| (&s.endpoint, &s.bucket)))
            .field("gotenberg_url", &self.gotenberg_url)
            .finish()
    }
}

impl Config {
    /// Build from process env.
    pub fn from_env() -> Result<Self, String> {
        let env = |key: &str| std::env::var(key).ok();
        let files = read_secret_files(env, |path| std::fs::read_to_string(path))?;
        Self::from_lookup(|key| files.get(key).cloned().or_else(|| env(key)))
    }

    // Lookup fn instead of env::var so tests skip global env mutation (unsafe in edition 2024).
    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let database_url = get("DATABASE_URL").ok_or("DATABASE_URL not set")?;
        // Empty counts as unset: compose passes `${GENESY_DATABASE_MAX_CONNECTIONS:-}`.
        let db_max_connections = match get("DATABASE_MAX_CONNECTIONS").filter(|v| !v.is_empty()) {
            None => DEFAULT_DB_MAX_CONNECTIONS,
            Some(raw) => raw
                .parse()
                .ok()
                .filter(|n| (1..=MAX_DB_MAX_CONNECTIONS).contains(n))
                .ok_or(format!(
                    "DATABASE_MAX_CONNECTIONS must be 1..={MAX_DB_MAX_CONNECTIONS}, got {raw:?}"
                ))?,
        };
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

        let frontend_url = base_url(&get, "FRONTEND_URL")?;
        let api_public_url = base_url(&get, "API_PUBLIC_URL")?;
        let smtp_url = get("SMTP_URL").ok_or("SMTP_URL not set")?;
        // Invite codes and SMTP credentials travel over this link: plain SMTP only on request.
        let smtp_tls = smtp_url.starts_with("smtps://") || query_has(&smtp_url, "tls=required");
        if !smtp_tls && !flag(&get, "SMTP_ALLOW_INSECURE")? {
            return Err("SMTP_URL must use TLS (smtps:// or ?tls=required); set \
                        SMTP_ALLOW_INSECURE=true only for a local Mailpit"
                .into());
        }
        let mail_from = get("MAIL_FROM").ok_or("MAIL_FROM not set")?;
        // Default 10 at once, then 1 per 6s: ~10 attempts a minute per IP.
        let rate_limit_burst = positive(&get, "RATE_LIMIT_BURST", 10)?;
        let rate_limit_replenish_secs = positive(&get, "RATE_LIMIT_REPLENISH_SECS", 6)?;
        let trust_proxy_headers = flag(&get, "TRUST_PROXY_HEADERS")?;
        let invites_per_hour = positive(&get, "INVITES_PER_HOUR", 20)?;
        // Default 120 at once, then 20 a second per session: a page load fires ~20 requests.
        let private_rate_limit_burst = positive(&get, "PRIVATE_RATE_LIMIT_BURST", 120)?;
        let private_rate_limit_replenish_ms =
            positive(&get, "PRIVATE_RATE_LIMIT_REPLENISH_MS", 50)?;
        // Default 600 at once, then 200 a second per IP: an office of users fits, a client
        // rotating made-up tokens doesn't.
        let private_ip_rate_limit_burst = positive(&get, "PRIVATE_IP_RATE_LIMIT_BURST", 600)?;
        let private_ip_rate_limit_replenish_ms =
            positive(&get, "PRIVATE_IP_RATE_LIMIT_REPLENISH_MS", 5)?;
        // Half the pool per workspace, a quarter per user (at least 2): one tenant's or one
        // user's slow requests leave the rest to everyone else.
        let workspace_max_inflight = positive(
            &get,
            "WORKSPACE_MAX_INFLIGHT",
            (db_max_connections / 2).max(1),
        )?;
        let user_max_inflight =
            positive(&get, "USER_MAX_INFLIGHT", (db_max_connections / 4).max(2))?;
        // A body (64 KiB) has 20s from arrival, item totals (2 MiB) 40s; both include the auth,
        // pool and slot waits before the body is read. The request timeout sits above the
        // longer one, so an upload in time is never cut by it.
        let request_timeout_secs: u64 = positive(&get, "REQUEST_TIMEOUT_SECS", 60)?;
        let request_body_deadline_secs: u64 = positive(&get, "REQUEST_BODY_DEADLINE_SECS", 20)?;
        if request_timeout_secs <= request_body_deadline_secs.saturating_mul(2) {
            return Err(format!(
                "REQUEST_TIMEOUT_SECS ({request_timeout_secs}) must be above twice \
                 REQUEST_BODY_DEADLINE_SECS ({request_body_deadline_secs})"
            ));
        }
        let credentials_key = get("CREDENTIALS_KEY")
            .filter(|v| !v.is_empty())
            .map(|mut v| {
                let key = CredentialKey::from_hex(&v);
                zeroize::Zeroize::zeroize(&mut v);
                key
            })
            .transpose()?;
        let storage = storage(&get)?;
        let gotenberg_url = optional_url(&get, "GOTENBERG_URL")?;

        Ok(Self {
            database_url,
            db_max_connections,
            bind_addr,
            token_ttl_days,
            frontend_url,
            api_public_url,
            smtp_url,
            mail_from,
            rate_limit_burst,
            rate_limit_replenish_secs,
            trust_proxy_headers,
            invites_per_hour,
            private_rate_limit_burst,
            private_rate_limit_replenish_ms,
            private_ip_rate_limit_burst,
            private_ip_rate_limit_replenish_ms,
            workspace_max_inflight,
            user_max_inflight,
            request_timeout_secs,
            request_body_deadline_secs,
            credentials_key,
            storage,
            gotenberg_url,
        })
    }
}

/// The uploads bucket: bucket, key id and key all set, or none (an empty secret file counts as
/// unset: deploy.sh creates one where there is no bucket).
fn storage(get: &impl Fn(&str) -> Option<String>) -> Result<Option<S3Config>, String> {
    let set = |k: &str| get(k).filter(|v| !v.trim().is_empty());
    let (bucket, key_id, secret) = (
        set("B2_UPLOADS_BUCKET"),
        set("B2_UPLOADS_KEY_ID"),
        set("B2_UPLOADS_APPLICATION_KEY"),
    );
    let (Some(bucket), Some(key_id), Some(secret)) = (&bucket, &key_id, &secret) else {
        if bucket.is_some() || key_id.is_some() || secret.is_some() {
            return Err("set B2_UPLOADS_BUCKET, B2_UPLOADS_KEY_ID and B2_UPLOADS_APPLICATION_KEY \
                        together, or none of them"
                .into());
        }
        return Ok(None);
    };
    let endpoint = optional_url(get, "B2_UPLOADS_ENDPOINT")?
        .unwrap_or_else(|| "https://s3.eu-central-003.backblazeb2.com".into());
    Ok(Some(S3Config {
        endpoint,
        region: set("B2_UPLOADS_REGION").unwrap_or_else(|| "eu-central-003".into()),
        bucket: bucket.clone(),
        key_id: key_id.clone(),
        secret: secret.clone(),
    }))
}

/// An optional http(s) URL without a trailing slash; blank = unset.
fn optional_url(get: &impl Fn(&str) -> Option<String>, key: &str) -> Result<Option<String>, String> {
    let Some(v) = get(key).filter(|v| !v.trim().is_empty()) else {
        return Ok(None);
    };
    let v = v.trim().trim_end_matches('/').to_owned();
    match crate::outbound::host_of(&v) {
        Some(_) if v.starts_with("http://") || v.starts_with("https://") => Ok(Some(v)),
        _ => Err(format!("{key} must be an http(s) URL, got {v:?}")),
    }
}

/// Vars that may come from `<KEY>_FILE` (Docker secrets), so passwords never sit in env files;
/// true = the file may be empty (an optional feature left off).
const SECRET_FILE_VARS: [(&str, bool); 4] = [
    ("DATABASE_URL", false),
    ("SMTP_URL", false),
    ("CREDENTIALS_KEY", false),
    ("B2_UPLOADS_APPLICATION_KEY", true),
];

/// Read each `<KEY>_FILE` that is set. Errors name the var and path, never the contents.
fn read_secret_files(
    get: impl Fn(&str) -> Option<String>,
    read: impl Fn(&str) -> std::io::Result<String>,
) -> Result<std::collections::HashMap<&'static str, String>, String> {
    let mut found = std::collections::HashMap::new();
    for (key, may_be_empty) in SECRET_FILE_VARS {
        let Some(path) = get(&format!("{key}_FILE")) else {
            continue;
        };
        if get(key).is_some() {
            return Err(format!("set {key} or {key}_FILE, not both"));
        }
        let value = read(&path).map_err(|e| format!("{key}_FILE unreadable at {path}: {e}"))?;
        let value = value.trim_end_matches(['\n', '\r']);
        if value.is_empty() {
            if may_be_empty {
                continue;
            }
            return Err(format!("{key}_FILE is empty ({path})"));
        }
        found.insert(key, value.to_owned());
    }
    Ok(found)
}

/// True when the URL's query string holds exactly `pair` (not as a substring of another pair,
/// and not somewhere in the userinfo).
fn query_has(url: &str, pair: &str) -> bool {
    url.split_once('?')
        .is_some_and(|(_, query)| query.split('&').any(|p| p == pair))
}

/// Optional `true|false` env var, default false.
fn flag(get: &impl Fn(&str) -> Option<String>, key: &str) -> Result<bool, String> {
    match get(key).as_deref() {
        None | Some("false") => Ok(false),
        Some("true") => Ok(true),
        Some(other) => Err(format!("{key} must be true|false, got {other:?}")),
    }
}

/// Required http(s) origin, trailing `/` trimmed so callers can append paths.
fn base_url(get: &impl Fn(&str) -> Option<String>, key: &str) -> Result<String, String> {
    let url = get(key).ok_or(format!("{key} not set"))?;
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(format!("{key} must start with http(s)://, got {url:?}"));
    }
    Ok(url.trim_end_matches('/').to_owned())
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
    const REQUIRED: [(&str, &str); 4] = [
        ("FRONTEND_URL", "https://app.test"),
        ("API_PUBLIC_URL", "https://api.test"),
        ("SMTP_URL", "smtps://smtp.test:465"),
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
    fn db_pool_size_defaults_and_bounds() {
        let with = |v: &'static str| {
            Config::from_lookup(lookup(&[
                ("DATABASE_URL", "postgres://x"),
                ("DATABASE_MAX_CONNECTIONS", v),
            ]))
        };
        let default = Config::from_lookup(lookup(&[("DATABASE_URL", "postgres://x")])).unwrap();
        assert_eq!(default.db_max_connections, 20);
        assert_eq!(with("50").unwrap().db_max_connections, 50);
        for bad in ["0", "-1", "abc", "101"] {
            let err = with(bad).unwrap_err();
            assert!(err.contains("DATABASE_MAX_CONNECTIONS"), "{bad}: {err}");
        }
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
        for var in ["FRONTEND_URL", "API_PUBLIC_URL", "SMTP_URL", "MAIL_FROM"] {
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
    fn api_public_url_trimmed_and_scheme_checked() {
        let cfg = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://x"),
            ("API_PUBLIC_URL", "https://api.genesy.app/"),
        ]))
        .unwrap();
        assert_eq!(cfg.api_public_url, "https://api.genesy.app");

        let err = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://x"),
            ("API_PUBLIC_URL", "api.genesy.app"),
        ]))
        .unwrap_err();
        assert!(err.starts_with("API_PUBLIC_URL must start"), "{err}");
    }

    #[test]
    fn smtp_url_must_use_tls_unless_insecure_allowed() {
        let plain = |extra: &'static [(&'static str, &'static str)]| {
            let mut pairs = vec![
                ("DATABASE_URL", "postgres://x"),
                ("SMTP_URL", "smtp://user:secret@smtp.test:587"),
            ];
            pairs.extend_from_slice(extra);
            Config::from_lookup(lookup(Box::leak(pairs.into_boxed_slice())))
        };

        let err = plain(&[]).unwrap_err();
        assert!(err.starts_with("SMTP_URL must use TLS"), "{err}");
        assert!(
            !err.contains("secret"),
            "error must not echo credentials: {err}"
        );
        assert!(plain(&[("SMTP_ALLOW_INSECURE", "true")]).is_ok());
        assert!(
            plain(&[("SMTP_ALLOW_INSECURE", "maybe")])
                .unwrap_err()
                .starts_with("SMTP_ALLOW_INSECURE")
        );

        // Only an exact `tls=required` query pair counts, not the substring anywhere.
        for bad in [
            "smtp://u:tls=required@smtp.test:587",
            "smtp://smtp.test:587?tls=requiredx",
            "smtp://smtp.test:587?notls=required",
            "smtp://smtp.test:587?tls=opportunistic&x=tls=required",
        ] {
            let err = Config::from_lookup(lookup(Box::leak(
                vec![("DATABASE_URL", "postgres://x"), ("SMTP_URL", bad)].into_boxed_slice(),
            )))
            .unwrap_err();
            assert!(err.starts_with("SMTP_URL must use TLS"), "{bad}: {err}");
        }

        for ok in [
            "smtps://smtp.test:465",
            "smtp://u:p@smtp.postmarkapp.com:587?tls=required",
            "smtp://smtp.test:587?pool=1&tls=required",
        ] {
            let cfg = Config::from_lookup(lookup(Box::leak(
                vec![("DATABASE_URL", "postgres://x"), ("SMTP_URL", ok)].into_boxed_slice(),
            )));
            assert!(cfg.is_ok(), "{ok}");
        }
    }

    #[test]
    fn debug_masks_smtp_url() {
        let cfg = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://x"),
            ("SMTP_URL", "smtps://user:smtpsecret@host:465"),
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
    fn private_limits_default_from_the_pool() {
        let cfg = Config::from_lookup(lookup(&[("DATABASE_URL", "postgres://x")])).unwrap();
        assert_eq!(
            (
                cfg.private_rate_limit_burst,
                cfg.private_rate_limit_replenish_ms,
                cfg.workspace_max_inflight
            ),
            (120, 50, 10),
            "half the default pool of 20"
        );
        assert_eq!(
            (
                cfg.user_max_inflight,
                cfg.private_ip_rate_limit_burst,
                cfg.private_ip_rate_limit_replenish_ms
            ),
            (5, 600, 5)
        );
        assert_eq!(
            (cfg.request_timeout_secs, cfg.request_body_deadline_secs),
            (60, 20)
        );
        let err = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://x"),
            ("REQUEST_TIMEOUT_SECS", "40"),
        ]))
        .unwrap_err();
        assert!(
            err.starts_with("REQUEST_TIMEOUT_SECS (40) must be above"),
            "{err}"
        );
        let cfg = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://x"),
            ("DATABASE_MAX_CONNECTIONS", "1"),
        ]))
        .unwrap();
        assert_eq!(
            (cfg.workspace_max_inflight, cfg.user_max_inflight),
            (1, 2),
            "never 0; a user keeps 2"
        );
        let cfg = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://x"),
            ("WORKSPACE_MAX_INFLIGHT", "4"),
            ("PRIVATE_RATE_LIMIT_BURST", "30"),
        ]))
        .unwrap();
        assert_eq!(
            (cfg.workspace_max_inflight, cfg.private_rate_limit_burst),
            (4, 30)
        );
        for key in [
            "WORKSPACE_MAX_INFLIGHT",
            "PRIVATE_RATE_LIMIT_BURST",
            "PRIVATE_RATE_LIMIT_REPLENISH_MS",
            "PRIVATE_IP_RATE_LIMIT_BURST",
            "USER_MAX_INFLIGHT",
            "REQUEST_TIMEOUT_SECS",
            "REQUEST_BODY_DEADLINE_SECS",
        ] {
            let err = Config::from_lookup(lookup(&[("DATABASE_URL", "postgres://x"), (key, "0")]))
                .unwrap_err();
            assert!(err.starts_with(key), "{key}: {err}");
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

    #[test]
    fn credentials_key_is_optional_and_never_echoed() {
        const HEX: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
        let with = |v: &'static str| {
            Config::from_lookup(lookup(&[
                ("DATABASE_URL", "postgres://x"),
                ("CREDENTIALS_KEY", v),
            ]))
        };
        assert!(with("").unwrap().credentials_key.is_none());
        let cfg = with(HEX).unwrap();
        assert!(cfg.credentials_key.is_some());
        assert!(!format!("{cfg:?}").contains("0001020304"), "{cfg:?}");
        let err = with("not-a-key-0001020304").unwrap_err();
        assert_eq!(err, "CREDENTIALS_KEY must be 64 hex characters (32 bytes)");
    }

    #[test]
    fn storage_is_all_or_nothing() {
        let base = [("DATABASE_URL", "postgres://x")];
        let none = Config::from_lookup(lookup(&base)).unwrap();
        assert!(none.storage.is_none() && none.gotenberg_url.is_none());
        let full = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://x"),
            ("B2_UPLOADS_BUCKET", "uploads"),
            ("B2_UPLOADS_KEY_ID", "kid"),
            ("B2_UPLOADS_APPLICATION_KEY", "s3cret"),
            ("GOTENBERG_URL", "http://gotenberg:3000/"),
        ]))
        .unwrap();
        let s = full.storage.as_ref().unwrap();
        assert_eq!(
            (s.endpoint.as_str(), s.region.as_str(), s.bucket.as_str()),
            ("https://s3.eu-central-003.backblazeb2.com", "eu-central-003", "uploads")
        );
        assert_eq!(full.gotenberg_url.as_deref(), Some("http://gotenberg:3000"));
        assert!(!format!("{full:?}").contains("s3cret"));
        let half = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://x"),
            ("B2_UPLOADS_BUCKET", "uploads"),
        ]))
        .unwrap_err();
        assert!(half.starts_with("set B2_UPLOADS_BUCKET"), "{half}");
        let bad = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://x"),
            ("GOTENBERG_URL", "gotenberg:3000"),
        ]))
        .unwrap_err();
        assert!(bad.starts_with("GOTENBERG_URL must be"), "{bad}");
    }
}

#[cfg(test)]
mod secret_file_tests {
    use super::*;
    use std::collections::HashMap;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        }
    }

    fn files(
        map: &'static [(&'static str, &'static str)],
    ) -> impl Fn(&str) -> std::io::Result<String> {
        move |path| {
            map.iter()
                .find(|(p, _)| *p == path)
                .map(|(_, v)| v.to_string())
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "missing"))
        }
    }

    #[test]
    fn reads_secret_file_and_trims_trailing_newline() {
        let got = read_secret_files(
            env(&[
                ("DATABASE_URL_FILE", "/run/secrets/db"),
                ("SMTP_URL_FILE", "/run/secrets/smtp"),
            ]),
            files(&[
                ("/run/secrets/db", "postgres://u:p@h/db\n"),
                ("/run/secrets/smtp", "smtps://h:465\r\n"),
            ]),
        )
        .unwrap();

        assert_eq!(
            got,
            HashMap::from([
                ("DATABASE_URL", "postgres://u:p@h/db".to_owned()),
                ("SMTP_URL", "smtps://h:465".to_owned()),
            ])
        );
    }

    #[test]
    fn an_optional_secret_file_may_be_empty() {
        let got = read_secret_files(
            env(&[("B2_UPLOADS_APPLICATION_KEY_FILE", "/run/secrets/b2")]),
            files(&[("/run/secrets/b2", "\n")]),
        )
        .unwrap();
        assert!(got.is_empty());
    }

    #[test]
    fn credentials_key_reads_from_its_file() {
        let got = read_secret_files(
            env(&[("CREDENTIALS_KEY_FILE", "/run/secrets/key")]),
            files(&[("/run/secrets/key", "abcd\n")]),
        )
        .unwrap();
        assert_eq!(got, HashMap::from([("CREDENTIALS_KEY", "abcd".to_owned())]));
    }

    #[test]
    fn no_file_vars_means_nothing_read() {
        assert!(read_secret_files(env(&[]), files(&[])).unwrap().is_empty());
    }

    #[test]
    fn plain_and_file_together_is_an_error() {
        let err = read_secret_files(
            env(&[
                ("DATABASE_URL", "postgres://x"),
                ("DATABASE_URL_FILE", "/run/secrets/db"),
            ]),
            files(&[("/run/secrets/db", "postgres://y")]),
        )
        .unwrap_err();

        assert!(err.contains("DATABASE_URL or DATABASE_URL_FILE"), "{err}");
    }

    #[test]
    fn unreadable_or_empty_file_names_the_var_not_the_contents() {
        let missing =
            read_secret_files(env(&[("SMTP_URL_FILE", "/nope")]), files(&[])).unwrap_err();
        let empty = read_secret_files(
            env(&[("SMTP_URL_FILE", "/run/secrets/smtp")]),
            files(&[("/run/secrets/smtp", "\n")]),
        )
        .unwrap_err();

        assert!(missing.starts_with("SMTP_URL_FILE"), "{missing}");
        assert!(empty.starts_with("SMTP_URL_FILE"), "{empty}");
    }
}
