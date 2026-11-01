//! PUE SQL. Every read filters `workspace_id` and the caller's scope: staff villages
//! (`$all OR village_id = ANY($villages)`) or, for a farmer, their own person.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// Who is reading: workspace, staff scope and own person.
pub struct Seen<'a> {
    pub ws: Uuid,
    pub all: bool,
    pub villages: &'a [Uuid],
    pub person: Option<Uuid>,
}

/// Which requests: one village, one id, some statuses, one page.
#[derive(Default)]
pub struct Pick {
    pub village: Option<Uuid>,
    pub id: Option<Uuid>,
    pub statuses: Option<Vec<String>>,
    pub limit: i64,
    pub offset: i64,
}

/// A request with its machine, its estimate (`e_*`, all null without one) and actor refs.
pub struct RequestRow {
    pub id: Uuid,
    pub village_id: Uuid,
    /// `{id, name}`.
    pub person: Value,
    pub farm_id: Option<Uuid>,
    pub farm_label: Option<String>,
    pub equipment_id: Uuid,
    pub equipment_code: String,
    pub equipment_name_en: String,
    pub equipment_name_sw: String,
    pub acquisition: String,
    pub quantity: i32,
    pub hours_per_day: Option<Decimal>,
    pub days_per_week: Option<Decimal>,
    pub purpose: Option<String>,
    pub status: String,
    pub submitted_at: Option<DateTime<Utc>>,
    pub decided_at: Option<DateTime<Utc>>,
    /// `{id, name}`.
    pub decided_by: Option<Value>,
    pub decision_note: Option<String>,
    pub provenance: Value,
    pub e_rated_power_kw: Option<Decimal>,
    pub e_quantity: Option<i32>,
    pub e_hours_per_day: Option<Decimal>,
    pub e_days_per_week: Option<Decimal>,
    pub e_power_kw: Option<Decimal>,
    pub e_kwh_per_day: Option<Decimal>,
    pub e_kwh_per_week: Option<Decimal>,
    pub e_method: Option<String>,
    pub e_source: Option<String>,
    pub e_confidence: Option<String>,
    pub e_computed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A request to insert. Provenance is the server's: farmer reported, captured by the caller.
pub struct NewRow<'a> {
    pub id: Option<Uuid>,
    pub village_id: Uuid,
    pub person_id: Uuid,
    pub farm_id: Option<Uuid>,
    pub equipment_id: Uuid,
    pub acquisition: &'a str,
    pub quantity: i32,
    pub hours_per_day: Option<Decimal>,
    pub days_per_week: Option<Decimal>,
    pub purpose: Option<&'a str>,
    pub status: &'a str,
    pub captured_by: Uuid,
}

/// One page of requests the caller may see, newest first.
pub async fn requests(
    db: &sqlx::PgPool,
    s: &Seen<'_>,
    p: &Pick,
) -> Result<(Vec<RequestRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        RequestRow,
        r#"SELECT r.id, r.village_id,
                  jsonb_build_object('id', p.id, 'name', concat_ws(' ', p.given_name, p.family_name)) AS "person!",
                  r.farm_id, fa.label AS "farm_label?", r.equipment_id, eq.code AS equipment_code,
                  eq.name_en AS equipment_name_en, eq.name_sw AS equipment_name_sw,
                  r.acquisition, r.quantity, r.hours_per_day, r.days_per_week, r.purpose, r.status,
                  r.submitted_at, r.decided_at,
                  (SELECT jsonb_build_object('id', u.id, 'name', concat_ws(' ', u.first_name, u.last_name))
                   FROM users u WHERE u.id = r.decided_by) AS decided_by,
                  r.decision_note,
                  farm_provenance(r.source, r.captured_at, r.captured_by, r.verification,
                                  r.verified_by, r.verified_at, r.confidence, r.evidence_ref) AS "provenance!",
                  e.rated_power_kw AS "e_rated_power_kw?", e.quantity AS "e_quantity?",
                  e.hours_per_day AS "e_hours_per_day?", e.days_per_week AS "e_days_per_week?",
                  e.est_power_kw AS "e_power_kw?", e.est_kwh_per_day AS "e_kwh_per_day?",
                  e.est_kwh_per_week AS "e_kwh_per_week?", e.method AS "e_method?",
                  e.source AS "e_source?", e.confidence AS "e_confidence?",
                  e.computed_at AS "e_computed_at?",
                  r.created_at, r.updated_at
           FROM farm_pue_requests r
           JOIN farm_persons p ON p.id = r.person_id
           JOIN farm_equipment eq ON eq.id = r.equipment_id
           LEFT JOIN farms fa ON fa.id = r.farm_id
           LEFT JOIN farm_energy_estimates e ON e.pue_request_id = r.id
           WHERE r.workspace_id = $1 AND r.deleted_at IS NULL
             AND ($2 OR r.village_id = ANY($3) OR r.person_id = $4)
             AND ($5::uuid IS NULL OR r.village_id = $5) AND ($6::uuid IS NULL OR r.id = $6)
             AND ($7::text[] IS NULL OR r.status = ANY($7))
           ORDER BY r.created_at DESC, r.id DESC LIMIT $8 OFFSET $9"#,
        s.ws,
        s.all,
        s.villages,
        s.person,
        p.village,
        p.id,
        p.statuses.as_deref(),
        p.limit,
        p.offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_pue_requests r
           WHERE r.workspace_id = $1 AND r.deleted_at IS NULL
             AND ($2 OR r.village_id = ANY($3) OR r.person_id = $4)
             AND ($5::uuid IS NULL OR r.village_id = $5) AND ($6::uuid IS NULL OR r.id = $6)
             AND ($7::text[] IS NULL OR r.status = ANY($7))"#,
        s.ws,
        s.all,
        s.villages,
        s.person,
        p.village,
        p.id,
        p.statuses.as_deref(),
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// The village of a live person of this workspace.
pub async fn person_village(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    person: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT village_id FROM farm_persons WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL",
        ws,
        person,
    )
    .fetch_optional(db)
    .await
}

/// True when `farm` is a live farm of `village`.
pub async fn farm_in(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    village: Uuid,
    farm: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM farms WHERE workspace_id = $1 AND village_id = $2
                          AND id = $3 AND deleted_at IS NULL) AS "e!""#,
        ws,
        village,
        farm,
    )
    .fetch_one(db)
    .await
}

/// Insert; the estimate trigger runs in the same statement. Returns the id.
pub async fn insert(
    conn: &mut PgConnection,
    ws: Uuid,
    r: &NewRow<'_>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO farm_pue_requests (id, workspace_id, village_id, person_id, farm_id,
             equipment_id, acquisition, quantity, hours_per_day, days_per_week, purpose, status,
             submitted_at, source, captured_by)
         VALUES (coalesce($1, uuidv7()), $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
             CASE WHEN $12 = 'submitted' THEN now() END, 'farmer_reported', $13)
         RETURNING id",
        r.id,
        ws,
        r.village_id,
        r.person_id,
        r.farm_id,
        r.equipment_id,
        r.acquisition,
        r.quantity,
        r.hours_per_day,
        r.days_per_week,
        r.purpose,
        r.status,
        r.captured_by,
    )
    .fetch_one(conn)
    .await
}

/// Lock a request: (village, person, status).
pub async fn lock(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<(Uuid, Uuid, String)>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT village_id, person_id, status FROM farm_pue_requests
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL FOR UPDATE",
        ws,
        id,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| (r.village_id, r.person_id, r.status)))
}

/// Move a request to `status`; stamps submission and decision server-side.
pub async fn move_to(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    status: &str,
    actor: Uuid,
    note: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE farm_pue_requests SET status = $3,
             submitted_at = CASE WHEN $3 = 'submitted' THEN coalesce(submitted_at, now())
                                 ELSE submitted_at END,
             decided_at = CASE WHEN $3 IN ('approved', 'rejected') THEN now() ELSE decided_at END,
             decided_by = CASE WHEN $3 IN ('approved', 'rejected') THEN $4 ELSE decided_by END,
             decision_note = coalesce($5, decision_note),
             updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        status,
        actor,
        note,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// RUAHA `v_village_energy` for one village. Prospective and approved load are never summed;
/// peak = raw × simultaneity; headroom = capacity − approved peak. Capacity fields are null
/// for a village without a current capacity row.
pub struct EnergyRow {
    pub capacity_kw: Option<Decimal>,
    pub capacity_basis: Option<String>,
    pub simultaneity_factor: Option<Decimal>,
    pub prospective_kw_raw: Decimal,
    pub approved_kw_raw: Decimal,
    pub prospective_peak_kw: Option<Decimal>,
    pub approved_peak_kw: Option<Decimal>,
    pub headroom_kw: Option<Decimal>,
    pub approved_kwh_per_week: Decimal,
    pub approved_assets: i64,
}

pub async fn energy(db: &sqlx::PgPool, ws: Uuid, village: Uuid) -> Result<EnergyRow, sqlx::Error> {
    sqlx::query_as!(
        EnergyRow,
        r#"WITH load AS (
               SELECT coalesce(sum(e.est_power_kw) FILTER (WHERE r.status IN ('submitted', 'under_review')), 0)::numeric(12,3) AS prospective,
                      coalesce(sum(e.est_power_kw) FILTER (WHERE r.status = 'approved'), 0)::numeric(12,3) AS approved,
                      coalesce(sum(e.est_kwh_per_week) FILTER (WHERE r.status = 'approved'), 0)::numeric(12,3) AS kwh_week,
                      count(e.id) FILTER (WHERE r.status = 'approved') AS assets
               FROM farm_energy_estimates e
               JOIN farm_pue_requests r ON r.id = e.pue_request_id AND r.deleted_at IS NULL
               WHERE e.workspace_id = $1 AND e.village_id = $2)
           SELECT c.capacity_kw AS "capacity_kw?", c.basis AS "capacity_basis?",
                  c.simultaneity_factor AS "simultaneity_factor?",
                  l.prospective AS "prospective_kw_raw!", l.approved AS "approved_kw_raw!",
                  round(l.prospective * c.simultaneity_factor, 3) AS "prospective_peak_kw?",
                  round(l.approved * c.simultaneity_factor, 3) AS "approved_peak_kw?",
                  round(c.capacity_kw - l.approved * c.simultaneity_factor, 3) AS "headroom_kw?",
                  l.kwh_week AS "approved_kwh_per_week!", l.assets AS "approved_assets!"
           FROM load l
           LEFT JOIN farm_village_capacities c ON c.village_id = $2 AND c.is_current"#,
        ws,
        village,
    )
    .fetch_one(db)
    .await
}

/// RUAHA `v_village_pue_pipeline`: per status and currency, buy requests add price ×
/// quantity, rent requests rent per day × quantity. Neither is financed value.
pub struct PipelineRow {
    pub status: String,
    pub request_count: i64,
    pub currency_id: Uuid,
    pub currency: String,
    pub decimals: i16,
    pub indicative_value_minor: Option<i64>,
    pub indicative_rent_per_day_minor: Option<i64>,
}

pub async fn pipeline(
    db: &sqlx::PgPool,
    ws: Uuid,
    village: Uuid,
) -> Result<Vec<PipelineRow>, sqlx::Error> {
    sqlx::query_as!(
        PipelineRow,
        r#"SELECT r.status, count(*) AS "request_count!", eq.currency_id, c.asset_type AS currency,
                  c.decimals,
                  (sum(eq.indicative_price_minor * r.quantity) FILTER (WHERE r.acquisition = 'buy'))::bigint
                      AS indicative_value_minor,
                  (sum(eq.indicative_rent_per_day_minor * r.quantity) FILTER (WHERE r.acquisition = 'rent'))::bigint
                      AS indicative_rent_per_day_minor
           FROM farm_pue_requests r
           JOIN farm_equipment eq ON eq.id = r.equipment_id
           JOIN currencies c ON c.id = eq.currency_id
           WHERE r.workspace_id = $1 AND r.village_id = $2 AND r.deleted_at IS NULL
           GROUP BY r.status, eq.currency_id, c.asset_type, c.decimals
           ORDER BY r.status, c.asset_type"#,
        ws,
        village,
    )
    .fetch_all(db)
    .await
}

/// What a request with these inputs would store as its estimate (the same SQL functions).
pub struct PreviewRow {
    pub rated_power_kw: Decimal,
    pub quantity: i32,
    pub hours_per_day: Decimal,
    pub days_per_week: Decimal,
    pub est_power_kw: Decimal,
    pub est_kwh_per_day: Decimal,
    pub est_kwh_per_week: Decimal,
}

/// The estimate a request would get; None when the machine has no rated power.
pub async fn preview(
    db: &sqlx::PgPool,
    ws: Uuid,
    equipment: Uuid,
    quantity: i32,
    hours: Option<Decimal>,
    days: Option<Decimal>,
) -> Result<Option<PreviewRow>, sqlx::Error> {
    sqlx::query_as!(
        PreviewRow,
        r#"SELECT i.rated_power_kw AS "rated_power_kw!", i.quantity AS "quantity!",
                  i.hours_per_day AS "hours_per_day!", i.days_per_week AS "days_per_week!",
                  farm_est_power_kw(i.rated_power_kw, i.quantity)::numeric(12,3) AS "est_power_kw!",
                  farm_est_kwh_per_day(i.rated_power_kw, i.quantity, i.hours_per_day)::numeric(12,3)
                      AS "est_kwh_per_day!",
                  farm_est_kwh_per_week(i.rated_power_kw, i.quantity, i.hours_per_day, i.days_per_week)::numeric(12,3)
                      AS "est_kwh_per_week!"
           FROM farm_pue_estimate_inputs($1, $2, $3, $4, $5) i"#,
        ws,
        equipment,
        quantity,
        hours,
        days,
    )
    .fetch_optional(db)
    .await
}
