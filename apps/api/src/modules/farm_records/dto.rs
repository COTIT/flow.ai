//! Farm record bodies: resources (person, household, farm, plot, crop cycle, harvest report),
//! the registration payload, officer corrections and harvest revisions.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, optional},
    modules::farm_records::{
        phone::normalize_phone,
        repo::{CycleRow, FarmRow, HarvestRow, HouseholdRow, PersonRow, PlotRow},
    },
};

const NAME_MAX: usize = 80;
const LABEL_MAX: usize = 120;
const SEASON_MAX: usize = 60;
const CONFIDENCE: &[&str] = &["low", "medium", "high"];
const STATUSES: &[&str] = &["planned", "growing", "harvested", "abandoned"];
const KINDS: &[&str] = &["expected", "actual"];
/// Sources a person may write; the others come from machines or are not in use yet.
const WRITABLE_SOURCES: &[&str] = &["farmer_reported", "field_verified"];
const MAX_CYCLES: usize = 20;

// ---- resources ----

/// An account named on a record.
#[derive(Serialize, ToSchema)]
pub struct ActorRef {
    id: Uuid,
    name: String,
}

/// Where a fact came from and who vouched for it.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "How a record was learned and checked. Registration never verifies; \
    verification is its own step and is one-way."
)]
pub struct Provenance {
    /// `farmer_reported`, `field_verified`, `transaction_derived`, `sensor_derived` or `model_estimated`.
    source: String,
    captured_at: DateTime<Utc>,
    #[schema(required = true)]
    captured_by: Option<ActorRef>,
    /// `unverified`, `pending`, `verified` or `disputed`.
    verification: String,
    #[schema(required = true)]
    verified_by: Option<ActorRef>,
    #[schema(required = true)]
    verified_at: Option<DateTime<Utc>>,
    /// `low`, `medium` or `high`.
    #[schema(required = true)]
    confidence: Option<String>,
    #[schema(required = true)]
    evidence_ref: Option<String>,
}

/// A registered person. `phone` is shown only to staff of the person's village.
#[derive(Serialize, ToSchema)]
pub struct PersonResource {
    id: Uuid,
    village_id: Uuid,
    given_name: String,
    family_name: String,
    /// `+255…`; null unless the caller is staff of this village.
    #[schema(required = true)]
    phone: Option<String>,
    #[schema(value_type = Provenance)]
    provenance: Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<PersonRow> for PersonResource {
    fn from(r: PersonRow) -> Self {
        Self {
            id: r.id,
            village_id: r.village_id,
            given_name: r.given_name,
            family_name: r.family_name,
            phone: r.phone,
            provenance: r.provenance,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// One member of a household.
#[derive(Serialize, ToSchema)]
pub struct HouseholdMember {
    person_id: Uuid,
    is_head: bool,
}

/// A household and its members.
#[derive(Serialize, ToSchema)]
pub struct HouseholdResource {
    id: Uuid,
    village_id: Uuid,
    label: String,
    #[schema(value_type = Vec<HouseholdMember>)]
    members: Value,
    #[schema(value_type = Provenance)]
    provenance: Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<HouseholdRow> for HouseholdResource {
    fn from(r: HouseholdRow) -> Self {
        Self {
            id: r.id,
            village_id: r.village_id,
            label: r.label,
            members: r.members,
            provenance: r.provenance,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// One person managing a farm.
#[derive(Serialize, ToSchema)]
pub struct FarmManager {
    person_id: Uuid,
    is_primary: bool,
}

/// A farm. Coordinates are shown only to staff of the farm's village.
#[derive(Serialize, ToSchema)]
pub struct FarmResource {
    id: Uuid,
    village_id: Uuid,
    #[schema(required = true)]
    household_id: Option<Uuid>,
    label: String,
    /// Decimal degrees as text; null unless the caller is staff of this village.
    #[schema(required = true)]
    latitude: Option<String>,
    #[schema(required = true)]
    longitude: Option<String>,
    #[schema(value_type = Vec<FarmManager>)]
    managers: Value,
    #[schema(value_type = Provenance)]
    provenance: Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<FarmRow> for FarmResource {
    fn from(r: FarmRow) -> Self {
        Self {
            id: r.id,
            village_id: r.village_id,
            household_id: r.household_id,
            label: r.label,
            latitude: r.latitude,
            longitude: r.longitude,
            managers: r.managers,
            provenance: r.provenance,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// A plot of a farm.
#[derive(Serialize, ToSchema)]
pub struct PlotResource {
    id: Uuid,
    village_id: Uuid,
    farm_id: Uuid,
    label: String,
    /// Hectares as decimal text (4 decimals).
    #[schema(required = true, example = "1.8000")]
    area_ha: Option<String>,
    /// Null unless the caller is staff of this village.
    #[schema(required = true)]
    latitude: Option<String>,
    #[schema(required = true)]
    longitude: Option<String>,
    #[schema(value_type = Provenance)]
    provenance: Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<PlotRow> for PlotResource {
    fn from(r: PlotRow) -> Self {
        Self {
            id: r.id,
            village_id: r.village_id,
            farm_id: r.farm_id,
            label: r.label,
            area_ha: r.area_ha,
            latitude: r.latitude,
            longitude: r.longitude,
            provenance: r.provenance,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// One crop growing on a plot for one season. Several may share a plot (intercropping), so
/// their areas can add up to more than the plot.
#[derive(Serialize, ToSchema)]
pub struct CropCycleResource {
    id: Uuid,
    village_id: Uuid,
    plot_id: Uuid,
    crop_id: Uuid,
    #[schema(required = true)]
    season_label: Option<String>,
    /// Hectares as decimal text, for crops measured by area.
    #[schema(required = true)]
    area_ha: Option<String>,
    #[schema(required = true)]
    tree_count: Option<i32>,
    #[schema(required = true)]
    unit_count: Option<i32>,
    #[schema(required = true)]
    planted_on: Option<NaiveDate>,
    #[schema(required = true)]
    harvest_start: Option<NaiveDate>,
    #[schema(required = true)]
    harvest_end: Option<NaiveDate>,
    /// `planned`, `growing`, `harvested` or `abandoned`.
    status: String,
    #[schema(value_type = Provenance)]
    provenance: Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<CycleRow> for CropCycleResource {
    fn from(r: CycleRow) -> Self {
        Self {
            id: r.id,
            village_id: r.village_id,
            plot_id: r.plot_id,
            crop_id: r.crop_id,
            season_label: r.season_label,
            area_ha: r.area_ha,
            tree_count: r.tree_count,
            unit_count: r.unit_count,
            planted_on: r.planted_on,
            harvest_start: r.harvest_start,
            harvest_end: r.harvest_end,
            status: r.status,
            provenance: r.provenance,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// One harvest figure. A cycle has a series per kind; exactly one row per kind is current.
#[derive(Serialize, ToSchema)]
pub struct HarvestReportResource {
    id: Uuid,
    village_id: Uuid,
    crop_cycle_id: Uuid,
    /// `expected` or `actual`.
    kind: String,
    /// Kilograms as decimal text (2 decimals).
    #[schema(example = "3200.00")]
    quantity_kg: String,
    #[schema(required = true)]
    reported_for: Option<NaiveDate>,
    /// False once a newer figure replaced it; kept for the record.
    is_current: bool,
    #[schema(value_type = Provenance)]
    provenance: Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<HarvestRow> for HarvestReportResource {
    fn from(r: HarvestRow) -> Self {
        Self {
            id: r.id,
            village_id: r.village_id,
            crop_cycle_id: r.crop_cycle_id,
            kind: r.kind,
            quantity_kg: r.quantity_kg,
            reported_for: r.reported_for,
            is_current: r.is_current,
            provenance: r.provenance,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// GET list query for records.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// Only this village.
    #[param(value_type = Option<Uuid>)]
    pub village_id: Option<String>,
    /// Comma-separated: `unverified`, `pending`, `verified`, `disputed`.
    #[param(value_type = Option<String>, example = "unverified,pending")]
    pub verification: Option<String>,
    /// Plots only: the plots of this farm.
    #[param(value_type = Option<Uuid>)]
    pub farm_id: Option<String>,
    /// Crop cycles only: the cycles on this plot.
    #[param(value_type = Option<Uuid>)]
    pub plot_id: Option<String>,
    /// Persons only: part of the name, any case, or of the phone number (`0712…`, `+255712…`;
    /// matched only on persons of the caller's own staff villages). At most 100 characters.
    #[param(value_type = Option<String>, max_length = 100)]
    pub q: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

const VERIFICATION: &[&str] = &["unverified", "pending", "verified", "disputed"];

/// `?verification=a,b` -> the states, else a 422 naming the bad one.
pub fn verification_filter(
    raw: Option<&str>,
) -> Result<Option<Vec<String>>, crate::error::AppError> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let states: Vec<String> = raw.split(',').map(|s| s.trim().to_owned()).collect();
    if let Some(bad) = states.iter().find(|s| !VERIFICATION.contains(&s.as_str())) {
        return Err(crate::error::AppError::field(
            "verification",
            format!("Unknown verification state {bad}."),
        ));
    }
    Ok(Some(states))
}

/// `?q=` -> (`ILIKE` pattern on the name, phone digits without `0`/`255`), else a 422 when too long.
pub fn search_filter(
    raw: Option<&str>,
) -> Result<(Option<String>, Option<String>), crate::error::AppError> {
    let Some(q) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok((None, None));
    };
    if q.chars().count() > 100 {
        return Err(crate::error::AppError::field(
            "q",
            "The q must not be greater than 100 characters.",
        ));
    }
    let escaped = q
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let digits: String = q.chars().filter(char::is_ascii_digit).collect();
    let local = digits
        .strip_prefix("255")
        .or_else(|| digits.strip_prefix('0'))
        .unwrap_or(&digits);
    Ok((
        Some(format!("%{escaped}%")),
        (local.len() >= 3).then(|| local.to_owned()),
    ))
}

/// GET harvest reports query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct HarvestQuery {
    /// `true`: only the current figure of each kind.
    #[param(value_type = Option<bool>)]
    pub current: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

/// GET workspace harvest reports query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct HarvestListQuery {
    /// Only this village.
    #[param(value_type = Option<Uuid>)]
    pub village_id: Option<String>,
    /// Comma-separated: `unverified`, `pending`, `verified`, `disputed`.
    #[param(value_type = Option<String>, example = "unverified,pending")]
    pub verification: Option<String>,
    /// `true`: only the current figure of each kind.
    #[param(value_type = Option<bool>)]
    pub current: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

/// What a registration created; also the stored receipt a retry replays.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct RegistrationResult {
    pub person_id: Uuid,
    pub household_id: Uuid,
    pub farm_id: Uuid,
    pub plot_id: Uuid,
    pub village_id: Uuid,
    pub crop_cycle_ids: Vec<Uuid>,
    pub harvest_report_ids: Vec<Uuid>,
    /// True when this reply repeats an earlier registration with the same `client_ref`.
    pub replayed: bool,
}

// ---- shared field rules ----

fn uuid_at(r: &mut Rules, field: &str, value: Option<String>) -> Option<Uuid> {
    let v = optional(value)?;
    let id = Uuid::parse_str(&v).ok();
    r.check(
        field,
        id.is_some(),
        format!("The {field} must be a valid UUID."),
    );
    id
}

fn date_at(r: &mut Rules, field: &str, value: Option<String>) -> Option<NaiveDate> {
    let v = optional(value)?;
    let d = NaiveDate::parse_from_str(&v, "%Y-%m-%d").ok();
    r.check(
        field,
        d.is_some(),
        format!("The {field} must be a date (YYYY-MM-DD)."),
    );
    d
}

fn decimal_at(
    r: &mut Rules,
    field: &str,
    value: Option<Value>,
    min: i64,
    max: i64,
) -> Option<Decimal> {
    let v = filled(value)?;
    r.decimal(field, &v, Decimal::from(min), Decimal::from(max))
}

fn count_at(r: &mut Rules, field: &str, value: Option<Value>) -> Option<i32> {
    let v = filled(value)?;
    r.integer(field, &v, 0, i64::from(i32::MAX))
        .and_then(|n| i32::try_from(n).ok())
}

fn confidence_at(r: &mut Rules, field: &str, value: Option<String>) -> Option<String> {
    r.one_of(field, value, CONFIDENCE, None)
}

/// GPS pair: both or neither, in range.
fn point(
    r: &mut Rules,
    prefix: &str,
    lat: Option<Value>,
    lon: Option<Value>,
) -> (Option<Decimal>, Option<Decimal>) {
    let lat = decimal_at(r, &format!("{prefix}latitude"), lat, -90, 90);
    let lon = decimal_at(r, &format!("{prefix}longitude"), lon, -180, 180);
    (lat, lon)
}

fn phone_at(r: &mut Rules, field: &str, value: Option<String>) -> Option<String> {
    let v = optional(value)?;
    match normalize_phone(&v) {
        Ok(p) => Some(p),
        Err(m) => {
            r.add(field, m);
            None
        }
    }
}

// ---- registration ----

#[derive(Deserialize, ToSchema)]
pub struct PersonInput {
    #[schema(value_type = String, max_length = 80)]
    given_name: Option<String>,
    #[schema(value_type = String, max_length = 80)]
    family_name: Option<String>,
    /// Any spelling of a Tanzanian mobile; stored as `+255…`. It is the farmer's login name.
    #[schema(value_type = String, example = "+255 712 345 678")]
    phone: Option<String>,
    #[schema(value_type = Option<String>)]
    confidence: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct HouseholdInput {
    /// New household's name (default: "<family name> household").
    #[schema(value_type = Option<String>, max_length = 120)]
    label: Option<String>,
    /// Join this household of the same village instead of creating one.
    #[schema(value_type = Option<Uuid>)]
    existing_household_id: Option<String>,
    /// Default: true for a new household, false when joining one.
    is_head: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct FarmInput {
    #[schema(value_type = Option<String>, max_length = 120)]
    label: Option<String>,
    /// Required for a new farm: the officer's GPS read.
    #[schema(value_type = Option<String>)]
    latitude: Option<Value>,
    #[schema(value_type = Option<String>)]
    longitude: Option<Value>,
    #[schema(value_type = Option<String>)]
    confidence: Option<String>,
    /// Add the person to this farm of the same village instead of creating one.
    #[schema(value_type = Option<Uuid>)]
    existing_farm_id: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct PlotInput {
    #[schema(value_type = String, max_length = 120)]
    label: Option<String>,
    #[schema(value_type = Option<String>, example = "1.8")]
    area_ha: Option<Value>,
    #[schema(value_type = Option<String>)]
    latitude: Option<Value>,
    #[schema(value_type = Option<String>)]
    longitude: Option<Value>,
    #[schema(value_type = Option<String>)]
    confidence: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct HarvestInput {
    #[schema(value_type = String, example = "4100")]
    quantity_kg: Option<Value>,
    #[schema(value_type = Option<String>)]
    confidence: Option<String>,
    #[schema(value_type = Option<NaiveDate>)]
    reported_for: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct CycleInput {
    #[schema(value_type = Uuid)]
    crop_id: Option<String>,
    #[schema(value_type = Option<String>, max_length = 60)]
    season_label: Option<String>,
    /// Required when the crop is measured by area.
    #[schema(value_type = Option<String>)]
    area_ha: Option<Value>,
    /// Required when the crop is measured by tree count.
    #[schema(value_type = Option<i32>)]
    tree_count: Option<Value>,
    /// Required when the crop is measured by unit count.
    #[schema(value_type = Option<i32>)]
    unit_count: Option<Value>,
    #[schema(value_type = Option<NaiveDate>)]
    planted_on: Option<String>,
    #[schema(value_type = Option<NaiveDate>)]
    harvest_start: Option<String>,
    #[schema(value_type = Option<NaiveDate>)]
    harvest_end: Option<String>,
    /// Default `growing`.
    #[schema(value_type = Option<String>)]
    status: Option<String>,
    #[schema(value_type = Option<String>)]
    confidence: Option<String>,
    /// The farmer's expected harvest for this cycle.
    harvest: Option<HarvestInput>,
}

/// POST /farm/registrations.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "A farmer, their household, farm, plot and crop cycles in one request. \
    Make `client_ref` once when the form opens and send the same value on every retry."
)]
pub struct RegistrationRequest {
    #[schema(value_type = Uuid)]
    client_ref: Option<String>,
    #[schema(value_type = Uuid)]
    village_id: Option<String>,
    person: Option<PersonInput>,
    household: Option<HouseholdInput>,
    farm: Option<FarmInput>,
    plot: Option<PlotInput>,
    /// One per crop on the plot.
    cycles: Option<Vec<CycleInput>>,
}

/// Validated person.
pub struct NewPerson {
    pub given_name: String,
    pub family_name: String,
    pub phone: String,
    pub confidence: Option<String>,
}

/// New household or one to join.
pub enum NewHousehold {
    Create {
        label: Option<String>,
        is_head: bool,
    },
    Join {
        id: Uuid,
        is_head: bool,
    },
}

/// New farm or one to join.
pub enum NewFarm {
    Create {
        label: String,
        latitude: Decimal,
        longitude: Decimal,
        confidence: Option<String>,
    },
    Join(Uuid),
}

/// Validated plot.
pub struct NewPlot {
    pub label: String,
    pub area_ha: Option<Decimal>,
    pub latitude: Option<Decimal>,
    pub longitude: Option<Decimal>,
    pub confidence: Option<String>,
}

/// Validated expected harvest.
pub struct NewHarvest {
    pub quantity_kg: Decimal,
    pub confidence: Option<String>,
    pub reported_for: Option<NaiveDate>,
}

/// Validated crop cycle; the measure is checked against the crop in the service.
pub struct NewCycle {
    pub crop_id: Uuid,
    pub fields: CycleFields,
    pub confidence: Option<String>,
    pub harvest: Option<NewHarvest>,
}

/// Editable cycle fields (registration and correction).
pub struct CycleFields {
    pub season_label: Option<String>,
    pub area_ha: Option<Decimal>,
    pub tree_count: Option<i32>,
    pub unit_count: Option<i32>,
    pub planted_on: Option<NaiveDate>,
    pub harvest_start: Option<NaiveDate>,
    pub harvest_end: Option<NaiveDate>,
    pub status: String,
}

/// Validated registration.
pub struct NewRegistration {
    pub client_ref: Uuid,
    pub village_id: Uuid,
    pub person: NewPerson,
    pub household: NewHousehold,
    pub farm: NewFarm,
    pub plot: NewPlot,
    pub cycles: Vec<NewCycle>,
}

#[allow(clippy::too_many_arguments)]
fn cycle_fields(
    r: &mut Rules,
    p: &str,
    season_label: Option<String>,
    area_ha: Option<Value>,
    tree_count: Option<Value>,
    unit_count: Option<Value>,
    planted_on: Option<String>,
    harvest_start: Option<String>,
    harvest_end: Option<String>,
    status: Option<String>,
    default_status: Option<&str>,
) -> CycleFields {
    let season_label = r.opt_text(&format!("{p}season_label"), season_label, SEASON_MAX);
    let area_ha = decimal_at(r, &format!("{p}area_ha"), area_ha, 0, 999_999);
    let tree_count = count_at(r, &format!("{p}tree_count"), tree_count);
    let unit_count = count_at(r, &format!("{p}unit_count"), unit_count);
    let planted_on = date_at(r, &format!("{p}planted_on"), planted_on);
    let harvest_start = date_at(r, &format!("{p}harvest_start"), harvest_start);
    let harvest_end = date_at(r, &format!("{p}harvest_end"), harvest_end);
    if let (Some(s), Some(e)) = (harvest_start, harvest_end) {
        r.check(
            &format!("{p}harvest_end"),
            e >= s,
            "harvest window must end on or after it starts",
        );
    }
    let field = format!("{p}status");
    let status = match (optional(status.clone()), default_status) {
        (None, Some(d)) => Some(d.to_owned()),
        (None, None) => {
            r.missing(&field);
            None
        }
        (Some(_), _) => r.one_of(&field, status, STATUSES, None),
    };
    CycleFields {
        season_label,
        area_ha,
        tree_count,
        unit_count,
        planted_on,
        harvest_start,
        harvest_end,
        status: status.unwrap_or_default(),
    }
}

impl Validate for RegistrationRequest {
    type Valid = NewRegistration;

    fn validate(self, r: &mut Rules) -> Option<NewRegistration> {
        let client_ref = match optional(self.client_ref) {
            None => {
                r.add(
                    "client_ref",
                    "client_ref is required so a retry cannot create a duplicate farmer",
                );
                None
            }
            v => uuid_at(r, "client_ref", v),
        };
        let village_id = match optional(self.village_id) {
            None => {
                r.missing("village_id");
                None
            }
            v => uuid_at(r, "village_id", v),
        };

        let p = self.person.unwrap_or(PersonInput {
            given_name: None,
            family_name: None,
            phone: None,
            confidence: None,
        });
        let given_name = r.text("person.given_name", p.given_name, NAME_MAX);
        let family_name = r.text("person.family_name", p.family_name, NAME_MAX);
        let phone = match optional(p.phone) {
            None => {
                r.add(
                    "person.phone",
                    "a phone number is required so the farmer can sign in",
                );
                None
            }
            v => phone_at(r, "person.phone", v),
        };
        let confidence = confidence_at(r, "person.confidence", p.confidence);

        let h = self.household.unwrap_or(HouseholdInput {
            label: None,
            existing_household_id: None,
            is_head: None,
        });
        let household = match uuid_at(
            r,
            "household.existing_household_id",
            h.existing_household_id,
        ) {
            Some(id) => NewHousehold::Join {
                id,
                is_head: h.is_head.unwrap_or(false),
            },
            None => NewHousehold::Create {
                label: r.opt_text("household.label", h.label, LABEL_MAX),
                is_head: h.is_head.unwrap_or(true),
            },
        };

        let fa = self.farm.unwrap_or(FarmInput {
            label: None,
            latitude: None,
            longitude: None,
            confidence: None,
            existing_farm_id: None,
        });
        let farm = match uuid_at(r, "farm.existing_farm_id", fa.existing_farm_id) {
            Some(id) => Some(NewFarm::Join(id)),
            None => {
                let label = r.text("farm.label", fa.label, LABEL_MAX);
                let confidence = confidence_at(r, "farm.confidence", fa.confidence);
                let located =
                    filled(fa.latitude.clone()).is_some() && filled(fa.longitude.clone()).is_some();
                if !located {
                    r.add(
                        "farm.latitude",
                        "the farm location is required: capture the GPS position before saving",
                    );
                }
                match point(r, "farm.", fa.latitude, fa.longitude) {
                    (Some(latitude), Some(longitude)) => Some(NewFarm::Create {
                        label,
                        latitude,
                        longitude,
                        confidence,
                    }),
                    _ => None,
                }
            }
        };

        let pl = self.plot.unwrap_or(PlotInput {
            label: None,
            area_ha: None,
            latitude: None,
            longitude: None,
            confidence: None,
        });
        let plot_label = r.text("plot.label", pl.label, LABEL_MAX);
        let plot_area = decimal_at(r, "plot.area_ha", pl.area_ha, 0, 999_999);
        let (plot_lat, plot_lon) = point(r, "plot.", pl.latitude, pl.longitude);
        let plot_confidence = confidence_at(r, "plot.confidence", pl.confidence);

        let inputs = self.cycles.unwrap_or_default();
        r.check(
            "cycles",
            inputs.len() <= MAX_CYCLES,
            format!("at most {MAX_CYCLES} crops per plot"),
        );
        let mut cycles = Vec::with_capacity(inputs.len());
        for (i, c) in inputs.into_iter().enumerate() {
            let p = format!("cycles.{i}.");
            let crop_id = match optional(c.crop_id) {
                None => {
                    r.missing(&format!("{p}crop_id"));
                    None
                }
                v => uuid_at(r, &format!("{p}crop_id"), v),
            };
            let fields = cycle_fields(
                r,
                &p,
                c.season_label,
                c.area_ha,
                c.tree_count,
                c.unit_count,
                c.planted_on,
                c.harvest_start,
                c.harvest_end,
                c.status,
                Some("growing"),
            );
            let confidence = confidence_at(r, &format!("{p}confidence"), c.confidence);
            let harvest = c.harvest.and_then(|h| {
                let q = decimal_at(
                    r,
                    &format!("{p}harvest.quantity_kg"),
                    h.quantity_kg,
                    0,
                    9_999_999_999,
                )?;
                Some(NewHarvest {
                    quantity_kg: q,
                    confidence: confidence_at(r, &format!("{p}harvest.confidence"), h.confidence),
                    reported_for: date_at(r, &format!("{p}harvest.reported_for"), h.reported_for),
                })
            });
            if let Some(crop_id) = crop_id {
                cycles.push(NewCycle {
                    crop_id,
                    fields,
                    confidence,
                    harvest,
                });
            }
        }
        let mut crops: Vec<Uuid> = cycles.iter().map(|c| c.crop_id).collect();
        crops.sort();
        crops.dedup();
        r.check(
            "cycles",
            crops.len() == cycles.len(),
            "each crop may be registered once per plot",
        );

        Some(NewRegistration {
            client_ref: client_ref?,
            village_id: village_id?,
            person: NewPerson {
                given_name,
                family_name,
                phone: phone?,
                confidence,
            },
            household,
            farm: farm?,
            plot: NewPlot {
                label: plot_label,
                area_ha: plot_area,
                latitude: plot_lat,
                longitude: plot_lon,
                confidence: plot_confidence,
            },
            cycles,
        })
    }
}

// ---- corrections (PUT: the section's editable fields, all sent) ----

/// PUT /farm/persons/{person}.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(
    description = "Officer correction. Resets verification; a second officer must verify again."
)]
pub struct PersonCorrection {
    #[schema(value_type = String, max_length = 80)]
    given_name: Option<String>,
    #[schema(value_type = String, max_length = 80)]
    family_name: Option<String>,
    #[schema(value_type = Option<String>)]
    phone: Option<String>,
}

/// PUT /farm/households/{household}.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(description = "Officer correction. Resets verification.")]
pub struct HouseholdCorrection {
    #[schema(value_type = String, max_length = 120)]
    label: Option<String>,
}

/// PUT /farm/farms/{farm}.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(description = "Officer correction. Resets verification.")]
pub struct FarmCorrection {
    #[schema(value_type = String, max_length = 120)]
    label: Option<String>,
    #[schema(value_type = Option<String>)]
    latitude: Option<Value>,
    #[schema(value_type = Option<String>)]
    longitude: Option<Value>,
}

/// PUT /farm/plots/{plot}.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(description = "Officer correction. Resets verification.")]
pub struct PlotCorrection {
    #[schema(value_type = String, max_length = 120)]
    label: Option<String>,
    #[schema(value_type = Option<String>)]
    area_ha: Option<Value>,
    #[schema(value_type = Option<String>)]
    latitude: Option<Value>,
    #[schema(value_type = Option<String>)]
    longitude: Option<Value>,
}

/// PUT /farm/crop-cycles/{cycle}.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(description = "Officer correction. The measure must match the crop. Resets verification.")]
pub struct CycleCorrection {
    #[schema(value_type = Uuid)]
    crop_id: Option<String>,
    #[schema(value_type = Option<String>)]
    season_label: Option<String>,
    #[schema(value_type = Option<String>)]
    area_ha: Option<Value>,
    #[schema(value_type = Option<i32>)]
    tree_count: Option<Value>,
    #[schema(value_type = Option<i32>)]
    unit_count: Option<Value>,
    #[schema(value_type = Option<NaiveDate>)]
    planted_on: Option<String>,
    #[schema(value_type = Option<NaiveDate>)]
    harvest_start: Option<String>,
    #[schema(value_type = Option<NaiveDate>)]
    harvest_end: Option<String>,
    #[schema(value_type = String)]
    status: Option<String>,
}

/// Validated person correction.
pub struct PersonFix {
    pub given_name: String,
    pub family_name: String,
    pub phone: Option<String>,
}

/// Validated farm/plot correction (plots also carry an area).
pub struct PlaceFix {
    pub label: String,
    pub area_ha: Option<Decimal>,
    pub latitude: Option<Decimal>,
    pub longitude: Option<Decimal>,
}

/// Validated cycle correction.
pub struct CycleFix {
    pub crop_id: Uuid,
    pub fields: CycleFields,
}

impl Validate for PersonCorrection {
    type Valid = PersonFix;

    fn validate(self, r: &mut Rules) -> Option<PersonFix> {
        Some(PersonFix {
            given_name: r.text("given_name", self.given_name, NAME_MAX),
            family_name: r.text("family_name", self.family_name, NAME_MAX),
            phone: phone_at(r, "phone", self.phone),
        })
    }
}

impl Validate for HouseholdCorrection {
    type Valid = String;

    fn validate(self, r: &mut Rules) -> Option<String> {
        Some(r.text("label", self.label, LABEL_MAX))
    }
}

impl Validate for FarmCorrection {
    type Valid = PlaceFix;

    fn validate(self, r: &mut Rules) -> Option<PlaceFix> {
        let label = r.text("label", self.label, LABEL_MAX);
        let (latitude, longitude) = point(r, "", self.latitude, self.longitude);
        Some(PlaceFix {
            label,
            area_ha: None,
            latitude,
            longitude,
        })
    }
}

impl Validate for PlotCorrection {
    type Valid = PlaceFix;

    fn validate(self, r: &mut Rules) -> Option<PlaceFix> {
        let label = r.text("label", self.label, LABEL_MAX);
        let area_ha = decimal_at(r, "area_ha", self.area_ha, 0, 999_999);
        let (latitude, longitude) = point(r, "", self.latitude, self.longitude);
        Some(PlaceFix {
            label,
            area_ha,
            latitude,
            longitude,
        })
    }
}

impl Validate for CycleCorrection {
    type Valid = CycleFix;

    fn validate(self, r: &mut Rules) -> Option<CycleFix> {
        let crop_id = match optional(self.crop_id) {
            None => {
                r.missing("crop_id");
                None
            }
            v => uuid_at(r, "crop_id", v),
        };
        let fields = cycle_fields(
            r,
            "",
            self.season_label,
            self.area_ha,
            self.tree_count,
            self.unit_count,
            self.planted_on,
            self.harvest_start,
            self.harvest_end,
            self.status,
            None,
        );
        Some(CycleFix {
            crop_id: crop_id?,
            fields,
        })
    }
}

// ---- harvest revision ----

/// POST /farm/crop-cycles/{cycle}/harvest-reports.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "A new figure for this cycle. It replaces the current one of the same \
    kind, which stays on record."
)]
pub struct HarvestRevision {
    /// `expected` or `actual`.
    #[schema(value_type = String)]
    kind: Option<String>,
    #[schema(value_type = String, example = "4100")]
    quantity_kg: Option<Value>,
    /// `farmer_reported` (the farmer gave the number) or `field_verified` (staff measured it).
    #[schema(value_type = String)]
    source: Option<String>,
    #[schema(value_type = Option<String>)]
    confidence: Option<String>,
    #[schema(value_type = Option<NaiveDate>)]
    reported_for: Option<String>,
    /// Made on the device: a retry with the same ref returns the same figure (200); the same
    /// ref with a different figure is a 409.
    #[schema(value_type = Option<Uuid>)]
    client_ref: Option<String>,
}

/// Validated revision.
pub struct NewRevision {
    pub kind: String,
    pub quantity_kg: Decimal,
    pub source: String,
    pub confidence: Option<String>,
    pub reported_for: Option<NaiveDate>,
    pub client_ref: Option<Uuid>,
}

impl Validate for HarvestRevision {
    type Valid = NewRevision;

    fn validate(self, r: &mut Rules) -> Option<NewRevision> {
        let kind = r
            .required("kind", self.kind.clone())
            .and_then(|_| r.one_of("kind", self.kind, KINDS, None));
        let source = r
            .required("source", self.source.clone())
            .and_then(|_| r.one_of("source", self.source, WRITABLE_SOURCES, None));
        let quantity = match filled(self.quantity_kg) {
            None => {
                r.missing("quantity_kg");
                None
            }
            v => decimal_at(r, "quantity_kg", v, 0, 9_999_999_999),
        };
        Some(NewRevision {
            kind: kind?,
            quantity_kg: quantity?,
            source: source?,
            confidence: confidence_at(r, "confidence", self.confidence),
            reported_for: date_at(r, "reported_for", self.reported_for),
            client_ref: uuid_at(r, "client_ref", self.client_ref),
        })
    }
}

// ---- holdings ----

/// A household member, named.
#[derive(Serialize, ToSchema)]
pub struct HoldingMember {
    person_id: Uuid,
    given_name: String,
    family_name: String,
    is_head: bool,
}

/// A household with its members' names.
#[derive(Serialize, ToSchema)]
pub struct HoldingHousehold {
    #[serde(flatten)]
    household: HouseholdResource,
    /// The members with their names, in joining order.
    people: Vec<HoldingMember>,
}

/// The crop of a cycle.
#[derive(Serialize, ToSchema)]
pub struct CropName {
    id: Uuid,
    code: String,
    name_en: String,
    name_sw: String,
}

/// A crop cycle with its crop and every harvest figure (newest first).
#[derive(Serialize, ToSchema)]
pub struct HoldingCycle {
    #[serde(flatten)]
    cycle: CropCycleResource,
    #[schema(required = true)]
    crop: Option<CropName>,
    harvests: Vec<HarvestReportResource>,
}

/// A plot with its crop cycles.
#[derive(Serialize, ToSchema)]
pub struct HoldingPlot {
    #[serde(flatten)]
    plot: PlotResource,
    cycles: Vec<HoldingCycle>,
}

/// A farm with its plots.
#[derive(Serialize, ToSchema)]
pub struct HoldingFarm {
    #[serde(flatten)]
    farm: FarmResource,
    plots: Vec<HoldingPlot>,
}

/// Everything one person holds, as the caller may see it.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "A person's households and the farms they manage or their household owns, \
    with plots, crop cycles and harvest figures. Phone and GPS follow the usual rule: village staff only."
)]
pub struct Holdings {
    households: Vec<HoldingHousehold>,
    farms: Vec<HoldingFarm>,
}

impl Holdings {
    /// Nest the flat reads: plots under farms, cycles under plots, figures under cycles.
    #[allow(clippy::too_many_arguments)]
    pub fn assemble(
        households: Vec<HouseholdRow>,
        members: Vec<crate::modules::farm_records::repo::NamedMember>,
        farms: Vec<FarmRow>,
        plots: Vec<PlotRow>,
        cycles: Vec<CycleRow>,
        harvests: Vec<HarvestRow>,
        crops: Vec<crate::modules::farm_crops::repo::CropRow>,
    ) -> Self {
        let mut harvests: Vec<HarvestRow> = harvests;
        let mut cycles: Vec<CycleRow> = cycles;
        let mut plots: Vec<PlotRow> = plots;
        let crop = |id: Uuid| {
            crops.iter().find(|c| c.id == id).map(|c| CropName {
                id: c.id,
                code: c.code.clone(),
                name_en: c.name_en.clone(),
                name_sw: c.name_sw.clone(),
            })
        };
        let farms = farms
            .into_iter()
            .map(|f| {
                let (mine, rest): (Vec<_>, Vec<_>) =
                    plots.drain(..).partition(|p| p.farm_id == f.id);
                plots = rest;
                let plots = mine
                    .into_iter()
                    .map(|p| {
                        let (mine, rest): (Vec<_>, Vec<_>) =
                            cycles.drain(..).partition(|c| c.plot_id == p.id);
                        cycles = rest;
                        let cycles = mine
                            .into_iter()
                            .map(|c| {
                                let (mine, rest): (Vec<_>, Vec<_>) =
                                    harvests.drain(..).partition(|h| h.crop_cycle_id == c.id);
                                harvests = rest;
                                HoldingCycle {
                                    crop: crop(c.crop_id),
                                    cycle: c.into(),
                                    harvests: mine.into_iter().map(Into::into).collect(),
                                }
                            })
                            .collect();
                        HoldingPlot {
                            plot: p.into(),
                            cycles,
                        }
                    })
                    .collect();
                HoldingFarm {
                    farm: f.into(),
                    plots,
                }
            })
            .collect();
        let households = households
            .into_iter()
            .map(|h| HoldingHousehold {
                people: members
                    .iter()
                    .filter(|m| m.household_id == h.id)
                    .map(|m| HoldingMember {
                        person_id: m.person_id,
                        given_name: m.given_name.clone(),
                        family_name: m.family_name.clone(),
                        is_head: m.is_head,
                    })
                    .collect(),
                household: h.into(),
            })
            .collect();
        Self { households, farms }
    }
}
