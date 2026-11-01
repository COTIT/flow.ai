//! Tower SQL. Every read filters `workspace_id` and one village the service has checked.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use uuid::Uuid;

/// One crop in one harvest month. `cycle_area_ha` is the SUM OF CYCLE AREAS, not land area:
/// intercropped cycles share a plot, so it can exceed the village's hectares.
pub struct ProductionRow {
    pub crop_id: Uuid,
    pub window_month: Option<NaiveDate>,
    pub cycle_count: i64,
    pub plot_count: i64,
    pub cycle_area_ha: Option<Decimal>,
    pub tree_count: Option<i64>,
    pub unit_count: Option<i64>,
    pub expected_kg: Option<Decimal>,
    pub actual_kg: Option<Decimal>,
    pub verified_cycles: i64,
}

pub async fn production(
    db: &sqlx::PgPool,
    ws: Uuid,
    village: Uuid,
) -> Result<Vec<ProductionRow>, sqlx::Error> {
    sqlx::query_as!(
        ProductionRow,
        r#"SELECT c.crop_id, date_trunc('month', c.harvest_start)::date AS window_month,
                  count(*) AS "cycle_count!", count(DISTINCT c.plot_id) AS "plot_count!",
                  sum(c.area_ha) AS cycle_area_ha, sum(c.tree_count) AS tree_count,
                  sum(c.unit_count) AS unit_count, sum(e.quantity_kg) AS expected_kg,
                  sum(a.quantity_kg) AS actual_kg,
                  count(*) FILTER (WHERE c.verification = 'verified') AS "verified_cycles!"
           FROM farm_crop_cycles c
           LEFT JOIN farm_harvest_reports e ON e.crop_cycle_id = c.id AND e.kind = 'expected'
                AND e.is_current AND e.deleted_at IS NULL
           LEFT JOIN farm_harvest_reports a ON a.crop_cycle_id = c.id AND a.kind = 'actual'
                AND a.is_current AND a.deleted_at IS NULL
           WHERE c.workspace_id = $1 AND c.village_id = $2 AND c.deleted_at IS NULL
           GROUP BY c.crop_id, date_trunc('month', c.harvest_start)
           ORDER BY window_month NULLS LAST, c.crop_id"#,
        ws,
        village,
    )
    .fetch_all(db)
    .await
}

/// How complete one village's records are.
pub struct QualityRow {
    pub persons: i64,
    pub persons_verified: i64,
    pub farms: i64,
    pub farms_with_gps: i64,
    pub cycles: i64,
    pub cycles_with_estimate: i64,
}

pub async fn data_quality(
    db: &sqlx::PgPool,
    ws: Uuid,
    village: Uuid,
) -> Result<QualityRow, sqlx::Error> {
    sqlx::query_as!(
        QualityRow,
        r#"SELECT
             (SELECT count(*) FROM farm_persons p WHERE p.workspace_id = $1 AND p.village_id = $2
                AND p.deleted_at IS NULL) AS "persons!",
             (SELECT count(*) FROM farm_persons p WHERE p.workspace_id = $1 AND p.village_id = $2
                AND p.deleted_at IS NULL AND p.verification = 'verified') AS "persons_verified!",
             (SELECT count(*) FROM farms f WHERE f.workspace_id = $1 AND f.village_id = $2
                AND f.deleted_at IS NULL) AS "farms!",
             (SELECT count(*) FROM farms f WHERE f.workspace_id = $1 AND f.village_id = $2
                AND f.deleted_at IS NULL AND f.latitude IS NOT NULL) AS "farms_with_gps!",
             (SELECT count(*) FROM farm_crop_cycles c WHERE c.workspace_id = $1 AND c.village_id = $2
                AND c.deleted_at IS NULL) AS "cycles!",
             (SELECT count(*) FROM farm_crop_cycles c WHERE c.workspace_id = $1 AND c.village_id = $2
                AND c.deleted_at IS NULL AND EXISTS (
                    SELECT 1 FROM farm_harvest_reports h WHERE h.crop_cycle_id = c.id
                      AND h.kind = 'expected' AND h.is_current AND h.deleted_at IS NULL))
               AS "cycles_with_estimate!""#,
        ws,
        village,
    )
    .fetch_one(db)
    .await
}

/// True when the village belongs to this workspace.
pub async fn village_exists(
    db: &sqlx::PgPool,
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
