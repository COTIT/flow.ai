//! VAT group request/response bodies. Keys follow Laravel VatGroupResource and the
//! Store/Update/GetVatGroupRequest rules (vimdesk-web vat-groups.store.js).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    error::AppError,
    http::json::{Rules, Validate, filled, present},
    modules::vat_groups::repo::VatGroupRow,
    money::{Zero, format_decimals},
};

const NAME_MAX: usize = 255;

/// Laravel VatGroupResource. `vat_amount` is a string: Laravel cast it to float and printed
/// it through MoneyHelper::formatDecimals (2 dp, zero is "0.00").
#[derive(Serialize)]
pub struct VatGroupResource {
    id: Uuid,
    name: String,
    vat_amount: String,
    default_group: bool,
    country: Option<String>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
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
#[derive(Serialize)]
pub struct VatGroupCreated {
    pub vat_group_id: Uuid,
}

/// GET list query (Laravel GetVatGroupRequest). Unknown params ignored.
#[derive(Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub country: Option<String>,
    pub page: Option<String>,
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

/// POST /settings/vat-groups.
#[derive(Deserialize)]
pub struct CreateVatGroupRequest {
    name: Option<String>,
    vat_amount: Option<Value>,
    default_group: Option<Value>,
    country: Option<String>,
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

/// POST /settings/vat-groups/{id}. Laravel's update rules equal the store rules, so `name`
/// and `vat_amount` stay required; the other keys patch (absent = keep).
#[derive(Deserialize)]
pub struct UpdateVatGroupRequest {
    name: Option<String>,
    vat_amount: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    default_group: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    country: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
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
