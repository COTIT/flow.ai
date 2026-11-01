//! Village request/response bodies.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, human, optional, present},
    modules::farm_villages::repo::{CapacityRow, NewCapacity, VillagePatch, VillageRow},
};

const CODE_MAX: usize = 32;
const NAME_MAX: usize = 120;
const LANGUAGES: &[&str] = &["sw", "en"];
/// No `measured`: capacity is never reported as measured (RUAHA plan S03).
const BASES: &[&str] = &["planned", "nameplate"];
const NOTE_MAX: usize = 500;
/// RUAHA `app_create_village`'s refusal, verbatim.
const CAPACITY_REQUIRED: &str = "a planned capacity is required";

/// A village's current capacity: planned or nameplate, never measured.
#[derive(Serialize, ToSchema)]
pub struct CapacityResource {
    id: Uuid,
    /// kW as text, 2 decimals.
    #[schema(example = "500.00")]
    capacity_kw: String,
    /// `planned` or `nameplate`.
    basis: String,
    /// Peak = rated power × this factor (0 < f ≤ 1), 3 decimals.
    #[schema(example = "0.600")]
    simultaneity_factor: String,
    #[schema(required = true)]
    source_note: Option<String>,
    effective_from: NaiveDate,
}

impl From<CapacityRow> for CapacityResource {
    fn from(c: CapacityRow) -> Self {
        Self {
            id: c.id,
            capacity_kw: c.capacity_kw.to_string(),
            basis: c.basis,
            simultaneity_factor: c.simultaneity_factor.to_string(),
            source_note: c.source_note,
            effective_from: c.effective_from,
        }
    }
}

/// A village: a scope inside the workspace that farmers and field officers belong to.
#[derive(Serialize, ToSchema)]
#[schema(description = "A village inside this workspace.")]
pub struct VillageResource {
    id: Uuid,
    code: String,
    name: String,
    /// Decimal degrees as text, 6 decimals.
    #[schema(required = true, example = "-7.700000")]
    latitude: Option<String>,
    #[schema(required = true, example = "35.700000")]
    longitude: Option<String>,
    /// `sw` or `en`.
    primary_language: String,
    /// Current capacity; null when the caller is neither a manager nor a member of this village.
    #[schema(required = true)]
    capacity: Option<CapacityResource>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<VillageRow> for VillageResource {
    fn from(v: VillageRow) -> Self {
        let capacity = match (
            v.capacity_id,
            v.capacity_kw,
            v.basis,
            v.simultaneity_factor,
            v.effective_from,
        ) {
            (
                Some(id),
                Some(capacity_kw),
                Some(basis),
                Some(simultaneity_factor),
                Some(effective_from),
            ) => Some(
                CapacityRow {
                    id,
                    capacity_kw,
                    basis,
                    simultaneity_factor,
                    source_note: v.source_note,
                    effective_from,
                }
                .into(),
            ),
            _ => None,
        };
        Self {
            id: v.id,
            code: v.code,
            name: v.name,
            latitude: v.latitude.map(|d| d.to_string()),
            longitude: v.longitude.map(|d| d.to_string()),
            primary_language: v.primary_language,
            capacity,
            created_at: v.created_at,
            updated_at: v.updated_at,
        }
    }
}

/// GET list query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

/// POST /farm/villages.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "New village. Send a client-made `id` to make retries safe: a repeat \
    with the same id returns the existing village."
)]
pub struct CreateVillageRequest {
    #[schema(value_type = Option<Uuid>)]
    id: Option<String>,
    #[schema(value_type = String, max_length = 32, example = "ILUNDO")]
    code: Option<String>,
    #[schema(value_type = String, max_length = 120, example = "Ilundo")]
    name: Option<String>,
    #[schema(value_type = Option<String>, example = "-7.700000")]
    latitude: Option<Value>,
    #[schema(value_type = Option<String>, example = "35.700000")]
    longitude: Option<Value>,
    /// `sw` (default) or `en`.
    #[schema(value_type = Option<String>)]
    primary_language: Option<String>,
    /// Required: the village's first planned capacity.
    #[schema(value_type = CapacityRequest)]
    capacity: Option<Value>,
}

/// POST /farm/villages/{village}/capacity, and `capacity` on village create.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "A capacity figure. It becomes the village's current one; the previous \
    one is kept as history."
)]
pub struct CapacityRequest {
    /// Client id: a retried save replays instead of recording the figure twice.
    #[schema(value_type = Option<Uuid>)]
    id: Option<String>,
    /// kW, at most 2 decimals.
    #[schema(value_type = String, example = "500")]
    capacity_kw: Option<Value>,
    /// `planned` (default) or `nameplate`. Never `measured`.
    #[schema(value_type = Option<String>)]
    basis: Option<String>,
    /// 0 < f ≤ 1, at most 3 decimals; default 1.
    #[schema(value_type = Option<String>, example = "0.600")]
    simultaneity_factor: Option<Value>,
    #[schema(value_type = Option<String>, max_length = 500)]
    source_note: Option<String>,
    /// YYYY-MM-DD; default today.
    #[schema(value_type = Option<String>, format = Date)]
    effective_from: Option<String>,
}

impl Validate for CapacityRequest {
    type Valid = NewCapacity;

    fn validate(self, r: &mut Rules) -> Option<NewCapacity> {
        let capacity_kw = r.measure(
            "capacity_kw",
            self.capacity_kw,
            Decimal::ZERO,
            Decimal::new(9_999_999_999, 2),
            2,
        );
        let factor = r.measure(
            "simultaneity_factor",
            self.simultaneity_factor,
            Decimal::ZERO,
            Decimal::ONE,
            3,
        );
        r.check(
            "simultaneity_factor",
            factor != Some(Decimal::ZERO),
            "The simultaneity factor must be greater than 0.",
        );
        let effective_from = optional(self.effective_from).and_then(|v| {
            let d = NaiveDate::parse_from_str(&v, "%Y-%m-%d").ok();
            r.check(
                "effective_from",
                d.is_some(),
                "The effective from must be a date (YYYY-MM-DD).",
            );
            d
        });
        let id = optional(self.id).and_then(|v| {
            let id = Uuid::parse_str(&v).ok();
            r.check("id", id.is_some(), "The id must be a valid UUID.");
            id
        });
        Some(NewCapacity {
            id,
            capacity_kw: capacity_kw?,
            basis: r.one_of("basis", self.basis, BASES, None),
            simultaneity_factor: factor,
            source_note: r.opt_text("source_note", self.source_note, NOTE_MAX),
            effective_from,
        })
    }
}

/// `capacity` inside a village create: required, checked as a `CapacityRequest` with
/// `capacity.` before each field.
fn capacity(r: &mut Rules, value: Option<Value>) -> Option<NewCapacity> {
    let Some(value) = value.filter(|v| filled(v.get("capacity_kw").cloned()).is_some()) else {
        r.add("capacity", CAPACITY_REQUIRED);
        return None;
    };
    let req = match serde_json::from_value::<CapacityRequest>(value.clone()) {
        Ok(req) => req,
        Err(_) => {
            // Only the text fields can fail to deserialize; name the one that did.
            let wrong: Vec<&str> = ["basis", "source_note", "effective_from"]
                .into_iter()
                .filter(|f| value.get(f).is_some_and(|v| !v.is_null() && !v.is_string()))
                .collect();
            for f in &wrong {
                r.add(
                    &format!("capacity.{f}"),
                    format!("The {} must be a string.", human(f)),
                );
            }
            if wrong.is_empty() {
                r.add("capacity", CAPACITY_REQUIRED);
            }
            return None;
        }
    };
    let mut inner = Rules::default();
    let c = req.validate(&mut inner);
    if inner.merge_into_prefixed(r, "capacity.") {
        return None;
    }
    c
}

/// Validated create input.
pub struct NewVillageInput {
    pub id: Option<Uuid>,
    pub code: String,
    pub name: String,
    pub latitude: Option<Decimal>,
    pub longitude: Option<Decimal>,
    pub primary_language: String,
    pub capacity: NewCapacity,
}

/// PATCH /farm/villages/{village}: only sent fields change.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Village changes; leave a field out to keep it, send null to clear a coordinate."
)]
pub struct UpdateVillageRequest {
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 32)]
    code: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 120)]
    name: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    latitude: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    longitude: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    primary_language: Option<Option<String>>,
}

/// Optional coordinate within ±`limit` degrees.
fn coordinate(r: &mut Rules, field: &str, value: Option<Value>, limit: i64) -> Option<Decimal> {
    let v = filled(value)?;
    r.decimal(field, &v, Decimal::from(-limit), Decimal::from(limit))
}

fn language(r: &mut Rules, value: Option<String>) -> Option<String> {
    r.one_of("primary_language", value, LANGUAGES, None)
}

impl Validate for CreateVillageRequest {
    type Valid = NewVillageInput;

    fn validate(self, r: &mut Rules) -> Option<NewVillageInput> {
        let id = self.id.filter(|s| !s.trim().is_empty()).and_then(|s| {
            let id = Uuid::parse_str(s.trim()).ok();
            r.check("id", id.is_some(), "The id must be a valid UUID.");
            id
        });
        let code = r.text("code", self.code, CODE_MAX);
        let name = r.text("name", self.name, NAME_MAX);
        let latitude = coordinate(r, "latitude", self.latitude, 90);
        let longitude = coordinate(r, "longitude", self.longitude, 180);
        let primary_language = language(r, self.primary_language).unwrap_or_else(|| "sw".into());
        Some(NewVillageInput {
            id,
            code,
            name,
            latitude,
            longitude,
            primary_language,
            capacity: capacity(r, self.capacity)?,
        })
    }
}

impl Validate for UpdateVillageRequest {
    type Valid = VillagePatch;

    fn validate(self, r: &mut Rules) -> Option<VillagePatch> {
        Some(VillagePatch {
            code: r.patch_text("code", self.code, CODE_MAX),
            name: r.patch_text("name", self.name, NAME_MAX),
            latitude: self.latitude.map(|v| coordinate(r, "latitude", v, 90)),
            longitude: self.longitude.map(|v| coordinate(r, "longitude", v, 180)),
            primary_language: self.primary_language.and_then(|v| {
                let l = language(r, v);
                if l.is_none() {
                    r.missing("primary_language");
                }
                l
            }),
        })
    }
}
