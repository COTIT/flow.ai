//! PUE request/response bodies. kW, kWh, hours and days are decimal text; indicative money is
//! integer minor units of a workspace currency.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    error::AppError,
    http::json::{Rules, Validate, filled, optional},
    modules::{
        farm_pue::{
            repo::{EnergyRow, Pick, PipelineRow, RequestRow},
            rules::{ACTIONS, target},
        },
        farm_records::dto::{ActorRef, Provenance},
        farm_surveys::dto::Money,
    },
};

const NOTE_MAX: usize = 1000;
const QUANTITY_MAX: i64 = 10_000;

/// GET /farm/pue-requests query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
    /// One village.
    #[param(value_type = Option<Uuid>)]
    pub village_id: Option<String>,
    /// Comma-separated statuses, e.g. `submitted,under_review`.
    pub status: Option<String>,
}

impl ListQuery {
    /// The filters; a malformed village id is a 422.
    pub fn pick(&self, limit: i64, offset: i64) -> Result<Pick, AppError> {
        let village = optional(self.village_id.clone())
            .map(|v| Uuid::parse_str(&v))
            .transpose()
            .map_err(|_| AppError::field("village_id", "The village id must be a valid UUID."))?;
        let statuses = optional(self.status.clone())
            .map(|s| s.split(',').map(|x| x.trim().to_owned()).collect());
        Ok(Pick {
            village,
            id: None,
            statuses,
            limit,
            offset,
        })
    }
}

/// The machine a request asks for.
#[derive(Serialize, ToSchema)]
pub struct PueEquipmentRef {
    id: Uuid,
    code: String,
    name_en: String,
    name_sw: String,
}

/// Level 1 estimate: rated kW × quantity × hours × days, from inputs snapshotted when it was
/// computed. Written only by the database.
#[derive(Serialize, ToSchema)]
pub struct EnergyEstimateResource {
    #[schema(example = "15.000")]
    rated_power_kw: String,
    quantity: i32,
    #[schema(example = "8.00")]
    hours_per_day: String,
    #[schema(example = "6.0")]
    days_per_week: String,
    #[schema(example = "15.000")]
    est_power_kw: String,
    est_kwh_per_day: String,
    est_kwh_per_week: String,
    /// `level1_rated_x_hours`.
    method: String,
    /// Always `model_estimated`.
    source: String,
    #[schema(required = true)]
    confidence: Option<String>,
    computed_at: DateTime<Utc>,
}

/// A productive-use request.
#[derive(Serialize, ToSchema)]
pub struct PueRequestResource {
    id: Uuid,
    village_id: Uuid,
    #[schema(value_type = ActorRef)]
    person: Value,
    #[schema(required = true)]
    farm_id: Option<Uuid>,
    equipment: PueEquipmentRef,
    /// `rent` or `buy`.
    acquisition: String,
    quantity: i32,
    /// Null = the catalogue's typical figure is used.
    #[schema(required = true, example = "8.00")]
    hours_per_day: Option<String>,
    #[schema(required = true, example = "6.0")]
    days_per_week: Option<String>,
    #[schema(required = true)]
    purpose: Option<String>,
    /// `draft`, `submitted`, `under_review`, `approved`, `rejected` or `withdrawn`.
    status: String,
    #[schema(required = true)]
    submitted_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    decided_at: Option<DateTime<Utc>>,
    #[schema(required = true, value_type = Option<ActorRef>)]
    decided_by: Option<Value>,
    #[schema(required = true)]
    decision_note: Option<String>,
    /// Null when the machine has no rated power.
    #[schema(required = true)]
    estimate: Option<EnergyEstimateResource>,
    #[schema(value_type = Provenance)]
    provenance: Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// `d` with exactly `places` decimals (a numeric zero comes back from Postgres without its scale).
fn fixed(d: Decimal, places: usize) -> String {
    format!("{d:.places$}")
}

impl From<RequestRow> for PueRequestResource {
    fn from(r: RequestRow) -> Self {
        let estimate = match (
            r.e_rated_power_kw,
            r.e_quantity,
            r.e_hours_per_day,
            r.e_days_per_week,
            r.e_computed_at,
        ) {
            (Some(rated), Some(quantity), Some(hours), Some(days), Some(computed_at)) => {
                Some(EnergyEstimateResource {
                    rated_power_kw: fixed(rated, 3),
                    quantity,
                    hours_per_day: fixed(hours, 2),
                    days_per_week: fixed(days, 1),
                    est_power_kw: fixed(r.e_power_kw.unwrap_or_default(), 3),
                    est_kwh_per_day: fixed(r.e_kwh_per_day.unwrap_or_default(), 3),
                    est_kwh_per_week: fixed(r.e_kwh_per_week.unwrap_or_default(), 3),
                    method: r.e_method.unwrap_or_default(),
                    source: r.e_source.unwrap_or_default(),
                    confidence: r.e_confidence,
                    computed_at,
                })
            }
            _ => None,
        };
        Self {
            id: r.id,
            village_id: r.village_id,
            person: r.person,
            farm_id: r.farm_id,
            equipment: PueEquipmentRef {
                id: r.equipment_id,
                code: r.equipment_code,
                name_en: r.equipment_name_en,
                name_sw: r.equipment_name_sw,
            },
            acquisition: r.acquisition,
            quantity: r.quantity,
            hours_per_day: r.hours_per_day.map(|d| fixed(d, 2)),
            days_per_week: r.days_per_week.map(|d| fixed(d, 1)),
            purpose: r.purpose,
            status: r.status,
            submitted_at: r.submitted_at,
            decided_at: r.decided_at,
            decided_by: r.decided_by,
            decision_note: r.decision_note,
            estimate,
            provenance: r.provenance,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// A validated new request. `status` is checked by the service (RUAHA's message is a rule,
/// not a field error).
pub struct NewRequest {
    pub id: Option<Uuid>,
    pub equipment_id: Uuid,
    pub person_id: Option<Uuid>,
    pub farm_id: Option<Uuid>,
    pub acquisition: String,
    pub quantity: i32,
    pub hours_per_day: Option<Decimal>,
    pub days_per_week: Option<Decimal>,
    pub purpose: Option<String>,
    pub status: String,
}

fn uuid(r: &mut Rules, field: &str, value: Option<String>) -> Option<Uuid> {
    let v = optional(value)?;
    let id = Uuid::parse_str(&v).ok();
    let f = field.replace('_', " ");
    r.check(
        field,
        id.is_some(),
        format!("The {f} must be a valid UUID."),
    );
    id
}

/// POST /farm/pue-requests.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "New request. A farmer files for themselves (`person_id` ignored); staff \
    name a person of their villages. Provenance, submission and decision fields and the \
    energy estimate are the server's. A client `id` makes retries safe."
)]
pub struct PueRequestCreate {
    #[schema(value_type = Option<Uuid>)]
    id: Option<String>,
    #[schema(value_type = Uuid)]
    equipment_id: Option<String>,
    /// Staff only: whose request this is.
    #[schema(value_type = Option<Uuid>)]
    person_id: Option<String>,
    /// A farm of the person's village.
    #[schema(value_type = Option<Uuid>)]
    farm_id: Option<String>,
    /// `rent` or `buy` (default); the catalogue must offer it.
    #[schema(value_type = Option<String>)]
    acquisition: Option<String>,
    /// Default 1.
    #[schema(value_type = Option<i64>)]
    quantity: Option<Value>,
    /// 0–24, at most 2 decimals; absent = the catalogue's typical figure.
    #[schema(value_type = Option<String>)]
    hours_per_day: Option<Value>,
    /// 0–7, at most 1 decimal; absent = the catalogue's typical figure.
    #[schema(value_type = Option<String>)]
    days_per_week: Option<Value>,
    #[schema(value_type = Option<String>, max_length = 1000)]
    purpose: Option<String>,
    /// `submitted` (default) or `draft`.
    #[schema(value_type = Option<String>)]
    status: Option<String>,
}

impl Validate for PueRequestCreate {
    type Valid = NewRequest;

    fn validate(self, r: &mut Rules) -> Option<NewRequest> {
        let id = uuid(r, "id", self.id);
        let equipment_id = uuid(r, "equipment_id", self.equipment_id.clone());
        if optional(self.equipment_id).is_none() {
            r.missing("equipment_id");
        }
        let acquisition = r.one_of("acquisition", self.acquisition, &["rent", "buy"], None);
        let quantity = filled(self.quantity)
            .map(|v| r.integer("quantity", &v, 1, QUANTITY_MAX))
            .unwrap_or(Some(1));
        let hours_per_day = r.measure(
            "hours_per_day",
            self.hours_per_day,
            Decimal::ZERO,
            Decimal::from(24),
            2,
        );
        let days_per_week = r.measure(
            "days_per_week",
            self.days_per_week,
            Decimal::ZERO,
            Decimal::from(7),
            1,
        );
        Some(NewRequest {
            id,
            equipment_id: equipment_id?,
            person_id: uuid(r, "person_id", self.person_id),
            farm_id: uuid(r, "farm_id", self.farm_id),
            acquisition: acquisition.unwrap_or_else(|| "buy".into()),
            quantity: i32::try_from(quantity?).ok()?,
            hours_per_day,
            days_per_week,
            purpose: r.opt_text("purpose", self.purpose, NOTE_MAX),
            status: optional(self.status).unwrap_or_else(|| "submitted".into()),
        })
    }
}

/// POST /farm/pue-requests/{request}/transitions.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "`submit` (draft → submitted), `withdraw` (draft or submitted), and for ops \
    or admin `start_review`, `approve`, `reject`. `note` is kept as the decision note."
)]
pub struct PueTransitionRequest {
    #[schema(value_type = String, example = "approve")]
    action: Option<String>,
    #[schema(value_type = Option<String>, max_length = 1000)]
    note: Option<String>,
}

/// (target status, note).
pub type Move = (&'static str, Option<String>);

impl Validate for PueTransitionRequest {
    type Valid = Move;

    fn validate(self, r: &mut Rules) -> Option<Move> {
        let actions: Vec<&str> = ACTIONS.iter().map(|(a, _)| *a).collect();
        let note = r.opt_text("note", self.note, NOTE_MAX);
        let Some(action) = optional(self.action.clone()) else {
            r.missing("action");
            return None;
        };
        let action = r.one_of("action", Some(action), &actions, None)?;
        Some((target(&action)?, note))
    }
}

/// RUAHA `v_village_energy` for one village. kW figures are decimal text; capacity fields are
/// null for a village without a current capacity row.
#[derive(Serialize, ToSchema)]
pub struct VillageEnergyResource {
    village_id: Uuid,
    #[schema(required = true, example = "500.00")]
    capacity_kw: Option<String>,
    /// `planned` or `nameplate`; never measured.
    #[schema(required = true)]
    capacity_basis: Option<String>,
    #[schema(required = true, example = "0.600")]
    simultaneity_factor: Option<String>,
    /// Sum of estimated kW of submitted and under-review requests.
    prospective_kw_raw: String,
    /// Sum of estimated kW of approved requests.
    approved_kw_raw: String,
    #[schema(required = true)]
    prospective_peak_kw: Option<String>,
    /// Approved raw × simultaneity.
    #[schema(required = true, example = "10.800")]
    approved_peak_kw: Option<String>,
    /// Capacity − approved peak.
    #[schema(required = true)]
    headroom_kw: Option<String>,
    approved_kwh_per_week: String,
    approved_assets: i64,
}

impl VillageEnergyResource {
    pub fn new(village_id: Uuid, e: EnergyRow) -> Self {
        Self {
            village_id,
            capacity_kw: e.capacity_kw.map(|d| fixed(d, 2)),
            capacity_basis: e.capacity_basis,
            simultaneity_factor: e.simultaneity_factor.map(|d| fixed(d, 3)),
            prospective_kw_raw: fixed(e.prospective_kw_raw, 3),
            approved_kw_raw: fixed(e.approved_kw_raw, 3),
            prospective_peak_kw: e.prospective_peak_kw.map(|d| fixed(d, 3)),
            approved_peak_kw: e.approved_peak_kw.map(|d| fixed(d, 3)),
            headroom_kw: e.headroom_kw.map(|d| fixed(d, 3)),
            approved_kwh_per_week: fixed(e.approved_kwh_per_week, 3),
            approved_assets: e.approved_assets,
        }
    }
}

/// One status of a village's pipeline in one currency. INDICATIVE: not financed value, not a
/// loan book.
#[derive(Serialize, ToSchema)]
pub struct PuePipelineResource {
    status: String,
    request_count: i64,
    /// Buy requests: catalogue price × quantity.
    #[schema(required = true)]
    indicative_value: Option<Money>,
    /// Rent requests: rent per day × quantity.
    #[schema(required = true)]
    indicative_rent_per_day: Option<Money>,
}

impl From<PipelineRow> for PuePipelineResource {
    fn from(p: PipelineRow) -> Self {
        let money = |minor: Option<i64>| {
            minor.map(|minor| Money {
                minor,
                currency: p.currency.clone(),
                decimals: p.decimals,
                currency_id: p.currency_id,
            })
        };
        Self {
            indicative_value: money(p.indicative_value_minor),
            indicative_rent_per_day: money(p.indicative_rent_per_day_minor),
            status: p.status,
            request_count: p.request_count,
        }
    }
}
