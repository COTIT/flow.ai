//! Attention summary body: per section counts plus the oldest few items. AttentionMoney is integer
//! minor units with its currency code and decimals.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::http::json::{Rules, Validate};

/// The summary. A section is null when the caller lacks its list permission.
#[derive(Serialize, ToSchema)]
pub struct Attention {
    pub generated_at: DateTime<Utc>,
    #[schema(required = true)]
    pub orders: Option<AttentionOrders>,
    #[schema(required = true)]
    pub invoices: Option<AttentionInvoices>,
    #[schema(required = true)]
    pub stock: Option<AttentionStock>,
    #[schema(required = true)]
    pub till_shifts: Option<AttentionShifts>,
    /// Any member (as the lot routes).
    pub lots: AttentionLots,
    #[schema(required = true)]
    pub webhooks: Option<AttentionWebhooks>,
}

/// AttentionOrders to confirm (draft) or fulfil (open), and web orders awaiting payment.
#[derive(Serialize, ToSchema)]
pub struct AttentionOrders {
    pub draft: i64,
    pub open: i64,
    pub awaiting_payment: i64,
    /// Draft and open orders, oldest first, at most 5.
    pub items: Vec<AttentionOrder>,
}

/// A draft or open order.
#[derive(Serialize, ToSchema)]
pub struct AttentionOrder {
    pub id: Uuid,
    pub number: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    /// Incl VAT, minor units.
    pub total: i64,
    #[schema(example = "EUR")]
    pub currency: String,
    pub decimals: i16,
}

/// AttentionInvoices to send (draft), unpaid (pending or partially paid, not credit) and overdue
/// (unpaid, due before today in `general.timezone`).
#[derive(Serialize, ToSchema)]
pub struct AttentionInvoices {
    pub draft: i64,
    pub unpaid: i64,
    pub overdue: i64,
    /// Open amount of the unpaid invoices, per currency.
    pub open: Vec<AttentionMoney>,
    /// Unpaid invoices, earliest due date first (none last), at most 5.
    pub items: Vec<AttentionInvoice>,
}

/// An amount in one currency.
#[derive(Serialize, ToSchema)]
pub struct AttentionMoney {
    #[schema(example = "EUR")]
    pub currency: String,
    pub decimals: i16,
    /// Minor units.
    pub amount: i64,
}

/// An unpaid invoice.
#[derive(Serialize, ToSchema)]
pub struct AttentionInvoice {
    pub id: Uuid,
    /// Formatted number (`INV-1001`); null while unnumbered.
    #[schema(required = true, example = "INV-1001")]
    pub number: Option<String>,
    pub status: String,
    #[schema(required = true)]
    pub customer_name: Option<String>,
    #[schema(required = true)]
    pub due_date: Option<DateTime<Utc>>,
    pub overdue: bool,
    /// Total minus paid, minor units.
    pub open: i64,
    #[schema(example = "EUR")]
    pub currency: String,
    pub decimals: i16,
}

/// Store stock rows below zero, and stock-tracked products sold out.
#[derive(Serialize, ToSchema)]
pub struct AttentionStock {
    /// Rows the negative-stock list shows (store warehouses, level below 0).
    pub negative: i64,
    /// Products whose sellable available (level − reserved; held or expired lots left out)
    /// is at most 0, as the shop's `sold_out`.
    pub out_of_stock: i64,
    /// Sold-out products, lowest available first, at most 5.
    pub items: Vec<AttentionStockItem>,
    /// Products below their minimum (see `BelowMinimum`).
    pub below_minimum: BelowMinimum,
}

/// Stock-tracked products whose sellable available (the `out_of_stock` rule) is below their
/// minimum: the sum of the product's row `minimal_stock_level` when above 0, else the workspace
/// `product.low_stock_threshold` (default 5).
#[derive(Serialize, ToSchema)]
pub struct BelowMinimum {
    pub count: i64,
    /// Largest shortfall first, at most 5.
    pub items: Vec<BelowMinimumItem>,
}

/// A product below its minimum. Quantities in units, 2 decimals.
#[derive(Serialize, ToSchema)]
pub struct BelowMinimumItem {
    pub product_id: Uuid,
    pub product_name: String,
    #[schema(example = "2.00")]
    pub available: String,
    #[schema(example = "5.00")]
    pub minimum: String,
    #[schema(example = "3.00")]
    pub shortfall: String,
}

/// A sold-out product.
#[derive(Serialize, ToSchema)]
pub struct AttentionStockItem {
    pub product_id: Uuid,
    pub product_name: String,
    /// Units, 2 decimals.
    #[schema(example = "0.00")]
    pub available: String,
}

/// Till shifts still open.
#[derive(Serialize, ToSchema)]
pub struct AttentionShifts {
    pub open: i64,
    /// Oldest first, at most 5.
    pub items: Vec<AttentionShift>,
}

/// An open till shift.
#[derive(Serialize, ToSchema)]
pub struct AttentionShift {
    pub id: Uuid,
    pub register_id: Uuid,
    pub register_name: String,
    /// Till user who opened it.
    pub opened_by: String,
    pub opened_at: DateTime<Utc>,
}

/// Quarantined lots (waiting for release or recall) and orders held on lots off sale.
#[derive(Serialize, ToSchema)]
pub struct AttentionLots {
    pub quarantined: i64,
    /// AttentionOrders with a hold on a quarantined or recalled lot (`GET /lots/{lot}/holds`).
    pub held_orders: i64,
    /// Quarantined lots, longest in quarantine first, at most 5.
    pub items: Vec<AttentionLot>,
}

/// A quarantined lot.
#[derive(Serialize, ToSchema)]
pub struct AttentionLot {
    pub id: Uuid,
    pub code: String,
    pub product_id: Uuid,
    pub product_name: String,
    pub held_orders: i64,
}

/// Failed webhook deliveries (dead letters, kept 30 days).
#[derive(Serialize, ToSchema)]
pub struct AttentionWebhooks {
    pub failed: i64,
    /// Oldest first, at most 5.
    pub items: Vec<AttentionDelivery>,
}

/// A failed delivery.
#[derive(Serialize, ToSchema)]
pub struct AttentionDelivery {
    pub id: Uuid,
    pub endpoint_id: Uuid,
    #[schema(example = "order.created")]
    pub event_type: String,
    pub attempts: i32,
    #[schema(required = true)]
    pub last_error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// Attention kinds, in their fixed priority order. Each is a mark `kind` too.
pub const KINDS: [&str; 6] = [
    "stock.below_minimum",
    "orders.open",
    "invoices.overdue",
    "till.open_shifts",
    "lots.quarantined",
    "webhooks.failed",
];

/// Flow noticed: alert groups the caller may see, most urgent first.
#[derive(Serialize, ToSchema)]
pub struct Priorities {
    pub generated_at: DateTime<Utc>,
    /// Groups with at least one item. Order: groups with something not snoozed first, then
    /// impact (open orders) desc, then kind order, then count desc.
    pub groups: Vec<PriorityGroup>,
    /// Kinds hidden for lack of permission (never shown as zero).
    #[schema(example = json!(["invoices.overdue"]))]
    pub restricted: Vec<&'static str>,
}

/// One alert kind. Counts come from the same statement as the items.
#[derive(Serialize, ToSchema)]
pub struct PriorityGroup {
    #[schema(example = "stock.below_minimum")]
    pub kind: &'static str,
    pub count: i64,
    /// `count` minus items the caller snoozed.
    pub open_count: i64,
    pub acknowledged_count: i64,
    pub snoozed_count: i64,
    /// Every item snoozed by the caller (home leaves it out).
    pub snoozed: bool,
    /// The first (most urgent) item's `ref_id`.
    #[schema(required = true)]
    pub headline_ref: Option<Uuid>,
    pub impact: PriorityImpact,
    /// Not-snoozed items, most urgent first, at most 5.
    pub items: Vec<PriorityItem>,
    /// What the caller may do here: `review`, `acknowledge`, `snooze`, and for stock `adjust`
    /// with `stock.mutate` or `stock.set_stock_levels`.
    pub actions: Vec<&'static str>,
}

/// Grounded impact.
#[derive(Serialize, ToSchema)]
pub struct PriorityImpact {
    /// stock.below_minimum: distinct draft or open orders holding a not-snoozed listed product.
    /// Null for other kinds and without `order.list`.
    #[schema(required = true)]
    pub open_orders: Option<i64>,
}

/// One alert item. Fields a kind lacks are null.
#[derive(Serialize, ToSchema)]
pub struct PriorityItem {
    /// Product, order, invoice, shift, lot or delivery id (the mark `ref_id`).
    pub ref_id: Uuid,
    /// Product name, order number, invoice number (else customer), register, lot code or event.
    pub label: String,
    /// Stock: units, 2 decimals.
    #[schema(required = true, example = "2.00")]
    pub available: Option<String>,
    #[schema(required = true, example = "5.00")]
    pub minimum: Option<String>,
    #[schema(required = true, example = "3.00")]
    pub shortfall: Option<String>,
    /// Stock: draft or open orders holding the product; null without `order.list`.
    #[schema(required = true)]
    pub open_orders: Option<i64>,
    /// Order total or invoice open amount.
    #[schema(required = true)]
    pub money: Option<AttentionMoney>,
    /// Order created, invoice due, shift opened, lot or delivery last changed.
    #[schema(required = true)]
    pub since: Option<DateTime<Utc>>,
    /// The caller acknowledged it (still listed until the condition clears).
    pub acknowledged: bool,
}

/// PUT /attention/marks body.
#[derive(Deserialize, ToSchema)]
pub struct MarkBody {
    #[schema(value_type = String, example = "stock.below_minimum")]
    pub kind: Option<String>,
    #[schema(value_type = Uuid)]
    pub ref_id: Option<String>,
    /// `acknowledged` or `snoozed`.
    #[schema(value_type = String, example = "snoozed")]
    pub state: Option<String>,
    /// With `snoozed` only: `until_tomorrow` (next local midnight in the workspace time zone) or
    /// `seven_days`.
    #[schema(value_type = Option<String>, example = "until_tomorrow")]
    pub snooze: Option<String>,
}

/// A validated mark request.
pub struct NewMark {
    pub kind: &'static str,
    pub ref_id: Uuid,
    pub state: &'static str,
    pub snooze: Option<&'static str>,
}

/// `v` as the matching entry of `allowed` (missing or unknown: 422 on `field`).
fn pick(
    r: &mut Rules,
    field: &str,
    v: Option<String>,
    allowed: &[&'static str],
) -> Option<&'static str> {
    if v.as_deref().is_none_or(|v| v.trim().is_empty()) {
        r.missing(field);
        return None;
    }
    let v = r.one_of(field, v, allowed, None)?;
    allowed.iter().find(|a| **a == v).copied()
}

impl Validate for MarkBody {
    type Valid = NewMark;

    fn validate(self, r: &mut Rules) -> Option<NewMark> {
        let kind = pick(r, "kind", self.kind, &KINDS);
        let ref_id = match self.ref_id.as_deref().map(str::trim) {
            None | Some("") => {
                r.missing("ref_id");
                None
            }
            Some(v) => {
                let id = Uuid::parse_str(v).ok();
                r.check("ref_id", id.is_some(), "The ref id must be a valid UUID.");
                id
            }
        };
        let state = pick(r, "state", self.state, &["acknowledged", "snoozed"]);
        let snooze = match (state, self.snooze) {
            (Some("snoozed"), v) => pick(r, "snooze", v, &["until_tomorrow", "seven_days"]),
            (_, Some(_)) => {
                r.check(
                    "snooze",
                    false,
                    "The snooze is only allowed when the state is snoozed.",
                );
                None
            }
            (_, None) => None,
        };
        Some(NewMark {
            kind: kind?,
            ref_id: ref_id?,
            state: state?,
            snooze,
        })
    }
}

/// A stored mark.
#[derive(Serialize, ToSchema)]
pub struct Mark {
    pub id: Uuid,
    #[schema(example = "stock.below_minimum")]
    pub kind: String,
    pub ref_id: Uuid,
    #[schema(example = "snoozed")]
    pub state: String,
    #[schema(required = true)]
    pub snoozed_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
