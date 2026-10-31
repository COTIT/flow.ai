//! Voucher SQL. Every statement filters `workspace_id`; the code is selected only for the
//! household's own list and the submit that issued it.

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

/// A voucher with what a scan, a redeem or a void needs.
pub struct VoucherRow {
    pub id: Uuid,
    pub village_id: Uuid,
    pub household_id: Uuid,
    pub status: String,
    pub audit_required: bool,
    pub amount_minor: i64,
    pub currency_id: Uuid,
    pub currency: String,
    pub decimals: i16,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub redeemed_by: Option<Uuid>,
    pub redeemed_at: Option<DateTime<Utc>>,
    pub void_reason: Option<String>,
    pub household_label: String,
    pub household_captured_by: Option<Uuid>,
    pub household_verified_by: Option<Uuid>,
    pub survey_title_en: String,
    pub survey_title_sw: Option<String>,
    pub head_name: Option<String>,
    pub respondent_name: String,
    /// Name on the latest `redeemed` event.
    pub redeemer_name: Option<String>,
}

/// One voucher by `$where` (params `$1` workspace, `$2` code or id): the shared select.
macro_rules! voucher_by {
    ($db:expr, $where:literal, $ws:expr, $key:expr) => {
        sqlx::query_as!(
            VoucherRow,
            r#"SELECT v.id, v.village_id, v.household_id, v.status, v.audit_required, v.amount_minor,
                  v.currency_id, c.asset_type AS currency, c.decimals, v.issued_at, v.expires_at,
                  v.redeemed_by, v.redeemed_at, v.void_reason, h.label AS household_label,
                  h.captured_by AS household_captured_by, h.verified_by AS household_verified_by,
                  s.title_en AS survey_title_en, s.title_sw AS survey_title_sw,
                  (SELECT concat_ws(' ', p.given_name, p.family_name) FROM farm_household_members hm
                   JOIN farm_persons p ON p.id = hm.person_id WHERE hm.household_id = h.id AND hm.is_head
                   LIMIT 1) AS head_name,
                  (SELECT concat_ws(' ', p.given_name, p.family_name) FROM farm_persons p
                   WHERE p.id = r.person_id) AS "respondent_name!",
                  (SELECT e.actor_name FROM farm_voucher_events e WHERE e.voucher_id = v.id
                   AND e.kind = 'redeemed' ORDER BY e.occurred_at DESC LIMIT 1) AS redeemer_name
               FROM farm_vouchers v
               JOIN currencies c ON c.id = v.currency_id
               JOIN farm_households h ON h.id = v.household_id
               JOIN farm_surveys s ON s.id = v.survey_id
               JOIN farm_survey_responses r ON r.id = v.response_id
               "# + $where,
            $ws,
            $key
        )
        .fetch_optional($db)
        .await
    };
}

/// The voucher with this (normalised) code in this workspace.
pub async fn by_code(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    code: &str,
) -> Result<Option<VoucherRow>, sqlx::Error> {
    voucher_by!(db, "WHERE v.workspace_id = $1 AND v.code = $2", ws, code)
}

/// Same, with the voucher row locked: two redeems of one voucher run one after the other.
pub async fn lock_by_code(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    code: &str,
) -> Result<Option<VoucherRow>, sqlx::Error> {
    voucher_by!(
        db,
        "WHERE v.workspace_id = $1 AND v.code = $2 FOR UPDATE OF v",
        ws,
        code
    )
}

pub async fn lock_by_id(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<VoucherRow>, sqlx::Error> {
    voucher_by!(
        db,
        "WHERE v.workspace_id = $1 AND v.id = $2 FOR UPDATE OF v",
        ws,
        id
    )
}

pub async fn by_id(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<VoucherRow>, sqlx::Error> {
    voucher_by!(db, "WHERE v.workspace_id = $1 AND v.id = $2", ws, id)
}

/// True when the person is a member of the household.
pub async fn in_household(
    db: impl PgExecutor<'_>,
    household: Uuid,
    person: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM farm_household_members WHERE household_id = $1 AND person_id = $2) AS "e!""#,
        household,
        person
    )
    .fetch_one(db)
    .await
}

/// Record a lookup and count this account's lookups in the last minute (this one included).
pub async fn count_lookup(db: impl PgExecutor<'_>, actor: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"WITH pruned AS (
               DELETE FROM farm_voucher_lookups WHERE actor_id = $1 AND looked_up_at < now() - interval '1 hour'
           ), added AS (
               INSERT INTO farm_voucher_lookups (actor_id) VALUES ($1)
           )
           SELECT count(*) + 1 AS "n!" FROM farm_voucher_lookups
           WHERE actor_id = $1 AND looked_up_at > now() - interval '1 minute'"#,
        actor
    )
    .fetch_one(db)
    .await
}

pub async fn redeem(
    db: impl PgExecutor<'_>,
    id: Uuid,
    by: Uuid,
    id_type: &str,
) -> Result<DateTime<Utc>, sqlx::Error> {
    sqlx::query_scalar!(
        "UPDATE farm_vouchers SET status = 'redeemed', redeemed_by = $2, redeemed_at = now(),
             id_type_seen = $3, updated_at = now()
         WHERE id = $1 RETURNING redeemed_at AS \"redeemed_at!\"",
        id,
        by,
        id_type
    )
    .fetch_one(db)
    .await
}

pub async fn void(
    db: impl PgExecutor<'_>,
    id: Uuid,
    by: Uuid,
    reason: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE farm_vouchers SET status = 'void', voided_by = $2, voided_at = now(), void_reason = $3,
             updated_at = now() WHERE id = $1",
        id,
        by,
        reason
    )
    .execute(db)
    .await?;
    Ok(())
}

/// The household's own vouchers, code included.
pub struct MineRow {
    pub id: Uuid,
    pub survey_id: Uuid,
    pub survey_title_en: String,
    pub survey_title_sw: Option<String>,
    pub code: String,
    pub amount_minor: i64,
    pub currency_id: Uuid,
    pub currency: String,
    pub decimals: i16,
    pub status: String,
    pub expired: bool,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub redeemed_at: Option<DateTime<Utc>>,
}

pub async fn mine(
    db: impl PgExecutor<'_> + Copy,
    ws: Uuid,
    person: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<MineRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        MineRow,
        r#"SELECT v.id, v.survey_id, s.title_en AS survey_title_en, s.title_sw AS survey_title_sw, v.code,
                  v.amount_minor, v.currency_id, c.asset_type AS currency, c.decimals, v.status,
                  (v.status = 'issued' AND v.expires_at <= now()) AS "expired!",
                  v.issued_at, v.expires_at, v.redeemed_at
           FROM farm_vouchers v
           JOIN farm_surveys s ON s.id = v.survey_id
           JOIN currencies c ON c.id = v.currency_id
           WHERE v.workspace_id = $1 AND v.household_id IN (SELECT farm_person_households($2))
           ORDER BY v.issued_at DESC, v.id DESC LIMIT $3 OFFSET $4"#,
        ws,
        person,
        limit,
        offset
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_vouchers
           WHERE workspace_id = $1 AND household_id IN (SELECT farm_person_households($2))"#,
        ws,
        person
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// One step of a voucher's trail.
pub struct TrailRow {
    pub occurred_at: DateTime<Utc>,
    pub kind: String,
    pub actor_name: String,
    pub actor_role: String,
    pub detail: Value,
}

/// The voucher's trail, merged with its household, login, survey and answer history, filtered
/// for `level`: `full` (ops/admin), `staff` (field staff of the village) or `farmer`.
pub async fn trail(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    voucher: Uuid,
    level: &str,
) -> Result<Vec<TrailRow>, sqlx::Error> {
    sqlx::query_as!(
        TrailRow,
        r#"WITH v AS (SELECT * FROM farm_vouchers WHERE workspace_id = $1 AND id = $2),
                h AS (SELECT * FROM farm_households WHERE id = (SELECT household_id FROM v)),
                s AS (SELECT * FROM farm_surveys WHERE id = (SELECT survey_id FROM v)),
                r AS (SELECT * FROM farm_survey_responses WHERE id = (SELECT response_id FROM v)),
                trail AS (
                    SELECT h.captured_at AS at, 'household_registered' AS kind, h.captured_by AS who,
                           NULL::text AS role, '{}'::jsonb AS detail
                    FROM h WHERE $3 <> 'farmer' AND h.captured_by IS NOT NULL
                    UNION ALL
                    SELECT h.verified_at, 'household_verified', h.verified_by, NULL, '{}'::jsonb
                    FROM h WHERE $3 <> 'farmer' AND h.verified_by IS NOT NULL
                    UNION ALL
                    SELECT li.issued_at, 'login_' || li.kind, li.issued_by, NULL, '{}'::jsonb
                    FROM farm_login_issues li JOIN r ON li.person_id = r.person_id
                    WHERE $3 = 'full' AND li.workspace_id = $1
                    UNION ALL
                    SELECT s.published_at, 'survey_published', s.published_by, NULL, '{}'::jsonb
                    FROM s WHERE $3 <> 'farmer' AND s.published_by IS NOT NULL
                    UNION ALL
                    SELECT r.submitted_at, 'answered', r.captured_by, 'farmer', '{}'::jsonb FROM r
                )
           SELECT t.at AS "occurred_at!", t.kind AS "kind!",
                  coalesce((SELECT concat_ws(' ', u.first_name, u.last_name) FROM users u WHERE u.id = t.who),
                           'unknown account') AS "actor_name!",
                  coalesce(t.role, farm_role_label($1, t.who)) AS "actor_role!", t.detail AS "detail!"
           FROM trail t WHERE t.at IS NOT NULL
           UNION ALL
           SELECT e.occurred_at, e.kind, e.actor_name, e.actor_role,
                  CASE WHEN $3 = 'full' THEN e.detail
                       WHEN $3 = 'staff' THEN e.detail - 'audit_required' - 'needs_ops'
                       WHEN e.kind = 'voided' THEN e.detail
                       ELSE '{}'::jsonb END
           FROM farm_voucher_events e
           WHERE e.voucher_id = $2
             AND ($3 = 'full' OR ($3 = 'staff' AND e.kind <> 'refused')
                  OR ($3 = 'farmer' AND e.kind IN ('issued', 'redeemed', 'voided')))
           ORDER BY 1, 2"#,
        ws,
        voucher,
        level
    )
    .fetch_all(db)
    .await
}

/// One redemption, for cash reconciliation.
pub struct LogRow {
    pub voucher_id: Uuid,
    pub redeemed_at: DateTime<Utc>,
    pub redeemed_by: Uuid,
    pub redeemed_by_name: String,
    pub village_name: String,
    pub household_label: String,
    pub survey_title_en: String,
    pub survey_title_sw: Option<String>,
    pub amount_minor: i64,
    pub currency_id: Uuid,
    pub currency: String,
    pub decimals: i16,
    pub id_type_seen: String,
    pub audit_required: bool,
}

/// Redemptions on Tanzanian calendar days `from..=to`, newest first.
pub async fn log(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<LogRow>, sqlx::Error> {
    sqlx::query_as!(
        LogRow,
        r#"SELECT v.id AS voucher_id, v.redeemed_at AS "redeemed_at!", v.redeemed_by AS "redeemed_by!",
                  concat_ws(' ', u.first_name, u.last_name) AS "redeemed_by_name!", vi.name AS village_name,
                  h.label AS household_label, s.title_en AS survey_title_en, s.title_sw AS survey_title_sw,
                  v.amount_minor, v.currency_id, c.asset_type AS currency, c.decimals,
                  v.id_type_seen AS "id_type_seen!", v.audit_required
           FROM farm_vouchers v
           JOIN users u ON u.id = v.redeemed_by
           JOIN farm_villages vi ON vi.id = v.village_id
           JOIN farm_households h ON h.id = v.household_id
           JOIN farm_surveys s ON s.id = v.survey_id
           JOIN currencies c ON c.id = v.currency_id
           WHERE v.workspace_id = $1 AND v.status = 'redeemed'
             AND (v.redeemed_at AT TIME ZONE 'Africa/Dar_es_Salaam')::date BETWEEN $2 AND $3
           ORDER BY v.redeemed_at DESC
           LIMIT 5000"#,
        ws,
        from,
        to
    )
    .fetch_all(db)
    .await
}

/// Redemptions per officer per Tanzanian day and currency: count and bigint sum.
pub struct TotalRow {
    pub day: NaiveDate,
    pub redeemed_by: Uuid,
    pub redeemed_by_name: String,
    pub vouchers: i64,
    pub minor: i64,
    pub currency: String,
    pub decimals: i16,
}

pub async fn totals(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<TotalRow>, sqlx::Error> {
    sqlx::query_as!(
        TotalRow,
        r#"SELECT (v.redeemed_at AT TIME ZONE 'Africa/Dar_es_Salaam')::date AS "day!",
                  v.redeemed_by AS "redeemed_by!", concat_ws(' ', u.first_name, u.last_name) AS "redeemed_by_name!",
                  count(*) AS "vouchers!", sum(v.amount_minor)::bigint AS "minor!",
                  c.asset_type AS currency, c.decimals
           FROM farm_vouchers v
           JOIN users u ON u.id = v.redeemed_by
           JOIN currencies c ON c.id = v.currency_id
           WHERE v.workspace_id = $1 AND v.status = 'redeemed'
             AND (v.redeemed_at AT TIME ZONE 'Africa/Dar_es_Salaam')::date BETWEEN $2 AND $3
           GROUP BY 1, 2, 3, c.asset_type, c.decimals
           ORDER BY 1 DESC, 3"#,
        ws,
        from,
        to
    )
    .fetch_all(db)
    .await
}
