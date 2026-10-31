//! Crop request/response bodies.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, present},
    modules::farm_crops::repo::{CropPatch, CropRow},
};

const CODE_MAX: usize = 32;
const NAME_MAX: usize = 120;
/// How a crop cycle of this crop is measured.
pub const MEASURES: &[&str] = &["area", "tree_count", "unit_count"];

/// A crop: names in both languages and the measure its cycles carry.
#[derive(Serialize, ToSchema)]
#[schema(description = "A crop farmers grow in this workspace.")]
pub struct CropResource {
    id: Uuid,
    code: String,
    name_en: String,
    name_sw: String,
    /// `area`, `tree_count` or `unit_count`: the measure each crop cycle must carry.
    measured_by: String,
    /// Inactive crops stay on old records but can't start new cycles.
    is_active: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<CropRow> for CropResource {
    fn from(c: CropRow) -> Self {
        Self {
            id: c.id,
            code: c.code,
            name_en: c.name_en,
            name_sw: c.name_sw,
            measured_by: c.measured_by,
            is_active: c.is_active,
            created_at: c.created_at,
            updated_at: c.updated_at,
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

/// POST /farm/crops.
#[derive(Deserialize, ToSchema)]
#[schema(description = "New crop.")]
pub struct CreateCropRequest {
    #[schema(value_type = String, max_length = 32, example = "MAIZE")]
    code: Option<String>,
    #[schema(value_type = String, max_length = 120, example = "Maize")]
    name_en: Option<String>,
    #[schema(value_type = String, max_length = 120, example = "Mahindi")]
    name_sw: Option<String>,
    /// `area` (default), `tree_count` or `unit_count`.
    #[schema(value_type = Option<String>)]
    measured_by: Option<String>,
    #[schema(value_type = Option<bool>)]
    is_active: Option<Value>,
}

/// PATCH /farm/crops/{crop}: only sent fields change.
#[derive(Deserialize, ToSchema)]
#[schema(description = "Crop changes; leave a field out to keep it.")]
pub struct UpdateCropRequest {
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 32)]
    code: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 120)]
    name_en: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 120)]
    name_sw: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    measured_by: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<bool>)]
    is_active: Option<Option<Value>>,
}

fn measure(r: &mut Rules, value: Option<String>) -> Option<String> {
    r.one_of("measured_by", value, MEASURES, None)
}

fn active(r: &mut Rules, value: Option<Value>) -> Option<bool> {
    filled(value).and_then(|v| r.boolean("is_active", &v))
}

impl Validate for CreateCropRequest {
    type Valid = CropPatch;

    fn validate(self, r: &mut Rules) -> Option<CropPatch> {
        Some(CropPatch {
            code: Some(r.text("code", self.code, CODE_MAX)),
            name_en: Some(r.text("name_en", self.name_en, NAME_MAX)),
            name_sw: Some(r.text("name_sw", self.name_sw, NAME_MAX)),
            measured_by: measure(r, self.measured_by),
            is_active: active(r, self.is_active),
        })
    }
}

impl Validate for UpdateCropRequest {
    type Valid = CropPatch;

    fn validate(self, r: &mut Rules) -> Option<CropPatch> {
        let measured_by = self.measured_by.map(|v| {
            let m = measure(r, v);
            if m.is_none() {
                r.missing("measured_by");
            }
            m
        });
        let is_active = self.is_active.map(|v| {
            let a = active(r, v);
            if a.is_none() {
                r.missing("is_active");
            }
            a
        });
        Some(CropPatch {
            code: r.patch_text("code", self.code, CODE_MAX),
            name_en: r.patch_text("name_en", self.name_en, NAME_MAX),
            name_sw: r.patch_text("name_sw", self.name_sw, NAME_MAX),
            measured_by: measured_by.flatten(),
            is_active: is_active.flatten(),
        })
    }
}
