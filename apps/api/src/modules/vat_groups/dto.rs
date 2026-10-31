//! VAT group request/response bodies. Keys follow Laravel VatGroupResource and the
//! Store/Update/GetVatGroupRequest rules (vimdesk-web vat-groups.store.js).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    error::AppError,
    http::json::{Rules, Validate, filled, present},
    modules::vat_groups::repo::VatGroupRow,
    money::{Zero, format_decimals},
};

const NAME_MAX: usize = 255;

// Laravel VatGroupResource. `vat_amount` is a string: Laravel cast it to float and printed
// it through MoneyHelper::formatDecimals (2 dp, zero is "0.00").
/// A VAT rate the workspace can apply; one is the default.
#[derive(Serialize, ToSchema)]
pub struct VatGroupResource {
    id: Uuid,
    name: String,
    /// Percentage, 2 decimals (`"21.00"`, zero is `"0.00"`).
    #[schema(example = "21.00")]
    vat_amount: String,
    default_group: bool,
    /// ISO 3166 alpha-2 country, uppercase.
    #[schema(required = true)]
    country: Option<String>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
}

impl From<VatGroupRow> for VatGroupResource {
    fn from(g: VatGroupRow) -> Self {
        Self {
            id: g.id,
            name: g.name,
            vat_amount: format_decimals(Some(g.vat_amount), 2, true, Zero::Falsy),
            default_group: g.default_group,
            country: g.country,
            created_at: g.created_at,
            last_updated_at: g.updated_at,
            meta: g.meta,
        }
    }
}

/// Create response: the new row's id.
#[derive(Serialize, ToSchema)]
pub struct VatGroupCreated {
    pub vat_group_id: Uuid,
}

/// GET list query (Laravel GetVatGroupRequest). Unknown params ignored.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// Search on name.
    pub q: Option<String>,
    /// ISO 3166 alpha-2 country, any case.
    pub country: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

impl ListQuery {
    /// Uppercase country filter; 422 when it isn't an ISO alpha-2 code.
    pub fn country(&self) -> Result<Option<String>, AppError> {
        let Some(code) = self
            .country
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty())
        else {
            return Ok(None);
        };
        let mut r = Rules::default();
        let code = r.country("country", code);
        r.into_result()?;
        Ok(code)
    }
}

fn vat_amount(r: &mut Rules, v: Option<Value>) -> Decimal {
    r.required_value("vat_amount", v)
        .and_then(|v| r.decimal("vat_amount", &v, Decimal::ZERO, Decimal::ONE_HUNDRED))
        .unwrap_or_default()
}

/// Laravel `nullable|boolean` on a NOT NULL column: null = false.
fn default_group(r: &mut Rules, v: Option<Value>) -> bool {
    filled(v)
        .and_then(|v| r.boolean("default_group", &v))
        .unwrap_or_default()
}

fn country(r: &mut Rules, v: Option<String>) -> Option<String> {
    let code = r.opt_text("country", v, usize::MAX)?;
    r.country("country", &code)
}

/// POST /settings/vat-groups. Raw `Value`s keep lenient parsing ("21", "true"); the schema
/// documents the clean types clients should send.
#[derive(Deserialize, ToSchema)]
#[schema(description = "New VAT group.")]
pub struct CreateVatGroupRequest {
    #[schema(value_type = String, max_length = 255)]
    name: Option<String>,
    /// Percentage 0..=100, as a decimal string.
    #[schema(value_type = String, example = "21.00")]
    vat_amount: Option<Value>,
    #[schema(value_type = Option<bool>)]
    default_group: Option<Value>,
    /// ISO 3166 alpha-2 country, any case.
    country: Option<String>,
    #[schema(value_type = Option<Object>)]
    meta: Option<Value>,
}

/// Validated create input.
pub struct NewVatGroup {
    pub name: String,
    pub vat_amount: Decimal,
    pub default_group: bool,
    pub country: Option<String>,
    pub meta: Option<Value>,
}

impl Validate for CreateVatGroupRequest {
    type Valid = NewVatGroup;

    fn validate(self, r: &mut Rules) -> Option<NewVatGroup> {
        Some(NewVatGroup {
            name: r.text("name", self.name, NAME_MAX),
            vat_amount: vat_amount(r, self.vat_amount),
            default_group: default_group(r, self.default_group),
            country: country(r, self.country),
            meta: r.meta(self.meta).flatten(),
        })
    }
}

// POST /settings/vat-groups/{id}. Laravel's update rules equal the store rules, so `name`
// and `vat_amount` stay required; the other keys patch (absent = keep).
/// POST /settings/vat-groups/{id}.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Changes to a VAT group. `name` and `vat_amount` are always required; leave \
        another field out to keep it, send null to clear it."
)]
pub struct UpdateVatGroupRequest {
    #[schema(value_type = String, max_length = 255)]
    name: Option<String>,
    /// Percentage 0..=100, as a decimal string.
    #[schema(value_type = String, example = "21.00")]
    vat_amount: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<bool>)]
    default_group: Option<Option<Value>>,
    /// ISO 3166 alpha-2 country, any case.
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    country: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
    meta: Option<Option<Value>>,
}

/// Validated update; `None` keeps the stored value.
pub struct VatGroupPatch {
    pub name: String,
    pub vat_amount: Decimal,
    pub default_group: Option<bool>,
    pub country: Option<Option<String>>,
    pub meta: Option<Option<Value>>,
}

impl Validate for UpdateVatGroupRequest {
    type Valid = VatGroupPatch;

    fn validate(self, r: &mut Rules) -> Option<VatGroupPatch> {
        Some(VatGroupPatch {
            name: r.text("name", self.name, NAME_MAX),
            vat_amount: vat_amount(r, self.vat_amount),
            default_group: self.default_group.map(|v| default_group(r, v)),
            country: self.country.map(|v| country(r, v)),
            meta: self.meta.and_then(|v| r.meta(v)),
        })
    }
}
