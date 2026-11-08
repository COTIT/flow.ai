//! Passbook and consent SQL. Passbook lines are insert-only (a trigger refuses the rest).

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::modules::farm_surveys::dto::Money;

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

/// Inclusive Tanzania days a passbook read keeps; `None` = no bound.
pub type Days = (Option<NaiveDate>, Option<NaiveDate>);

/// One page of a person's lines in `days`, newest first, and the count. The running balance is
/// summed per currency over all lines before the days and the page cut.
pub async fn entries(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    person_id: Uuid,
    (from, to): Days,
    limit: i64,
    offset: i64,
) -> Result<(Vec<EntryRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        EntryRow,
        r#"SELECT l.id AS "id!", l.kind AS "kind!", l.amount AS "amount!",
                  l.balance_after AS "balance_after!", l.currency_id AS "currency_id!",
                  l.currency AS "currency!", l.decimals AS "decimals!",
                  l.attribution_id, l.payout_id, l.created_at AS "created_at!"
           FROM (
               SELECT e.id, e.kind, e.amount,
                      sum(e.amount) OVER (PARTITION BY e.currency_id ORDER BY e.created_at, e.id)::bigint
                          AS balance_after,
                      e.currency_id, c.asset_type AS currency, c.decimals, e.attribution_id,
                      e.payout_id, e.created_at
               FROM farm_passbook_entries e
               JOIN currencies c ON c.workspace_id = e.workspace_id AND c.id = e.currency_id
               WHERE e.workspace_id = $1 AND e.person_id = $2
           ) l
           WHERE ($3::date IS NULL OR (l.created_at AT TIME ZONE 'Africa/Dar_es_Salaam')::date >= $3)
             AND ($4::date IS NULL OR (l.created_at AT TIME ZONE 'Africa/Dar_es_Salaam')::date <= $4)
           ORDER BY l.created_at DESC, l.id DESC LIMIT $5 OFFSET $6"#,
        workspace_id,
        person_id,
        from,
        to,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_passbook_entries
           WHERE workspace_id = $1 AND person_id = $2
             AND ($3::date IS NULL OR (created_at AT TIME ZONE 'Africa/Dar_es_Salaam')::date >= $3)
             AND ($4::date IS NULL OR (created_at AT TIME ZONE 'Africa/Dar_es_Salaam')::date <= $4)"#,
        workspace_id,
        person_id,
        from,
        to,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// A person's passport consent as stored.
pub async fn consent(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    person_id: Uuid,
) -> Result<ConsentRow, sqlx::Error> {
    sqlx::query_as!(
        ConsentRow,
        "SELECT id, passport_profile_consent, passport_profile_consent_at,
                passport_profile_consent_by
         FROM farm_persons WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        person_id,
    )
    .fetch_one(db)
    .await
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

/// A person's balance per currency, by currency code (currencies with lines only).
pub async fn balances(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    person_id: Uuid,
) -> Result<Vec<Money>, sqlx::Error> {
    sqlx::query_as!(
        Money,
        r#"SELECT sum(e.amount)::bigint AS "minor!", c.asset_type AS currency, c.decimals,
                  e.currency_id
           FROM farm_passbook_entries e
           JOIN currencies c ON c.workspace_id = e.workspace_id AND c.id = e.currency_id
           WHERE e.workspace_id = $1 AND e.person_id = $2
           GROUP BY e.currency_id, c.asset_type, c.decimals
           ORDER BY c.asset_type"#,
        workspace_id,
        person_id,
    )
    .fetch_all(db)
    .await
}

/// One farmer owed money in one currency.
pub struct OwedRow {
    pub person_id: Uuid,
    pub name: String,
    pub village_id: Uuid,
    pub village: String,
    pub balance: Money,
}

/// Farmers with a positive balance in the caller's villages (`village` narrows), largest first,
/// the count, and the total owed per currency over every page. All sums run in SQL.
pub async fn owed(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    all_villages: bool,
    villages: &[Uuid],
    village: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<OwedRow>, i64, Vec<Money>), sqlx::Error> {
    let rows = sqlx::query!(
        r#"WITH owed AS (
               SELECT e.person_id, e.currency_id, sum(e.amount)::bigint AS minor
               FROM farm_passbook_entries e
               JOIN farm_persons p ON p.workspace_id = e.workspace_id AND p.id = e.person_id
               WHERE e.workspace_id = $1
                 AND ($2 OR p.village_id = ANY($3)) AND ($4::uuid IS NULL OR p.village_id = $4)
               GROUP BY e.person_id, e.currency_id
               HAVING sum(e.amount) > 0
           )
           SELECT o.person_id AS "person_id!", p.given_name || ' ' || p.family_name AS "name!", p.village_id,
                  v.name AS village, o.minor AS "minor!", c.asset_type AS currency, c.decimals,
                  o.currency_id AS "currency_id!", count(*) OVER () AS "total!"
           FROM owed o
           JOIN farm_persons p ON p.workspace_id = $1 AND p.id = o.person_id
           JOIN farm_villages v ON v.workspace_id = $1 AND v.id = p.village_id
           JOIN currencies c ON c.workspace_id = $1 AND c.id = o.currency_id
           ORDER BY o.minor DESC, 2, o.person_id
           LIMIT $5 OFFSET $6"#,
        workspace_id,
        all_villages,
        villages,
        village,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| OwedRow {
            person_id: r.person_id,
            name: r.name,
            village_id: r.village_id,
            village: r.village,
            balance: Money {
                minor: r.minor,
                currency: r.currency,
                decimals: r.decimals,
                currency_id: r.currency_id,
            },
        })
        .collect();
    let totals = sqlx::query_as!(
        Money,
        r#"WITH owed AS (
               SELECT e.currency_id, sum(e.amount)::bigint AS minor
               FROM farm_passbook_entries e
               JOIN farm_persons p ON p.workspace_id = e.workspace_id AND p.id = e.person_id
               WHERE e.workspace_id = $1
                 AND ($2 OR p.village_id = ANY($3)) AND ($4::uuid IS NULL OR p.village_id = $4)
               GROUP BY e.person_id, e.currency_id
               HAVING sum(e.amount) > 0
           )
           SELECT sum(o.minor)::bigint AS "minor!", c.asset_type AS currency, c.decimals,
                  o.currency_id AS "currency_id!"
           FROM owed o
           JOIN currencies c ON c.workspace_id = $1 AND c.id = o.currency_id
           GROUP BY o.currency_id, c.asset_type, c.decimals
           ORDER BY c.asset_type"#,
        workspace_id,
        all_villages,
        villages,
        village,
    )
    .fetch_all(db)
    .await?;
    Ok((rows, total, totals))
}
