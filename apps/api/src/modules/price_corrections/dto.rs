//! Price correction request/response bodies.
// Laravel Store/UpdatePriceCorrectionRequest, PriceCorrectionResource.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, present},
    modules::price_corrections::repo::CorrectionRow,
    money::{MAX, Zero, format_decimals},
};

// Laravel PriceCorrectionResource: uncast decimal columns through `formatDecimals` (2 dp,
/// trimmed; zero `"0"`, null `"0.00"`).
#[derive(Serialize, ToSchema)]
#[schema(
    description = "A price correction band: prices from `value_from` to `value_to` get rounded. Decimals are strings, 2 places trimmed (zero `\"0\"`, unset `\"0.00\"`)."
)]
pub struct CorrectionResource {
    id: Uuid,
    value_from: String,
    value_to: String,
    round_on: String,
    remove_amount: String,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
}

impl From<CorrectionRow> for CorrectionResource {
    fn from(c: CorrectionRow) -> Self {
        let fmt = |v: Option<Decimal>| format_decimals(v, 2, true, Zero::Column);
        Self {
            id: c.id,
            value_from: fmt(Some(c.value_from)),
            value_to: fmt(Some(c.value_to)),
            round_on: fmt(c.round_on),
            remove_amount: fmt(c.remove_amount),
            meta: c.meta,
            created_at: c.created_at,
            last_updated_at: c.updated_at,
        }
    }
}

/// Create response.
#[derive(Serialize, ToSchema)]
pub struct CorrectionCreated {
    pub price_correction_id: Uuid,
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

/// POST /price-corrections and POST /price-corrections/{id}.
#[derive(Deserialize, ToSchema)]
#[schema(as = PriceCorrectionRequest, description = "Create needs `value_from` and `value_to`; update keeps fields left out. Decimals as strings (`\"12.50\"`); JSON numbers also work.")]
pub struct CorrectionRequest {
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    value_from: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    value_to: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    round_on: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    remove_amount: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
    meta: Option<Option<Value>>,
}

/// Validated fields; on update `None` keeps the stored value, `Some(None)` clears it.
pub struct CorrectionInput {
    pub value_from: Option<Decimal>,
    pub value_to: Option<Decimal>,
    pub round_on: Option<Option<Decimal>>,
    pub remove_amount: Option<Option<Decimal>>,
    pub meta: Option<Option<Value>>,
}

/// Create: `value_from` and `value_to` required.
pub struct CreateCorrectionRequest(CorrectionRequest);
/// Update: every field optional; null on a band end is "required".
pub struct UpdateCorrectionRequest(CorrectionRequest);

impl<'de> Deserialize<'de> for CreateCorrectionRequest {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        CorrectionRequest::deserialize(d).map(Self)
    }
}

impl<'de> Deserialize<'de> for UpdateCorrectionRequest {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        CorrectionRequest::deserialize(d).map(Self)
    }
}

impl Validate for CreateCorrectionRequest {
    type Valid = CorrectionInput;

    fn validate(self, r: &mut Rules) -> Option<CorrectionInput> {
        let b = self.0;
        let value_from = end(r, "value_from", Some(b.value_from.flatten()));
        let value_to = end(r, "value_to", Some(b.value_to.flatten()));
        Some(CorrectionInput {
            value_from,
            value_to,
            round_on: nullable(r, "round_on", b.round_on),
            remove_amount: nullable(r, "remove_amount", b.remove_amount),
            meta: b.meta.and_then(|v| r.meta(v)),
        })
    }
}

impl Validate for UpdateCorrectionRequest {
    type Valid = CorrectionInput;

    fn validate(self, r: &mut Rules) -> Option<CorrectionInput> {
        let b = self.0;
        Some(CorrectionInput {
            value_from: end(r, "value_from", b.value_from),
            value_to: end(r, "value_to", b.value_to),
            round_on: nullable(r, "round_on", b.round_on),
            remove_amount: nullable(r, "remove_amount", b.remove_amount),
            meta: b.meta.and_then(|v| r.meta(v)),
        })
    }
}

/// Laravel `numeric` within the column range.
fn amount(r: &mut Rules, field: &str, v: &Value) -> Option<Decimal> {
    r.decimal(field, v, -MAX, MAX)
}

/// A NOT NULL band end: absent = keep (`None` in), sent null or blank = "required".
fn end(r: &mut Rules, field: &str, v: Option<Option<Value>>) -> Option<Decimal> {
    let v = r.required_value(field, v?)?;
    amount(r, field, &v)
}

/// A nullable column: absent = keep, null or blank = clear.
fn nullable(r: &mut Rules, field: &str, v: Option<Option<Value>>) -> Option<Option<Decimal>> {
    match filled(v?) {
        None => Some(None),
        Some(v) => amount(r, field, &v).map(Some),
    }
}
