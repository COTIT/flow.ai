//! Packing and transfer request/response bodies.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::{lots::dto::code, supply_chain::repo::TransferRow, supply_links::dto::Partner},
};

fn uuid(r: &mut Rules, field: &str, v: Option<String>) -> Uuid {
    let Some(v) = r.required(field, v) else {
        return Uuid::nil();
    };
    let id = Uuid::parse_str(v.trim()).ok();
    r.check(
        field,
        id.is_some(),
        format!("The {field} must be a valid UUID."),
    );
    id.unwrap_or_default()
}

/// A positive quantity with at most 2 decimals.
fn quantity(r: &mut Rules, field: &str, v: Option<Value>) -> Decimal {
    let Some(v) = r.required_value(field, v) else {
        return Decimal::ZERO;
    };
    let max = Decimal::new(99_999_999_999_999_999, 2);
    let Some(q) = r.decimal(field, &v, Decimal::new(1, 2), max) else {
        return Decimal::ZERO;
    };
    r.check(
        field,
        q.scale() <= 2,
        format!(
            "The {} must have at most 2 decimal places.",
            field.replace('_', " ")
        ),
    );
    q
}

fn date(r: &mut Rules, field: &str, v: Option<String>) -> Option<NaiveDate> {
    let v = v.filter(|s| !s.trim().is_empty())?;
    let d = NaiveDate::parse_from_str(v.trim(), "%Y-%m-%d").ok();
    r.check(
        field,
        d.is_some(),
        format!("The {field} must be a date (YYYY-MM-DD)."),
    );
    d
}

/// POST /lots/{lot}/packings.
#[derive(Deserialize, ToSchema)]
#[schema(description = "Pack kg of a supply lot into a new child lot of units (jars).")]
pub struct PackingRequest {
    /// Stock-tracked product of the jars.
    #[schema(value_type = Uuid)]
    product_id: Option<String>,
    /// The new lot's code.
    #[schema(value_type = String, example = "ILU-J-2026-01")]
    code: Option<String>,
    /// Units made, more than 0, at most 2 decimals.
    #[schema(value_type = String, example = "10")]
    units: Option<Value>,
    /// Kg of the parent lot used, more than 0, at most 2 decimals.
    #[schema(value_type = String, example = "7.50")]
    raw_kg: Option<Value>,
    /// Where the parent's kg are taken from and the jars go.
    #[schema(value_type = Uuid)]
    warehouse_id: Option<String>,
    #[schema(value_type = Option<NaiveDate>)]
    best_before: Option<String>,
    #[schema(value_type = Option<NaiveDate>)]
    expires_on: Option<String>,
}

/// Validated packing.
pub struct NewPacking {
    pub product_id: Uuid,
    pub code: String,
    pub units: Decimal,
    pub raw_kg: Decimal,
    pub warehouse_id: Uuid,
    pub best_before: Option<NaiveDate>,
    pub expires_on: Option<NaiveDate>,
}

impl Validate for PackingRequest {
    type Valid = NewPacking;

    fn validate(self, r: &mut Rules) -> Option<NewPacking> {
        Some(NewPacking {
            product_id: uuid(r, "product_id", self.product_id),
            code: code(r, self.code),
            units: quantity(r, "units", self.units),
            raw_kg: quantity(r, "raw_kg", self.raw_kg),
            warehouse_id: uuid(r, "warehouse_id", self.warehouse_id),
            best_before: date(r, "best_before", self.best_before),
            expires_on: date(r, "expires_on", self.expires_on),
        })
    }
}

/// The new packed lot.
#[derive(Serialize, ToSchema)]
pub struct PackedLotResource {
    pub id: Uuid,
    pub code: String,
    pub public_code: String,
    pub product_id: Uuid,
    pub parent_lot_id: Uuid,
    /// Kg of the parent used.
    #[schema(example = "7.50")]
    pub pack_raw_kg: String,
    /// Units made.
    #[schema(example = "10.00")]
    pub pack_units: String,
    #[schema(required = true)]
    pub best_before: Option<NaiveDate>,
    #[schema(required = true)]
    pub expires_on: Option<NaiveDate>,
}

/// POST /lots/{lot}/transfers.
#[derive(Deserialize, ToSchema)]
#[schema(description = "Send lot stock over a live supply link this workspace granted.")]
pub struct DispatchRequest {
    #[schema(value_type = Uuid)]
    link_id: Option<String>,
    /// Where the stock is taken from.
    #[schema(value_type = Uuid)]
    warehouse_id: Option<String>,
    #[schema(value_type = String, example = "4")]
    quantity: Option<Value>,
}

/// Validated dispatch.
pub struct NewDispatch {
    pub link_id: Uuid,
    pub warehouse_id: Uuid,
    pub quantity: Decimal,
}

impl Validate for DispatchRequest {
    type Valid = NewDispatch;

    fn validate(self, r: &mut Rules) -> Option<NewDispatch> {
        Some(NewDispatch {
            link_id: uuid(r, "link_id", self.link_id),
            warehouse_id: uuid(r, "warehouse_id", self.warehouse_id),
            quantity: quantity(r, "quantity", self.quantity),
        })
    }
}

/// POST /supply-transfers/{transfer}/receive.
#[derive(Deserialize, ToSchema)]
#[schema(description = "Receive a dispatched transfer into a new lot of this workspace.")]
pub struct ReceiveRequest {
    /// Stock-tracked product of the new lot.
    #[schema(value_type = Uuid)]
    product_id: Option<String>,
    #[schema(value_type = String, example = "AFM-ILU-2026-01")]
    code: Option<String>,
    #[schema(value_type = Uuid)]
    warehouse_id: Option<String>,
    /// At most the dispatched quantity; a shortfall is recorded.
    #[schema(value_type = String, example = "4")]
    quantity: Option<Value>,
}

/// Validated receipt.
pub struct NewReceipt {
    pub product_id: Uuid,
    pub code: String,
    pub warehouse_id: Uuid,
    pub quantity: Decimal,
}

impl Validate for ReceiveRequest {
    type Valid = NewReceipt;

    fn validate(self, r: &mut Rules) -> Option<NewReceipt> {
        Some(NewReceipt {
            product_id: uuid(r, "product_id", self.product_id),
            code: code(r, self.code),
            warehouse_id: uuid(r, "warehouse_id", self.warehouse_id),
            quantity: quantity(r, "quantity", self.quantity),
        })
    }
}

/// A transfer as one side sees it. No farmers, kg shares or prices.
#[derive(Serialize, ToSchema)]
pub struct SupplyTransferResource {
    id: Uuid,
    link_id: Uuid,
    /// `outgoing` (dispatched by this workspace) or `incoming`.
    direction: &'static str,
    partner: Partner,
    from_lot: TransferLot,
    #[schema(example = "4.00")]
    quantity: String,
    /// `dispatched` or `received`.
    status: &'static str,
    dispatched_at: DateTime<Utc>,
    /// The receiver's lot, once received.
    #[schema(required = true)]
    to_lot: Option<ReceivedLot>,
    #[schema(required = true)]
    received_quantity: Option<String>,
    #[schema(required = true)]
    received_at: Option<DateTime<Utc>>,
}

/// The dispatched lot's public codes.
#[derive(Serialize, ToSchema)]
pub struct TransferLot {
    code: String,
    public_code: String,
    /// The sent lot's status at the sender now; `recalled` or `quarantine` means the supplier
    /// took it off sale (receiving stays allowed).
    status: String,
}

/// The lot a transfer was received into.
#[derive(Serialize, ToSchema)]
pub struct ReceivedLot {
    id: Uuid,
    code: String,
}

impl SupplyTransferResource {
    /// `t` as `workspace_id` sees it.
    pub fn new(t: TransferRow, workspace_id: Uuid) -> Self {
        let outgoing = t.from_workspace_id == workspace_id;
        Self {
            id: t.id,
            link_id: t.link_id,
            direction: if outgoing { "outgoing" } else { "incoming" },
            partner: Partner::new(
                if outgoing {
                    t.to_workspace_id
                } else {
                    t.from_workspace_id
                },
                t.partner_name,
                t.partner_display_name,
            ),
            from_lot: TransferLot {
                code: t.from_lot_code,
                public_code: t.from_lot_public_code,
                status: t.from_lot_status,
            },
            quantity: format!("{:.2}", t.quantity),
            status: if t.to_lot_id.is_some() {
                "received"
            } else {
                "dispatched"
            },
            dispatched_at: t.dispatched_at,
            to_lot: t
                .to_lot_id
                .zip(t.to_lot_code)
                .map(|(id, code)| ReceivedLot { id, code }),
            received_quantity: t.received_quantity.map(|q| format!("{q:.2}")),
            received_at: t.received_at,
        }
    }
}

/// Transfer statuses, as `SupplyTransferResource.status` prints them.
pub const STATUSES: &[&str] = &["dispatched", "received"];

/// GET list query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// Only `dispatched` (on the way, not received yet) or `received` transfers.
    #[param(value_type = Option<String>, example = "dispatched")]
    pub status: Option<String>,
    /// Only transfers this workspace sent (`outgoing`) or is sent (`incoming`).
    #[param(value_type = Option<String>, example = "incoming")]
    pub direction: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

impl ListQuery {
    /// `(status, direction)`, each None when absent or blank; 422 on another value.
    pub fn filter(&self) -> Result<(Option<String>, Option<String>), crate::error::AppError> {
        let mut r = Rules::default();
        let status = r.one_of("status", self.status.clone(), STATUSES, None);
        let direction = r.one_of(
            "direction",
            self.direction.clone(),
            crate::modules::supply_links::dto::DIRECTIONS,
            None,
        );
        r.into_result()?;
        Ok((status, direction))
    }
}
