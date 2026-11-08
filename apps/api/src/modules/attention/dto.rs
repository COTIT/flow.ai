//! Attention summary body: per section counts plus the oldest few items. AttentionMoney is integer
//! minor units with its currency code and decimals.

use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

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
