//! Tower response bodies. kg are decimal text with 2 decimals, hectares with 4.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::modules::farm_tower::repo::{ProductionRow, QualityRow};

fn fixed(d: Option<Decimal>, places: usize) -> Option<String> {
    d.map(|d| format!("{d:.places$}"))
}

/// One crop in one harvest month of a village.
#[derive(Serialize, ToSchema)]
pub struct VillageProductionResource {
    crop_id: Uuid,
    /// First day of the harvest month; null for cycles without a harvest window.
    #[schema(required = true)]
    window_month: Option<NaiveDate>,
    cycle_count: i64,
    plot_count: i64,
    /// The SUM OF CYCLE AREAS, not land area: intercropped cycles share a plot.
    #[schema(required = true, example = "1.6000")]
    cycle_area_ha: Option<String>,
    #[schema(required = true)]
    tree_count: Option<i64>,
    #[schema(required = true)]
    unit_count: Option<i64>,
    /// Current expected figures.
    #[schema(required = true, example = "3200.00")]
    expected_kg: Option<String>,
    /// Current actual figures.
    #[schema(required = true)]
    actual_kg: Option<String>,
    verified_cycles: i64,
}

impl From<ProductionRow> for VillageProductionResource {
    fn from(r: ProductionRow) -> Self {
        Self {
            crop_id: r.crop_id,
            window_month: r.window_month,
            cycle_count: r.cycle_count,
            plot_count: r.plot_count,
            cycle_area_ha: fixed(r.cycle_area_ha, 4),
            tree_count: r.tree_count,
            unit_count: r.unit_count,
            expected_kg: fixed(r.expected_kg, 2),
            actual_kg: fixed(r.actual_kg, 2),
            verified_cycles: r.verified_cycles,
        }
    }
}

/// How complete one village's records are (live rows only).
#[derive(Serialize, ToSchema)]
pub struct VillageDataQualityResource {
    village_id: Uuid,
    persons: i64,
    persons_verified: i64,
    farms: i64,
    farms_with_gps: i64,
    cycles: i64,
    /// Cycles with a current expected harvest figure.
    cycles_with_estimate: i64,
}

impl VillageDataQualityResource {
    pub fn new(village_id: Uuid, q: QualityRow) -> Self {
        Self {
            village_id,
            persons: q.persons,
            persons_verified: q.persons_verified,
            farms: q.farms,
            farms_with_gps: q.farms_with_gps,
            cycles: q.cycles,
            cycles_with_estimate: q.cycles_with_estimate,
        }
    }
}
