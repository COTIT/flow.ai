//! Integration credentials of the kept providers (SMTP, Mollie, PAY.nl, Graph): which keys
//! are stored (sealed by `crypto`, in `workspace_credentials`), what GET prints for them, and
//! the fields a kept one is bound to.

/// What GET prints for a stored credential; sent back unchanged it keeps the stored one.
pub const MASK: &str = "********";

/// 422 text when a credential is sent but no `CREDENTIALS_KEY` is configured.
pub const NO_KEY: &str = "Credential storage isn't configured.";

/// 422 text when the mask is sent but nothing is stored, or the fields it is bound to changed.
pub const RE_ENTER: &str = "Enter this credential again.";

/// Stored credential paths (dotted, from the section name). The dropped providers' keys are
/// not here: they stay refused.
pub const PATHS: &[&str] = &[
    "smtp.password",
    "integrations.payment.payment_providers.mollie.api_key",
    "integrations.payment.payment_providers.pay.api_code",
    "integrations.payment.payment_providers.pay.api_token",
    "integrations.payment.payment_providers.pay.service_id",
    "integrations.mail.graph.client_secret",
];

/// Fields that say where a credential is sent: the mask keeps it only while they stay as
/// stored, so a member can't point a kept password at their own server. Mollie, PAY.nl and
/// Graph send theirs to fixed provider hosts.
pub const BOUND: &[(&str, &[&str])] = &[(
    "smtp.password",
    &["smtp.host", "smtp.port", "smtp.username", "smtp.encryption"],
)];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_check_lists_every_path() {
        let migration = include_str!("../../../migrations/20261021151609_credentials.up.sql");
        for path in PATHS {
            assert!(migration.contains(&format!("'{path}'")), "{path}");
        }
        assert_eq!(
            migration.matches("'smtp.").count() + migration.matches("'integrations.").count(),
            PATHS.len()
        );
    }
}
