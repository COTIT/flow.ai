//! Shared provider-neutral types and traits (contracts §4.2, §4.3). Traits are static dispatch
//! only: callers hold `providers::SourceClient` / `SinkClient`, never a trait object.

// Neutral shapes for C3/C4; unused until those land.
#![allow(dead_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Failure class of any provider call (PLAN §3, §6d).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClass {
    Transient,
    Ambiguous,
    Auth,
    Permission,
    NotFound,
    Validation,
    Conflict,
    Unknown,
}

/// Provider error: class + readable, secret-free detail + provider code (per R1).
#[derive(Debug, Clone, thiserror::Error)]
#[error("{class:?}: {detail}")]
pub struct ProviderError {
    pub class: ErrorClass,
    pub detail: String,
    pub code: Option<String>,
    pub http_status: Option<u16>,
}

/// Static capability keys. No dynamic registry.
pub type CapabilityKey = &'static str;
pub const PURCHASE_REQUEST_LIST: CapabilityKey = "purchase_request.list";
pub const PURCHASE_REQUEST_READ: CapabilityKey = "purchase_request.read";
/// R3-§2 / O5.
pub const SOURCE_CONTROLS_READ: CapabilityKey = "source_controls.read";
pub const PURCHASE_ORDER_FIND: CapabilityKey = "purchase_order.find_by_external_id";
pub const PURCHASE_ORDER_CREATE: CapabilityKey = "purchase_order.create";
pub const PURCHASE_ORDER_READ: CapabilityKey = "purchase_order.read";
/// NetSuite M2M token.
pub const AUTH_TOKEN: CapabilityKey = "auth.token";
pub const ODOO_CAPABILITIES: &[CapabilityKey] = &[
    PURCHASE_REQUEST_LIST,
    PURCHASE_REQUEST_READ,
    SOURCE_CONTROLS_READ,
];
pub const NETSUITE_CAPABILITIES: &[CapabilityKey] = &[
    AUTH_TOKEN,
    PURCHASE_ORDER_FIND,
    PURCHASE_ORDER_CREATE,
    PURCHASE_ORDER_READ,
];

/// One probed capability.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CapabilityCheck {
    pub key: String,
    pub ok: bool,
    #[schema(required = true)]
    pub error_class: Option<ErrorClass>,
    #[schema(required = true)]
    pub detail: Option<String>,
}

/// Exact money: integer minor units of `currency_ref`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    pub minor: i64,
    pub currency_ref: String,
}

/// A changed source record.
pub struct SourceRef {
    pub id: String,
    pub changed_at: chrono::DateTime<chrono::Utc>,
}

/// Poll position: change time, then id.
pub struct Cursor {
    pub ts: chrono::DateTime<chrono::Utc>,
    pub id: i64,
}

/// One page of a cursor walk.
pub struct Page<T> {
    pub items: Vec<T>,
    pub next: Option<Cursor>,
}

/// Neutral purchase request (WF-01 source). Decimals are exact.
pub struct PurchaseRequest {
    pub id: String,
    pub state: SourceState,
    pub company_id: String,
    pub vendor_id: String,
    pub currency_id: String,
    pub currency_ref: String,
    pub currency_decimals: u8,
    pub buyer_id: Option<String>,
    pub location_id: Option<String>,
    pub date_order: Option<String>,
    pub lines: Vec<RequestLine>,
    pub amount_untaxed: Decimal,
    pub amount_tax: Decimal,
    pub amount_total: Decimal,
    /// `"vendor:<id>"` → display name, when the source gives one.
    pub labels: std::collections::BTreeMap<String, String>,
}

/// Source document state.
pub enum SourceState {
    Draft,
    Sent,
    ToApprove,
    Confirmed,
    Cancelled,
    Other(String),
}

/// One purchase request line.
pub struct RequestLine {
    pub id: String,
    pub product_id: String,
    pub uom_id: String,
    pub quantity: Decimal,
    pub price_unit: Decimal,
    pub subtotal: Decimal,
    pub tax_ids: Vec<String>,
    pub date_planned: Option<String>,
}

/// Source approval settings per company (R3-§2).
pub struct SourceControls {
    pub company_id: String,
    pub two_step: bool,
    pub threshold_zero: bool,
}

/// What Flow sends (values from the snapshot, ids already mapped).
pub struct NewPo {
    pub external_id: String,
    pub vendor_id: String,
    pub subsidiary_id: String,
    pub currency_id: String,
    pub location_id: Option<String>,
    pub lines: Vec<NewPoLine>,
}

/// One order line to send.
pub struct NewPoLine {
    pub item_id: String,
    pub unit_id: Option<String>,
    pub quantity: Decimal,
    pub rate: Decimal,
}

/// What the target reports (read-back for verify).
pub struct TargetPo {
    pub id: String,
    pub external_id: Option<String>,
    pub vendor_id: String,
    pub subsidiary_id: String,
    pub currency_id: String,
    pub lines: Vec<NewPoLine>,
    pub untaxed: Option<Decimal>,
    pub tax: Option<Decimal>,
}

/// Result of one create call.
pub enum CreateOutcome {
    Created {
        id: String,
        location: Option<String>,
    },
    Ambiguous(ProviderError),
    Rejected(ProviderError),
    Conflict(ProviderError),
}

// Static dispatch only (K20): no Send bound needed on the returned futures yet.
#[allow(async_fn_in_trait)]
/// Reads purchase requests from a source system.
pub trait PurchaseRequestSource {
    async fn list_changed(
        &self,
        company_ids: &[String],
        cursor: &Cursor,
        limit: u32,
    ) -> Result<Page<SourceRef>, ProviderError>;
    /// Full scan.
    async fn list_eligible_ids(
        &self,
        company_id: &str,
        after_id: i64,
        limit: u32,
    ) -> Result<Vec<String>, ProviderError>;
    /// Absent ids are returned in `.1` (gone).
    async fn read(
        &self,
        ids: &[String],
    ) -> Result<(Vec<PurchaseRequest>, Vec<String>), ProviderError>;
    async fn read_controls(
        &self,
        company_ids: &[String],
    ) -> Result<Vec<SourceControls>, ProviderError>;
}

#[allow(async_fn_in_trait)]
/// Writes purchase orders to a target system.
pub trait PurchaseOrderSink {
    async fn find_by_external_id(&self, eid: &str) -> Result<Option<TargetPo>, ProviderError>;
    async fn create(&self, po: &NewPo) -> CreateOutcome;
    async fn read(&self, id: &str) -> Result<TargetPo, ProviderError>;
}

#[allow(async_fn_in_trait)]
/// Checks every capability of one connection.
pub trait ConnectionProbe {
    async fn test(&self) -> Vec<CapabilityCheck>;
}

/// Capability keys a provider must pass.
pub fn keys_of(provider: &str) -> &'static [CapabilityKey] {
    match provider {
        "odoo" => ODOO_CAPABILITIES,
        _ => NETSUITE_CAPABILITIES,
    }
}
