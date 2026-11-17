//! Connection probe runner and the status rule (contracts §3.6): probe checks plus the
//! user-entered credential expiry decide a connection's status and reason.

use chrono::{DateTime, Duration, Utc};

use crate::integrations::{
    capability::{CapabilityCheck, ConnectionProbe, ErrorClass, ProviderError, keys_of},
    connections::repo::ConnectionRow,
    providers::{ConnCtx, SinkClient, SourceClient},
};

/// Days before expiry a connection turns `degraded`.
pub const EXPIRY_WARNING_DAYS: i64 = 14;

/// Probe every capability of `row`. A client that can't be built fails every key with its error.
pub async fn run(ctx: &ConnCtx, row: &ConnectionRow) -> Vec<CapabilityCheck> {
    let tested = if row.provider == "odoo" {
        match SourceClient::for_connection(ctx, row) {
            Ok(c) => Ok(c.test().await),
            Err(e) => Err(e),
        }
    } else {
        match SinkClient::for_connection(ctx, row) {
            Ok(c) => Ok(c.test().await),
            Err(e) => Err(e),
        }
    };
    tested.unwrap_or_else(|e: ProviderError| {
        keys_of(&row.provider)
            .iter()
            .map(|key| CapabilityCheck {
                key: (*key).to_owned(),
                ok: false,
                error_class: Some(e.class),
                detail: Some(e.detail.clone()),
            })
            .collect()
    })
}

/// `expired` past `expires_at`; `degraded` within 14 days of it.
pub fn expiry(
    expires_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Option<(&'static str, &'static str)> {
    let at = expires_at?;
    if at < now {
        Some(("expired", "credential_expired"))
    } else if at <= now + Duration::days(EXPIRY_WARNING_DAYS) {
        Some(("degraded", "credential_expiring"))
    } else {
        None
    }
}

/// Status + reason after a probe. Order: expired, auth, permission, any failed check,
/// expiring, active.
pub fn status(
    checks: &[CapabilityCheck],
    expires_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> (&'static str, Option<String>) {
    let soon = expiry(expires_at, now);
    if let Some(("expired", reason)) = soon {
        return ("expired", Some(reason.into()));
    }
    let failed = |class| checks.iter().any(|c| !c.ok && c.error_class == Some(class));
    if failed(ErrorClass::Auth) {
        return ("unauthorized", Some("auth_failed".into()));
    }
    if failed(ErrorClass::Permission) {
        return ("degraded", Some("permission_denied".into()));
    }
    if let Some(c) = checks.iter().find(|c| !c.ok) {
        return ("degraded", Some(format!("probe_failed:{}", c.key)));
    }
    match soon {
        Some((status, reason)) => (status, Some(reason.into())),
        None => ("active", None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(key: &str, class: Option<ErrorClass>) -> CapabilityCheck {
        CapabilityCheck {
            key: key.into(),
            ok: class.is_none(),
            error_class: class,
            detail: None,
        }
    }

    #[test]
    fn status_rule_order() {
        let now = Utc::now();
        let ok = [check("a", None), check("b", None)];
        assert_eq!(status(&ok, None, now), ("active", None));
        let far = Some(now + Duration::days(15));
        assert_eq!(status(&ok, far, now).0, "active");
        let edge = Some(now + Duration::days(14));
        assert_eq!(
            status(&ok, edge, now),
            ("degraded", Some("credential_expiring".into()))
        );
        let past = Some(now - Duration::seconds(1));
        let auth = [check("a", Some(ErrorClass::Auth))];
        assert_eq!(
            status(&auth, past, now),
            ("expired", Some("credential_expired".into()))
        );
        assert_eq!(
            status(&auth, edge, now),
            ("unauthorized", Some("auth_failed".into()))
        );
        let mixed = [
            check("a", Some(ErrorClass::Transient)),
            check("b", Some(ErrorClass::Permission)),
        ];
        assert_eq!(
            status(&mixed, None, now),
            ("degraded", Some("permission_denied".into()))
        );
        let one = [check("a", None), check("b", Some(ErrorClass::NotFound))];
        assert_eq!(
            status(&one, None, now),
            ("degraded", Some("probe_failed:b".into()))
        );
    }
}
