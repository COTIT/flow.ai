//! Stocktake payloads (inside the accounting C/A envelopes), their validation, and response
//! resources. Quantities are decimal strings (≤2 places); values are decimal strings in major
//! units of the functional currency.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::{
        accounting::dto::{date, uuid},
        stock::repo::LEVEL_MAX,
    },
    money,
};

const REASON_MAX: usize = 1000;
/// Lines per counts request.
const COUNT_MAX: usize = 1000;

fn reason(r: &mut Rules, v: Option<String>) -> Option<String> {
    let t = r.required("reason", v)?;
    r.max_chars("reason", &t, REASON_MAX);
    Some(t)
}

/// POST /stocktakes payload.
#[derive(Deserialize, ToSchema)]
pub struct CreatePayload {
    #[schema(value_type = Uuid)]
    pub warehouse_id: Option<String>,
    pub reason: Option<String>,
}

/// Validated create.
pub struct CreateInput {
    pub warehouse_id: Uuid,
    pub reason: String,
}

impl Validate for CreatePayload {
    type Valid = CreateInput;

    fn validate(self, r: &mut Rules) -> Option<CreateInput> {
        let warehouse_id = uuid(r, "warehouse_id", self.warehouse_id);
        let reason = reason(r, self.reason);
        Some(CreateInput {
            warehouse_id: warehouse_id?,
            reason: reason?,
        })
    }
}

/// POST /stocktakes/{id}/start payload (empty).
#[derive(Deserialize, ToSchema)]
pub struct StartPayload {}

impl Validate for StartPayload {
    type Valid = ();

    fn validate(self, _: &mut Rules) -> Option<()> {
        Some(())
    }
}

/// POST /stocktakes/{id}/cancel payload.
#[derive(Deserialize, ToSchema)]
pub struct StocktakeCancelPayload {
    pub reason: Option<String>,
}

impl Validate for StocktakeCancelPayload {
    type Valid = String;

    fn validate(self, r: &mut Rules) -> Option<String> {
        reason(r, self.reason)
    }
}

/// One counted stock row.
#[derive(Deserialize, ToSchema)]
pub struct CountLinePayload {
    /// `stock_identity` of a snapshot line.
    #[schema(value_type = Uuid)]
    pub stock_identity: Option<String>,
    #[schema(value_type = String, example = "8")]
    pub counted_quantity: Option<Value>,
    /// Functional-currency value of found stock; read only when an active book has no cost for
    /// the product here (ignored in setup books).
    #[schema(value_type = Option<String>, example = "4.50")]
    pub positive_adjustment_value: Option<Value>,
}

/// PUT /stocktakes/{id}/counts payload: lines not sent keep their count.
#[derive(Deserialize, ToSchema)]
pub struct CountPayload {
    pub lines: Option<Vec<CountLinePayload>>,
}

/// One validated count.
pub struct CountInput {
    pub stock_id: Uuid,
    pub counted: Decimal,
    /// Major units; SQL turns it into minor units.
    pub value: Option<Decimal>,
}

impl Validate for CountPayload {
    type Valid = Vec<CountInput>;

    fn validate(self, r: &mut Rules) -> Option<Vec<CountInput>> {
        let raw = self.lines.unwrap_or_default();
        r.check(
            "lines",
            (1..=COUNT_MAX).contains(&raw.len()),
            format!("Send 1 to {COUNT_MAX} lines."),
        );
        let mut out = Vec::with_capacity(raw.len());
        for (i, l) in raw.into_iter().enumerate() {
            let f = |n: &str| format!("lines.{i}.{n}");
            let id = uuid(r, &f("stock_identity"), l.stock_identity);
            let counted = match l.counted_quantity {
                None | Some(Value::Null) => {
                    r.missing(&f("counted_quantity"));
                    None
                }
                v => r.measure(&f("counted_quantity"), v, Decimal::ZERO, LEVEL_MAX, 2),
            };
            let value = match l.positive_adjustment_value {
                None | Some(Value::Null) => Some(None),
                Some(v) => r
                    .decimal(
                        &f("positive_adjustment_value"),
                        &v,
                        Decimal::ZERO,
                        money::MAX,
                    )
                    .map(Some),
            };
            if let (Some(stock_id), Some(counted), Some(value)) = (id, counted, value) {
                out.push(CountInput {
                    stock_id,
                    counted,
                    value,
                });
            }
        }
        Some(out)
    }
}

/// POST /stocktakes/{id}/post/review body.
#[derive(Deserialize, ToSchema)]
pub struct PostReviewBody {
    #[schema(value_type = String, format = Date)]
    pub accounting_date: Option<String>,
}

impl Validate for PostReviewBody {
    type Valid = NaiveDate;

    fn validate(self, r: &mut Rules) -> Option<NaiveDate> {
        date(r, "accounting_date", self.accounting_date)
    }
}

/// POST /stocktakes/{id}/post payload.
#[derive(Deserialize, ToSchema)]
pub struct StocktakePostPayload {
    #[schema(value_type = String, format = Date)]
    pub accounting_date: Option<String>,
    /// From `POST /stocktakes/{id}/post/review`.
    pub review_fingerprint: Option<String>,
}

impl Validate for StocktakePostPayload {
    type Valid = (NaiveDate, String);

    fn validate(self, r: &mut Rules) -> Option<(NaiveDate, String)> {
        let d = date(r, "accounting_date", self.accounting_date);
        let f = r.required("review_fingerprint", self.review_fingerprint);
        Some((d?, f?))
    }
}

/// GET /stocktakes query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageQuery {
    pub page: Option<String>,
    pub page_size: Option<String>,
}

/// One frozen stock row of a stocktake.
#[derive(Serialize, ToSchema)]
pub struct StocktakeLine {
    /// The stock row (product, bin, lot) this line counts.
    pub stock_identity: Uuid,
    pub product_id: Uuid,
    pub bin_id: Option<Uuid>,
    pub lot_id: Option<Uuid>,
    /// Level when counting started.
    pub expected_quantity: String,
    pub counted_quantity: Option<String>,
    pub positive_adjustment_value: Option<String>,
}

/// A stocktake; `lines` only on single reads and writes.
#[derive(Serialize, ToSchema)]
pub struct StocktakeResource {
    pub id: Uuid,
    pub warehouse_id: Uuid,
    /// draft, counting (warehouse paused), posted or cancelled.
    pub state: String,
    pub version: i64,
    pub reason: String,
    pub cancel_reason: Option<String>,
    pub accounting_date: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub posted_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<Vec<StocktakeLine>>,
}

/// One difference the post would book.
#[derive(Serialize, ToSchema)]
pub struct PostReviewLine {
    pub stock_identity: Uuid,
    /// counted − expected.
    pub delta: String,
    /// Inventory value change in the functional currency; null in setup books.
    pub value: Option<String>,
}

/// POST /stocktakes/{id}/post/review answer.
#[derive(Serialize, ToSchema)]
pub struct PostReview {
    pub accounting_date: NaiveDate,
    /// Changed lines only.
    pub lines: Vec<PostReviewLine>,
    pub review_fingerprint: String,
}
