//! Purchasing payloads (inside the accounting C/A envelopes), their validation, and response
//! resources. Quantities are base-unit decimal strings (≤2 places), prices ≤8 places, amounts
//! exact decimal strings in major units of their currency.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::accounting::dto::{CurrencyRef, MAX_LINES, date, opt_uuid, uuid},
};

const REF_MAX: usize = 255;
const NOTE_MAX: usize = 1000;
/// Largest base quantity a stock row holds (`stock::repo::LEVEL_MAX`).
const QTY_MAX: Decimal = Decimal::from_parts(2_764_472_320, 232_830, 0, false, 0);

fn quantity(r: &mut Rules, field: &str, v: Option<Value>) -> Option<Decimal> {
    let Some(v) = v else {
        r.missing(field);
        return None;
    };
    r.measure(field, Some(v), Decimal::new(1, 2), QTY_MAX, 2)
}

fn ids(r: &mut Rules, field: &str, raw: Option<Vec<String>>) -> Vec<Uuid> {
    let raw = raw.unwrap_or_default();
    r.check(
        field,
        (1..=MAX_LINES).contains(&raw.len()),
        format!("Send 1 to {MAX_LINES} ids."),
    );
    let mut out = Vec::with_capacity(raw.len());
    for (i, s) in raw.into_iter().take(MAX_LINES).enumerate() {
        if let Some(id) = uuid(r, &format!("{field}.{i}"), Some(s)) {
            r.check(field, !out.contains(&id), "Each id may appear once.");
            out.push(id);
        }
    }
    out
}

// --- purchase orders -----------------------------------------------------------------------

/// One order line as sent.
#[derive(Deserialize, ToSchema)]
pub struct OrderLinePayload {
    #[schema(value_type = Uuid)]
    pub product_id: Option<String>,
    /// In the purchase unit when given, else in the product's base unit; ≤2 places.
    #[schema(value_type = String, example = "5")]
    pub quantity: Option<Value>,
    #[schema(value_type = Option<Uuid>)]
    pub purchase_unit_id: Option<String>,
    /// Price per input unit in the order currency, ≤8 places.
    #[schema(value_type = String, example = "12.50")]
    pub unit_price: Option<Value>,
    #[schema(value_type = Option<Uuid>)]
    pub warehouse_id: Option<String>,
}

/// Validated order line.
pub struct OrderLineInput {
    pub product: Uuid,
    pub quantity: Decimal,
    pub unit: Option<Uuid>,
    pub unit_price: Decimal,
    pub warehouse: Option<Uuid>,
}

/// POST/PATCH purchase order payload (PATCH replaces every draft field and line).
#[derive(Deserialize, ToSchema)]
pub struct OrderPayload {
    #[schema(value_type = Uuid)]
    pub supplier_id: Option<String>,
    #[schema(value_type = Uuid)]
    pub currency_id: Option<String>,
    #[schema(value_type = NaiveDate)]
    pub purchase_date: Option<String>,
    #[schema(value_type = Option<NaiveDate>)]
    pub expected_date: Option<String>,
    pub reference: Option<String>,
    pub note: Option<String>,
    #[schema(value_type = Option<Uuid>)]
    pub default_warehouse_id: Option<String>,
    pub lines: Option<Vec<OrderLinePayload>>,
}

/// Validated order.
pub struct OrderInput {
    pub supplier: Uuid,
    pub currency: Uuid,
    pub purchase_date: NaiveDate,
    pub expected_date: Option<NaiveDate>,
    pub reference: Option<String>,
    pub note: Option<String>,
    pub default_warehouse: Option<Uuid>,
    pub lines: Vec<OrderLineInput>,
}

impl Validate for OrderPayload {
    type Valid = OrderInput;

    fn validate(self, r: &mut Rules) -> Option<OrderInput> {
        let supplier = uuid(r, "supplier_id", self.supplier_id);
        let currency = uuid(r, "currency_id", self.currency_id);
        let purchase_date = date(r, "purchase_date", self.purchase_date);
        let expected_date = match crate::http::json::optional(self.expected_date) {
            None => Some(None),
            Some(v) => date(r, "expected_date", Some(v)).map(Some),
        };
        if let (Some(p), Some(Some(e))) = (purchase_date, expected_date) {
            r.check(
                "expected_date",
                e >= p,
                "The expected date can't be before the purchase date.",
            );
        }
        let reference = r.opt_text("reference", self.reference, REF_MAX);
        let note = r.opt_text("note", self.note, NOTE_MAX);
        let default_warehouse = opt_uuid(r, "default_warehouse_id", self.default_warehouse_id);
        let raw = self.lines.unwrap_or_default();
        r.check(
            "lines",
            (1..=MAX_LINES).contains(&raw.len()),
            format!("An order needs 1 to {MAX_LINES} lines."),
        );
        let mut lines = Vec::with_capacity(raw.len());
        for (i, l) in raw.into_iter().take(MAX_LINES).enumerate() {
            let mut lr = Rules::default();
            let product = uuid(&mut lr, "product_id", l.product_id);
            let quantity = quantity(&mut lr, "quantity", l.quantity);
            let unit = opt_uuid(&mut lr, "purchase_unit_id", l.purchase_unit_id);
            let unit_price = match l.unit_price {
                Some(v) => lr.decimal("unit_price", &v, Decimal::ZERO, crate::money::MAX),
                None => {
                    lr.missing("unit_price");
                    None
                }
            };
            let warehouse = opt_uuid(&mut lr, "warehouse_id", l.warehouse_id);
            let ok = !lr.merge_into_prefixed(r, &format!("lines.{i}."));
            if let (true, Some(product), Some(quantity), Some(unit), Some(unit_price), Some(w)) =
                (ok, product, quantity, unit, unit_price, warehouse)
            {
                lines.push(OrderLineInput {
                    product,
                    quantity,
                    unit,
                    unit_price,
                    warehouse: w,
                });
            }
        }
        Some(OrderInput {
            supplier: supplier?,
            currency: currency?,
            purchase_date: purchase_date?,
            expected_date: expected_date?,
            reference,
            note,
            default_warehouse: default_warehouse?,
            lines,
        })
    }
}

/// POST /purchase-orders/{id}/confirm payload.
#[derive(Deserialize, ToSchema)]
pub struct ConfirmPayload {
    /// From `POST /purchase-orders/{id}/confirm/review`.
    pub review_fingerprint: Option<String>,
}

impl Validate for ConfirmPayload {
    type Valid = String;

    fn validate(self, r: &mut Rules) -> Option<String> {
        r.required("review_fingerprint", self.review_fingerprint)
    }
}

/// One remainder to close.
#[derive(Deserialize, ToSchema)]
pub struct CloseLinePayload {
    #[schema(value_type = Uuid)]
    pub line_id: Option<String>,
    /// Base units, ≤ the line's open quantity.
    #[schema(value_type = String, example = "3")]
    pub quantity: Option<Value>,
}

/// POST /purchase-orders/{id}/close-lines payload.
#[derive(Deserialize, ToSchema)]
pub struct CloseLinesPayload {
    pub reason: Option<String>,
    pub lines: Option<Vec<CloseLinePayload>>,
}

/// Validated close request.
pub struct CloseInput {
    pub reason: String,
    pub lines: Vec<(Uuid, Decimal)>,
}

fn reason(r: &mut Rules, v: Option<String>) -> Option<String> {
    let t = r.required("reason", v)?;
    r.max_chars("reason", &t, NOTE_MAX);
    Some(t)
}

impl Validate for CloseLinesPayload {
    type Valid = CloseInput;

    fn validate(self, r: &mut Rules) -> Option<CloseInput> {
        let reason = reason(r, self.reason);
        let raw = self.lines.unwrap_or_default();
        r.check(
            "lines",
            (1..=MAX_LINES).contains(&raw.len()),
            format!("Send 1 to {MAX_LINES} lines."),
        );
        let mut lines: Vec<(Uuid, Decimal)> = Vec::with_capacity(raw.len());
        for (i, l) in raw.into_iter().take(MAX_LINES).enumerate() {
            let mut lr = Rules::default();
            let id = uuid(&mut lr, "line_id", l.line_id);
            let q = quantity(&mut lr, "quantity", l.quantity);
            if let Some(id) = id {
                lr.check(
                    "line_id",
                    !lines.iter().any(|(l, _)| *l == id),
                    "Each line may appear once.",
                );
            }
            let ok = !lr.merge_into_prefixed(r, &format!("lines.{i}."));
            if let (true, Some(id), Some(q)) = (ok, id, q) {
                lines.push((id, q));
            }
        }
        Some(CloseInput {
            reason: reason?,
            lines,
        })
    }
}

/// POST /purchase-orders/{id}/cancel payload.
#[derive(Deserialize, ToSchema)]
pub struct CancelPayload {
    pub reason: Option<String>,
}

impl Validate for CancelPayload {
    type Valid = String;

    fn validate(self, r: &mut Rules) -> Option<String> {
        reason(r, self.reason)
    }
}

/// GET /purchase-orders query.
#[derive(Deserialize, IntoParams)]
pub struct OrderListQuery {
    /// draft, confirmed or cancelled.
    pub state: Option<String>,
    pub supplier_id: Option<Uuid>,
    pub page: Option<String>,
    pub page_size: Option<String>,
}

/// GET list query of child resources.
#[derive(Deserialize, IntoParams)]
pub struct PageQuery {
    pub page: Option<String>,
    pub page_size: Option<String>,
}

/// Supplier as named on the order.
#[derive(Serialize, ToSchema)]
pub struct SupplierRef {
    pub id: Uuid,
    pub name: Option<String>,
}

/// A product as named on a line.
#[derive(Serialize, ToSchema)]
pub struct ProductRef {
    pub id: Uuid,
    pub name: String,
    /// Base unit (`kg`, `pc`, …); null when not set.
    pub unit: Option<String>,
}

/// The purchase unit a line was ordered in.
#[derive(Serialize, ToSchema)]
pub struct UnitRef {
    pub id: Uuid,
    pub code: String,
    /// Base units per input unit, frozen on the line.
    pub ratio: String,
}

/// One order line.
#[derive(Serialize, ToSchema)]
pub struct PurchaseOrderLineResource {
    pub id: Uuid,
    pub line_no: i32,
    pub product: ProductRef,
    pub purchase_unit: Option<UnitRef>,
    pub input_quantity: String,
    pub ratio: String,
    pub base_quantity: String,
    pub unit_price: String,
    /// input_quantity × unit_price, order currency.
    pub extension: String,
    pub warehouse_id: Option<Uuid>,
    pub received_quantity: String,
    pub booked_quantity: String,
    pub closed_quantity: String,
    /// base − received − closed.
    pub open_quantity: String,
}

/// Order totals in the order currency.
#[derive(Serialize, ToSchema)]
pub struct PurchaseOrderTotals {
    pub ordered: String,
    /// Extension share of received quantities.
    pub received: String,
    /// Extension share of open quantities.
    pub open: String,
}

/// A purchase order.
#[derive(Serialize, ToSchema)]
pub struct PurchaseOrderResource {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub number: i64,
    pub version: i64,
    /// draft, confirmed or cancelled.
    pub state: String,
    /// not_received, partial or received (nothing open).
    pub receipt_status: &'static str,
    pub supplier: SupplierRef,
    /// Frozen at confirm; null on drafts.
    #[schema(value_type = Option<Object>)]
    pub supplier_snapshot: Option<Value>,
    pub currency: CurrencyRef,
    pub purchase_date: NaiveDate,
    pub expected_date: Option<NaiveDate>,
    pub reference: Option<String>,
    pub note: Option<String>,
    pub default_warehouse_id: Option<Uuid>,
    pub totals: PurchaseOrderTotals,
    pub lines: Vec<PurchaseOrderLineResource>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub confirmed_by: Option<Uuid>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancel_reason: Option<String>,
    /// Subset of update, confirm, receive, close, cancel.
    pub allowed_actions: Vec<&'static str>,
}

/// One confirm-review line.
#[derive(Serialize, ToSchema)]
pub struct ConfirmLine {
    pub line_id: Uuid,
    pub product_id: Uuid,
    pub purchase_unit_id: Option<Uuid>,
    pub ratio: String,
    pub base_quantity: String,
    pub unit_price: String,
    pub extension: String,
}

/// What confirming would freeze.
#[derive(Serialize, ToSchema)]
pub struct ConfirmReview {
    pub purchase_order_id: Uuid,
    pub version: i64,
    #[schema(value_type = Object)]
    pub supplier_snapshot: Value,
    pub currency: CurrencyRef,
    pub lines: Vec<ConfirmLine>,
    pub totals: PurchaseOrderTotals,
    pub review_fingerprint: String,
}

/// One history event of an order.
#[derive(Serialize, ToSchema)]
pub struct PurchaseOrderEvent {
    pub at: DateTime<Utc>,
    /// created, confirmed, received, booked, closed or cancelled.
    pub kind: String,
    pub actor: Uuid,
    /// Receipt (received/booked), order line (closed) or the order.
    pub ref_id: Uuid,
    /// Short text: receipt number, closed quantity, reason.
    pub detail: Option<String>,
}

// --- receipts ------------------------------------------------------------------------------

/// One received line.
#[derive(Deserialize, ToSchema)]
pub struct ReceiptLinePayload {
    #[schema(value_type = Uuid)]
    pub po_line_id: Option<String>,
    /// Base units accepted, ≤ the line's open quantity.
    #[schema(value_type = String, example = "2")]
    pub base_quantity: Option<Value>,
    #[schema(value_type = Uuid)]
    pub warehouse_id: Option<String>,
    #[schema(value_type = Option<Uuid>)]
    pub bin_id: Option<String>,
    #[schema(value_type = Option<Uuid>)]
    pub lot_id: Option<String>,
}

/// Validated received line.
pub struct ReceiptLineInput {
    pub po_line: Uuid,
    pub quantity: Decimal,
    pub warehouse: Uuid,
    pub bin: Option<Uuid>,
    pub lot: Option<Uuid>,
}

/// POST /purchase-orders/{id}/receipts payload.
#[derive(Deserialize, ToSchema)]
pub struct ReceiptPayload {
    /// When ownership passed; not in the future.
    #[schema(value_type = NaiveDate)]
    pub effective_receipt_date: Option<String>,
    pub note: Option<String>,
    pub lines: Option<Vec<ReceiptLinePayload>>,
}

/// Validated receipt.
pub struct ReceiptInput {
    pub date: NaiveDate,
    pub note: Option<String>,
    pub lines: Vec<ReceiptLineInput>,
}

impl Validate for ReceiptPayload {
    type Valid = ReceiptInput;

    fn validate(self, r: &mut Rules) -> Option<ReceiptInput> {
        let date = date(r, "effective_receipt_date", self.effective_receipt_date);
        let note = r.opt_text("note", self.note, NOTE_MAX);
        let raw = self.lines.unwrap_or_default();
        r.check(
            "lines",
            (1..=MAX_LINES).contains(&raw.len()),
            format!("A receipt needs 1 to {MAX_LINES} lines."),
        );
        let mut lines: Vec<ReceiptLineInput> = Vec::with_capacity(raw.len());
        for (i, l) in raw.into_iter().take(MAX_LINES).enumerate() {
            let mut lr = Rules::default();
            let po_line = uuid(&mut lr, "po_line_id", l.po_line_id);
            if let Some(id) = po_line {
                lr.check(
                    "po_line_id",
                    !lines.iter().any(|x| x.po_line == id),
                    "Each order line may appear once per receipt.",
                );
            }
            let q = quantity(&mut lr, "base_quantity", l.base_quantity);
            let warehouse = uuid(&mut lr, "warehouse_id", l.warehouse_id);
            let bin = opt_uuid(&mut lr, "bin_id", l.bin_id);
            let lot = opt_uuid(&mut lr, "lot_id", l.lot_id);
            let ok = !lr.merge_into_prefixed(r, &format!("lines.{i}."));
            if let (true, Some(po_line), Some(quantity), Some(warehouse), Some(bin), Some(lot)) =
                (ok, po_line, q, warehouse, bin, lot)
            {
                lines.push(ReceiptLineInput {
                    po_line,
                    quantity,
                    warehouse,
                    bin,
                    lot,
                });
            }
        }
        Some(ReceiptInput {
            date: date?,
            note,
            lines,
        })
    }
}

/// An explicit share of a landed component for one receipt line.
#[derive(Deserialize, ToSchema)]
pub struct ExplicitAllocationPayload {
    #[schema(value_type = Uuid)]
    pub receipt_line_id: Option<String>,
    /// Component currency, major units.
    #[schema(value_type = String, example = "4.00")]
    pub amount: Option<Value>,
}

/// One freight/duty component to capitalise.
#[derive(Deserialize, ToSchema)]
pub struct LandedCostPayload {
    /// Identity of the component for the whole order: never capitalised twice.
    #[schema(value_type = Uuid)]
    pub client_component_ref: Option<String>,
    /// Component currency, major units, > 0.
    #[schema(value_type = String, example = "10.00")]
    pub amount: Option<Value>,
    #[schema(value_type = Uuid)]
    pub currency_id: Option<String>,
    pub evidence_ref: Option<String>,
    /// Required when the selected lines have no net value; must sum to `amount`.
    pub explicit_line_allocations: Option<Vec<ExplicitAllocationPayload>>,
}

/// Validated component; amounts still major units.
pub struct LandedCostInput {
    pub component: Uuid,
    pub amount: Decimal,
    pub currency: Uuid,
    pub evidence_ref: String,
    pub explicit: Option<Vec<(Uuid, Decimal)>>,
}

/// POST .../book/review body (the book payload without fingerprint).
#[derive(Deserialize, ToSchema)]
pub struct BookReviewBody {
    #[schema(value_type = Vec<Uuid>)]
    pub receipt_line_ids: Option<Vec<String>>,
    pub landed_costs: Option<Vec<LandedCostPayload>>,
}

/// POST .../book payload.
#[derive(Deserialize, ToSchema)]
pub struct BookPayload {
    #[schema(value_type = Vec<Uuid>)]
    pub receipt_line_ids: Option<Vec<String>>,
    pub landed_costs: Option<Vec<LandedCostPayload>>,
    /// From `POST .../book/review`.
    pub review_fingerprint: Option<String>,
}

/// Validated booking request.
pub struct BookInput {
    pub lines: Vec<Uuid>,
    pub landed: Vec<LandedCostInput>,
}

fn book_input(
    r: &mut Rules,
    lines: Option<Vec<String>>,
    landed: Option<Vec<LandedCostPayload>>,
) -> BookInput {
    let lines = ids(r, "receipt_line_ids", lines);
    let raw = landed.unwrap_or_default();
    r.check(
        "landed_costs",
        raw.len() <= MAX_LINES,
        format!("At most {MAX_LINES} landed costs."),
    );
    let mut out: Vec<LandedCostInput> = Vec::with_capacity(raw.len());
    for (i, c) in raw.into_iter().take(MAX_LINES).enumerate() {
        let mut lr = Rules::default();
        let component = uuid(&mut lr, "client_component_ref", c.client_component_ref);
        if let Some(id) = component {
            lr.check(
                "client_component_ref",
                !out.iter().any(|x| x.component == id),
                "Each component may appear once.",
            );
        }
        let positive = |lr: &mut Rules, f: &str, v: Option<Value>| match v {
            Some(v) => lr.decimal(f, &v, Decimal::ZERO, crate::money::MAX),
            None => {
                lr.missing(f);
                None
            }
        };
        let amount = positive(&mut lr, "amount", c.amount);
        if let Some(a) = amount {
            lr.check(
                "amount",
                a > Decimal::ZERO,
                "The amount must be above zero.",
            );
        }
        let currency = uuid(&mut lr, "currency_id", c.currency_id);
        let evidence_ref = lr.required("evidence_ref", c.evidence_ref);
        if let Some(e) = &evidence_ref {
            lr.max_chars("evidence_ref", e, REF_MAX);
        }
        let explicit = c.explicit_line_allocations.map(|xs| {
            xs.into_iter()
                .take(MAX_LINES)
                .enumerate()
                .filter_map(|(j, x)| {
                    let f = format!("explicit_line_allocations.{j}.");
                    let line = uuid(&mut lr, &format!("{f}receipt_line_id"), x.receipt_line_id);
                    let amount = positive(&mut lr, &format!("{f}amount"), x.amount);
                    Some((line?, amount?))
                })
                .collect::<Vec<_>>()
        });
        let ok = !lr.merge_into_prefixed(r, &format!("landed_costs.{i}."));
        if let (true, Some(component), Some(amount), Some(currency), Some(evidence_ref)) =
            (ok, component, amount, currency, evidence_ref)
        {
            out.push(LandedCostInput {
                component,
                amount,
                currency,
                evidence_ref,
                explicit,
            });
        }
    }
    BookInput { lines, landed: out }
}

impl Validate for BookReviewBody {
    type Valid = BookInput;

    fn validate(self, r: &mut Rules) -> Option<BookInput> {
        Some(book_input(r, self.receipt_line_ids, self.landed_costs))
    }
}

impl Validate for BookPayload {
    type Valid = (BookInput, String);

    fn validate(self, r: &mut Rules) -> Option<(BookInput, String)> {
        let input = book_input(r, self.receipt_line_ids, self.landed_costs);
        let fingerprint = r.required("review_fingerprint", self.review_fingerprint)?;
        Some((input, fingerprint))
    }
}

/// A rate snapshot: `rate` major units of functional per unit of the source currency, or the
/// other way round when `inverse`.
#[derive(Serialize, ToSchema, Clone)]
pub struct RateSnapshot {
    pub rate: String,
    pub inverse: bool,
    /// identity or workspace.
    pub source: String,
    pub date: Option<NaiveDate>,
}

/// One receipt line.
#[derive(Serialize, ToSchema)]
pub struct ReceiptLineResource {
    pub id: Uuid,
    pub po_line_id: Uuid,
    pub product_id: Uuid,
    pub warehouse_id: Uuid,
    pub bin_id: Option<Uuid>,
    pub lot_id: Option<Uuid>,
    pub base_quantity: String,
    /// pending, late (books through the late-receipt review, M5) or booked.
    pub state: &'static str,
    pub booked_at: Option<DateTime<Utc>>,
    pub accounting_date: Option<NaiveDate>,
    /// Functional currency; null until booked in an active book.
    pub net_value: Option<String>,
    pub landed_value: Option<String>,
    pub value: Option<String>,
    pub rate: Option<RateSnapshot>,
    /// Needs `accounting.read`; null otherwise.
    pub journal_entry_id: Option<Uuid>,
}

/// A landed component's share of one receipt line.
#[derive(Serialize, ToSchema)]
pub struct AllocationResource {
    pub receipt_line_id: Uuid,
    pub functional_amount: String,
}

/// A capitalised landed component.
#[derive(Serialize, ToSchema)]
pub struct LandedCostResource {
    pub id: Uuid,
    pub client_component_ref: Uuid,
    pub amount: String,
    pub currency: CurrencyRef,
    pub functional_amount: String,
    pub rate: RateSnapshot,
    pub evidence_ref: String,
    pub allocations: Vec<AllocationResource>,
}

/// A purchase receipt.
#[derive(Serialize, ToSchema)]
pub struct PurchaseReceiptResource {
    pub id: Uuid,
    pub purchase_order_id: Uuid,
    pub number: i64,
    pub version: i64,
    pub effective_receipt_date: NaiveDate,
    pub note: Option<String>,
    pub recorded_at: DateTime<Utc>,
    pub created_by: Uuid,
    /// unbooked, partial or booked (late lines count as unbooked).
    pub booking_status: &'static str,
    pub lines: Vec<ReceiptLineResource>,
    pub landed_costs: Vec<LandedCostResource>,
    /// [`book`] while a pending line is left.
    pub allowed_actions: Vec<&'static str>,
}

/// One line a booking would add.
#[derive(Serialize, ToSchema)]
pub struct BookReviewLine {
    pub receipt_line_id: Uuid,
    pub base_quantity: String,
    /// Functional currency; null in a setup book.
    pub net_value: Option<String>,
    pub landed_value: Option<String>,
    pub value: Option<String>,
    pub rate: Option<RateSnapshot>,
}

/// One component a booking would capitalise.
#[derive(Serialize, ToSchema)]
pub struct BookReviewCost {
    pub client_component_ref: Uuid,
    pub amount: String,
    pub currency: CurrencyRef,
    pub functional_amount: String,
    pub rate: RateSnapshot,
    pub allocations: Vec<AllocationResource>,
}

/// The journal each booked line posts.
#[derive(Serialize, ToSchema)]
pub struct BookJournal {
    pub debit_role: &'static str,
    pub credit_role: &'static str,
}

/// A pool before and after the booking (needs `accounting.read`).
#[derive(Serialize, ToSchema)]
pub struct PoolPreview {
    pub product_id: Uuid,
    pub warehouse_id: Uuid,
    pub quantity_before: String,
    pub value_before: String,
    pub quantity_after: String,
    pub value_after: String,
}

/// What booking would write, computed from current facts.
#[derive(Serialize, ToSchema)]
pub struct BookReview {
    pub receipt_id: Uuid,
    pub receipt_version: i64,
    pub purchase_order_version: i64,
    pub accounting_date: NaiveDate,
    /// Null in a setup book.
    pub functional_currency: Option<CurrencyRef>,
    pub lines: Vec<BookReviewLine>,
    pub landed_costs: Vec<BookReviewCost>,
    /// Functional currency; null in a setup book.
    pub total: Option<String>,
    /// Null in a setup book (stock only).
    pub journal: Option<BookJournal>,
    /// Null without `accounting.read` or in a setup book.
    pub pool: Option<Vec<PoolPreview>>,
    pub review_fingerprint: String,
}
