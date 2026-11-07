//! Disposable local SQLx database guard for RAM-only native exercises.

/// Allow SQLx's exact disposable name on the configured local admin server only.
pub(crate) fn database_guard(
    database: &str,
    pool: (&str, u16, &str),
    admin: (&str, u16, &str),
) -> Result<(), &'static str> {
    if database.len() != 63
        || !database.starts_with("_sqlx_test_")
        || !database
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        || !["127.0.0.1", "localhost"].contains(&pool.0)
        || pool != admin
    {
        return Err(
            "native exercise requires a disposable SQLx database on the exact local admin server",
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_disposable_sqlx_database_on_exact_loopback_admin_server_is_allowed() {
        let database = format!("_sqlx_test_{}", "a".repeat(52));
        let server = ("127.0.0.1", 54320, "postgres");
        assert!(database_guard(&database, server, server).is_ok());
        for name in [
            "postgres",
            "genesy",
            "genesy_assistant_checks_5dd5c09",
            "_sqlx_test_short",
        ] {
            assert!(database_guard(name, server, server).is_err());
        }
        assert!(
            database_guard(
                &database,
                ("remote.example", 54320, "postgres"),
                ("remote.example", 54320, "postgres")
            )
            .is_err()
        );
        assert!(database_guard(&database, server, ("127.0.0.1", 5432, "postgres")).is_err());
        assert!(database_guard(&database, server, ("127.0.0.1", 54320, "other")).is_err());
    }
}
