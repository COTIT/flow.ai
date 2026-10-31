//! Lot request/response bodies.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, present},
    modules::lots::repo::{EventRow, LotRow},
};

const CODE_MAX: usize = 64;
const ORIGIN_MAX: usize = 255;
const BAD_CODE: &str = "The code may only contain letters, digits and . _ / -";
/// Lot statuses; only `released` lots are sellable.
pub const STATUSES: [&str; 3] = ["quarantine", "released", "recalled"];

/// A lot (batch) of one product.
#[derive(Serialize, ToSchema)]
#[schema(description = "A batch of one stock-tracked product.")]
pub struct LotResource {
    id: Uuid,
    product_id: Uuid,
    /// Batch code printed on the label, unique per workspace.
    #[schema(example = "ILU-2026-07")]
    code: String,
    /// Code in the QR link `/b/{public_code}`; fixed at creation.
    #[schema(example = "7K2M9QXH4D")]
    public_code: String,
    /// `quarantine`, `released` (sellable) or `recalled`.
    status: String,
    #[schema(required = true)]
    produced_on: Option<NaiveDate>,
    #[schema(required = true)]
    best_before: Option<NaiveDate>,
    #[schema(required = true)]
    expires_on: Option<NaiveDate>,
    #[schema(required = true)]
    origin: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<LotRow> for LotResource {
    fn from(l: LotRow) -> Self {
        Self {
            id: l.id,
            product_id: l.product_id,
            code: l.code,
            public_code: l.public_code,
            status: l.status,
            produced_on: l.produced_on,
            best_before: l.best_before,
            expires_on: l.expires_on,
            origin: l.origin,
            created_at: l.created_at,
            updated_at: l.updated_at,
        }
    }
}

/// One row of a lot's trail.
#[derive(Serialize, ToSchema)]
#[schema(description = "One change in a lot's life, oldest kept forever.")]
pub struct LotEventResource {
    id: Uuid,
    /// `created`, `updated`, `status_changed`, `passport_published` or `passport_unpublished`.
    kind: String,
    #[schema(required = true)]
    from_status: Option<String>,
    #[schema(required = true)]
    to_status: Option<String>,
    /// Who did it; null once the user is gone.
    #[schema(required = true)]
    user_id: Option<Uuid>,
    /// `updated`: `{changed: [field, ...]}`; passport events: `{version}`.
    #[schema(required = true, value_type = Option<Object>)]
    meta: Option<Value>,
    created_at: DateTime<Utc>,
}

impl From<EventRow> for LotEventResource {
    fn from(e: EventRow) -> Self {
        Self {
            id: e.id,
            kind: e.kind,
            from_status: e.from_status,
            to_status: e.to_status,
            user_id: e.user_id,
            meta: e.meta,
            created_at: e.created_at,
        }
    }
}

/// GET list query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// Only lots of this product.
    #[param(value_type = Option<Uuid>)]
    pub product_id: Option<String>,
    /// Only lots in this status.
    pub status: Option<String>,
    /// Code prefix, any case.
    pub q: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

/// GET events query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

/// POST /lots.
#[derive(Deserialize, ToSchema)]
#[schema(description = "New lot. Dates are `YYYY-MM-DD`; status defaults to `released`.")]
pub struct CreateLotRequest {
    #[schema(value_type = Uuid)]
    product_id: Option<String>,
    #[schema(value_type = String, max_length = 64, pattern = "^[A-Za-z0-9._/-]+$")]
    code: Option<String>,
    #[schema(value_type = Option<String>)]
    status: Option<String>,
    #[schema(value_type = Option<NaiveDate>)]
    produced_on: Option<Value>,
    #[schema(value_type = Option<NaiveDate>)]
    best_before: Option<Value>,
    #[schema(value_type = Option<NaiveDate>)]
    expires_on: Option<Value>,
    #[schema(max_length = 255)]
    origin: Option<String>,
}

/// Validated create input.
pub struct NewLotInput {
    pub product_id: Uuid,
    pub code: String,
    pub status: String,
    pub produced_on: Option<NaiveDate>,
    pub best_before: Option<NaiveDate>,
    pub expires_on: Option<NaiveDate>,
    pub origin: Option<String>,
}

/// PATCH /lots/{lot}: absent keeps the field, null clears a nullable one.
#[derive(Deserialize, ToSchema)]
#[schema(description = "Changes to a lot. Leave a field out to keep it.")]
pub struct UpdateLotRequest {
    #[schema(value_type = Option<String>, max_length = 64)]
    code: Option<String>,
    #[schema(value_type = Option<String>)]
    status: Option<String>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<NaiveDate>)]
    produced_on: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<NaiveDate>)]
    best_before: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<NaiveDate>)]
    expires_on: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 255)]
    origin: Option<Option<String>>,
}

/// Validated patch: `None` keeps, `Some(None)` clears.
pub struct LotPatch {
    pub code: Option<String>,
    pub status: Option<String>,
    pub produced_on: Option<Option<NaiveDate>>,
    pub best_before: Option<Option<NaiveDate>>,
    pub expires_on: Option<Option<NaiveDate>>,
    pub origin: Option<Option<String>>,
}

fn code(r: &mut Rules, v: Option<String>) -> String {
    let v = r.text("code", v, CODE_MAX);
    let ok = v
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "._/-".contains(c));
    r.check("code", ok, BAD_CODE);
    v
}

fn status(r: &mut Rules, v: Option<String>) -> Option<String> {
    r.one_of("status", v, &STATUSES, None)
}

/// `YYYY-MM-DD`, blank or null = None.
fn date(r: &mut Rules, field: &str, v: Option<Value>) -> Option<NaiveDate> {
    let v = filled(v)?;
    let parsed = v
        .as_str()
        .and_then(|s| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok());
    if parsed.is_none() {
        r.add(
            field,
            format!(
                "The {} is not a valid date (YYYY-MM-DD).",
                field.replace('_', " ")
            ),
        );
    }
    parsed
}

impl Validate for CreateLotRequest {
    type Valid = NewLotInput;

    fn validate(self, r: &mut Rules) -> Option<NewLotInput> {
        // Malformed = unknown (nil matches no row): the service gives the one message.
        let product_id = r
            .required("product_id", self.product_id)
            .map(|p| Uuid::parse_str(&p).unwrap_or(Uuid::nil()))
            .unwrap_or_default();
        Some(NewLotInput {
            product_id,
            code: code(r, self.code),
            status: status(r, self.status).unwrap_or_else(|| "released".to_owned()),
            produced_on: date(r, "produced_on", self.produced_on),
            best_before: date(r, "best_before", self.best_before),
            expires_on: date(r, "expires_on", self.expires_on),
            origin: r.opt_text("origin", self.origin, ORIGIN_MAX),
        })
    }
}

impl Validate for UpdateLotRequest {
    type Valid = LotPatch;

    fn validate(self, r: &mut Rules) -> Option<LotPatch> {
        Some(LotPatch {
            code: self.code.map(|c| code(r, Some(c))),
            status: self.status.and_then(|s| {
                if s.trim().is_empty() {
                    r.missing("status");
                }
                status(r, Some(s))
            }),
            produced_on: self.produced_on.map(|v| date(r, "produced_on", v)),
            best_before: self.best_before.map(|v| date(r, "best_before", v)),
            expires_on: self.expires_on.map(|v| date(r, "expires_on", v)),
            origin: r.patch_opt_text("origin", self.origin, ORIGIN_MAX),
        })
    }
}
