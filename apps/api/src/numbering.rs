//! Document numbers as Laravel DomainNumber printed them: prefix + zero-padded number +
//! suffix. The appendix is snapshotted on each document when it gets its number. Counter
//! rows (`document_counters`) hand out the numbers.

use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

/// A numbering series; documents of one series share one counter per workspace.
#[derive(Clone, Copy, Debug)]
pub enum Series {
    /// Invoices and credit notes (Laravel's MAX+1 over both).
    Invoice,
    /// Orders (office and web shop).
    Order,
}

impl Series {
    fn key(self) -> &'static str {
        match self {
            Self::Invoice => "invoice",
            Self::Order => "order",
        }
    }
}

/// Next number of `series` in the workspace: the counter row bumped, at least `start` (an
/// admin raises the start to move a series forward; lowering it never moves it back). The
/// row lock holds until commit, so call it last in the transaction that stores the number.
pub async fn next(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    series: Series,
    start: i64,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO document_counters (workspace_id, series, last_number) VALUES ($1, $2, $3)
         ON CONFLICT (workspace_id, series) DO UPDATE
            SET last_number = GREATEST(document_counters.last_number + 1, EXCLUDED.last_number),
                updated_at = now()
         RETURNING last_number",
        workspace_id,
        series.key(),
        start,
    )
    .fetch_one(conn)
    .await
}

/// `{prefix, digits, suffix}` of a numbering series (a settings section's numbering fields).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Appendix {
    pub prefix: Option<String>,
    pub digits: usize,
    pub suffix: Option<String>,
}

/// Highest start number settings accept; leaves room for the series to grow inside i64.
pub const MAX_START: i64 = 999_999_999_999;

/// Most digits a series pads to (settings validation: `digits` max 10).
const MAX_DIGITS: usize = 10;

impl Appendix {
    /// Numbering fields of a settings section; Laravel DomainNumber::from used 1 digit when unset.
    pub fn from_section(section: &Value) -> Self {
        let text = |key: &str| section.get(key).and_then(Value::as_str).map(str::to_owned);
        let digits = section
            .get("digits")
            .and_then(Value::as_u64)
            .map_or(1, |d| {
                usize::try_from(d)
                    .unwrap_or(MAX_DIGITS)
                    .clamp(1, MAX_DIGITS)
            });
        Self {
            prefix: text("prefix"),
            digits,
            suffix: text("suffix"),
        }
    }

    /// The snapshot stored on a document (`*_appendix`).
    pub fn to_json(&self) -> Value {
        json!({ "prefix": self.prefix, "digits": self.digits, "suffix": self.suffix })
    }

    /// Laravel `prefix . str_pad(number, digits, '0', STR_PAD_LEFT) . suffix`.
    pub fn format(&self, number: i64) -> String {
        format!(
            "{}{number:0>width$}{}",
            self.prefix.as_deref().unwrap_or(""),
            self.suffix.as_deref().unwrap_or(""),
            width = self.digits
        )
    }
}

/// `start_number` of a settings section within 1..=MAX_START; Laravel fell back to 1.
pub fn start_number(section: &Value) -> i64 {
    section
        .get("start_number")
        .and_then(Value::as_i64)
        .map_or(1, |n| n.clamp(1, MAX_START))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_domain_number() {
        let a = Appendix::from_section(&json!({ "prefix": "INV-", "digits": 5, "suffix": "-B" }));
        assert_eq!(a.format(42), "INV-00042-B");
        assert_eq!(
            a.format(123_456),
            "INV-123456-B",
            "longer than digits: no cut"
        );
        assert_eq!(
            a.to_json(),
            json!({ "prefix": "INV-", "digits": 5, "suffix": "-B" })
        );
    }

    #[test]
    fn missing_fields_fall_back_like_laravel() {
        let a = Appendix::from_section(&json!({ "prefix": null, "digits": null }));
        assert_eq!(a.format(7), "7");
        assert_eq!(
            a.to_json(),
            json!({ "prefix": null, "digits": 1, "suffix": null })
        );
        assert_eq!(start_number(&json!({ "start_number": null })), 1);
        assert_eq!(start_number(&json!({ "start_number": 1000 })), 1000);
    }

    async fn workspace(pool: &sqlx::PgPool, name: &str) -> Uuid {
        let user: Uuid = sqlx::query_scalar(
            "INSERT INTO users (email, first_name, last_name, password_hash)
             VALUES ($1, 'O', 'W', 'h') RETURNING id",
        )
        .bind(format!("{name}@x.io"))
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query_scalar(
            "INSERT INTO workspaces (name, created_by_user_id) VALUES ($2, $1) RETURNING id",
        )
        .bind(user)
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn bump(pool: &sqlx::PgPool, ws: Uuid, start: i64) -> i64 {
        let mut conn = pool.acquire().await.unwrap();
        next(&mut conn, ws, Series::Invoice, start).await.unwrap()
    }

    #[sqlx::test]
    async fn counter_seeds_bumps_and_only_moves_forward(pool: sqlx::PgPool) {
        let ws = workspace(&pool, "alpha").await;
        assert_eq!(
            bump(&pool, ws, 1000).await,
            1000,
            "first number is the start"
        );
        assert_eq!(bump(&pool, ws, 1000).await, 1001);
        assert_eq!(
            bump(&pool, ws, 5000).await,
            5000,
            "a raised start moves the series"
        );
        assert_eq!(
            bump(&pool, ws, 1).await,
            5001,
            "a lowered start never moves it back"
        );
        let other = workspace(&pool, "beta").await;
        assert_eq!(bump(&pool, other, 1).await, 1, "workspaces count apart");
    }

    #[sqlx::test]
    async fn rolled_back_number_is_given_again(pool: sqlx::PgPool) {
        let ws = workspace(&pool, "alpha").await;
        assert_eq!(bump(&pool, ws, 1).await, 1);
        let mut tx = pool.begin().await.unwrap();
        assert_eq!(next(&mut tx, ws, Series::Invoice, 1).await.unwrap(), 2);
        tx.rollback().await.unwrap();
        assert_eq!(bump(&pool, ws, 1).await, 2, "no gap");
    }

    #[test]
    fn junk_and_bounds_are_tamed() {
        // Validation caps digits at 10; an older or hand-edited row must not blow up.
        let a = Appendix::from_section(&json!({ "prefix": 5, "digits": 999, "suffix": [] }));
        assert_eq!(a.format(1), "0000000001");
        assert_eq!(start_number(&json!({ "start_number": -3 })), 1);
        assert_eq!(start_number(&json!("not an object")), 1);
        assert_eq!(
            start_number(&json!({ "start_number": i64::MAX })),
            MAX_START
        );
    }
}
