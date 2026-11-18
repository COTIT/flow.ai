//! Payables payloads (inside the accounting C/A envelopes), their validation, and response
//! resources. Quantities are base-unit decimal strings (≤2 places), prices ≤8 places, amounts
//! exact decimal strings in major units of their currency.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, optional},
    modules::{
        accounting::dto::{CurrencyRef, MAX_LINES, date, opt_uuid, uuid},
        purchasing::dto::{RateSnapshot, SupplierRef},
    },
};

pub(crate) const REF_MAX: usize = 255;
pub(crate) const NOTE_MAX: usize = 1000;
const QTY_MAX: Decimal = crate::modules::stock::repo::LEVEL_MAX;

pub(crate) fn quantity(r: &mut Rules, field: &str, v: Option<Value>) -> Option<Decimal> {
    let Some(v) = v else {
        r.missing(field);
        return None;
    };
    r.measure(field, Some(v), Decimal::new(1, 2), QTY_MAX, 2)
}

/// Required review fingerprint.
pub(crate) fn fingerprint(r: &mut Rules, v: Option<String>) -> Option<String> {
    r.required("review_fingerprint", v)
}

// --- supplier bills ------------------------------------------------------------------------

/// One bill line as sent: exactly one of `product_id` (goods, matched to receipt lines),
/// `landed_cost_id` (clears one landed component; quantity 1) or `expense_account_id`.
#[derive(Deserialize, ToSchema)]
pub struct BillLinePayload {
    #[schema(value_type = Option<Uuid>)]
    pub product_id: Option<String>,
    #[schema(value_type = Option<Uuid>)]
    pub landed_cost_id: Option<String>,
    #[schema(value_type = Option<Uuid>)]
    pub expense_account_id: Option<String>,
    /// Base units, ≤2 places.
    #[schema(value_type = String, example = "4")]
    pub quantity: Option<Value>,
    /// Price per base unit in the bill currency, ≤8 places.
    #[schema(value_type = String, example = "10.00")]
    pub unit_price: Option<Value>,
    /// Verified purchase tax profile; tax comes from SQL `tax_line` at post.
    #[schema(value_type = Option<Uuid>)]
    pub tax_profile_id: Option<String>,
    pub description: Option<String>,
}

/// What a line points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineTarget {
    Goods(Uuid),
    Landed(Uuid),
    Expense(Uuid),
}

/// Validated bill line.
pub struct BillLineInput {
    pub target: LineTarget,
    pub quantity: Decimal,
    pub unit_price: Decimal,
    pub tax_profile: Option<Uuid>,
    pub description: Option<String>,
}

/// POST/PATCH supplier bill payload (PATCH replaces every draft field and line).
#[derive(Deserialize, ToSchema)]
pub struct BillPayload {
    #[schema(value_type = Uuid)]
    pub supplier_id: Option<String>,
    #[schema(value_type = Uuid)]
    pub currency_id: Option<String>,
    pub supplier_invoice_number: Option<String>,
    #[schema(value_type = NaiveDate)]
    pub invoice_date: Option<String>,
    #[schema(value_type = Option<NaiveDate>)]
    pub due_date: Option<String>,
    #[schema(value_type = NaiveDate)]
    pub accounting_date: Option<String>,
    pub evidence_ref: Option<String>,
    pub note: Option<String>,
    /// The reversed bill whose supplier invoice number this one reuses.
    #[schema(value_type = Option<Uuid>)]
    pub corrects_bill_id: Option<String>,
    pub lines: Option<Vec<BillLinePayload>>,
}

/// Validated bill.
pub struct BillInput {
    pub supplier: Uuid,
    pub currency: Uuid,
    pub supplier_invoice_number: String,
    pub invoice_date: NaiveDate,
    pub due_date: Option<NaiveDate>,
    pub accounting_date: NaiveDate,
    pub evidence_ref: Option<String>,
    pub note: Option<String>,
    pub corrects: Option<Uuid>,
    pub lines: Vec<BillLineInput>,
}

impl Validate for BillPayload {
    type Valid = BillInput;

    fn validate(self, r: &mut Rules) -> Option<BillInput> {
        let supplier = uuid(r, "supplier_id", self.supplier_id);
        let currency = uuid(r, "currency_id", self.currency_id);
        let number = r.required("supplier_invoice_number", self.supplier_invoice_number);
        if let Some(n) = &number {
            r.check(
                "supplier_invoice_number",
                !n.trim().is_empty(),
                "The supplier invoice number is required.",
            );
            r.max_chars("supplier_invoice_number", n.trim(), REF_MAX);
        }
        let invoice_date = date(r, "invoice_date", self.invoice_date);
        let due_date = match optional(self.due_date) {
            None => Some(None),
            Some(v) => date(r, "due_date", Some(v)).map(Some),
        };
        if let (Some(i), Some(Some(d))) = (invoice_date, due_date) {
            r.check(
                "due_date",
                d >= i,
                "The due date can't be before the invoice date.",
            );
        }
        let accounting_date = date(r, "accounting_date", self.accounting_date);
        let evidence_ref = r.opt_text("evidence_ref", self.evidence_ref, REF_MAX);
        let note = r.opt_text("note", self.note, NOTE_MAX);
        let corrects = opt_uuid(r, "corrects_bill_id", self.corrects_bill_id);
        let raw = self.lines.unwrap_or_default();
        r.check(
            "lines",
            (1..=MAX_LINES).contains(&raw.len()),
            format!("A bill needs 1 to {MAX_LINES} lines."),
        );
        let mut lines = Vec::with_capacity(raw.len());
        for (i, l) in raw.into_iter().take(MAX_LINES).enumerate() {
            let mut lr = Rules::default();
            let product = opt_uuid(&mut lr, "product_id", l.product_id);
            let landed = opt_uuid(&mut lr, "landed_cost_id", l.landed_cost_id);
            let expense = opt_uuid(&mut lr, "expense_account_id", l.expense_account_id);
            let target = match (product.flatten(), landed.flatten(), expense.flatten()) {
                (Some(p), None, None) => Some(LineTarget::Goods(p)),
                (None, Some(c), None) => Some(LineTarget::Landed(c)),
                (None, None, Some(a)) => Some(LineTarget::Expense(a)),
                _ => {
                    if product.is_some() && landed.is_some() && expense.is_some() {
                        lr.add(
                            "product_id",
                            "Name exactly one of product_id, landed_cost_id or expense_account_id.",
                        );
                    }
                    None
                }
            };
            let quantity = quantity(&mut lr, "quantity", l.quantity);
            if let (Some(LineTarget::Landed(_)), Some(q)) = (target, quantity) {
                lr.check(
                    "quantity",
                    q == Decimal::ONE,
                    "A landed cost line has quantity 1.",
                );
            }
            let unit_price = match l.unit_price {
                Some(v) => lr.decimal("unit_price", &v, Decimal::ZERO, crate::money::MAX),
                None => {
                    lr.missing("unit_price");
                    None
                }
            };
            let tax_profile = opt_uuid(&mut lr, "tax_profile_id", l.tax_profile_id);
            let description = lr.opt_text("description", l.description, REF_MAX);
            let ok = !lr.merge_into_prefixed(r, &format!("lines.{i}."));
            if let (true, Some(target), Some(quantity), Some(unit_price), Some(tax_profile)) =
                (ok, target, quantity, unit_price, tax_profile)
            {
                lines.push(BillLineInput {
                    target,
                    quantity,
                    unit_price,
                    tax_profile,
                    description,
                });
            }
        }
        Some(BillInput {
            supplier: supplier?,
            currency: currency?,
            supplier_invoice_number: number?.trim().to_owned(),
            invoice_date: invoice_date?,
            due_date: due_date?,
            accounting_date: accounting_date?,
            evidence_ref,
            note,
            corrects: corrects?,
            lines,
        })
    }
}

/// One goods-line claim on a booked receipt line.
#[derive(Deserialize, ToSchema)]
pub struct MatchAllocationPayload {
    #[schema(value_type = Uuid)]
    pub bill_line_id: Option<String>,
    #[schema(value_type = Uuid)]
    pub receipt_line_id: Option<String>,
    /// Base units, ≤2 places.
    #[schema(value_type = String, example = "4")]
    pub quantity: Option<Value>,
}

/// POST /supplier-bills/{id}/match payload: replaces every goods reservation of the draft
/// (an empty list releases them).
#[derive(Deserialize, ToSchema)]
pub struct MatchPayload {
    pub allocations: Option<Vec<MatchAllocationPayload>>,
}

/// Validated match: (bill line, receipt line, quantity).
pub type MatchInput = Vec<(Uuid, Uuid, Decimal)>;

impl Validate for MatchPayload {
    type Valid = MatchInput;

    fn validate(self, r: &mut Rules) -> Option<MatchInput> {
        let Some(raw) = self.allocations else {
            r.missing("allocations");
            return None;
        };
        r.check(
            "allocations",
            raw.len() <= MAX_LINES,
            format!("Send at most {MAX_LINES} allocations."),
        );
        let mut out: MatchInput = Vec::with_capacity(raw.len());
        for (i, a) in raw.into_iter().take(MAX_LINES).enumerate() {
            let mut ar = Rules::default();
            let line = uuid(&mut ar, "bill_line_id", a.bill_line_id);
            let receipt = uuid(&mut ar, "receipt_line_id", a.receipt_line_id);
            let q = quantity(&mut ar, "quantity", a.quantity);
            if let (Some(_), Some(rl)) = (line, receipt) {
                ar.check(
                    "receipt_line_id",
                    !out.iter().any(|(_, x, _)| *x == rl),
                    "Each receipt line may appear once per bill.",
                );
            }
            let ok = !ar.merge_into_prefixed(r, &format!("allocations.{i}."));
            if let (true, Some(l), Some(rl), Some(q)) = (ok, line, receipt, q) {
                out.push((l, rl, q));
            }
        }
        Some(out)
    }
}

/// `{review_fingerprint}`: post an action exactly as reviewed.
#[derive(Deserialize, ToSchema)]
pub struct ReviewedPayload {
    /// From the action's `/review`.
    pub review_fingerprint: Option<String>,
}

impl Validate for ReviewedPayload {
    type Valid = String;

    fn validate(self, r: &mut Rules) -> Option<String> {
        fingerprint(r, self.review_fingerprint)
    }
}

/// Body of a reversal review: date and reason.
#[derive(Deserialize, ToSchema)]
pub struct ReverseBody {
    /// Open period, not before the original's date.
    #[schema(value_type = NaiveDate)]
    pub accounting_date: Option<String>,
    pub reason: Option<String>,
}

/// Validated reversal facts.
#[derive(Clone, Debug)]
pub struct ReverseInput {
    pub date: NaiveDate,
    pub reason: String,
}

impl Validate for ReverseBody {
    type Valid = ReverseInput;

    fn validate(self, r: &mut Rules) -> Option<ReverseInput> {
        let date = date(r, "accounting_date", self.accounting_date);
        let reason = r.required("reason", self.reason);
        if let Some(v) = &reason {
            r.check("reason", !v.trim().is_empty(), "The reason is required.");
            r.max_chars("reason", v, NOTE_MAX);
        }
        Some(ReverseInput {
            date: date?,
            reason: reason?.trim().to_owned(),
        })
    }
}

/// A reversal payload: the reviewed date and reason plus the review fingerprint.
#[derive(Deserialize, ToSchema)]
#[schema(as = PayablesReversePayload)]
pub struct ReversePayload {
    #[schema(value_type = NaiveDate)]
    pub accounting_date: Option<String>,
    pub reason: Option<String>,
    pub review_fingerprint: Option<String>,
}

impl Validate for ReversePayload {
    type Valid = (ReverseInput, String);

    fn validate(self, r: &mut Rules) -> Option<(ReverseInput, String)> {
        let input = ReverseBody {
            accounting_date: self.accounting_date,
            reason: self.reason,
        }
        .validate(r);
        let fp = fingerprint(r, self.review_fingerprint);
        Some((input?, fp?))
    }
}

/// GET list query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    pub page: Option<String>,
    pub page_size: Option<String>,
    /// Only this supplier.
    #[param(value_type = Option<Uuid>)]
    pub supplier_id: Option<Uuid>,
}

/// A match of a bill line.
#[derive(Serialize, ToSchema)]
pub struct BillMatchResource {
    pub id: Uuid,
    pub receipt_line_id: Option<Uuid>,
    pub landed_cost_id: Option<Uuid>,
    pub quantity: String,
    /// draft (reservation), posted or reversed.
    pub state: String,
    /// Posted only, with `accounting.read` (functional currency).
    pub grni_functional: Option<String>,
    pub fx: Option<String>,
    pub variance: Option<String>,
}

/// One bill line.
#[derive(Serialize, ToSchema)]
pub struct BillLineResource {
    pub id: Uuid,
    pub line_no: i32,
    /// goods, landed_cost or expense.
    pub kind: String,
    pub product_id: Option<Uuid>,
    pub landed_cost_id: Option<Uuid>,
    pub expense_account_id: Option<Uuid>,
    pub quantity: String,
    pub unit_price: String,
    pub extension: String,
    pub tax_profile_id: Option<Uuid>,
    pub description: Option<String>,
    /// Posted only (bill currency).
    pub tax: Option<String>,
    pub matches: Vec<BillMatchResource>,
}

/// Bill totals in the bill currency; tax and total are known from post.
#[derive(Serialize, ToSchema)]
pub struct BillTotals {
    pub net: String,
    pub tax: Option<String>,
    pub total: Option<String>,
    /// AP in the functional currency (posted, `accounting.read`).
    pub functional_total: Option<String>,
}

/// A supplier bill.
#[derive(Serialize, ToSchema)]
pub struct SupplierBillResource {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub number: i64,
    /// draft, posted or reversed.
    pub state: String,
    pub version: i64,
    pub supplier: SupplierRef,
    pub currency: CurrencyRef,
    pub supplier_invoice_number: String,
    pub corrects_bill_id: Option<Uuid>,
    pub invoice_date: NaiveDate,
    pub due_date: Option<NaiveDate>,
    pub accounting_date: NaiveDate,
    pub evidence_ref: Option<String>,
    pub note: Option<String>,
    pub totals: BillTotals,
    pub rate: Option<RateSnapshot>,
    pub ap_item_id: Option<Uuid>,
    pub lines: Vec<BillLineResource>,
    pub posted_at: Option<DateTime<Utc>>,
    pub reversed_at: Option<DateTime<Utc>>,
    pub reversal_date: Option<NaiveDate>,
    pub reverse_reason: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub allowed_actions: Vec<&'static str>,
}

/// One tax component of a line, as `tax_line` computes it.
#[derive(Serialize, ToSchema, Clone)]
#[schema(as = BillTaxComponent)]
pub struct TaxComponent {
    pub code: String,
    pub rate: String,
    pub base: String,
    pub tax: String,
    pub recoverable: bool,
    pub exempt: bool,
}

/// A match as posting will freeze it (functional currency).
#[derive(Serialize, ToSchema)]
pub struct ReviewMatch {
    pub match_id: Uuid,
    pub receipt_line_id: Option<Uuid>,
    pub landed_cost_id: Option<Uuid>,
    pub quantity: String,
    /// GRNI relieved in the PO / landed-cost currency.
    pub grni_nominal: String,
    pub grni_functional: Option<String>,
    pub share_functional: Option<String>,
    pub fx: Option<String>,
    pub variance: Option<String>,
}

/// A line as posting will freeze it.
#[derive(Serialize, ToSchema)]
pub struct ReviewLine {
    pub line_id: Uuid,
    pub kind: String,
    pub extension: String,
    pub tax_recoverable: String,
    pub tax_nonrecoverable: String,
    pub functional: Option<String>,
    pub taxes: Vec<TaxComponent>,
    pub matches: Vec<ReviewMatch>,
}

/// One journal line a posting writes (functional currency): a mapped role or an account.
#[derive(Serialize, ToSchema, Clone)]
pub struct JournalPreview {
    pub role: Option<String>,
    pub account_id: Option<Uuid>,
    pub debit: String,
    pub credit: String,
}

/// POST /supplier-bills/{id}/post/review.
#[derive(Serialize, ToSchema)]
pub struct BillPostReview {
    pub bill_id: Uuid,
    pub version: i64,
    pub accounting_date: NaiveDate,
    pub currency: CurrencyRef,
    pub functional_currency: CurrencyRef,
    pub rate: RateSnapshot,
    pub net: String,
    pub tax: String,
    pub total: String,
    pub functional_total: Option<String>,
    pub lines: Vec<ReviewLine>,
    /// With `accounting.read`.
    pub journal: Option<Vec<JournalPreview>>,
    pub review_fingerprint: String,
}

/// POST .../reverse/review of a bill, payment, return or cost adjustment.
#[derive(Serialize, ToSchema)]
pub struct ReverseReview {
    pub id: Uuid,
    pub version: i64,
    pub accounting_date: NaiveDate,
    pub reason: String,
    /// The compensating journal (with `accounting.read`).
    pub journal: Option<Vec<JournalPreview>>,
    pub review_fingerprint: String,
}

// --- supplier payments ---------------------------------------------------------------------

/// POST /supplier-payments (and its review) payload: real money to (`payment`) or from
/// (`refund_received`) the supplier at one cash place or bank account.
#[derive(Deserialize, ToSchema)]
pub struct PaymentPayload {
    #[schema(value_type = Uuid)]
    pub supplier_id: Option<String>,
    #[schema(value_type = Uuid)]
    pub currency_id: Option<String>,
    /// `payment` or `refund_received`.
    pub direction: Option<String>,
    /// Major units of the payment currency.
    #[schema(value_type = String, example = "100.00")]
    pub amount: Option<Value>,
    /// The book's business day.
    #[schema(value_type = NaiveDate)]
    pub date: Option<String>,
    /// Where the money left or arrived: `cash_place` or `bank_account`.
    pub location: Option<crate::modules::accounting::settlement::MoneyLocationRequest>,
    pub reference: Option<String>,
    pub evidence_ref: Option<String>,
    pub note: Option<String>,
    /// Required on create: from `POST /supplier-payments/review`.
    pub review_fingerprint: Option<String>,
}

/// Validated payment.
pub struct PaymentInput {
    pub supplier: Uuid,
    pub currency: Uuid,
    pub direction: &'static str,
    pub amount: Decimal,
    pub date: NaiveDate,
    pub location: crate::modules::accounting::settlement::MoneyLocation,
    pub reference: Option<String>,
    pub evidence_ref: String,
    pub note: Option<String>,
    pub review_fingerprint: Option<String>,
}

impl Validate for PaymentPayload {
    type Valid = PaymentInput;

    fn validate(self, r: &mut Rules) -> Option<PaymentInput> {
        use crate::modules::accounting::settlement::MoneyLocation;
        let supplier = uuid(r, "supplier_id", self.supplier_id);
        let currency = uuid(r, "currency_id", self.currency_id);
        let direction = match self.direction.as_deref() {
            Some("payment") => Some("payment"),
            Some("refund_received") => Some("refund_received"),
            None => {
                r.missing("direction");
                None
            }
            Some(_) => {
                r.add(
                    "direction",
                    "The direction must be payment or refund_received.",
                );
                None
            }
        };
        let amount = match self.amount {
            Some(v) => r.decimal("amount", &v, Decimal::new(1, 8), crate::money::MAX),
            None => {
                r.missing("amount");
                None
            }
        };
        let date = date(r, "date", self.date);
        let location = match self.location {
            Some(l) => l.validate(r, "location"),
            None => {
                r.missing("location");
                None
            }
        };
        if let Some(MoneyLocation::ProviderClearing(_)) = &location {
            r.add(
                "location.kind",
                "Supplier money moves through a cash place or a bank account.",
            );
        }
        let reference = r.opt_text("reference", self.reference, REF_MAX);
        let evidence_ref = r.required("evidence_ref", self.evidence_ref);
        if let Some(v) = &evidence_ref {
            r.check(
                "evidence_ref",
                !v.trim().is_empty(),
                "The evidence ref is required.",
            );
            r.max_chars("evidence_ref", v, REF_MAX);
        }
        let note = r.opt_text("note", self.note, NOTE_MAX);
        Some(PaymentInput {
            supplier: supplier?,
            currency: currency?,
            direction: direction?,
            amount: amount?,
            date: date?,
            location: location.filter(|l| !matches!(l, MoneyLocation::ProviderClearing(_)))?,
            reference,
            evidence_ref: evidence_ref?.trim().to_owned(),
            note,
            review_fingerprint: self.review_fingerprint,
        })
    }
}

/// POST /supplier-payments/{id}/allocate (and review) payload.
#[derive(Deserialize, ToSchema)]
pub struct AllocatePayload {
    /// A payable (for a payment) or supplier credit (for a refund received) of this supplier.
    #[schema(value_type = Uuid)]
    pub target_open_item_id: Option<String>,
    /// Major units of the payment currency.
    #[schema(value_type = String, example = "100.00")]
    pub amount: Option<Value>,
    /// Required on allocate: from `.../allocate/review`.
    pub review_fingerprint: Option<String>,
}

/// Validated allocation.
pub struct AllocateInput {
    pub target: Uuid,
    pub amount: Decimal,
    pub review_fingerprint: Option<String>,
}

impl Validate for AllocatePayload {
    type Valid = AllocateInput;

    fn validate(self, r: &mut Rules) -> Option<AllocateInput> {
        let target = uuid(r, "target_open_item_id", self.target_open_item_id);
        let amount = match self.amount {
            Some(v) => r.decimal("amount", &v, Decimal::new(1, 8), crate::money::MAX),
            None => {
                r.missing("amount");
                None
            }
        };
        Some(AllocateInput {
            target: target?,
            amount: amount?,
            review_fingerprint: self.review_fingerprint,
        })
    }
}

/// One application of a payment.
#[derive(Serialize, ToSchema)]
pub struct PaymentAllocationResource {
    pub allocation_id: Uuid,
    pub target_open_item_id: Uuid,
    pub amount: String,
    pub accounting_date: NaiveDate,
    /// Realized FX (functional, `accounting.read`): positive = loss.
    pub fx: Option<String>,
    pub reversed: bool,
}

/// A supplier payment or refund received.
#[derive(Serialize, ToSchema)]
pub struct SupplierPaymentResource {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub number: i64,
    pub direction: String,
    /// posted or reversed.
    pub state: String,
    pub version: i64,
    pub supplier: SupplierRef,
    pub currency: CurrencyRef,
    pub amount: String,
    /// Still to apply.
    pub unapplied: String,
    pub date: NaiveDate,
    pub location_kind: String,
    pub register_id: Option<Uuid>,
    pub vault_id: Option<Uuid>,
    pub bank_account_id: Option<Uuid>,
    pub reference: Option<String>,
    pub evidence_ref: String,
    pub note: Option<String>,
    pub rate: RateSnapshot,
    pub functional_amount: Option<String>,
    pub open_item_id: Uuid,
    pub allocations: Vec<PaymentAllocationResource>,
    pub reversed_at: Option<DateTime<Utc>>,
    pub reversal_date: Option<NaiveDate>,
    pub reverse_reason: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub allowed_actions: Vec<&'static str>,
}

/// POST /supplier-payments/review.
#[derive(Serialize, ToSchema)]
pub struct PaymentReview {
    pub supplier_id: Uuid,
    pub currency: CurrencyRef,
    pub direction: String,
    pub amount: String,
    pub date: NaiveDate,
    pub location_kind: String,
    pub functional_currency: CurrencyRef,
    pub rate: RateSnapshot,
    pub functional_amount: Option<String>,
    pub journal: Option<Vec<JournalPreview>>,
    pub review_fingerprint: String,
}

/// POST /supplier-payments/{id}/allocate/review.
#[derive(Serialize, ToSchema)]
pub struct AllocateReview {
    pub payment_id: Uuid,
    pub version: i64,
    pub target_open_item_id: Uuid,
    pub amount: String,
    /// Functional carrying relieved on each side, and realized FX (positive = loss).
    pub source_functional: Option<String>,
    pub target_functional: Option<String>,
    pub fx: Option<String>,
    pub journal: Option<Vec<JournalPreview>>,
    pub review_fingerprint: String,
}

// --- purchase returns ----------------------------------------------------------------------

/// One returned receipt line as sent.
#[derive(Deserialize, ToSchema)]
pub struct ReturnLinePayload {
    #[schema(value_type = Uuid)]
    pub receipt_line_id: Option<String>,
    /// Base units, ≤2 places.
    #[schema(value_type = String, example = "1")]
    pub quantity: Option<Value>,
    /// The posted bill match this quantity was billed on (billed return); omit for unbilled.
    #[schema(value_type = Option<Uuid>)]
    pub source_bill_allocation_id: Option<String>,
    /// Where the goods leave from; defaults to the receipt line's place.
    #[schema(value_type = Option<Uuid>)]
    pub warehouse_id: Option<String>,
    #[schema(value_type = Option<Uuid>)]
    pub bin_id: Option<String>,
    #[schema(value_type = Option<Uuid>)]
    pub lot_id: Option<String>,
}

/// Validated return line.
pub struct ReturnLineInput {
    pub receipt_line: Uuid,
    pub quantity: Decimal,
    pub bill_match: Option<Uuid>,
    pub warehouse: Option<Uuid>,
    pub bin: Option<Uuid>,
    pub lot: Option<Uuid>,
}

/// POST /purchase-returns payload: a draft return of one receipt's goods.
#[derive(Deserialize, ToSchema)]
pub struct ReturnPayload {
    #[schema(value_type = Uuid)]
    pub receipt_id: Option<String>,
    /// `vendor_return` or `receipt_correction` (unbilled lines only).
    pub purpose: Option<String>,
    pub reason: Option<String>,
    #[schema(value_type = NaiveDate)]
    pub accounting_date: Option<String>,
    pub supplier_credit_evidence_ref: Option<String>,
    /// Apply the supplier credit to the original bill's open AP at post.
    pub apply_credit: Option<bool>,
    pub lines: Option<Vec<ReturnLinePayload>>,
}

/// Validated return.
pub struct ReturnInput {
    pub receipt: Uuid,
    pub purpose: &'static str,
    pub reason: String,
    pub accounting_date: NaiveDate,
    pub evidence_ref: Option<String>,
    pub apply_credit: bool,
    pub lines: Vec<ReturnLineInput>,
}

impl Validate for ReturnPayload {
    type Valid = ReturnInput;

    fn validate(self, r: &mut Rules) -> Option<ReturnInput> {
        let receipt = uuid(r, "receipt_id", self.receipt_id);
        let purpose = match self.purpose.as_deref() {
            Some("vendor_return") => Some("vendor_return"),
            Some("receipt_correction") => Some("receipt_correction"),
            None => {
                r.missing("purpose");
                None
            }
            Some(_) => {
                r.add(
                    "purpose",
                    "The purpose must be vendor_return or receipt_correction.",
                );
                None
            }
        };
        let reason = r.required("reason", self.reason);
        if let Some(v) = &reason {
            r.check("reason", !v.trim().is_empty(), "The reason is required.");
            r.max_chars("reason", v, NOTE_MAX);
        }
        let accounting_date = date(r, "accounting_date", self.accounting_date);
        let evidence_ref = r.opt_text(
            "supplier_credit_evidence_ref",
            self.supplier_credit_evidence_ref,
            REF_MAX,
        );
        let raw = self.lines.unwrap_or_default();
        r.check(
            "lines",
            (1..=MAX_LINES).contains(&raw.len()),
            format!("A return needs 1 to {MAX_LINES} lines."),
        );
        let mut lines: Vec<ReturnLineInput> = Vec::with_capacity(raw.len());
        for (i, l) in raw.into_iter().take(MAX_LINES).enumerate() {
            let mut lr = Rules::default();
            let receipt_line = uuid(&mut lr, "receipt_line_id", l.receipt_line_id);
            let quantity = quantity(&mut lr, "quantity", l.quantity);
            let bill_match = opt_uuid(
                &mut lr,
                "source_bill_allocation_id",
                l.source_bill_allocation_id,
            );
            let warehouse = opt_uuid(&mut lr, "warehouse_id", l.warehouse_id);
            let bin = opt_uuid(&mut lr, "bin_id", l.bin_id);
            let lot = opt_uuid(&mut lr, "lot_id", l.lot_id);
            if let Some(rl) = receipt_line {
                lr.check(
                    "receipt_line_id",
                    !lines.iter().any(|x| x.receipt_line == rl),
                    "Each receipt line may appear once per return.",
                );
            }
            let ok = !lr.merge_into_prefixed(r, &format!("lines.{i}."));
            if let (
                true,
                Some(receipt_line),
                Some(quantity),
                Some(bill_match),
                Some(warehouse),
                Some(bin),
                Some(lot),
            ) = (ok, receipt_line, quantity, bill_match, warehouse, bin, lot)
            {
                lines.push(ReturnLineInput {
                    receipt_line,
                    quantity,
                    bill_match,
                    warehouse,
                    bin,
                    lot,
                });
            }
        }
        Some(ReturnInput {
            receipt: receipt?,
            purpose: purpose?,
            reason: reason?.trim().to_owned(),
            accounting_date: accounting_date?,
            evidence_ref,
            apply_credit: self.apply_credit.unwrap_or(false),
            lines,
        })
    }
}

/// Reversal of a posted return (§15): both components together in M5-A.
#[derive(Deserialize, ToSchema)]
pub struct ReturnReverseBody {
    #[schema(value_type = NaiveDate)]
    pub accounting_date: Option<String>,
    pub reverse_financial: Option<bool>,
    pub reverse_physical: Option<bool>,
    pub evidence_ref: Option<String>,
    pub reason: Option<String>,
    /// Required on reverse: from `.../reverse/review`.
    pub review_fingerprint: Option<String>,
}

/// Validated return reversal.
pub struct ReturnReverseInput {
    pub reverse: ReverseInput,
    pub financial: bool,
    pub physical: bool,
    pub evidence_ref: Option<String>,
    pub review_fingerprint: Option<String>,
}

impl Validate for ReturnReverseBody {
    type Valid = ReturnReverseInput;

    fn validate(self, r: &mut Rules) -> Option<ReturnReverseInput> {
        let reverse = ReverseBody {
            accounting_date: self.accounting_date,
            reason: self.reason,
        }
        .validate(r);
        let evidence_ref = r.opt_text("evidence_ref", self.evidence_ref, REF_MAX);
        Some(ReturnReverseInput {
            reverse: reverse?,
            financial: self.reverse_financial.unwrap_or(false),
            physical: self.reverse_physical.unwrap_or(false),
            evidence_ref,
            review_fingerprint: self.review_fingerprint,
        })
    }
}

/// One return line; functional facts posted only, with `accounting.read`.
#[derive(Serialize, ToSchema)]
pub struct ReturnLineResource {
    pub id: Uuid,
    pub line_no: i32,
    pub receipt_line_id: Uuid,
    pub bill_match_id: Option<Uuid>,
    pub billed: bool,
    pub quantity: String,
    pub warehouse_id: Uuid,
    pub bin_id: Option<Uuid>,
    pub lot_id: Option<Uuid>,
    /// C: the current average value the goods left at.
    pub issue_value: Option<String>,
    pub grni_functional: Option<String>,
    /// Supplier credit (document currency) and its functional basis.
    pub credit_nominal: Option<String>,
    pub credit_functional: Option<String>,
    pub tax_functional: Option<String>,
    /// Purchase-return variance (debit positive).
    pub variance: Option<String>,
    pub fx: Option<String>,
}

/// A purchase return.
#[derive(Serialize, ToSchema)]
pub struct PurchaseReturnResource {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub number: i64,
    /// draft, posted or reversed.
    pub state: String,
    pub version: i64,
    pub purpose: String,
    pub receipt_id: Uuid,
    pub supplier: SupplierRef,
    pub currency: CurrencyRef,
    pub accounting_date: NaiveDate,
    pub reason: String,
    pub evidence_ref: Option<String>,
    pub apply_credit: bool,
    /// Supplier credit open item (billed lines).
    pub credit_item_id: Option<Uuid>,
    pub lines: Vec<ReturnLineResource>,
    pub posted_at: Option<DateTime<Utc>>,
    pub reversed_at: Option<DateTime<Utc>>,
    pub reverse_reason: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub allowed_actions: Vec<&'static str>,
}

/// One line of a return review (functional unless *_nominal).
#[derive(Serialize, ToSchema)]
pub struct ReturnReviewLine {
    pub line_id: Uuid,
    pub receipt_line_id: Uuid,
    pub billed: bool,
    pub quantity: String,
    pub issue_value: Option<String>,
    pub grni_nominal: Option<String>,
    pub grni_functional: Option<String>,
    pub credit_nominal: Option<String>,
    pub credit_functional: Option<String>,
    pub tax_nominal: Option<String>,
    pub tax_functional: Option<String>,
    pub variance: Option<String>,
    pub fx: Option<String>,
}

/// POST /purchase-returns/{id}/post/review.
#[derive(Serialize, ToSchema)]
pub struct ReturnPostReview {
    pub return_id: Uuid,
    pub version: i64,
    pub accounting_date: NaiveDate,
    pub lines: Vec<ReturnReviewLine>,
    pub journal: Option<Vec<JournalPreview>>,
    pub review_fingerprint: String,
}

// --- purchase cost adjustments ---------------------------------------------------------------

/// One reviewed split line: the share of a bill's pending variance for one receipt line.
#[derive(Deserialize, ToSchema)]
pub struct CostAllocationPayload {
    #[schema(value_type = Uuid)]
    pub receipt_line_id: Option<String>,
    /// Signed, functional major units: onto the stock still held (value-only pool entry).
    #[schema(value_type = String, example = "2.00")]
    pub inventory_amount: Option<Value>,
    /// Signed, functional major units: onto cost of goods already sold.
    #[schema(value_type = String, example = "1.00")]
    pub cogs_amount: Option<Value>,
}

/// POST /purchase-cost-adjustments payload.
#[derive(Deserialize, ToSchema)]
pub struct CostAdjustmentPayload {
    #[schema(value_type = Uuid)]
    pub bill_id: Option<String>,
    pub reason: Option<String>,
    pub evidence_ref: Option<String>,
    pub allocations: Option<Vec<CostAllocationPayload>>,
}

/// Validated adjustment: (receipt line, inventory, cogs) in functional major units.
pub struct CostAdjustmentInput {
    pub bill: Uuid,
    pub reason: String,
    pub evidence_ref: String,
    pub allocations: Vec<(Uuid, Decimal, Decimal)>,
}

impl Validate for CostAdjustmentPayload {
    type Valid = CostAdjustmentInput;

    fn validate(self, r: &mut Rules) -> Option<CostAdjustmentInput> {
        let bill = uuid(r, "bill_id", self.bill_id);
        let reason = r.required("reason", self.reason);
        if let Some(v) = &reason {
            r.check("reason", !v.trim().is_empty(), "The reason is required.");
            r.max_chars("reason", v, NOTE_MAX);
        }
        let evidence_ref = r.required("evidence_ref", self.evidence_ref);
        if let Some(v) = &evidence_ref {
            r.check(
                "evidence_ref",
                !v.trim().is_empty(),
                "The evidence ref is required.",
            );
            r.max_chars("evidence_ref", v, REF_MAX);
        }
        let raw = self.allocations.unwrap_or_default();
        r.check(
            "allocations",
            (1..=MAX_LINES).contains(&raw.len()),
            format!("Send 1 to {MAX_LINES} allocations."),
        );
        let neg = -crate::money::MAX;
        let mut out: Vec<(Uuid, Decimal, Decimal)> = Vec::with_capacity(raw.len());
        for (i, a) in raw.into_iter().take(MAX_LINES).enumerate() {
            let mut ar = Rules::default();
            let line = uuid(&mut ar, "receipt_line_id", a.receipt_line_id);
            let amt = |ar: &mut Rules, f: &str, v: Option<Value>| match v {
                None => Some(Decimal::ZERO),
                Some(v) => ar.decimal(f, &v, neg, crate::money::MAX),
            };
            let inv = amt(&mut ar, "inventory_amount", a.inventory_amount);
            let cogs = amt(&mut ar, "cogs_amount", a.cogs_amount);
            if let (Some(i), Some(c)) = (inv, cogs) {
                ar.check(
                    "inventory_amount",
                    !(i.is_zero() && c.is_zero()),
                    "Give an inventory or a COGS amount.",
                );
            }
            if let Some(l) = line {
                ar.check(
                    "receipt_line_id",
                    !out.iter().any(|x| x.0 == l),
                    "Each receipt line may appear once.",
                );
            }
            let ok = !ar.merge_into_prefixed(r, &format!("allocations.{i}."));
            if let (true, Some(l), Some(i), Some(c)) = (ok, line, inv, cogs) {
                out.push((l, i, c));
            }
        }
        Some(CostAdjustmentInput {
            bill: bill?,
            reason: reason?.trim().to_owned(),
            evidence_ref: evidence_ref?.trim().to_owned(),
            allocations: out,
        })
    }
}

/// One adjustment line (functional currency).
#[derive(Serialize, ToSchema)]
pub struct CostAdjustmentLineResource {
    pub id: Uuid,
    pub line_no: i32,
    pub receipt_line_id: Uuid,
    pub inventory_amount: String,
    pub cogs_amount: String,
    pub value_entry_id: Option<Uuid>,
}

/// A purchase cost adjustment.
#[derive(Serialize, ToSchema)]
pub struct CostAdjustmentResource {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub bill_id: Uuid,
    /// draft, posted or reversed.
    pub state: String,
    pub version: i64,
    pub reason: String,
    pub evidence_ref: String,
    pub functional_currency: CurrencyRef,
    pub accounting_date: Option<NaiveDate>,
    /// The bill's pending purchase cost variance still to split.
    pub bill_pending_variance: String,
    pub lines: Vec<CostAdjustmentLineResource>,
    pub posted_at: Option<DateTime<Utc>>,
    pub reversed_at: Option<DateTime<Utc>>,
    pub reverse_reason: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub allowed_actions: Vec<&'static str>,
}

/// POST /purchase-cost-adjustments/{id}/post/review.
#[derive(Serialize, ToSchema)]
pub struct CostAdjustmentReview {
    pub adjustment_id: Uuid,
    pub version: i64,
    pub accounting_date: NaiveDate,
    pub bill_pending_variance: String,
    pub total: String,
    pub journal: Vec<JournalPreview>,
    pub review_fingerprint: String,
}
