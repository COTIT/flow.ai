//! Farm record SQL. Every read filters `workspace_id`, `deleted_at IS NULL` and the caller's
//! scope: staff villages (`$all OR village_id = ANY($villages)`) or, for a farmer, the rows
//! reachable from their own person (`farm_person_*` functions). Phone and GPS come back only
//! for staff of the row's village.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

/// Who is reading: workspace, staff scope and own person.
pub struct Seen<'a> {
    pub ws: Uuid,
    pub all: bool,
    pub villages: &'a [Uuid],
    pub person: Option<Uuid>,
}

/// Which rows: one village, one id, one page.
#[derive(Default)]
pub struct Pick {
    pub village: Option<Uuid>,
    pub id: Option<Uuid>,
    /// Only rows in one of these verification states.
    pub verification: Option<Vec<String>>,
    /// Only these rows.
    pub ids: Option<Vec<Uuid>>,
    /// Plots of these farms, or cycles of these plots (other kinds ignore it).
    pub parents: Option<Vec<Uuid>>,
    /// Persons only: `ILIKE` pattern on the full name, and phone digits (staff rows only).
    pub name_like: Option<String>,
    pub phone_digits: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

impl Pick {
    /// Exactly the row with this id.
    pub fn one(id: Uuid) -> Self {
        Self {
            id: Some(id),
            limit: 1,
            ..Self::default()
        }
    }
}

pub struct PersonRow {
    pub id: Uuid,
    pub village_id: Uuid,
    pub given_name: String,
    pub family_name: String,
    pub phone: Option<String>,
    pub provenance: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct HouseholdRow {
    pub id: Uuid,
    pub village_id: Uuid,
    pub label: String,
    pub members: Value,
    pub provenance: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct FarmRow {
    pub id: Uuid,
    pub village_id: Uuid,
    pub household_id: Option<Uuid>,
    pub label: String,
    pub latitude: Option<String>,
    pub longitude: Option<String>,
    pub managers: Value,
    pub provenance: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct PlotRow {
    pub id: Uuid,
    pub village_id: Uuid,
    pub farm_id: Uuid,
    pub label: String,
    pub area_ha: Option<String>,
    pub latitude: Option<String>,
    pub longitude: Option<String>,
    pub provenance: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct CycleRow {
    pub id: Uuid,
    pub village_id: Uuid,
    pub plot_id: Uuid,
    pub crop_id: Uuid,
    pub season_label: Option<String>,
    pub area_ha: Option<String>,
    pub tree_count: Option<i32>,
    pub unit_count: Option<i32>,
    pub planted_on: Option<NaiveDate>,
    pub harvest_start: Option<NaiveDate>,
    pub harvest_end: Option<NaiveDate>,
    pub status: String,
    pub provenance: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct HarvestRow {
    pub id: Uuid,
    pub village_id: Uuid,
    pub crop_cycle_id: Uuid,
    pub kind: String,
    pub quantity_kg: String,
    pub reported_for: Option<NaiveDate>,
    pub is_current: bool,
    pub provenance: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---- reads ----

/// Visible persons, and how many match.
pub async fn persons(
    db: &sqlx::PgPool,
    s: &Seen<'_>,
    p: &Pick,
) -> Result<(Vec<PersonRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        PersonRow,
        r#"SELECT t.id, t.village_id, t.given_name, t.family_name,
                  CASE WHEN $2 OR t.village_id = ANY($3) THEN t.phone END AS phone,
                  farm_provenance(t.source, t.captured_at, t.captured_by, t.verification,
                                  t.verified_by, t.verified_at, t.confidence, t.evidence_ref) AS "provenance!",
                  t.created_at, t.updated_at
           FROM farm_persons t
           WHERE t.workspace_id = $1 AND t.deleted_at IS NULL
             AND ($2 OR t.village_id = ANY($3) OR t.id IN (SELECT farm_person_persons($4)))
             AND ($5::uuid IS NULL OR t.village_id = $5) AND ($6::uuid IS NULL OR t.id = $6)
             AND ($9::text[] IS NULL OR t.verification = ANY($9)) AND ($10::uuid[] IS NULL OR t.id = ANY($10))
             AND ($11::text IS NULL OR (t.given_name || ' ' || t.family_name) ILIKE $11
                  OR ($12::text IS NOT NULL AND ($2 OR t.village_id = ANY($3)) AND t.phone LIKE '%' || $12 || '%'))
           ORDER BY t.created_at, t.id LIMIT $7 OFFSET $8"#,
        s.ws,
        s.all,
        s.villages,
        s.person,
        p.village,
        p.id,
        p.limit,
        p.offset,
        p.verification.as_deref(),
        p.ids.as_deref(),
        p.name_like.as_deref(),
        p.phone_digits.as_deref(),
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_persons t
           WHERE t.workspace_id = $1 AND t.deleted_at IS NULL
             AND ($2 OR t.village_id = ANY($3) OR t.id IN (SELECT farm_person_persons($4)))
             AND ($5::uuid IS NULL OR t.village_id = $5) AND ($6::uuid IS NULL OR t.id = $6)
             AND ($7::text[] IS NULL OR t.verification = ANY($7)) AND ($8::uuid[] IS NULL OR t.id = ANY($8))
             AND ($9::text IS NULL OR (t.given_name || ' ' || t.family_name) ILIKE $9
                  OR ($10::text IS NOT NULL AND ($2 OR t.village_id = ANY($3)) AND t.phone LIKE '%' || $10 || '%'))"#,
        s.ws,
        s.all,
        s.villages,
        s.person,
        p.village,
        p.id,
        p.verification.as_deref(),
        p.ids.as_deref(),
        p.name_like.as_deref(),
        p.phone_digits.as_deref(),
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// Visible households with their members, and how many match.
pub async fn households(
    db: &sqlx::PgPool,
    s: &Seen<'_>,
    p: &Pick,
) -> Result<(Vec<HouseholdRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        HouseholdRow,
        r#"SELECT t.id, t.village_id, t.label,
                  coalesce((SELECT jsonb_agg(jsonb_build_object('person_id', m.person_id, 'is_head', m.is_head)
                                             ORDER BY m.created_at, m.id)
                            FROM farm_household_members m
                            JOIN farm_persons mp ON mp.id = m.person_id AND mp.deleted_at IS NULL
                            WHERE m.household_id = t.id), '[]') AS "members!",
                  farm_provenance(t.source, t.captured_at, t.captured_by, t.verification,
                                  t.verified_by, t.verified_at, t.confidence, t.evidence_ref) AS "provenance!",
                  t.created_at, t.updated_at
           FROM farm_households t
           WHERE t.workspace_id = $1 AND t.deleted_at IS NULL
             AND ($2 OR t.village_id = ANY($3) OR t.id IN (SELECT farm_person_households($4)))
             AND ($5::uuid IS NULL OR t.village_id = $5) AND ($6::uuid IS NULL OR t.id = $6)
             AND ($9::text[] IS NULL OR t.verification = ANY($9)) AND ($10::uuid[] IS NULL OR t.id = ANY($10))
           ORDER BY t.created_at, t.id LIMIT $7 OFFSET $8"#,
        s.ws,
        s.all,
        s.villages,
        s.person,
        p.village,
        p.id,
        p.limit,
        p.offset,
        p.verification.as_deref(),
        p.ids.as_deref(),
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_households t
           WHERE t.workspace_id = $1 AND t.deleted_at IS NULL
             AND ($2 OR t.village_id = ANY($3) OR t.id IN (SELECT farm_person_households($4)))
             AND ($5::uuid IS NULL OR t.village_id = $5) AND ($6::uuid IS NULL OR t.id = $6)
             AND ($7::text[] IS NULL OR t.verification = ANY($7)) AND ($8::uuid[] IS NULL OR t.id = ANY($8))"#,
        s.ws,
        s.all,
        s.villages,
        s.person,
        p.village,
        p.id,
        p.verification.as_deref(),
        p.ids.as_deref(),
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// Visible farms with their managers, and how many match.
pub async fn farms(
    db: &sqlx::PgPool,
    s: &Seen<'_>,
    p: &Pick,
) -> Result<(Vec<FarmRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        FarmRow,
        r#"SELECT t.id, t.village_id, t.household_id, t.label,
                  CASE WHEN $2 OR t.village_id = ANY($3) THEN t.latitude::text END AS latitude,
                  CASE WHEN $2 OR t.village_id = ANY($3) THEN t.longitude::text END AS longitude,
                  coalesce((SELECT jsonb_agg(jsonb_build_object('person_id', m.person_id, 'is_primary', m.is_primary)
                                             ORDER BY m.created_at, m.id)
                            FROM farm_managers m
                            JOIN farm_persons mp ON mp.id = m.person_id AND mp.deleted_at IS NULL
                            WHERE m.farm_id = t.id), '[]') AS "managers!",
                  farm_provenance(t.source, t.captured_at, t.captured_by, t.verification,
                                  t.verified_by, t.verified_at, t.confidence, t.evidence_ref) AS "provenance!",
                  t.created_at, t.updated_at
           FROM farms t
           WHERE t.workspace_id = $1 AND t.deleted_at IS NULL
             AND ($2 OR t.village_id = ANY($3) OR t.id IN (SELECT farm_person_farms($4)))
             AND ($5::uuid IS NULL OR t.village_id = $5) AND ($6::uuid IS NULL OR t.id = $6)
             AND ($9::text[] IS NULL OR t.verification = ANY($9)) AND ($10::uuid[] IS NULL OR t.id = ANY($10))
           ORDER BY t.created_at, t.id LIMIT $7 OFFSET $8"#,
        s.ws,
        s.all,
        s.villages,
        s.person,
        p.village,
        p.id,
        p.limit,
        p.offset,
        p.verification.as_deref(),
        p.ids.as_deref(),
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farms t
           WHERE t.workspace_id = $1 AND t.deleted_at IS NULL
             AND ($2 OR t.village_id = ANY($3) OR t.id IN (SELECT farm_person_farms($4)))
             AND ($5::uuid IS NULL OR t.village_id = $5) AND ($6::uuid IS NULL OR t.id = $6)
             AND ($7::text[] IS NULL OR t.verification = ANY($7)) AND ($8::uuid[] IS NULL OR t.id = ANY($8))"#,
        s.ws,
        s.all,
        s.villages,
        s.person,
        p.village,
        p.id,
        p.verification.as_deref(),
        p.ids.as_deref(),
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// Visible plots, and how many match.
pub async fn plots(
    db: &sqlx::PgPool,
    s: &Seen<'_>,
    p: &Pick,
) -> Result<(Vec<PlotRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        PlotRow,
        r#"SELECT t.id, t.village_id, t.farm_id, t.label, t.area_ha::text AS area_ha,
                  CASE WHEN $2 OR t.village_id = ANY($3) THEN t.latitude::text END AS latitude,
                  CASE WHEN $2 OR t.village_id = ANY($3) THEN t.longitude::text END AS longitude,
                  farm_provenance(t.source, t.captured_at, t.captured_by, t.verification,
                                  t.verified_by, t.verified_at, t.confidence, t.evidence_ref) AS "provenance!",
                  t.created_at, t.updated_at
           FROM farm_plots t
           WHERE t.workspace_id = $1 AND t.deleted_at IS NULL
             AND ($2 OR t.village_id = ANY($3) OR t.farm_id IN (SELECT farm_person_farms($4)))
             AND ($5::uuid IS NULL OR t.village_id = $5) AND ($6::uuid IS NULL OR t.id = $6)
             AND ($9::text[] IS NULL OR t.verification = ANY($9)) AND ($10::uuid[] IS NULL OR t.id = ANY($10))
             AND ($11::uuid[] IS NULL OR t.farm_id = ANY($11))
           ORDER BY t.created_at, t.id LIMIT $7 OFFSET $8"#,
        s.ws,
        s.all,
        s.villages,
        s.person,
        p.village,
        p.id,
        p.limit,
        p.offset,
        p.verification.as_deref(),
        p.ids.as_deref(),
        p.parents.as_deref(),
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_plots t
           WHERE t.workspace_id = $1 AND t.deleted_at IS NULL
             AND ($2 OR t.village_id = ANY($3) OR t.farm_id IN (SELECT farm_person_farms($4)))
             AND ($5::uuid IS NULL OR t.village_id = $5) AND ($6::uuid IS NULL OR t.id = $6)
             AND ($7::text[] IS NULL OR t.verification = ANY($7)) AND ($8::uuid[] IS NULL OR t.id = ANY($8))
             AND ($9::uuid[] IS NULL OR t.farm_id = ANY($9))"#,
        s.ws,
        s.all,
        s.villages,
        s.person,
        p.village,
        p.id,
        p.verification.as_deref(),
        p.ids.as_deref(),
        p.parents.as_deref(),
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// Visible crop cycles, and how many match.
pub async fn cycles(
    db: &sqlx::PgPool,
    s: &Seen<'_>,
    p: &Pick,
) -> Result<(Vec<CycleRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        CycleRow,
        r#"SELECT t.id, t.village_id, t.plot_id, t.crop_id, t.season_label,
                  t.area_ha::text AS area_ha, t.tree_count, t.unit_count, t.planted_on,
                  t.harvest_start, t.harvest_end, t.status,
                  farm_provenance(t.source, t.captured_at, t.captured_by, t.verification,
                                  t.verified_by, t.verified_at, t.confidence, t.evidence_ref) AS "provenance!",
                  t.created_at, t.updated_at
           FROM farm_crop_cycles t
           WHERE t.workspace_id = $1 AND t.deleted_at IS NULL
             AND ($2 OR t.village_id = ANY($3)
                  OR t.plot_id IN (SELECT pl.id FROM farm_plots pl WHERE pl.deleted_at IS NULL
                                   AND pl.farm_id IN (SELECT farm_person_farms($4))))
             AND ($5::uuid IS NULL OR t.village_id = $5) AND ($6::uuid IS NULL OR t.id = $6)
             AND ($9::text[] IS NULL OR t.verification = ANY($9)) AND ($10::uuid[] IS NULL OR t.id = ANY($10))
             AND ($11::uuid[] IS NULL OR t.plot_id = ANY($11))
           ORDER BY t.created_at, t.id LIMIT $7 OFFSET $8"#,
        s.ws,
        s.all,
        s.villages,
        s.person,
        p.village,
        p.id,
        p.limit,
        p.offset,
        p.verification.as_deref(),
        p.ids.as_deref(),
        p.parents.as_deref(),
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_crop_cycles t
           WHERE t.workspace_id = $1 AND t.deleted_at IS NULL
             AND ($2 OR t.village_id = ANY($3)
                  OR t.plot_id IN (SELECT pl.id FROM farm_plots pl WHERE pl.deleted_at IS NULL
                                   AND pl.farm_id IN (SELECT farm_person_farms($4))))
             AND ($5::uuid IS NULL OR t.village_id = $5) AND ($6::uuid IS NULL OR t.id = $6)
             AND ($7::text[] IS NULL OR t.verification = ANY($7)) AND ($8::uuid[] IS NULL OR t.id = ANY($8))
             AND ($9::uuid[] IS NULL OR t.plot_id = ANY($9))"#,
        s.ws,
        s.all,
        s.villages,
        s.person,
        p.village,
        p.id,
        p.verification.as_deref(),
        p.ids.as_deref(),
        p.parents.as_deref(),
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// Harvest reports of one cycle the caller already sees (or one report by id), newest first.
#[allow(clippy::too_many_arguments)]
pub async fn harvests(
    db: impl PgExecutor<'_> + Copy,
    ws: Uuid,
    cycle: Option<Uuid>,
    id: Option<Uuid>,
    current_only: bool,
    limit: i64,
    offset: i64,
) -> Result<(Vec<HarvestRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        HarvestRow,
        r#"SELECT t.id, t.village_id, t.crop_cycle_id, t.kind, t.quantity_kg::text AS "quantity_kg!",
                  t.reported_for, t.is_current,
                  farm_provenance(t.source, t.captured_at, t.captured_by, t.verification,
                                  t.verified_by, t.verified_at, t.confidence, t.evidence_ref) AS "provenance!",
                  t.created_at, t.updated_at
           FROM farm_harvest_reports t
           WHERE t.workspace_id = $1 AND t.deleted_at IS NULL
             AND ($2::uuid IS NULL OR t.crop_cycle_id = $2) AND ($3::uuid IS NULL OR t.id = $3)
             AND (NOT $4 OR t.is_current)
           ORDER BY t.created_at DESC, t.id DESC LIMIT $5 OFFSET $6"#,
        ws,
        cycle,
        id,
        current_only,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_harvest_reports t
           WHERE t.workspace_id = $1 AND t.deleted_at IS NULL
             AND ($2::uuid IS NULL OR t.crop_cycle_id = $2) AND ($3::uuid IS NULL OR t.id = $3)
             AND (NOT $4 OR t.is_current)"#,
        ws,
        cycle,
        id,
        current_only,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// Harvest reports across the workspace, by their cycle's visibility (the `cycles` rule),
/// newest first.
pub async fn harvest_list(
    db: &sqlx::PgPool,
    s: &Seen<'_>,
    p: &Pick,
    current_only: bool,
) -> Result<(Vec<HarvestRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        HarvestRow,
        r#"SELECT t.id, t.village_id, t.crop_cycle_id, t.kind, t.quantity_kg::text AS "quantity_kg!",
                  t.reported_for, t.is_current,
                  farm_provenance(t.source, t.captured_at, t.captured_by, t.verification,
                                  t.verified_by, t.verified_at, t.confidence, t.evidence_ref) AS "provenance!",
                  t.created_at, t.updated_at
           FROM farm_harvest_reports t
           JOIN farm_crop_cycles c ON c.id = t.crop_cycle_id AND c.workspace_id = $1 AND c.deleted_at IS NULL
           WHERE t.workspace_id = $1 AND t.deleted_at IS NULL
             AND ($2 OR c.village_id = ANY($3)
                  OR c.plot_id IN (SELECT pl.id FROM farm_plots pl WHERE pl.deleted_at IS NULL
                                   AND pl.farm_id IN (SELECT farm_person_farms($4))))
             AND ($5::uuid IS NULL OR t.village_id = $5) AND ($6::text[] IS NULL OR t.verification = ANY($6))
             AND (NOT $7 OR t.is_current)
           ORDER BY t.created_at DESC, t.id DESC LIMIT $8 OFFSET $9"#,
        s.ws,
        s.all,
        s.villages,
        s.person,
        p.village,
        p.verification.as_deref(),
        current_only,
        p.limit,
        p.offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_harvest_reports t
           JOIN farm_crop_cycles c ON c.id = t.crop_cycle_id AND c.workspace_id = $1 AND c.deleted_at IS NULL
           WHERE t.workspace_id = $1 AND t.deleted_at IS NULL
             AND ($2 OR c.village_id = ANY($3)
                  OR c.plot_id IN (SELECT pl.id FROM farm_plots pl WHERE pl.deleted_at IS NULL
                                   AND pl.farm_id IN (SELECT farm_person_farms($4))))
             AND ($5::uuid IS NULL OR t.village_id = $5) AND ($6::text[] IS NULL OR t.verification = ANY($6))
             AND (NOT $7 OR t.is_current)"#,
        s.ws,
        s.all,
        s.villages,
        s.person,
        p.village,
        p.verification.as_deref(),
        current_only,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

// ---- registration ----

/// A replayable registration: who made it and what it returned.
pub struct Receipt {
    pub created_by: Uuid,
    pub result: Value,
}

pub async fn receipt(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    client_ref: Uuid,
) -> Result<Option<Receipt>, sqlx::Error> {
    sqlx::query_as!(
        Receipt,
        "SELECT created_by, result FROM farm_registration_receipts
         WHERE workspace_id = $1 AND client_ref = $2",
        ws,
        client_ref,
    )
    .fetch_optional(db)
    .await
}

pub async fn insert_receipt(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    client_ref: Uuid,
    created_by: Uuid,
    result: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO farm_registration_receipts (workspace_id, client_ref, created_by, result)
         VALUES ($1, $2, $3, $4)",
        ws,
        client_ref,
        created_by,
        result,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// True when the village belongs to this workspace.
pub async fn village_exists(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    village: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM farm_villages WHERE workspace_id = $1 AND id = $2) AS "e!""#,
        ws,
        village,
    )
    .fetch_one(db)
    .await
}

/// `measured_by` of each active crop of this workspace among `ids`.
pub async fn crop_measures(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    ids: &[Uuid],
) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT id, measured_by FROM farm_crops WHERE workspace_id = $1 AND id = ANY($2) AND is_active",
        ws,
        ids,
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.measured_by)).collect())
}

/// A live household of this village, locked; Some(has a head) when found.
pub async fn household_for_join(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    village: Uuid,
    id: Uuid,
) -> Result<Option<bool>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM farm_household_members m WHERE m.household_id = h.id AND m.is_head) AS "head!"
           FROM farm_households h
           WHERE h.workspace_id = $1 AND h.village_id = $2 AND h.id = $3 AND h.deleted_at IS NULL
           FOR UPDATE"#,
        ws,
        village,
        id,
    )
    .fetch_optional(db)
    .await
}

/// A live farm of this village, locked; Some(has a primary manager) when found.
pub async fn farm_for_join(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    village: Uuid,
    id: Uuid,
) -> Result<Option<bool>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM farm_managers m WHERE m.farm_id = f.id AND m.is_primary) AS "primary!"
           FROM farms f
           WHERE f.workspace_id = $1 AND f.village_id = $2 AND f.id = $3 AND f.deleted_at IS NULL
           FOR UPDATE"#,
        ws,
        village,
        id,
    )
    .fetch_optional(db)
    .await
}

/// Who wrote a record and when, as registration and corrections stamp it.
pub struct Stamp<'a> {
    pub ws: Uuid,
    pub village: Uuid,
    pub source: &'a str,
    pub actor: Uuid,
    pub confidence: Option<&'a str>,
}

pub async fn insert_person(
    db: impl PgExecutor<'_>,
    s: &Stamp<'_>,
    given_name: &str,
    family_name: &str,
    phone: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO farm_persons (workspace_id, village_id, given_name, family_name, phone,
                                   source, captured_by, confidence)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
        s.ws,
        s.village,
        given_name,
        family_name,
        phone,
        s.source,
        s.actor,
        s.confidence,
    )
    .fetch_one(db)
    .await
}

pub async fn insert_household(
    db: impl PgExecutor<'_>,
    s: &Stamp<'_>,
    label: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO farm_households (workspace_id, village_id, label, source, captured_by, confidence)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        s.ws,
        s.village,
        label,
        s.source,
        s.actor,
        s.confidence,
    )
    .fetch_one(db)
    .await
}

pub async fn insert_member(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    village: Uuid,
    household: Uuid,
    person: Uuid,
    is_head: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO farm_household_members (workspace_id, village_id, household_id, person_id, is_head)
         VALUES ($1, $2, $3, $4, $5)",
        ws,
        village,
        household,
        person,
        is_head,
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn insert_farm(
    db: impl PgExecutor<'_>,
    s: &Stamp<'_>,
    household: Uuid,
    label: &str,
    latitude: Decimal,
    longitude: Decimal,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO farms (workspace_id, village_id, household_id, label, latitude, longitude,
                            source, captured_by, confidence)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
        s.ws,
        s.village,
        household,
        label,
        latitude,
        longitude,
        s.source,
        s.actor,
        s.confidence,
    )
    .fetch_one(db)
    .await
}

pub async fn insert_manager(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    village: Uuid,
    farm: Uuid,
    person: Uuid,
    is_primary: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO farm_managers (workspace_id, village_id, farm_id, person_id, is_primary)
         VALUES ($1, $2, $3, $4, $5)",
        ws,
        village,
        farm,
        person,
        is_primary,
    )
    .execute(db)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_plot(
    db: impl PgExecutor<'_>,
    s: &Stamp<'_>,
    farm: Uuid,
    label: &str,
    area_ha: Option<Decimal>,
    latitude: Option<Decimal>,
    longitude: Option<Decimal>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO farm_plots (workspace_id, village_id, farm_id, label, area_ha, latitude, longitude,
                                 source, captured_by, confidence)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
        s.ws,
        s.village,
        farm,
        label,
        area_ha,
        latitude,
        longitude,
        s.source,
        s.actor,
        s.confidence,
    )
    .fetch_one(db)
    .await
}

/// Editable crop-cycle columns.
pub struct CycleCols<'a> {
    pub crop_id: Uuid,
    pub season_label: Option<&'a str>,
    pub area_ha: Option<Decimal>,
    pub tree_count: Option<i32>,
    pub unit_count: Option<i32>,
    pub planted_on: Option<NaiveDate>,
    pub harvest_start: Option<NaiveDate>,
    pub harvest_end: Option<NaiveDate>,
    pub status: &'a str,
}

pub async fn insert_cycle(
    db: impl PgExecutor<'_>,
    s: &Stamp<'_>,
    plot: Uuid,
    c: &CycleCols<'_>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO farm_crop_cycles (workspace_id, village_id, plot_id, crop_id, season_label,
                                       area_ha, tree_count, unit_count, planted_on, harvest_start,
                                       harvest_end, status, source, captured_by, confidence)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) RETURNING id",
        s.ws,
        s.village,
        plot,
        c.crop_id,
        c.season_label,
        c.area_ha,
        c.tree_count,
        c.unit_count,
        c.planted_on,
        c.harvest_start,
        c.harvest_end,
        c.status,
        s.source,
        s.actor,
        s.confidence,
    )
    .fetch_one(db)
    .await
}

pub async fn insert_harvest(
    db: impl PgExecutor<'_>,
    s: &Stamp<'_>,
    cycle: Uuid,
    kind: &str,
    quantity_kg: Decimal,
    reported_for: Option<NaiveDate>,
    client_ref: Option<Uuid>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO farm_harvest_reports (workspace_id, village_id, crop_cycle_id, kind, quantity_kg,
                                           reported_for, source, captured_by, confidence, client_ref)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
        s.ws,
        s.village,
        cycle,
        kind,
        quantity_kg,
        reported_for,
        s.source,
        s.actor,
        s.confidence,
        client_ref,
    )
    .fetch_one(db)
    .await
}

// ---- harvest revision ----

/// The harvest figure an earlier request with this ref wrote, as that request sent it.
pub struct RefRow {
    pub id: Uuid,
    pub crop_cycle_id: Uuid,
    pub kind: String,
    pub quantity_kg: Decimal,
    pub source: String,
    pub confidence: Option<String>,
    pub reported_for: Option<NaiveDate>,
}

pub async fn harvest_by_ref(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    client_ref: Uuid,
) -> Result<Option<RefRow>, sqlx::Error> {
    sqlx::query_as!(
        RefRow,
        "SELECT id, crop_cycle_id, kind, quantity_kg, source, confidence, reported_for
         FROM farm_harvest_reports WHERE workspace_id = $1 AND client_ref = $2",
        ws,
        client_ref
    )
    .fetch_optional(db)
    .await
}

// ---- holdings ----

/// Live households the person belongs to.
pub async fn household_ids_of(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    person: Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT hm.household_id FROM farm_household_members hm
         JOIN farm_households h ON h.id = hm.household_id AND h.deleted_at IS NULL
         WHERE hm.workspace_id = $1 AND hm.person_id = $2",
        ws,
        person
    )
    .fetch_all(db)
    .await
}

/// Farms the person manages or their household owns.
pub async fn farm_ids_of(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    person: Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT f.id AS "id!" FROM farms f
           WHERE f.workspace_id = $1 AND f.id IN (SELECT farm_person_farms($2))"#,
        ws,
        person
    )
    .fetch_all(db)
    .await
}

/// A household member with their name.
pub struct NamedMember {
    pub household_id: Uuid,
    pub person_id: Uuid,
    pub given_name: String,
    pub family_name: String,
    pub is_head: bool,
}

/// Live members of these households, in joining order.
pub async fn named_members(
    db: impl PgExecutor<'_>,
    households: &[Uuid],
) -> Result<Vec<NamedMember>, sqlx::Error> {
    sqlx::query_as!(
        NamedMember,
        "SELECT m.household_id, m.person_id, p.given_name, p.family_name, m.is_head
         FROM farm_household_members m
         JOIN farm_persons p ON p.id = m.person_id AND p.deleted_at IS NULL
         WHERE m.household_id = ANY($1)
         ORDER BY m.created_at, m.id",
        households
    )
    .fetch_all(db)
    .await
}

/// Every harvest figure of these cycles, newest first.
pub async fn harvests_of_cycles(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    cycles: &[Uuid],
) -> Result<Vec<HarvestRow>, sqlx::Error> {
    sqlx::query_as!(
        HarvestRow,
        r#"SELECT t.id, t.village_id, t.crop_cycle_id, t.kind, t.quantity_kg::text AS "quantity_kg!",
                  t.reported_for, t.is_current,
                  farm_provenance(t.source, t.captured_at, t.captured_by, t.verification,
                                  t.verified_by, t.verified_at, t.confidence, t.evidence_ref) AS "provenance!",
                  t.created_at, t.updated_at
           FROM farm_harvest_reports t
           WHERE t.workspace_id = $1 AND t.crop_cycle_id = ANY($2) AND t.deleted_at IS NULL
           ORDER BY t.created_at DESC, t.id DESC"#,
        ws,
        cycles
    )
    .fetch_all(db)
    .await
}

/// Lock a live cycle in the staff scope; its village when found.
pub async fn lock_cycle(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    all: bool,
    villages: &[Uuid],
    id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT village_id FROM farm_crop_cycles
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL AND ($3 OR village_id = ANY($4))
         FOR UPDATE",
        ws,
        id,
        all,
        villages,
    )
    .fetch_optional(db)
    .await
}

/// Retire the current figure of this kind (it stays on record).
pub async fn retire_current(
    db: impl PgExecutor<'_>,
    cycle: Uuid,
    kind: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE farm_harvest_reports SET is_current = false, updated_at = now()
         WHERE crop_cycle_id = $1 AND kind = $2 AND is_current AND deleted_at IS NULL",
        cycle,
        kind,
    )
    .execute(db)
    .await?;
    Ok(())
}

// ---- verification: lock + read, then stamp. One pair per table, all static SQL. ----

/// A record awaiting a verifier: its state and who registered it.
pub struct Verifiable {
    pub verification: String,
    pub captured_by: Option<Uuid>,
}

macro_rules! verifiable {
    ($name:ident, $stamp:ident, $select:literal, $update:literal) => {
        /// Lock a live row in the staff scope.
        pub async fn $name(
            db: impl PgExecutor<'_>,
            ws: Uuid,
            all: bool,
            villages: &[Uuid],
            id: Uuid,
        ) -> Result<Option<Verifiable>, sqlx::Error> {
            sqlx::query_as!(Verifiable, $select, ws, id, all, villages)
                .fetch_optional(db)
                .await
        }

        /// Mark verified by `actor` now.
        pub async fn $stamp(
            db: impl PgExecutor<'_>,
            id: Uuid,
            actor: Uuid,
        ) -> Result<(), sqlx::Error> {
            sqlx::query!($update, id, actor).execute(db).await?;
            Ok(())
        }
    };
}

verifiable!(
    lock_person,
    verify_person,
    "SELECT verification, captured_by FROM farm_persons WHERE workspace_id = $1 AND id = $2
     AND deleted_at IS NULL AND ($3 OR village_id = ANY($4)) FOR UPDATE",
    "UPDATE farm_persons SET verification = 'verified', verified_by = $2, verified_at = now(),
     updated_at = now() WHERE id = $1"
);
verifiable!(
    lock_household,
    verify_household,
    "SELECT verification, captured_by FROM farm_households WHERE workspace_id = $1 AND id = $2
     AND deleted_at IS NULL AND ($3 OR village_id = ANY($4)) FOR UPDATE",
    "UPDATE farm_households SET verification = 'verified', verified_by = $2, verified_at = now(),
     updated_at = now() WHERE id = $1"
);
verifiable!(
    lock_farm,
    verify_farm,
    "SELECT verification, captured_by FROM farms WHERE workspace_id = $1 AND id = $2
     AND deleted_at IS NULL AND ($3 OR village_id = ANY($4)) FOR UPDATE",
    "UPDATE farms SET verification = 'verified', verified_by = $2, verified_at = now(),
     updated_at = now() WHERE id = $1"
);
verifiable!(
    lock_plot,
    verify_plot,
    "SELECT verification, captured_by FROM farm_plots WHERE workspace_id = $1 AND id = $2
     AND deleted_at IS NULL AND ($3 OR village_id = ANY($4)) FOR UPDATE",
    "UPDATE farm_plots SET verification = 'verified', verified_by = $2, verified_at = now(),
     updated_at = now() WHERE id = $1"
);
verifiable!(
    lock_cycle_for_verify,
    verify_cycle,
    "SELECT verification, captured_by FROM farm_crop_cycles WHERE workspace_id = $1 AND id = $2
     AND deleted_at IS NULL AND ($3 OR village_id = ANY($4)) FOR UPDATE",
    "UPDATE farm_crop_cycles SET verification = 'verified', verified_by = $2, verified_at = now(),
     updated_at = now() WHERE id = $1"
);
verifiable!(
    lock_harvest,
    verify_harvest,
    "SELECT verification, captured_by FROM farm_harvest_reports WHERE workspace_id = $1 AND id = $2
     AND deleted_at IS NULL AND ($3 OR village_id = ANY($4)) FOR UPDATE",
    "UPDATE farm_harvest_reports SET verification = 'verified', verified_by = $2, verified_at = now(),
     updated_at = now() WHERE id = $1"
);

// ---- corrections: allow-listed columns, provenance re-stamped, verification reset ----

/// Scope of a correction: the row must be live and in the caller's staff villages.
pub struct Fix<'a> {
    pub ws: Uuid,
    pub id: Uuid,
    pub all: bool,
    pub villages: &'a [Uuid],
    pub actor: Uuid,
}

pub async fn correct_person(
    db: impl PgExecutor<'_>,
    f: &Fix<'_>,
    given_name: &str,
    family_name: &str,
    phone: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE farm_persons SET given_name = $6, family_name = $7, phone = $8,
             source = 'field_verified', captured_at = now(), captured_by = $5,
             verification = 'unverified', verified_by = NULL, verified_at = NULL, updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL AND ($3 OR village_id = ANY($4))",
        f.ws,
        f.id,
        f.all,
        f.villages,
        f.actor,
        given_name,
        family_name,
        phone,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

pub async fn correct_household(
    db: impl PgExecutor<'_>,
    f: &Fix<'_>,
    label: &str,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE farm_households SET label = $6,
             source = 'field_verified', captured_at = now(), captured_by = $5,
             verification = 'unverified', verified_by = NULL, verified_at = NULL, updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL AND ($3 OR village_id = ANY($4))",
        f.ws,
        f.id,
        f.all,
        f.villages,
        f.actor,
        label,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

pub async fn correct_farm(
    db: impl PgExecutor<'_>,
    f: &Fix<'_>,
    label: &str,
    latitude: Option<Decimal>,
    longitude: Option<Decimal>,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE farms SET label = $6, latitude = $7, longitude = $8,
             source = 'field_verified', captured_at = now(), captured_by = $5,
             verification = 'unverified', verified_by = NULL, verified_at = NULL, updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL AND ($3 OR village_id = ANY($4))",
        f.ws,
        f.id,
        f.all,
        f.villages,
        f.actor,
        label,
        latitude,
        longitude,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

pub async fn correct_plot(
    db: impl PgExecutor<'_>,
    f: &Fix<'_>,
    label: &str,
    area_ha: Option<Decimal>,
    latitude: Option<Decimal>,
    longitude: Option<Decimal>,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE farm_plots SET label = $6, area_ha = $7, latitude = $8, longitude = $9,
             source = 'field_verified', captured_at = now(), captured_by = $5,
             verification = 'unverified', verified_by = NULL, verified_at = NULL, updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL AND ($3 OR village_id = ANY($4))",
        f.ws,
        f.id,
        f.all,
        f.villages,
        f.actor,
        label,
        area_ha,
        latitude,
        longitude,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

pub async fn correct_cycle(
    db: impl PgExecutor<'_>,
    f: &Fix<'_>,
    c: &CycleCols<'_>,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE farm_crop_cycles SET crop_id = $6, season_label = $7, area_ha = $8, tree_count = $9,
             unit_count = $10, planted_on = $11, harvest_start = $12, harvest_end = $13, status = $14,
             source = 'field_verified', captured_at = now(), captured_by = $5,
             verification = 'unverified', verified_by = NULL, verified_at = NULL, updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL AND ($3 OR village_id = ANY($4))",
        f.ws,
        f.id,
        f.all,
        f.villages,
        f.actor,
        c.crop_id,
        c.season_label,
        c.area_ha,
        c.tree_count,
        c.unit_count,
        c.planted_on,
        c.harvest_start,
        c.harvest_end,
        c.status,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}
