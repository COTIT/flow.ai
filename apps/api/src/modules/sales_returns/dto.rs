//! Sales return payloads (inside the accounting C/A envelopes), their validation, and response
//! resources. Quantities are decimal strings (≤2 places); amounts exact decimal strings in
//! major units of the sale currency.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::{
        accounting::dto::{MAX_LINES, date, opt_uuid, uuid},
        stock::repo::LEVEL_MAX,
    },
};

const TEXT_MAX: usize = 1000;
const REF_MAX: usize = 255;
/// Largest amount accepted (well inside bigint minor units).
const AMOUNT_MAX: Decimal = Decimal::from_parts(1_000_000_000, 0, 0, false, 0);

fn quantity(r: &mut Rules, v: Option<Value>) -> Option<Decimal> {
    match v {
        None => {
            r.missing("quantity");
            None
        }
        v => r.measure("quantity", v, Decimal::new(1, 2), LEVEL_MAX, 2),
    }
}

fn reason(r: &mut Rules, v: Option<String>) -> Option<String> {
    let t = r.required("reason", v)?;
    r.max_chars("reason", &t, TEXT_MAX);
    Some(t)
}

/// One recognition line to credit.
#[derive(Deserialize, ToSchema)]
#[schema(as = SalesReturnReturnLinePayload)]
pub struct ReturnLinePayload {
    #[schema(value_type = Uuid)]
    pub recognition_line_id: Option<String>,
    /// Units of the recognised line, ≤2 places; Σ draft + posted credits ≤ recognised.
    #[schema(value_type = String, example = "1")]
    pub quantity: Option<Value>,
}

/// Validated lines (recognition line, quantity), each line once.
fn lines(r: &mut Rules, raw: Option<Vec<ReturnLinePayload>>) -> Vec<(Uuid, Decimal)> {
    let raw = raw.unwrap_or_default();
    r.check(
        "lines",
        (1..=MAX_LINES).contains(&raw.len()),
        format!("Send 1 to {MAX_LINES} lines."),
    );
    let mut out: Vec<(Uuid, Decimal)> = Vec::with_capacity(raw.len());
    for (i, l) in raw.into_iter().take(MAX_LINES).enumerate() {
        let mut lr = Rules::default();
        let id = uuid(&mut lr, "recognition_line_id", l.recognition_line_id);
        let q = quantity(&mut lr, l.quantity);
        if let Some(id) = id {
            lr.check(
                "recognition_line_id",
                !out.iter().any(|(x, _)| *x == id),
                "Each line may appear once.",
            );
        }
        let ok = !lr.merge_into_prefixed(r, &format!("lines.{i}."));
        if let (true, Some(id), Some(q)) = (ok, id, q) {
            out.push((id, q));
        }
    }
    out
}

/// Create payload.
#[derive(Deserialize, ToSchema)]
pub struct ReturnCreatePayload {
    /// The sale (recognition) credited.
    #[schema(value_type = Uuid)]
    pub recognition_id: Option<String>,
    /// `YYYY-MM-DD`; the post must happen on this book day.
    pub accounting_date: Option<String>,
    pub reason: Option<String>,
    pub lines: Option<Vec<ReturnLinePayload>>,
}

/// Validated draft fields.
pub struct ReturnInput {
    pub accounting_date: NaiveDate,
    pub reason: String,
    pub lines: Vec<(Uuid, Decimal)>,
}

impl Validate for ReturnCreatePayload {
    type Valid = (Uuid, ReturnInput);

    fn validate(self, r: &mut Rules) -> Option<(Uuid, ReturnInput)> {
        let rec = uuid(r, "recognition_id", self.recognition_id);
        let d = date(r, "accounting_date", self.accounting_date);
        let why = reason(r, self.reason);
        let lines = lines(r, self.lines);
        Some((
            rec?,
            ReturnInput {
                accounting_date: d?,
                reason: why?,
                lines,
            },
        ))
    }
}

/// Update payload: replaces the draft's date, reason and lines.
#[derive(Deserialize, ToSchema)]
pub struct ReturnUpdatePayload {
    pub accounting_date: Option<String>,
    pub reason: Option<String>,
    pub lines: Option<Vec<ReturnLinePayload>>,
}

impl Validate for ReturnUpdatePayload {
    type Valid = ReturnInput;

    fn validate(self, r: &mut Rules) -> Option<ReturnInput> {
        let d = date(r, "accounting_date", self.accounting_date);
        let why = reason(r, self.reason);
        let lines = lines(r, self.lines);
        Some(ReturnInput {
            accounting_date: d?,
            reason: why?,
            lines,
        })
    }
}

/// POST .../post and .../restocks/{id}/reverse payload.
#[derive(Deserialize, ToSchema)]
pub struct FingerprintPayload {
    /// From the matching `/review`.
    pub review_fingerprint: Option<String>,
}

impl Validate for FingerprintPayload {
    type Valid = String;

    fn validate(self, r: &mut Rules) -> Option<String> {
        r.required("review_fingerprint", self.review_fingerprint)
    }
}

/// One return line to put back on stock.
#[derive(Deserialize, ToSchema)]
pub struct RestockLinePayload {
    #[schema(value_type = Uuid)]
    pub return_line_id: Option<String>,
    #[schema(value_type = String, example = "1")]
    pub quantity: Option<Value>,
    /// Must be the warehouse the sale left (its booked cost lives there).
    #[schema(value_type = Uuid)]
    pub destination_warehouse_id: Option<String>,
    /// A reviewed legacy return cost, for a sale with no booked cost in the book.
    #[schema(value_type = Option<Uuid>)]
    pub legacy_cost_evidence_id: Option<String>,
}

/// POST .../restock/review body.
#[derive(Deserialize, ToSchema)]
pub struct RestockReviewBody {
    pub lines: Option<Vec<RestockLinePayload>>,
}

/// POST .../restock payload.
#[derive(Deserialize, ToSchema)]
pub struct RestockPayload {
    pub lines: Option<Vec<RestockLinePayload>>,
    pub review_fingerprint: Option<String>,
}

/// One validated restock line.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RestockLine {
    pub return_line_id: Uuid,
    pub quantity: Decimal,
    pub warehouse_id: Uuid,
    pub legacy_cost_id: Option<Uuid>,
}

fn restock_lines(r: &mut Rules, raw: Option<Vec<RestockLinePayload>>) -> Vec<RestockLine> {
    let raw = raw.unwrap_or_default();
    r.check(
        "lines",
        (1..=MAX_LINES).contains(&raw.len()),
        format!("Send 1 to {MAX_LINES} lines."),
    );
    let mut out: Vec<RestockLine> = Vec::with_capacity(raw.len());
    for (i, l) in raw.into_iter().take(MAX_LINES).enumerate() {
        let mut lr = Rules::default();
        let id = uuid(&mut lr, "return_line_id", l.return_line_id);
        let q = quantity(&mut lr, l.quantity);
        let w = uuid(
            &mut lr,
            "destination_warehouse_id",
            l.destination_warehouse_id,
        );
        let legacy = opt_uuid(
            &mut lr,
            "legacy_cost_evidence_id",
            l.legacy_cost_evidence_id,
        );
        if let Some(id) = id {
            lr.check(
                "return_line_id",
                !out.iter().any(|x| x.return_line_id == id),
                "Each line may appear once.",
            );
        }
        let ok = !lr.merge_into_prefixed(r, &format!("lines.{i}."));
        if let (true, Some(id), Some(q), Some(w), Some(legacy)) = (ok, id, q, w, legacy) {
            out.push(RestockLine {
                return_line_id: id,
                quantity: q,
                warehouse_id: w,
                legacy_cost_id: legacy,
            });
        }
    }
    out
}

impl Validate for RestockReviewBody {
    type Valid = Vec<RestockLine>;

    fn validate(self, r: &mut Rules) -> Option<Vec<RestockLine>> {
        Some(restock_lines(r, self.lines))
    }
}

impl Validate for RestockPayload {
    type Valid = (Vec<RestockLine>, String);

    fn validate(self, r: &mut Rules) -> Option<(Vec<RestockLine>, String)> {
        let lines = restock_lines(r, self.lines);
        let f = r.required("review_fingerprint", self.review_fingerprint);
        Some((lines, f?))
    }
}

/// POST .../reverse/review body.
#[derive(Deserialize, ToSchema)]
#[schema(as = SalesReturnReverseReviewBody)]
pub struct ReverseReviewBody {
    pub accounting_date: Option<String>,
}

impl Validate for ReverseReviewBody {
    type Valid = NaiveDate;

    fn validate(self, r: &mut Rules) -> Option<NaiveDate> {
        date(r, "accounting_date", self.accounting_date)
    }
}

/// POST .../reverse payload (financial credit only; restocks stay).
#[derive(Deserialize, ToSchema)]
#[schema(as = SalesReturnReversePayload)]
pub struct ReversePayload {
    pub accounting_date: Option<String>,
    pub reason: Option<String>,
    pub review_fingerprint: Option<String>,
}

impl Validate for ReversePayload {
    type Valid = (NaiveDate, String, String);

    fn validate(self, r: &mut Rules) -> Option<(NaiveDate, String, String)> {
        let d = date(r, "accounting_date", self.accounting_date);
        let why = reason(r, self.reason);
        let f = r.required("review_fingerprint", self.review_fingerprint);
        Some((d?, why?, f?))
    }
}

/// POST .../restocks/{restock}/reverse payload (physical only; the credit stays).
#[derive(Deserialize, ToSchema)]
pub struct RestockReversePayload {
    pub reason: Option<String>,
    pub review_fingerprint: Option<String>,
}

impl Validate for RestockReversePayload {
    type Valid = (String, String);

    fn validate(self, r: &mut Rules) -> Option<(String, String)> {
        let why = reason(r, self.reason);
        let f = r.required("review_fingerprint", self.review_fingerprint);
        Some((why?, f?))
    }
}

/// POST /legacy-return-costs payload.
#[derive(Deserialize, ToSchema)]
pub struct LegacyCostPayload {
    /// The original order.
    #[schema(value_type = Uuid)]
    pub original_sale_id: Option<String>,
    /// Its order line.
    #[schema(value_type = Uuid)]
    pub line_id: Option<String>,
    #[schema(value_type = String, example = "2")]
    pub quantity: Option<Value>,
    /// Reviewed value of `quantity` units, major units of the book currency.
    #[schema(value_type = String, example = "3.00")]
    pub restoration_amount: Option<Value>,
    /// Must be the line's product when given.
    #[schema(value_type = Option<Uuid>)]
    pub target_product_id: Option<String>,
    pub evidence_ref: Option<String>,
    pub reason: Option<String>,
}

/// Validated legacy return cost.
pub struct LegacyCostInput {
    pub order_id: Uuid,
    pub line_id: Uuid,
    pub quantity: Decimal,
    pub amount: Decimal,
    pub target_product_id: Option<Uuid>,
    pub evidence_ref: String,
    pub reason: String,
}

impl Validate for LegacyCostPayload {
    type Valid = LegacyCostInput;

    fn validate(self, r: &mut Rules) -> Option<LegacyCostInput> {
        let order = uuid(r, "original_sale_id", self.original_sale_id);
        let line = uuid(r, "line_id", self.line_id);
        let q = quantity(r, self.quantity);
        let amount = match self.restoration_amount {
            None => {
                r.missing("restoration_amount");
                None
            }
            v => r.measure("restoration_amount", v, Decimal::ZERO, AMOUNT_MAX, 8),
        };
        let target = opt_uuid(r, "target_product_id", self.target_product_id);
        let evidence = r.required("evidence_ref", self.evidence_ref);
        if let Some(e) = &evidence {
            r.max_chars("evidence_ref", e, REF_MAX);
        }
        let why = reason(r, self.reason);
        Some(LegacyCostInput {
            order_id: order?,
            line_id: line?,
            quantity: q?,
            amount: amount?,
            target_product_id: target?,
            evidence_ref: evidence?,
            reason: why?,
        })
    }
}

/// A reviewed legacy return cost.
#[derive(Serialize, ToSchema)]
pub struct LegacyCostResource {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub original_sale_id: Uuid,
    pub line_id: Uuid,
    #[schema(example = "2")]
    pub quantity: String,
    #[schema(example = "3.00")]
    pub restoration_amount: String,
    pub target_product_id: Option<Uuid>,
    pub evidence_ref: String,
    pub reason: String,
    #[schema(example = "0")]
    pub consumed_quantity: String,
    pub reviewed_by: Uuid,
    pub created_at: DateTime<Utc>,
}

/// A credited line.
#[derive(Serialize, ToSchema)]
#[schema(as = SalesReturnReturnLineResource)]
pub struct ReturnLineResource {
    pub id: Uuid,
    pub recognition_line_id: Uuid,
    pub product_id: Option<Uuid>,
    #[schema(example = "1")]
    pub quantity: String,
    /// Set once posted.
    #[schema(example = "10.00")]
    pub net: Option<String>,
    #[schema(example = "2.10")]
    pub tax: Option<String>,
    /// Units put back on stock (posted restocks).
    #[schema(example = "0")]
    pub restocked: String,
    pub product_name: Option<String>,
    /// Sale's order line (legacy return cost `line_id`); None for a standalone invoice sale.
    pub order_line_id: Option<Uuid>,
    /// Warehouse the sale took this line from: the only restock destination. None: no stock moved.
    pub origin_warehouse_id: Option<Uuid>,
    pub origin_warehouse_name: Option<String>,
}

/// A restock line.
#[derive(Serialize, ToSchema)]
pub struct RestockLineResource {
    pub return_line_id: Uuid,
    #[schema(example = "1")]
    pub quantity: String,
    pub lot_id: Option<Uuid>,
    /// Booked cost restored, book currency.
    #[schema(example = "1.00")]
    pub value: String,
    pub legacy_return_cost_id: Option<Uuid>,
}

/// A physical restock of a return.
#[derive(Serialize, ToSchema)]
pub struct RestockResource {
    pub id: Uuid,
    pub warehouse_id: Uuid,
    pub warehouse_name: String,
    /// posted or reversed.
    pub state: String,
    pub version: i64,
    pub accounting_date: NaiveDate,
    pub lines: Vec<RestockLineResource>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub reversed_at: Option<DateTime<Utc>>,
    pub reversal_reason: Option<String>,
    /// `reverse` while posted, else none.
    pub allowed_actions: Vec<&'static str>,
}

/// A customer return / credit of one sale.
#[derive(Serialize, ToSchema)]
pub struct SalesReturnResource {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// receipt_credit or invoice_credit.
    pub kind: String,
    pub recognition_id: Uuid,
    pub order_id: Option<Uuid>,
    /// None for an anonymous retail sale.
    pub relation_id: Option<Uuid>,
    /// Customer display name (company name, else alias).
    pub relation_name: Option<String>,
    pub order_number: Option<String>,
    /// The legacy credit note document of this credit.
    pub credit_invoice_id: Option<Uuid>,
    /// draft, posted, reversed or cancelled.
    pub state: String,
    pub version: i64,
    pub reason: String,
    pub accounting_date: NaiveDate,
    #[schema(example = "10.00")]
    pub net: Option<String>,
    #[schema(example = "2.10")]
    pub tax: Option<String>,
    /// Cash rounding share (signed).
    #[schema(example = "-0.05")]
    pub rounding: Option<String>,
    /// Customer entitlement = net + tax + rounding.
    #[schema(example = "12.05")]
    pub credit: Option<String>,
    /// The customer credit open item.
    pub credit_open_item_id: Option<Uuid>,
    pub lines: Vec<ReturnLineResource>,
    pub restocks: Vec<RestockResource>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub posted_at: Option<DateTime<Utc>>,
    pub reversed_at: Option<DateTime<Utc>>,
    pub reversal_reason: Option<String>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancel_reason: Option<String>,
    /// Subset of update, post, cancel, restock, reverse.
    pub allowed_actions: Vec<&'static str>,
}

/// What a post would credit, from current facts (no writes).
#[derive(Serialize, ToSchema)]
#[schema(as = SalesReturnPostReview)]
pub struct PostReview {
    pub return_id: Uuid,
    pub version: i64,
    pub accounting_date: NaiveDate,
    #[schema(example = "10.00")]
    pub net: String,
    #[schema(example = "2.10")]
    pub tax: String,
    #[schema(example = "-0.05")]
    pub rounding: String,
    #[schema(example = "12.05")]
    pub credit: String,
    /// Part applied at once to what the customer still owes on this sale.
    #[schema(example = "12.05")]
    pub applied: String,
    pub review_fingerprint: String,
}

/// What a restock or a reversal would do (no writes).
#[derive(Serialize, ToSchema)]
pub struct ActionReview {
    pub return_id: Uuid,
    pub version: i64,
    /// Book value moved (reversals), book currency; a restock's value is set as it posts.
    #[schema(example = "1.00")]
    pub value: Option<String>,
    pub review_fingerprint: String,
}
