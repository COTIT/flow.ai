//! Passbook and consent SQL. Passbook lines are insert-only (a trigger refuses the rest).

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

/// One passbook line with its currency and the balance after it.
pub struct EntryRow {
    pub id: Uuid,
    pub kind: String,
    pub amount: i64,
    pub balance_after: i64,
    pub currency_id: Uuid,
    pub currency: String,
    pub decimals: i16,
    pub attribution_id: Option<Uuid>,
    pub payout_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// A person's consent state.
pub struct ConsentRow {
    pub id: Uuid,
    pub passport_profile_consent: bool,
    pub passport_profile_consent_at: Option<DateTime<Utc>>,
    pub passport_profile_consent_by: Option<Uuid>,
}

/// Village of a live person of the workspace.
pub async fn person_village(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    person_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT village_id FROM farm_persons
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL",
        workspace_id,
        person_id,
    )
    .fetch_optional(db)
    .await
}

/// One page of a person's lines, newest first, and the count. The running balance is summed
/// per currency over all lines before the page is cut.
pub async fn entries(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    person_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<EntryRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        EntryRow,
        r#"SELECT e.id, e.kind, e.amount,
                  sum(e.amount) OVER (PARTITION BY e.currency_id ORDER BY e.created_at, e.id)::bigint
                      AS "balance_after!",
                  e.currency_id, c.asset_type AS currency, c.decimals, e.attribution_id,
                  e.payout_id, e.created_at
           FROM farm_passbook_entries e
           JOIN currencies c ON c.workspace_id = e.workspace_id AND c.id = e.currency_id
           WHERE e.workspace_id = $1 AND e.person_id = $2
           ORDER BY e.created_at DESC, e.id DESC LIMIT $3 OFFSET $4"#,
        workspace_id,
        person_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_passbook_entries
           WHERE workspace_id = $1 AND person_id = $2"#,
        workspace_id,
        person_id,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// Set or clear a person's passport consent, stamped with who and when.
pub async fn set_consent(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    person_id: Uuid,
    consent: bool,
    by: Uuid,
) -> Result<ConsentRow, sqlx::Error> {
    sqlx::query_as!(
        ConsentRow,
        "UPDATE farm_persons
         SET passport_profile_consent = $3,
             passport_profile_consent_at = CASE WHEN $3 THEN now() END,
             passport_profile_consent_by = CASE WHEN $3 THEN $4::uuid END,
             updated_at = now()
         WHERE workspace_id = $1 AND id = $2
         RETURNING id, passport_profile_consent, passport_profile_consent_at,
                   passport_profile_consent_by",
        workspace_id,
        person_id,
        consent,
        by,
    )
    .fetch_one(db)
    .await
}
