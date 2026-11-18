//! Request/response shapes of privileged Till returns and refunds (masterplan §15).

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::{
        accounting::dto::{opt_uuid, uuid},
        orders::dto::{LINES_MAX, QUANTITY_MAX},
        sales_returns::dto::SalesReturnResource,
    },
};

/// One sold line to take back.
#[derive(Deserialize, ToSchema)]
pub struct ReturnLineRequest {
    #[schema(value_type = Uuid)]
    original_line_id: Option<String>,
    #[schema(value_type = String, example = "1")]
    quantity: Option<Value>,
}

/// POST /till/sales/{order}/returns/review body.
#[derive(Deserialize, ToSchema)]
pub struct ReturnReviewRequest {
    lines: Option<Vec<ReturnLineRequest>>,
    /// Put the goods back on stock at the sale's warehouse (default false).
    #[schema(value_type = Option<bool>)]
    restock: Option<Value>,
    #[schema(max_length = 1000)]
    reason: Option<String>,
    /// Must be the sale's warehouse when given.
    #[schema(value_type = Option<Uuid>)]
    destination_warehouse_id: Option<String>,
}

/// A validated till return.
pub struct ReturnInput {
    pub lines: Vec<(Uuid, Decimal)>,
    pub restock: bool,
    pub reason: String,
    pub warehouse: Option<Uuid>,
}

impl Validate for ReturnReviewRequest {
    type Valid = ReturnInput;

    fn validate(self, r: &mut Rules) -> Option<ReturnInput> {
        let mut lines = Vec::new();
        match self.lines.filter(|l| (1..=LINES_MAX).contains(&l.len())) {
            None => r.add(
                "lines",
                format!("The lines must list 1 to {LINES_MAX} lines."),
            ),
            Some(rows) => {
                for (n, l) in rows.into_iter().enumerate() {
                    let id = uuid(
                        r,
                        &format!("lines.{n}.original_line_id"),
                        l.original_line_id,
                    );
                    let field = format!("lines.{n}.quantity");
                    let q = r
                        .required_value(&field, l.quantity)
                        .and_then(|v| r.decimal(&field, &v, Decimal::ZERO, QUANTITY_MAX));
                    if let (Some(id), Some(q)) = (id, q) {
                        lines.push((id, q));
                    }
                }
            }
        }
        let restock = match self.restock {
            None | Some(Value::Null) => Some(false),
            Some(v) => r.boolean("restock", &v),
        };
        let reason = r.text("reason", self.reason, 1000);
        let warehouse = opt_uuid(r, "destination_warehouse_id", self.destination_warehouse_id);
        Some(ReturnInput {
            lines,
            restock: restock?,
            reason,
            warehouse: warehouse?,
        })
    }
}

/// POST /till/sales/{order}/returns payload.
#[derive(Deserialize, ToSchema)]
#[schema(as = TillReturnPayload)]
pub struct ReturnPayload {
    #[serde(flatten)]
    review: ReturnReviewRequest,
    /// `review_fingerprint` from the review.
    review_fingerprint: Option<String>,
}

impl Validate for ReturnPayload {
    type Valid = (ReturnInput, String);

    fn validate(self, r: &mut Rules) -> Option<Self::Valid> {
        let input = self.review.validate(r);
        let fingerprint = r.required("review_fingerprint", self.review_fingerprint);
        Some((input?, fingerprint?))
    }
}

/// What a till return will post: the credits (one per recognised sale) and the fingerprint.
#[derive(Serialize, ToSchema)]
pub struct ReturnReview {
    pub returns: Vec<SalesReturnResource>,
    pub review_fingerprint: String,
}

/// cash or card_recorded.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RefundMethod {
    Cash,
    CardRecorded,
}

impl RefundMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cash => "cash",
            Self::CardRecorded => "card_recorded",
        }
    }
}

/// POST /till/sales/{order}/refunds/review body.
#[derive(Deserialize, ToSchema)]
pub struct RefundReviewRequest {
    /// The customer credit open item of a posted return of this sale.
    #[schema(value_type = Uuid)]
    credit_open_item_id: Option<String>,
    /// Integer minor units of the sale's currency (1090 = €10.90).
    #[schema(value_type = i64, minimum = 1, example = 1090)]
    amount: Option<Value>,
    /// `cash` (out of this register's drawer) or `card_recorded` (refund already made on the
    /// card terminal).
    #[schema(example = "cash")]
    method: Option<String>,
    /// card_recorded: the terminal's refund reference.
    #[schema(max_length = 255)]
    external_reference: Option<String>,
    /// card_recorded: must be true (the terminal confirmed the refund).
    #[schema(value_type = Option<bool>)]
    actual_refund_confirmed: Option<Value>,
}

/// A validated refund request.
pub struct RefundInput {
    pub item: Uuid,
    pub amount: i64,
    pub method: RefundMethod,
    pub reference: Option<String>,
    pub confirmed: bool,
}

const NEEDS_REFERENCE: &str = "A card refund needs the terminal's refund reference.";
const NEEDS_CONFIRMED: &str = "Record a card refund only after the terminal confirmed it.";
const CASH_NO_REFERENCE: &str = "A cash refund has no terminal reference.";

impl Validate for RefundReviewRequest {
    type Valid = RefundInput;

    fn validate(self, r: &mut Rules) -> Option<RefundInput> {
        let item = uuid(r, "credit_open_item_id", self.credit_open_item_id);
        let amount = r
            .required_value("amount", self.amount)
            .and_then(|v| r.integer("amount", &v, 1, 1_000_000_000_000_000));
        let method = r
            .required("method", self.method.clone())
            .and_then(|_| r.one_of("method", self.method, &["cash", "card_recorded"], None))
            .map(|m| match m.as_str() {
                "cash" => RefundMethod::Cash,
                _ => RefundMethod::CardRecorded,
            });
        let reference = r.opt_text("external_reference", self.external_reference, 255);
        let confirmed = match self.actual_refund_confirmed {
            None | Some(Value::Null) => Some(false),
            Some(v) => r.boolean("actual_refund_confirmed", &v),
        };
        match (method, confirmed) {
            (Some(RefundMethod::CardRecorded), Some(c)) => {
                r.check("external_reference", reference.is_some(), NEEDS_REFERENCE);
                r.check("actual_refund_confirmed", c, NEEDS_CONFIRMED);
            }
            (Some(RefundMethod::Cash), _) => {
                r.check("external_reference", reference.is_none(), CASH_NO_REFERENCE);
            }
            _ => {}
        }
        Some(RefundInput {
            item: item?,
            amount: amount?,
            method: method?,
            reference,
            confirmed: confirmed?,
        })
    }
}

/// POST /till/sales/{order}/refunds payload.
#[derive(Deserialize, ToSchema)]
pub struct RefundPayload {
    #[serde(flatten)]
    review: RefundReviewRequest,
    #[schema(max_length = 1000)]
    reason: Option<String>,
    /// `review_fingerprint` from the review.
    review_fingerprint: Option<String>,
}

/// A validated refund post.
pub struct RefundPost {
    pub input: RefundInput,
    pub reason: String,
    pub fingerprint: String,
}

impl Validate for RefundPayload {
    type Valid = RefundPost;

    fn validate(self, r: &mut Rules) -> Option<RefundPost> {
        let input = self.review.validate(r);
        let reason = r.text("reason", self.reason, 1000);
        let fingerprint = r.required("review_fingerprint", self.review_fingerprint);
        Some(RefundPost {
            input: input?,
            reason,
            fingerprint: fingerprint?,
        })
    }
}

/// The journal a refund posts.
#[derive(Serialize, ToSchema)]
pub struct RefundJournal {
    #[schema(example = "receivables")]
    pub debit_role: &'static str,
    #[schema(example = "cash")]
    pub credit_role: &'static str,
    pub amount: i64,
}

/// What a refund will post (no writes).
#[derive(Serialize, ToSchema)]
pub struct RefundReview {
    pub order_id: Uuid,
    pub credit_open_item_id: Uuid,
    pub method: &'static str,
    /// Minor units.
    pub amount: i64,
    /// The credit's open residual (minor units).
    pub residual: i64,
    /// Residual minus pending refund holds of other routes.
    pub refundable: i64,
    pub register_id: Uuid,
    /// Cash: the open drawer shift and the cash it holds.
    pub shift_id: Option<Uuid>,
    pub drawer_expected: Option<i64>,
    pub accounting_date: NaiveDate,
    pub journal: RefundJournal,
    pub review_fingerprint: String,
}

/// A posted Till refund.
#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct TillRefundResource {
    pub id: Uuid,
    pub order_id: Uuid,
    pub credit_open_item_id: Uuid,
    /// The receivable item opened for the money paid out (applied to the credit).
    pub refund_open_item_id: Uuid,
    #[schema(example = "cash")]
    pub method: String,
    /// Minor units.
    pub amount: i64,
    pub currency_id: Uuid,
    pub register_id: Uuid,
    pub shift_id: Option<Uuid>,
    /// Cashier context.
    pub till_user_id: Uuid,
    /// The approving member.
    pub member_user_id: Uuid,
    pub external_reference: Option<String>,
    pub actual_refund_confirmed: bool,
    pub reason: String,
    pub accounting_date: NaiveDate,
    pub journal_entry_id: Uuid,
    pub created_at: DateTime<Utc>,
}
