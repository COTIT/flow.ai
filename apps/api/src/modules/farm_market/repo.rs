//! Market SQL. Buyers and demand are workspace rows read by managers only (the service checks);
//! opportunities filter `workspace_id` and the caller's staff villages.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

pub struct BuyerRow {
    pub id: Uuid,
    pub name: String,
    pub channel: String,
    pub contact_note: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct NewBuyer {
    pub id: Option<Uuid>,
    pub name: String,
    pub channel: String,
    pub contact_note: Option<String>,
}

pub async fn buyers(
    db: &sqlx::PgPool,
    ws: Uuid,
    active: Option<bool>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<BuyerRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        BuyerRow,
        "SELECT id, name, channel, contact_note, is_active, created_at, updated_at
         FROM farm_buyers WHERE workspace_id = $1 AND ($2::bool IS NULL OR is_active = $2)
         ORDER BY name, id LIMIT $3 OFFSET $4",
        ws,
        active,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_buyers
           WHERE workspace_id = $1 AND ($2::bool IS NULL OR is_active = $2)"#,
        ws,
        active,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

pub async fn find_buyer(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<BuyerRow>, sqlx::Error> {
    sqlx::query_as!(
        BuyerRow,
        "SELECT id, name, channel, contact_note, is_active, created_at, updated_at
         FROM farm_buyers WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
    )
    .fetch_optional(db)
    .await
}

pub async fn insert_buyer(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    b: &NewBuyer,
) -> Result<BuyerRow, sqlx::Error> {
    sqlx::query_as!(
        BuyerRow,
        "INSERT INTO farm_buyers (id, workspace_id, name, channel, contact_note)
         VALUES (coalesce($1, uuidv7()), $2, $3, $4, $5)
         RETURNING id, name, channel, contact_note, is_active, created_at, updated_at",
        b.id,
        ws,
        b.name,
        b.channel,
        b.contact_note,
    )
    .fetch_one(db)
    .await
}

/// A demand with its buyer, crop and price currency.
pub struct DemandRow {
    pub id: Uuid,
    pub buyer_id: Uuid,
    pub buyer_name: String,
    pub buyer_channel: String,
    pub crop_id: Uuid,
    pub crop_code: String,
    pub crop_name_en: String,
    pub crop_name_sw: String,
    pub quantity_kg: Decimal,
    pub quality_note: Option<String>,
    pub window_start: NaiveDate,
    pub window_end: NaiveDate,
    pub delivery_point: Option<String>,
    pub indicative_price_per_kg_minor: Option<i64>,
    pub currency_id: Option<Uuid>,
    pub currency: Option<String>,
    pub decimals: Option<i16>,
    pub status: String,
    pub verification: String,
    pub confidence: Option<String>,
    pub captured_at: DateTime<Utc>,
    /// `{id, name}`.
    pub captured_by: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct NewDemand {
    pub id: Option<Uuid>,
    pub buyer_id: Uuid,
    pub crop_id: Uuid,
    pub quantity_kg: Decimal,
    pub quality_note: Option<String>,
    pub window_start: NaiveDate,
    pub window_end: NaiveDate,
    pub delivery_point: Option<String>,
    pub indicative_price_per_kg_minor: Option<i64>,
    pub currency_id: Option<Uuid>,
    pub confidence: Option<String>,
}

/// Demands by window start; `id` picks one, `statuses` filters.
pub async fn demands(
    db: &sqlx::PgPool,
    ws: Uuid,
    id: Option<Uuid>,
    statuses: Option<&[String]>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<DemandRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        DemandRow,
        r#"SELECT d.id, d.buyer_id, b.name AS buyer_name, b.channel AS buyer_channel,
                  d.crop_id, c.code AS crop_code, c.name_en AS crop_name_en, c.name_sw AS crop_name_sw,
                  d.quantity_kg, d.quality_note, d.window_start, d.window_end, d.delivery_point,
                  d.indicative_price_per_kg_minor, d.currency_id, cur.asset_type AS "currency?",
                  cur.decimals AS "decimals?", d.status, d.verification, d.confidence, d.captured_at,
                  (SELECT jsonb_build_object('id', u.id, 'name', concat_ws(' ', u.first_name, u.last_name))
                   FROM users u WHERE u.id = d.captured_by) AS captured_by,
                  d.created_at, d.updated_at
           FROM farm_buyer_demands d
           JOIN farm_buyers b ON b.id = d.buyer_id
           JOIN farm_crops c ON c.id = d.crop_id
           LEFT JOIN currencies cur ON cur.id = d.currency_id
           WHERE d.workspace_id = $1 AND d.deleted_at IS NULL
             AND ($2::uuid IS NULL OR d.id = $2) AND ($3::text[] IS NULL OR d.status = ANY($3))
           ORDER BY d.window_start, d.id LIMIT $4 OFFSET $5"#,
        ws,
        id,
        statuses,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_buyer_demands d
           WHERE d.workspace_id = $1 AND d.deleted_at IS NULL
             AND ($2::uuid IS NULL OR d.id = $2) AND ($3::text[] IS NULL OR d.status = ANY($3))"#,
        ws,
        id,
        statuses,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// True when the crop belongs to this workspace.
pub async fn crop_exists(db: impl PgExecutor<'_>, ws: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM farm_crops WHERE workspace_id = $1 AND id = $2) AS "e!""#,
        ws,
        id,
    )
    .fetch_one(db)
    .await
}

/// Insert a demand captured by `actor`, open and unverified; its id.
pub async fn insert_demand(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    d: &NewDemand,
    actor: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO farm_buyer_demands (id, workspace_id, buyer_id, crop_id, quantity_kg,
             quality_note, window_start, window_end, delivery_point, indicative_price_per_kg_minor,
             currency_id, confidence, captured_by)
         VALUES (coalesce($1, uuidv7()), $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
         RETURNING id",
        d.id,
        ws,
        d.buyer_id,
        d.crop_id,
        d.quantity_kg,
        d.quality_note,
        d.window_start,
        d.window_end,
        d.delivery_point,
        d.indicative_price_per_kg_minor,
        d.currency_id,
        d.confidence,
        actor,
    )
    .fetch_one(db)
    .await
}

/// Which opportunities: the caller's staff villages, plus filters.
pub struct OppPick<'a> {
    pub ws: Uuid,
    pub all: bool,
    pub villages: &'a [Uuid],
    /// The caller's own person: a farmer sees opportunities carrying their supply.
    pub person: Option<Uuid>,
    pub id: Option<Uuid>,
    pub village: Option<Uuid>,
    pub demand: Option<Uuid>,
    pub statuses: Option<Vec<String>>,
    pub limit: i64,
    pub offset: i64,
}

/// An opportunity with its demand, buyer, village and crop.
pub struct OppRow {
    pub id: Uuid,
    pub buyer_demand_id: Uuid,
    pub demand_quantity_kg: Decimal,
    pub window_start: NaiveDate,
    pub window_end: NaiveDate,
    pub buyer_id: Uuid,
    pub buyer_name: String,
    pub village_id: Uuid,
    pub village_name: String,
    pub crop_id: Uuid,
    pub crop_code: String,
    pub crop_name_en: String,
    pub crop_name_sw: String,
    pub offered_quantity_kg: Decimal,
    pub status: String,
    pub note: Option<String>,
    pub captured_at: DateTime<Utc>,
    /// `{id, name}`.
    pub captured_by: Option<Value>,
    /// Kg from the caller's own farms; null for a caller with no person.
    pub my_contribution_kg: Option<Decimal>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Opportunities the caller may see, newest first.
pub async fn opportunities(
    db: &sqlx::PgPool,
    p: &OppPick<'_>,
) -> Result<(Vec<OppRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        OppRow,
        r#"SELECT o.id, o.buyer_demand_id, d.quantity_kg AS demand_quantity_kg, d.window_start,
                  d.window_end, b.id AS buyer_id, b.name AS buyer_name, o.village_id,
                  v.name AS village_name, o.crop_id, c.code AS crop_code, c.name_en AS crop_name_en,
                  c.name_sw AS crop_name_sw, o.offered_quantity_kg, o.status, o.note, o.captured_at,
                  (SELECT jsonb_build_object('id', u.id, 'name', concat_ws(' ', u.first_name, u.last_name))
                   FROM users u WHERE u.id = o.captured_by) AS captured_by,
                  CASE WHEN $10::uuid IS NOT NULL THEN
                      (SELECT coalesce(sum(s.contributed_kg), 0) FROM farm_opportunity_supply s
                       JOIN farm_crop_cycles cc ON cc.id = s.crop_cycle_id
                       JOIN farm_plots pl ON pl.id = cc.plot_id
                       WHERE s.opportunity_id = o.id AND pl.farm_id IN (SELECT farm_person_farms($10)))
                  END AS my_contribution_kg,
                  o.created_at, o.updated_at
           FROM farm_opportunities o
           JOIN farm_buyer_demands d ON d.id = o.buyer_demand_id
           JOIN farm_buyers b ON b.id = d.buyer_id
           JOIN farm_villages v ON v.id = o.village_id
           JOIN farm_crops c ON c.id = o.crop_id
           WHERE o.workspace_id = $1 AND o.deleted_at IS NULL
             AND ($2 OR o.village_id = ANY($3) OR o.id IN (SELECT farm_person_opportunities($10)))
             AND ($4::uuid IS NULL OR o.id = $4) AND ($5::uuid IS NULL OR o.village_id = $5)
             AND ($6::uuid IS NULL OR o.buyer_demand_id = $6) AND ($7::text[] IS NULL OR o.status = ANY($7))
           ORDER BY o.created_at DESC, o.id DESC LIMIT $8 OFFSET $9"#,
        p.ws,
        p.all,
        p.villages,
        p.id,
        p.village,
        p.demand,
        p.statuses.as_deref(),
        p.limit,
        p.offset,
        p.person,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_opportunities o
           WHERE o.workspace_id = $1 AND o.deleted_at IS NULL
             AND ($2 OR o.village_id = ANY($3) OR o.id IN (SELECT farm_person_opportunities($8)))
             AND ($4::uuid IS NULL OR o.id = $4) AND ($5::uuid IS NULL OR o.village_id = $5)
             AND ($6::uuid IS NULL OR o.buyer_demand_id = $6) AND ($7::text[] IS NULL OR o.status = ANY($7))"#,
        p.ws,
        p.all,
        p.villages,
        p.id,
        p.village,
        p.demand,
        p.statuses.as_deref(),
        p.person,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// An opportunity's (demand, village, note), for a replayed proposal.
pub async fn opportunity_of(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<(Uuid, Uuid, Option<String>)>, sqlx::Error> {
    let r = sqlx::query!(
        "SELECT buyer_demand_id, village_id, note FROM farm_opportunities
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
    )
    .fetch_optional(db)
    .await?;
    Ok(r.map(|r| (r.buyer_demand_id, r.village_id, r.note)))
}

/// True when the village belongs to this workspace.
pub async fn village_exists(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM farm_villages WHERE workspace_id = $1 AND id = $2) AS "e!""#,
        ws,
        id,
    )
    .fetch_one(db)
    .await
}

/// Insert a proposed opportunity captured by `actor`; its id.
#[allow(clippy::too_many_arguments)]
pub async fn insert_opportunity(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Option<Uuid>,
    demand: Uuid,
    village: Uuid,
    crop: Uuid,
    note: Option<&str>,
    actor: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO farm_opportunities (id, workspace_id, buyer_demand_id, village_id, crop_id,
             note, captured_by)
         VALUES (coalesce($1, uuidv7()), $2, $3, $4, $5, $6, $7)
         RETURNING id",
        id,
        ws,
        demand,
        village,
        crop,
        note,
        actor,
    )
    .fetch_one(db)
    .await
}

/// Lock an opportunity: (village, crop, status).
pub async fn lock_opportunity(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<(Uuid, Uuid, String)>, sqlx::Error> {
    let r = sqlx::query!(
        "SELECT village_id, crop_id, status FROM farm_opportunities
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL FOR UPDATE",
        ws,
        id,
    )
    .fetch_optional(conn)
    .await?;
    Ok(r.map(|r| (r.village_id, r.crop_id, r.status)))
}

pub async fn set_status(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE farm_opportunities SET status = $3, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        status,
    )
    .execute(conn)
    .await?;
    Ok(())
}

// ---- supply lines ----

/// A harvest report as the supply guard checks it.
pub struct HarvestLock {
    pub village_id: Uuid,
    pub crop_cycle_id: Uuid,
    pub crop_id: Uuid,
    pub kind: String,
    pub is_current: bool,
    pub deleted: bool,
    pub quantity_kg: Decimal,
}

/// Lock a harvest report of this workspace. Lock order (shared with supply batches): harvest
/// report rows first, then the opportunity, then `committed` in its own statement.
pub async fn lock_harvest(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<HarvestLock>, sqlx::Error> {
    sqlx::query_as!(
        HarvestLock,
        r#"SELECT h.village_id, h.crop_cycle_id, c.crop_id, h.kind, h.is_current,
                  h.deleted_at IS NOT NULL AS "deleted!", h.quantity_kg
           FROM farm_harvest_reports h
           JOIN farm_crop_cycles c ON c.id = h.crop_cycle_id
           WHERE h.workspace_id = $1 AND h.id = $2
           FOR UPDATE OF h"#,
        ws,
        id,
    )
    .fetch_optional(conn)
    .await
}

/// Kg already committed from the harvest `id` measures. Its own statement, after the lock.
pub async fn committed(conn: &mut PgConnection, id: Uuid) -> Result<Decimal, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT farm_harvest_committed_kg($1) AS "kg!""#, id)
        .fetch_one(conn)
        .await
}

/// A stored line by id: (opportunity, harvest, kg), for a replay.
pub async fn find_line(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<(Uuid, Uuid, Decimal)>, sqlx::Error> {
    let r = sqlx::query!(
        "SELECT opportunity_id, harvest_report_id, contributed_kg FROM farm_opportunity_supply
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
    )
    .fetch_optional(db)
    .await?;
    Ok(r.map(|r| (r.opportunity_id, r.harvest_report_id, r.contributed_kg)))
}

/// A line to insert; `captured_by` is the caller.
pub struct NewLineRow {
    pub id: Option<Uuid>,
    pub village_id: Uuid,
    pub opportunity_id: Uuid,
    pub harvest_report_id: Uuid,
    pub crop_cycle_id: Uuid,
    pub contributed_kg: Decimal,
    pub captured_by: Uuid,
}

/// Insert a line and re-sum its opportunity's offered total.
pub async fn insert_line(
    conn: &mut PgConnection,
    ws: Uuid,
    l: &NewLineRow,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO farm_opportunity_supply (id, workspace_id, village_id, opportunity_id,
             harvest_report_id, crop_cycle_id, contributed_kg, captured_by)
         VALUES (coalesce($1, uuidv7()), $2, $3, $4, $5, $6, $7, $8)",
        l.id,
        ws,
        l.village_id,
        l.opportunity_id,
        l.harvest_report_id,
        l.crop_cycle_id,
        l.contributed_kg,
        l.captured_by,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "UPDATE farm_opportunities SET updated_at = now(), offered_quantity_kg =
             (SELECT coalesce(sum(contributed_kg), 0) FROM farm_opportunity_supply
              WHERE opportunity_id = $1)
         WHERE id = $1",
        l.opportunity_id,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// A supply line with its plot, crop and farmer (the farm's primary manager).
pub struct LineRow {
    pub opportunity_id: Uuid,
    pub id: Uuid,
    pub harvest_report_id: Uuid,
    pub crop_cycle_id: Uuid,
    pub plot_id: Uuid,
    pub plot_label: String,
    pub crop_id: Uuid,
    pub farmer_person_id: Option<Uuid>,
    pub farmer_name: Option<String>,
    pub contributed_kg: Decimal,
    /// `{id, name}`; null unless the caller is staff of the opportunity's village.
    pub captured_by: Option<Value>,
    pub captured_at: DateTime<Utc>,
}

/// Lines of `opportunities` the caller may read: every line for staff of the opportunity's
/// village, otherwise only lines from the caller's own farms, without the attacher.
pub async fn lines(
    db: &sqlx::PgPool,
    ws: Uuid,
    all: bool,
    villages: &[Uuid],
    person: Option<Uuid>,
    opportunities: &[Uuid],
) -> Result<Vec<LineRow>, sqlx::Error> {
    sqlx::query_as!(
        LineRow,
        r#"SELECT s.opportunity_id, s.id, s.harvest_report_id, s.crop_cycle_id, c.plot_id,
                  pl.label AS plot_label, c.crop_id, mp.id AS "farmer_person_id?",
                  concat_ws(' ', mp.given_name, mp.family_name) AS farmer_name, s.contributed_kg,
                  CASE WHEN $2 OR s.village_id = ANY($3) THEN
                      (SELECT jsonb_build_object('id', u.id, 'name', concat_ws(' ', u.first_name, u.last_name))
                       FROM users u WHERE u.id = s.captured_by)
                  END AS captured_by,
                  s.captured_at
           FROM farm_opportunity_supply s
           JOIN farm_crop_cycles c ON c.id = s.crop_cycle_id
           JOIN farm_plots pl ON pl.id = c.plot_id
           LEFT JOIN farm_managers m ON m.farm_id = pl.farm_id AND m.is_primary
           LEFT JOIN farm_persons mp ON mp.id = m.person_id
           WHERE s.workspace_id = $1 AND s.opportunity_id = ANY($5)
             AND ($2 OR s.village_id = ANY($3) OR pl.farm_id IN (SELECT farm_person_farms($4)))
           ORDER BY s.captured_at, s.id"#,
        ws,
        all,
        villages,
        person,
        opportunities,
    )
    .fetch_all(db)
    .await
}

// ---- availability, village supply, matches ----

/// A current expected figure with what is already promised from it.
pub struct AvailableRow {
    pub harvest_report_id: Uuid,
    pub crop_cycle_id: Uuid,
    pub village_id: Uuid,
    pub crop_id: Uuid,
    pub plot_id: Uuid,
    pub plot_label: String,
    pub farmer_person_id: Option<Uuid>,
    pub farmer_name: Option<String>,
    pub harvest_start: Option<NaiveDate>,
    pub harvest_end: Option<NaiveDate>,
    pub quantity_kg: Decimal,
    pub confidence: Option<String>,
    pub verification: String,
    pub committed_kg: Decimal,
    pub available_kg: Decimal,
}

/// Availability the caller may read: staff villages, plus a farmer's own cycles.
#[allow(clippy::too_many_arguments)]
pub async fn available(
    db: &sqlx::PgPool,
    ws: Uuid,
    all: bool,
    villages: &[Uuid],
    person: Option<Uuid>,
    village: Option<Uuid>,
    crop: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<AvailableRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        AvailableRow,
        r#"SELECT a.harvest_report_id AS "harvest_report_id!", a.crop_cycle_id AS "crop_cycle_id!",
                  a.village_id AS "village_id!", a.crop_id AS "crop_id!", a.plot_id AS "plot_id!",
                  pl.label AS plot_label, mp.id AS "farmer_person_id?",
                  concat_ws(' ', mp.given_name, mp.family_name) AS farmer_name,
                  a.harvest_start, a.harvest_end, a.quantity_kg AS "quantity_kg!", a.confidence,
                  a.verification AS "verification!", a.committed_kg AS "committed_kg!",
                  a.available_kg AS "available_kg!"
           FROM farm_harvest_available a
           JOIN farm_plots pl ON pl.id = a.plot_id
           LEFT JOIN farm_managers m ON m.farm_id = pl.farm_id AND m.is_primary
           LEFT JOIN farm_persons mp ON mp.id = m.person_id
           WHERE a.workspace_id = $1
             AND ($2 OR a.village_id = ANY($3) OR pl.farm_id IN (SELECT farm_person_farms($4)))
             AND ($5::uuid IS NULL OR a.village_id = $5) AND ($6::uuid IS NULL OR a.crop_id = $6)
           ORDER BY a.harvest_start NULLS LAST, a.harvest_report_id LIMIT $7 OFFSET $8"#,
        ws,
        all,
        villages,
        person,
        village,
        crop,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_harvest_available a
           JOIN farm_plots pl ON pl.id = a.plot_id
           WHERE a.workspace_id = $1
             AND ($2 OR a.village_id = ANY($3) OR pl.farm_id IN (SELECT farm_person_farms($4)))
             AND ($5::uuid IS NULL OR a.village_id = $5) AND ($6::uuid IS NULL OR a.crop_id = $6)"#,
        ws,
        all,
        villages,
        person,
        village,
        crop,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// RUAHA v_village_supply: expected, committed and available kg by crop and harvest month.
pub struct SupplyRow {
    pub crop_id: Uuid,
    pub window_month: Option<NaiveDate>,
    pub window_start: Option<NaiveDate>,
    pub window_end: Option<NaiveDate>,
    pub expected_kg: Decimal,
    pub committed_kg: Decimal,
    pub available_kg: Decimal,
    pub cycle_count: i64,
    pub verified_cycles: i64,
}

pub async fn village_supply(
    db: &sqlx::PgPool,
    ws: Uuid,
    village: Uuid,
) -> Result<Vec<SupplyRow>, sqlx::Error> {
    sqlx::query_as!(
        SupplyRow,
        r#"SELECT a.crop_id AS "crop_id!", date_trunc('month', a.harvest_start)::date AS window_month,
                  min(a.harvest_start) AS window_start, max(a.harvest_end) AS window_end,
                  sum(a.quantity_kg) AS "expected_kg!", sum(a.committed_kg) AS "committed_kg!",
                  sum(a.available_kg) AS "available_kg!", count(*) AS "cycle_count!",
                  count(*) FILTER (WHERE a.verification = 'verified') AS "verified_cycles!"
           FROM farm_harvest_available a
           WHERE a.workspace_id = $1 AND a.village_id = $2
           GROUP BY a.crop_id, date_trunc('month', a.harvest_start)
           ORDER BY window_month NULLS LAST, a.crop_id"#,
        ws,
        village,
    )
    .fetch_all(db)
    .await
}

/// RUAHA v_demand_match (mvp_security v2): per open demand and village, the availability of
/// harvests whose window overlaps the demand's, each harvest counted once.
pub struct MatchRow {
    pub buyer_demand_id: Uuid,
    pub buyer_id: Uuid,
    pub buyer_name: String,
    pub crop_id: Uuid,
    pub demand_kg: Decimal,
    pub window_start: NaiveDate,
    pub window_end: NaiveDate,
    pub village_id: Uuid,
    pub village_name: String,
    pub available_kg: Decimal,
    pub committed_kg: Decimal,
    pub coverable_kg: Decimal,
    pub coverage_pct: Decimal,
    pub opportunity_id: Option<Uuid>,
    pub opportunity_status: Option<String>,
}

pub async fn matches(
    db: &sqlx::PgPool,
    ws: Uuid,
    demand: Option<Uuid>,
    village: Option<Uuid>,
) -> Result<Vec<MatchRow>, sqlx::Error> {
    sqlx::query_as!(
        MatchRow,
        r#"SELECT d.id AS buyer_demand_id, b.id AS buyer_id, b.name AS buyer_name, d.crop_id,
                  d.quantity_kg AS demand_kg, d.window_start, d.window_end,
                  s.village_id AS "village_id!", v.name AS village_name,
                  s.available_kg AS "available_kg!", s.committed_kg AS "committed_kg!",
                  least(d.quantity_kg, s.available_kg) AS "coverable_kg!",
                  round(100 * least(d.quantity_kg, s.available_kg) / d.quantity_kg, 1) AS "coverage_pct!",
                  o.id AS "opportunity_id?", o.status AS "opportunity_status?"
           FROM farm_buyer_demands d
           JOIN farm_buyers b ON b.id = d.buyer_id
           JOIN LATERAL (
               SELECT a.village_id, sum(a.available_kg) AS available_kg, sum(a.committed_kg) AS committed_kg
               FROM farm_harvest_available a
               WHERE a.workspace_id = d.workspace_id AND a.crop_id = d.crop_id
                 AND a.harvest_start <= d.window_end AND a.harvest_end >= d.window_start
                 AND ($3::uuid IS NULL OR a.village_id = $3)
               GROUP BY a.village_id
           ) s ON true
           JOIN farm_villages v ON v.id = s.village_id
           LEFT JOIN farm_opportunities o ON o.buyer_demand_id = d.id AND o.village_id = s.village_id
                AND o.deleted_at IS NULL
           WHERE d.workspace_id = $1 AND d.deleted_at IS NULL AND d.status = 'open'
             AND ($2::uuid IS NULL OR d.id = $2)
           ORDER BY d.window_start, d.id, v.name"#,
        ws,
        demand,
        village,
    )
    .fetch_all(db)
    .await
}
