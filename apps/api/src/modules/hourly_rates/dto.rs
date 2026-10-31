//! Hourly rate request/response bodies. Keys follow Laravel HourlyRateResource and the
//! Store/UpdateHourlyRateRequest rules (vimdesk-web settings/hourly-rate).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, present},
    modules::{hourly_rates::repo::HourlyRateRow, vat_groups::dto::VatGroupResource},
    money::{self, Zero, format_decimals},
};

const TEXT_MAX: usize = 255;
const LEDGER_MAX: usize = 55;
/// Laravel `rules.ValidateId` for attribute `vat group id`, domain `vat_group`.
pub const BAD_VAT_GROUP: &str = "The vat group id is an invalid id for the vat_group domain.";

// Laravel HourlyRateResource. Prices went through MoneyHelper::formatDecimals uncast,
// so a zero price is "0" and a null purchase price "0.00".
/// A billable hourly rate with its VAT group.
#[derive(Serialize, ToSchema)]
pub struct HourlyRateResource {
    id: Uuid,
    title: String,
    description: String,
    /// Decimal string, 2 decimals (zero is `"0"`).
    #[schema(example = "65.00")]
    consumer_price: String,
    /// Decimal string, 2 decimals (zero is `"0"`).
    #[schema(example = "55.00")]
    business_price: String,
    #[schema(required = true)]
    ledger_number: Option<String>,
    /// Decimal string, 2 decimals; `"0.00"` when not set.
    #[schema(example = "40.00")]
    purchase_price: String,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
    #[schema(inline)]
    relationships: Relationships,
}

#[derive(Serialize, ToSchema)]
struct Relationships {
    /// Null when the VAT group was deleted.
    #[schema(required = true)]
    vat_group: Option<VatGroupResource>,
}

fn price(v: Option<Decimal>) -> String {
    format_decimals(v, 2, true, Zero::Column)
}

impl From<HourlyRateRow> for HourlyRateResource {
    fn from(r: HourlyRateRow) -> Self {
        Self {
            id: r.id,
            title: r.title,
            description: r.description,
            consumer_price: price(Some(r.consumer_price)),
            business_price: price(Some(r.business_price)),
            ledger_number: r.ledger_number,
            purchase_price: price(r.purchase_price),
            created_at: r.created_at,
            last_updated_at: r.updated_at,
            meta: r.meta,
            relationships: Relationships {
                vat_group: r.vat_group.map(Into::into),
            },
        }
    }
}

/// Create response: the new row's id.
#[derive(Serialize, ToSchema)]
pub struct HourlyRateCreated {
    pub hourly_rate_id: Uuid,
}

/// GET list query. Unknown params ignored.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// Search text.
    pub q: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

fn required_price(r: &mut Rules, field: &str, v: Option<Value>) -> Decimal {
    r.required_value(field, v)
        .and_then(|v| r.decimal(field, &v, Decimal::ZERO, money::MAX))
        .unwrap_or_default()
}

fn optional_price(r: &mut Rules, v: Option<Value>) -> Option<Decimal> {
    filled(v).and_then(|v| r.decimal("purchase_price", &v, Decimal::ZERO, money::MAX))
}

fn vat_group_id(r: &mut Rules, v: &str) -> Option<Uuid> {
    let id = Uuid::parse_str(v.trim()).ok();
    if id.is_none() {
        r.add("vat_group_id", BAD_VAT_GROUP);
    }
    id
}

/// POST /hourly-rates. Raw `Value`s keep lenient parsing; the schema documents the clean
/// types clients should send.
#[derive(Deserialize, ToSchema)]
#[schema(description = "New hourly rate. Prices are decimal strings >= 0.")]
pub struct CreateHourlyRateRequest {
    #[schema(value_type = String, max_length = 255)]
    title: Option<String>,
    #[schema(value_type = String, max_length = 255)]
    description: Option<String>,
    #[schema(value_type = String, example = "65.00")]
    consumer_price: Option<Value>,
    #[schema(value_type = String, example = "55.00")]
    business_price: Option<Value>,
    #[schema(value_type = Option<String>, example = "40.00")]
    purchase_price: Option<Value>,
    #[schema(max_length = 55)]
    ledger_number: Option<String>,
    #[schema(value_type = Uuid)]
    vat_group_id: Option<String>,
    #[schema(value_type = Option<Object>)]
    meta: Option<Value>,
}

/// Validated create input.
pub struct NewHourlyRate {
    pub title: String,
    pub description: String,
    pub consumer_price: Decimal,
    pub business_price: Decimal,
    pub purchase_price: Option<Decimal>,
    pub ledger_number: Option<String>,
    pub vat_group_id: Uuid,
    pub meta: Option<Value>,
}

impl Validate for CreateHourlyRateRequest {
    type Valid = NewHourlyRate;

    fn validate(self, r: &mut Rules) -> Option<NewHourlyRate> {
        let vat = r.text("vat_group_id", self.vat_group_id, usize::MAX);
        let vat_group_id = if vat.is_empty() {
            None
        } else {
            vat_group_id(r, &vat)
        };
        Some(NewHourlyRate {
            title: r.text("title", self.title, TEXT_MAX),
            description: r.text("description", self.description, TEXT_MAX),
            consumer_price: required_price(r, "consumer_price", self.consumer_price),
            business_price: required_price(r, "business_price", self.business_price),
            purchase_price: optional_price(r, self.purchase_price),
            ledger_number: r.opt_text("ledger_number", self.ledger_number, LEDGER_MAX),
            vat_group_id: vat_group_id.unwrap_or_default(),
            meta: r.meta(self.meta).flatten(),
        })
    }
}

// POST /hourly-rates/{id}. Laravel requires both prices on update; the rest patches
// (absent = keep, null clears the nullable ones, null title/description = 422).
/// POST /hourly-rates/{id}.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Changes to an hourly rate. Both prices are always required; leave another \
        field out to keep it, send null to clear it (`title` and `description` can't be null)."
)]
pub struct UpdateHourlyRateRequest {
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = String, required = false, max_length = 255)]
    title: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = String, required = false, max_length = 255)]
    description: Option<Option<String>>,
    #[schema(value_type = String, example = "65.00")]
    consumer_price: Option<Value>,
    #[schema(value_type = String, example = "55.00")]
    business_price: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, example = "40.00")]
    purchase_price: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 55)]
    ledger_number: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>)]
    vat_group_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
    meta: Option<Option<Value>>,
}

/// Validated update; `None` keeps the stored value.
pub struct HourlyRatePatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub consumer_price: Decimal,
    pub business_price: Decimal,
    pub purchase_price: Option<Option<Decimal>>,
    pub ledger_number: Option<Option<String>>,
    pub vat_group_id: Option<Option<Uuid>>,
    pub meta: Option<Option<Value>>,
}

impl Validate for UpdateHourlyRateRequest {
    type Valid = HourlyRatePatch;

    fn validate(self, r: &mut Rules) -> Option<HourlyRatePatch> {
        let vat_group = self
            .vat_group_id
            .map(|v| match v.as_deref().map(str::trim) {
                None | Some("") => None,
                Some(v) => vat_group_id(r, v),
            });
        Some(HourlyRatePatch {
            title: r.patch_text("title", self.title, TEXT_MAX),
            description: r.patch_text("description", self.description, TEXT_MAX),
            consumer_price: required_price(r, "consumer_price", self.consumer_price),
            business_price: required_price(r, "business_price", self.business_price),
            purchase_price: self.purchase_price.map(|v| optional_price(r, v)),
            ledger_number: r.patch_opt_text("ledger_number", self.ledger_number, LEDGER_MAX),
            vat_group_id: vat_group,
            meta: self.meta.and_then(|v| r.meta(v)),
        })
    }
}
