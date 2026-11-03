//! Lot request/response bodies.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    error::AppError,
    http::json::{Rules, Validate, filled, present},
    modules::{
        lots::{
            passport::Passport,
            repo::{EventRow, LineageRow, LotRow, PassportRow},
        },
        stock::repo::LEVEL_MAX,
    },
    money::round,
};

const CODE_MAX: usize = 64;
const ORIGIN_MAX: usize = 255;
const REASON_MAX: usize = 255;
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
    /// The lot this one was packed from; null when not packed here.
    #[schema(required = true)]
    parent_lot: Option<LotRef>,
    /// How it was packed from its parent; null when not packed.
    #[schema(required = true)]
    pack: Option<Pack>,
    /// The supply transfer this lot was received from; null when not received.
    #[schema(required = true)]
    received_from: Option<ReceivedFrom>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// Another lot of this workspace.
#[derive(Serialize, ToSchema)]
pub struct LotRef {
    id: Uuid,
    #[schema(example = "ILU-H-1")]
    code: String,
}

/// Kg of the parent used and units made: kg per unit = raw_kg / units.
#[derive(Serialize, ToSchema)]
pub struct Pack {
    /// Decimal string.
    #[schema(example = "7.50")]
    raw_kg: String,
    /// Decimal string.
    #[schema(example = "10.00")]
    units: String,
}

/// The transfer a lot was received from, with the sender's lot codes.
#[derive(Serialize, ToSchema)]
pub struct ReceivedFrom {
    transfer_id: Uuid,
    from_lot_code: String,
    from_lot_public_code: String,
}

impl LotResource {
    /// `l` with where it came from (`lineage` of the same lot, if read).
    pub fn new(l: LotRow, lineage: Option<&LineageRow>) -> Self {
        let parent_lot = lineage.and_then(|g| {
            Some(LotRef {
                id: g.parent_id?,
                code: g.parent_code.clone()?,
            })
        });
        let pack = lineage.and_then(|g| {
            Some(Pack {
                raw_kg: g.pack_raw_kg.clone()?,
                units: g.pack_units.clone()?,
            })
        });
        let received_from = lineage.and_then(|g| {
            Some(ReceivedFrom {
                transfer_id: g.transfer_id?,
                from_lot_code: g.from_lot_code.clone()?,
                from_lot_public_code: g.from_lot_public_code.clone()?,
            })
        });
        Self {
            parent_lot,
            pack,
            received_from,
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
    /// Who did it, with their name while they are a member of this workspace; null with no
    /// user.
    #[schema(required = true)]
    user: Option<EventUser>,
    /// `updated`: `{changed: [field, ...]}`; passport events: `{version}`.
    #[schema(required = true, value_type = Option<Object>)]
    meta: Option<Value>,
    created_at: DateTime<Utc>,
}

/// The user behind a lot event.
#[derive(Serialize, ToSchema)]
pub struct EventUser {
    id: Uuid,
    /// Null once they left the workspace.
    #[schema(required = true)]
    name: Option<String>,
}

impl From<EventRow> for LotEventResource {
    fn from(e: EventRow) -> Self {
        Self {
            id: e.id,
            kind: e.kind,
            from_status: e.from_status,
            to_status: e.to_status,
            user: e.user_id.map(|id| EventUser {
                id,
                name: e.user_name,
            }),
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

/// Batch code rule (letters, digits, . _ / -; 1-64 chars). Shared with supply batches.
pub(crate) fn code(r: &mut Rules, v: Option<String>) -> String {
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

/// A lot's passport: the draft being edited and the copy the public sees.
#[derive(Serialize, ToSchema)]
#[schema(description = "Draft and published copy of a lot's passport.")]
pub struct PassportResource {
    draft: Passport,
    /// What the public page shows; null when not published.
    #[schema(required = true)]
    published: Option<Passport>,
    /// Published versions so far; unpublishing keeps it.
    version: i32,
    #[schema(required = true)]
    published_at: Option<DateTime<Utc>>,
}

impl TryFrom<PassportRow> for PassportResource {
    type Error = AppError;

    fn try_from(r: PassportRow) -> Result<Self, AppError> {
        Ok(Self {
            draft: Passport::from_stored(r.draft)?,
            published: r.published.map(Passport::from_stored).transpose()?,
            version: r.version,
            published_at: r.published_at,
        })
    }
}

/// POST /lots/{lot}/stock-mutations.
#[derive(Deserialize, ToSchema)]
#[schema(description = "Stock in (positive) or out (negative) of this lot at a warehouse.")]
pub struct StockMutationRequest {
    #[schema(value_type = Uuid)]
    warehouse_id: Option<String>,
    /// Non-zero, 2 decimals; out may not take more than the lot has free (level − reserved).
    #[schema(value_type = String, example = "12.5")]
    quantity: Option<Value>,
    #[schema(max_length = 255)]
    reason: Option<String>,
    #[schema(value_type = Option<bool>)]
    is_correction: Option<Value>,
}

/// Validated stock mutation.
pub struct StockMutation {
    pub warehouse_id: Uuid,
    pub quantity: Decimal,
    pub reason: Option<String>,
    pub is_correction: bool,
}

impl Validate for StockMutationRequest {
    type Valid = StockMutation;

    fn validate(self, r: &mut Rules) -> Option<StockMutation> {
        // Malformed = unknown (nil matches no row): the service gives the one message.
        let warehouse_id = r
            .required("warehouse_id", self.warehouse_id)
            .map(|w| Uuid::parse_str(&w).unwrap_or(Uuid::nil()))
            .unwrap_or_default();
        let quantity = r
            .required_value("quantity", self.quantity)
            .and_then(|v| r.decimal("quantity", &v, -LEVEL_MAX, LEVEL_MAX))
            .map(|q| round(q, 2));
        if let Some(q) = quantity {
            r.check("quantity", !q.is_zero(), "The quantity must not be zero.");
        }
        let quantity = quantity.unwrap_or_default();
        Some(StockMutation {
            warehouse_id,
            quantity,
            reason: r.opt_text("reason", self.reason, REASON_MAX),
            is_correction: self
                .is_correction
                .and_then(|v| r.boolean("is_correction", &v))
                .unwrap_or(false),
        })
    }
}

/// A lot's stock in one warehouse: on hand, held by orders, and free to sell or ship.
#[derive(Serialize, ToSchema)]
pub struct LotFreeStock {
    warehouse_id: Uuid,
    /// 2 decimals.
    #[schema(example = "10.00")]
    level: String,
    #[schema(example = "2.50")]
    reserved: String,
    /// `level - reserved`.
    #[schema(example = "7.50")]
    available: String,
}

impl From<crate::modules::lots::repo::FreeRow> for LotFreeStock {
    fn from(r: crate::modules::lots::repo::FreeRow) -> Self {
        Self {
            warehouse_id: r.warehouse_id,
            level: r.level,
            reserved: r.reserved,
            available: r.available,
        }
    }
}

/// The lot's level at the warehouse after a stock mutation.
#[derive(Serialize, ToSchema)]
pub struct LotStockLevel {
    pub warehouse_id: Uuid,
    /// 2 decimals.
    #[schema(example = "12.50")]
    pub stock_lvl: String,
}

/// A sale or return of the lot that credited or debited its farmers.
#[derive(Serialize, ToSchema)]
pub struct FarmerCreditResource {
    order_id: Uuid,
    #[schema(example = "ORD-0042")]
    order_number: String,
    /// The till's `client_ref` when a till sold it; null for web and office orders.
    #[schema(required = true)]
    till_ref: Option<String>,
    /// `sale` or `return`.
    kind: String,
    /// Units sold (sale), or the change in units returned (return; positive = jars came back).
    /// Decimal string.
    #[schema(example = "3.00")]
    jars: String,
    at: DateTime<Utc>,
}

impl From<crate::modules::lots::repo::CreditRow> for FarmerCreditResource {
    fn from(r: crate::modules::lots::repo::CreditRow) -> Self {
        Self {
            order_id: r.order_id,
            order_number: r.order_number,
            till_ref: r.till_ref,
            kind: r.kind,
            jars: r.jars,
            at: r.at,
        }
    }
}

/// GET /lots/{lot}/farmer-credits: a page of credits and the farmer groups the public page
/// shows. No farmer names, ids, kg, prices or amounts.
#[derive(Serialize, ToSchema)]
pub struct FarmerCreditsPage {
    pub data: Vec<FarmerCreditResource>,
    pub meta: crate::http::list::Meta,
    /// How many farmer groups (villages) the lot's public page shows, under the supply link's
    /// allow-list; null when it shows none (no supply lineage, or `village` not shared).
    #[schema(required = true, example = 1)]
    pub farmer_groups: Option<i64>,
    /// Those groups, exactly as the public page shows them: village, farmer count (null below
    /// 3) and harvest window, under the link's allow-list. Never names, shares or amounts.
    pub farmers: Vec<crate::modules::lots::public::PublicFarmerGroup>,
}
