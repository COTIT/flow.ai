//! Village request/response bodies.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, present},
    modules::farm_villages::repo::{VillagePatch, VillageRow},
};

const CODE_MAX: usize = 32;
const NAME_MAX: usize = 120;
const LANGUAGES: &[&str] = &["sw", "en"];

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
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<VillageRow> for VillageResource {
    fn from(v: VillageRow) -> Self {
        Self {
            id: v.id,
            code: v.code,
            name: v.name,
            latitude: v.latitude.map(|d| d.to_string()),
            longitude: v.longitude.map(|d| d.to_string()),
            primary_language: v.primary_language,
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
}

/// Validated create input.
pub struct NewVillageInput {
    pub id: Option<Uuid>,
    pub code: String,
    pub name: String,
    pub latitude: Option<Decimal>,
    pub longitude: Option<Decimal>,
    pub primary_language: String,
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
        Some(NewVillageInput {
            id,
            code: r.text("code", self.code, CODE_MAX),
            name: r.text("name", self.name, NAME_MAX),
            latitude: coordinate(r, "latitude", self.latitude, 90),
            longitude: coordinate(r, "longitude", self.longitude, 180),
            primary_language: language(r, self.primary_language).unwrap_or_else(|| "sw".into()),
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
