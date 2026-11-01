//! Survey SQL. Every statement filters `workspace_id`.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

/// A survey with its money, publisher and questions.
pub struct SurveyRow {
    pub id: Uuid,
    pub village_id: Option<Uuid>,
    pub title_en: String,
    pub title_sw: Option<String>,
    pub description_en: Option<String>,
    pub description_sw: Option<String>,
    pub reward_minor: i64,
    pub currency_id: Uuid,
    pub currency: String,
    pub decimals: i16,
    pub max_households: Option<i32>,
    pub audit_percent: i16,
    pub closes_at: Option<DateTime<Utc>>,
    pub status: String,
    pub published_at: Option<DateTime<Utc>>,
    pub published_by: Option<Value>,
    pub closed_at: Option<DateTime<Utc>>,
    pub created_by: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Voucher sums as on `/results`; None unless `SurveyScope::staff`.
    pub summary: Option<Value>,
    /// `[{id, position, kind, prompt_en, prompt_sw, required, options}]`, live questions by position.
    pub questions: Value,
    /// Households that answered.
    pub answered: i64,
}

/// Which surveys: managers all; others live/closed ones for every village or one of theirs.
pub struct SurveyScope<'a> {
    pub ws: Uuid,
    pub all: bool,
    pub villages: &'a [Uuid],
    /// Field staff and managers also get the voucher sums.
    pub staff: bool,
}

/// Visible surveys (`id` Some = that one), newest first, and the count.
pub async fn surveys(
    db: &sqlx::PgPool,
    s: &SurveyScope<'_>,
    id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<SurveyRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        SurveyRow,
        r#"SELECT s.id, s.village_id, s.title_en, s.title_sw, s.description_en, s.description_sw,
                  s.reward_minor, s.currency_id, c.asset_type AS currency, c.decimals,
                  s.max_households, s.audit_percent, s.closes_at, s.status, s.published_at,
                  (SELECT jsonb_build_object('id', u.id, 'name', concat_ws(' ', u.first_name, u.last_name))
                   FROM users u WHERE u.id = s.published_by) AS published_by,
                  s.closed_at,
                  (SELECT jsonb_build_object('id', u.id, 'name', concat_ws(' ', u.first_name, u.last_name))
                   FROM users u WHERE u.id = s.created_by) AS created_by,
                  s.created_at, s.updated_at,
                  -- Same sums as summary() below; CASE skips the subquery for farmers.
                  CASE WHEN $7 THEN (
                      SELECT jsonb_build_object(
                          'issued', jsonb_build_object('count', count(v.id),
                                                       'minor', coalesce(sum(v.amount_minor), 0)::bigint),
                          'redeemed', jsonb_build_object(
                              'count', count(v.id) FILTER (WHERE v.status = 'redeemed'),
                              'minor', coalesce(sum(v.amount_minor) FILTER (WHERE v.status = 'redeemed'), 0)::bigint),
                          'outstanding', jsonb_build_object(
                              'count', count(v.id) FILTER (WHERE v.status = 'issued' AND v.expires_at > now()),
                              'minor', coalesce(sum(v.amount_minor)
                                           FILTER (WHERE v.status = 'issued' AND v.expires_at > now()), 0)::bigint),
                          'expired_count', count(v.id) FILTER (WHERE v.status = 'issued' AND v.expires_at <= now()),
                          'void_count', count(v.id) FILTER (WHERE v.status = 'void'))
                      FROM farm_survey_responses r
                      LEFT JOIN farm_vouchers v ON v.response_id = r.id
                      WHERE r.survey_id = s.id AND r.deleted_at IS NULL) END AS summary,
                  coalesce((SELECT jsonb_agg(jsonb_build_object('id', q.id, 'position', q.position, 'kind', q.kind,
                                     'prompt_en', q.prompt_en, 'prompt_sw', q.prompt_sw, 'required', q.required,
                                     'options', q.options) ORDER BY q.position, q.id)
                            FROM farm_survey_questions q
                            WHERE q.survey_id = s.id AND q.deleted_at IS NULL), '[]') AS "questions!",
                  (SELECT count(*) FROM farm_survey_responses r
                   WHERE r.survey_id = s.id AND r.deleted_at IS NULL) AS "answered!"
           FROM farm_surveys s
           JOIN currencies c ON c.id = s.currency_id
           WHERE s.workspace_id = $1 AND ($4::uuid IS NULL OR s.id = $4)
             AND ($2 OR (s.status <> 'draft' AND (s.village_id IS NULL OR s.village_id = ANY($3))))
           ORDER BY s.created_at DESC, s.id DESC LIMIT $5 OFFSET $6"#,
        s.ws,
        s.all,
        s.villages,
        id,
        limit,
        offset,
        s.staff,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_surveys s
           WHERE s.workspace_id = $1 AND ($4::uuid IS NULL OR s.id = $4)
             AND ($2 OR (s.status <> 'draft' AND (s.village_id IS NULL OR s.village_id = ANY($3))))"#,
        s.ws,
        s.all,
        s.villages,
        id,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// Survey fields as authored.
pub struct SurveyCols<'a> {
    pub village_id: Option<Uuid>,
    pub title_en: &'a str,
    pub title_sw: Option<&'a str>,
    pub description_en: Option<&'a str>,
    pub description_sw: Option<&'a str>,
    pub reward_minor: i64,
    pub currency_id: Uuid,
    pub max_households: Option<i32>,
    pub audit_percent: i16,
    pub closes_at: Option<DateTime<Utc>>,
}

/// `id` None = a new uuidv7.
pub async fn insert(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    by: Uuid,
    id: Option<Uuid>,
    c: &SurveyCols<'_>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO farm_surveys (id, workspace_id, village_id, title_en, title_sw, description_en, description_sw,
                                   reward_minor, currency_id, max_households, audit_percent, closes_at, created_by)
         VALUES (coalesce($13, uuidv7()), $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) RETURNING id",
        ws,
        c.village_id,
        c.title_en,
        c.title_sw,
        c.description_en,
        c.description_sw,
        c.reward_minor,
        c.currency_id,
        c.max_households,
        c.audit_percent,
        c.closes_at,
        by,
        id,
    )
    .fetch_one(db)
    .await
}

pub async fn update(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    c: &SurveyCols<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE farm_surveys SET village_id = $3, title_en = $4, title_sw = $5, description_en = $6,
             description_sw = $7, reward_minor = $8, currency_id = $9, max_households = $10,
             audit_percent = $11, closes_at = $12, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        c.village_id,
        c.title_en,
        c.title_sw,
        c.description_en,
        c.description_sw,
        c.reward_minor,
        c.currency_id,
        c.max_households,
        c.audit_percent,
        c.closes_at,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Status of a survey, locked.
pub struct Lock {
    pub status: String,
}

pub async fn lock(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<Lock>, sqlx::Error> {
    sqlx::query_as!(
        Lock,
        "SELECT status FROM farm_surveys WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
        ws,
        id
    )
    .fetch_optional(db)
    .await
}

pub async fn publish(db: impl PgExecutor<'_>, id: Uuid, by: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE farm_surveys SET status = 'live', published_at = clock_timestamp(), published_by = $2,
             closed_at = NULL, updated_at = now() WHERE id = $1",
        id,
        by
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn close(db: impl PgExecutor<'_>, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE farm_surveys SET status = 'closed', closed_at = clock_timestamp(), updated_at = now() WHERE id = $1",
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// A currency of this workspace, not deleted.
pub async fn currency_exists(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM currencies WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL) AS "e!""#,
        ws,
        id
    )
    .fetch_one(db)
    .await
}

/// Question fields as authored.
pub struct QuestionCols<'a> {
    pub position: i32,
    pub kind: &'a str,
    pub prompt_en: &'a str,
    pub prompt_sw: Option<&'a str>,
    pub required: bool,
    pub options: &'a Value,
}

pub async fn insert_question(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    survey: Uuid,
    q: &QuestionCols<'_>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO farm_survey_questions (workspace_id, survey_id, position, kind, prompt_en, prompt_sw, required, options)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
        ws,
        survey,
        q.position,
        q.kind,
        q.prompt_en,
        q.prompt_sw,
        q.required,
        q.options,
    )
    .fetch_one(db)
    .await
}

/// Replace a live question's fields; false when no such question on this survey.
pub async fn update_question(
    db: impl PgExecutor<'_>,
    survey: Uuid,
    id: Uuid,
    q: &QuestionCols<'_>,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE farm_survey_questions SET position = $3, kind = $4, prompt_en = $5, prompt_sw = $6,
             required = $7, options = $8, updated_at = now()
         WHERE survey_id = $1 AND id = $2 AND deleted_at IS NULL",
        survey,
        id,
        q.position,
        q.kind,
        q.prompt_en,
        q.prompt_sw,
        q.required,
        q.options,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

pub async fn delete_question(
    db: impl PgExecutor<'_>,
    survey: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE farm_survey_questions SET deleted_at = now(), updated_at = now()
         WHERE survey_id = $1 AND id = $2 AND deleted_at IS NULL",
        survey,
        id
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// The caller's live households, with what eligibility needs per survey.
pub struct HouseholdRow {
    pub id: Uuid,
    pub village_id: Uuid,
    pub verification: String,
    pub captured_by: Option<Uuid>,
    pub verified_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    /// Surveys this household answered.
    pub answered: Vec<Uuid>,
}

pub async fn households_of(
    db: impl PgExecutor<'_>,
    person: Uuid,
) -> Result<Vec<HouseholdRow>, sqlx::Error> {
    sqlx::query_as!(
        HouseholdRow,
        r#"SELECT h.id, h.village_id, h.verification, h.captured_by, h.verified_by, h.created_at,
                  ARRAY(SELECT r.survey_id FROM farm_survey_responses r
                        WHERE r.household_id = h.id AND r.deleted_at IS NULL) AS "answered!"
           FROM farm_household_members m
           JOIN farm_households h ON h.id = m.household_id AND h.deleted_at IS NULL
           WHERE m.person_id = $1"#,
        person
    )
    .fetch_all(db)
    .await
}

/// The account must still replace its temporary password.
pub async fn must_change_password(
    db: impl PgExecutor<'_>,
    user: Uuid,
) -> Result<bool, sqlx::Error> {
    Ok(
        sqlx::query_scalar!("SELECT must_change_password FROM users WHERE id = $1", user)
            .fetch_optional(db)
            .await?
            .unwrap_or(false),
    )
}

/// The household's response (and voucher) per survey, for the eligibility list.
pub struct Answered {
    pub survey_id: Uuid,
    pub response_id: Uuid,
    pub voucher_id: Option<Uuid>,
}

pub async fn answered_by(
    db: impl PgExecutor<'_>,
    households: &[Uuid],
) -> Result<Vec<Answered>, sqlx::Error> {
    sqlx::query_as!(
        Answered,
        r#"SELECT r.survey_id, r.id AS response_id, v.id AS "voucher_id?"
           FROM farm_survey_responses r LEFT JOIN farm_vouchers v ON v.response_id = r.id
           WHERE r.household_id = ANY($1) AND r.deleted_at IS NULL"#,
        households
    )
    .fetch_all(db)
    .await
}

/// An earlier submit with this client_ref, and who made it.
pub struct Replay {
    pub captured_by: Option<Uuid>,
    pub response_id: Uuid,
    pub voucher_id: Uuid,
    pub code: String,
    pub amount_minor: i64,
    pub currency_id: Uuid,
    pub currency: String,
    pub decimals: i16,
    pub expires_at: DateTime<Utc>,
}

pub async fn replay(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    client_ref: Uuid,
) -> Result<Option<Replay>, sqlx::Error> {
    sqlx::query_as!(
        Replay,
        r#"SELECT r.captured_by, r.id AS response_id, v.id AS voucher_id, v.code, v.amount_minor,
                  v.currency_id, c.asset_type AS currency, c.decimals, v.expires_at
           FROM farm_survey_responses r
           JOIN farm_vouchers v ON v.response_id = r.id
           JOIN currencies c ON c.id = v.currency_id
           WHERE r.workspace_id = $1 AND r.client_ref = $2"#,
        ws,
        client_ref
    )
    .fetch_optional(db)
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_response(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    village: Uuid,
    survey: Uuid,
    household: Uuid,
    person: Uuid,
    client_ref: Uuid,
    by: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO farm_survey_responses (workspace_id, village_id, survey_id, household_id, person_id,
                                            client_ref, source, captured_by)
         VALUES ($1, $2, $3, $4, $5, $6, 'farmer_reported', $7) RETURNING id",
        ws,
        village,
        survey,
        household,
        person,
        client_ref,
        by,
    )
    .fetch_one(db)
    .await
}

pub async fn insert_answers(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    response: Uuid,
    questions: &[Uuid],
    values: &[Value],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO farm_survey_answers (workspace_id, response_id, question_id, value)
         SELECT $1, $2, q, v FROM unnest($3::uuid[], $4::jsonb[]) AS a(q, v)",
        ws,
        response,
        questions,
        values,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// A new voucher.
pub struct NewVoucher<'a> {
    pub ws: Uuid,
    pub village: Uuid,
    pub response: Uuid,
    pub survey: Uuid,
    pub household: Uuid,
    pub code: &'a str,
    pub amount_minor: i64,
    pub currency_id: Uuid,
    pub audit_required: bool,
}

/// Insert; None when the code is already taken (draw again). Returns (id, expires_at).
pub async fn insert_voucher(
    db: impl PgExecutor<'_>,
    v: &NewVoucher<'_>,
) -> Result<Option<(Uuid, DateTime<Utc>)>, sqlx::Error> {
    let row = sqlx::query!(
        "INSERT INTO farm_vouchers (workspace_id, village_id, response_id, survey_id, household_id, code,
                                    amount_minor, currency_id, audit_required, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now() + interval '30 days')
         ON CONFLICT (workspace_id, code) DO NOTHING
         RETURNING id, expires_at",
        v.ws,
        v.village,
        v.response,
        v.survey,
        v.household,
        v.code,
        v.amount_minor,
        v.currency_id,
        v.audit_required,
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| (r.id, r.expires_at)))
}

/// Append one voucher event.
#[allow(clippy::too_many_arguments)]
pub async fn insert_event(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    voucher: Uuid,
    kind: &str,
    actor: Uuid,
    actor_role: &str,
    detail: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO farm_voucher_events (workspace_id, voucher_id, kind, actor_id, actor_name, actor_role, detail)
         SELECT $1, $2, $3, $4, coalesce(concat_ws(' ', u.first_name, u.last_name), 'unknown account'), $5, $6
         FROM (SELECT 1) one LEFT JOIN users u ON u.id = $4",
        ws,
        voucher,
        kind,
        actor,
        actor_role,
        detail,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Counts and money (bigint minor units) of a survey's vouchers.
pub struct Summary {
    pub responses: i64,
    pub issued_count: i64,
    pub issued_minor: i64,
    pub redeemed_count: i64,
    pub redeemed_minor: i64,
    pub outstanding_count: i64,
    pub outstanding_minor: i64,
    pub expired_count: i64,
    pub void_count: i64,
}

pub async fn summary(db: impl PgExecutor<'_>, survey: Uuid) -> Result<Summary, sqlx::Error> {
    sqlx::query_as!(
        Summary,
        r#"SELECT count(r.id) AS "responses!",
                  count(v.id) AS "issued_count!",
                  coalesce(sum(v.amount_minor), 0)::bigint AS "issued_minor!",
                  count(v.id) FILTER (WHERE v.status = 'redeemed') AS "redeemed_count!",
                  coalesce(sum(v.amount_minor) FILTER (WHERE v.status = 'redeemed'), 0)::bigint AS "redeemed_minor!",
                  count(v.id) FILTER (WHERE v.status = 'issued' AND v.expires_at > now()) AS "outstanding_count!",
                  coalesce(sum(v.amount_minor) FILTER (WHERE v.status = 'issued' AND v.expires_at > now()), 0)::bigint
                      AS "outstanding_minor!",
                  count(v.id) FILTER (WHERE v.status = 'issued' AND v.expires_at <= now()) AS "expired_count!",
                  count(v.id) FILTER (WHERE v.status = 'void') AS "void_count!"
           FROM farm_survey_responses r
           LEFT JOIN farm_vouchers v ON v.response_id = r.id
           WHERE r.survey_id = $1 AND r.deleted_at IS NULL"#,
        survey
    )
    .fetch_one(db)
    .await
}

/// Choice and yes/no answers counted per option, and number answers summarised, as JSON rows.
pub async fn tallies(
    db: impl PgExecutor<'_> + Copy,
    survey: Uuid,
) -> Result<(Value, Value), sqlx::Error> {
    let tally = sqlx::query_scalar!(
        r#"SELECT coalesce(jsonb_agg(t ORDER BY t.position, t.option_value), '[]') AS "t!" FROM (
               SELECT q.id AS question_id, q.position, x.option_value, count(*) AS answers
               FROM farm_survey_questions q
               JOIN farm_survey_answers a ON a.question_id = q.id
               CROSS JOIN LATERAL (
                   SELECT e #>> '{}' AS option_value
                   FROM jsonb_array_elements(CASE WHEN jsonb_typeof(a.value) = 'array' THEN a.value ELSE '[]' END) e
                   UNION ALL
                   SELECT a.value #>> '{}' WHERE jsonb_typeof(a.value) IN ('string', 'boolean')
               ) x
               WHERE q.survey_id = $1 AND q.kind IN ('single_choice', 'multi_choice', 'yes_no')
               GROUP BY q.id, q.position, x.option_value) t"#,
        survey
    )
    .fetch_one(db)
    .await?;
    let numbers = sqlx::query_scalar!(
        r#"SELECT coalesce(jsonb_agg(t ORDER BY t.position), '[]') AS "n!" FROM (
               SELECT q.id AS question_id, q.position, count(*) AS answers,
                      round(avg((a.value #>> '{}')::numeric), 2)::text AS average,
                      min((a.value #>> '{}')::numeric)::text AS minimum,
                      max((a.value #>> '{}')::numeric)::text AS maximum
               FROM farm_survey_questions q
               JOIN farm_survey_answers a ON a.question_id = q.id
               WHERE q.survey_id = $1 AND q.kind = 'number' AND jsonb_typeof(a.value) = 'number'
               GROUP BY q.id, q.position) t"#,
        survey
    )
    .fetch_one(db)
    .await?;
    Ok((tally, numbers))
}

/// A survey's vouchers for ops: never the code.
pub struct SurveyVoucherRow {
    pub id: Uuid,
    pub household_label: String,
    pub respondent_name: String,
    pub submitted_at: DateTime<Utc>,
    pub status: String,
    pub expired: bool,
    pub amount_minor: i64,
    pub currency: String,
    pub decimals: i16,
    pub currency_id: Uuid,
    pub audit_required: bool,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub redeemed_at: Option<DateTime<Utc>>,
    pub redeemed_by_name: Option<String>,
    pub id_type_seen: Option<String>,
    pub void_reason: Option<String>,
}

/// One page of a survey's vouchers, newest answer first, and the count.
pub async fn survey_vouchers(
    db: impl PgExecutor<'_> + Copy,
    ws: Uuid,
    survey: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<SurveyVoucherRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        SurveyVoucherRow,
        r#"SELECT v.id, h.label AS household_label, concat_ws(' ', p.given_name, p.family_name) AS "respondent_name!",
                  r.submitted_at, v.status, (v.status = 'issued' AND v.expires_at <= now()) AS "expired!",
                  v.amount_minor, c.asset_type AS currency, c.decimals, v.currency_id, v.audit_required,
                  v.issued_at, v.expires_at, v.redeemed_at,
                  (SELECT concat_ws(' ', u.first_name, u.last_name) FROM users u WHERE u.id = v.redeemed_by) AS redeemed_by_name,
                  v.id_type_seen, v.void_reason
           FROM farm_vouchers v
           JOIN farm_survey_responses r ON r.id = v.response_id
           JOIN farm_households h ON h.id = v.household_id
           JOIN farm_persons p ON p.id = r.person_id
           JOIN currencies c ON c.id = v.currency_id
           WHERE v.workspace_id = $1 AND v.survey_id = $2
           ORDER BY r.submitted_at DESC, v.id DESC
           LIMIT $3 OFFSET $4"#,
        ws,
        survey,
        limit,
        offset
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_vouchers WHERE workspace_id = $1 AND survey_id = $2"#,
        ws,
        survey
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}
