//! Shipment payloads (inside the accounting C/A envelopes), their validation, and response
//! resources. Quantities are decimal strings (≤2 places); amounts exact decimal strings in
//! major units of the order currency.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::{
        accounting::dto::{MAX_LINES, date, uuid},
        stock::repo::LEVEL_MAX,
    },
};

const NOTE_MAX: usize = 1000;

/// One order line to ship.
#[derive(Deserialize, ToSchema)]
pub struct ShipmentLinePayload {
    #[schema(value_type = Uuid)]
    pub order_line_id: Option<String>,
    /// Order-line units, ≤2 places; Σ draft + posted ≤ ordered − returned.
    #[schema(value_type = String, example = "4")]
    pub quantity: Option<Value>,
}

/// Create/update payload: replaces the draft's lines and note.
#[derive(Deserialize, ToSchema)]
pub struct ShipmentPayload {
    pub lines: Option<Vec<ShipmentLinePayload>>,
    pub note: Option<String>,
}

/// Validated shipment draft.
pub struct ShipmentInput {
    pub lines: Vec<(Uuid, Decimal)>,
    pub note: Option<String>,
}

impl Validate for ShipmentPayload {
    type Valid = ShipmentInput;

    fn validate(self, r: &mut Rules) -> Option<ShipmentInput> {
        let note = r.opt_text("note", self.note, NOTE_MAX);
        let raw = self.lines.unwrap_or_default();
        r.check(
            "lines",
            (1..=MAX_LINES).contains(&raw.len()),
            format!("Send 1 to {MAX_LINES} lines."),
        );
        let mut lines: Vec<(Uuid, Decimal)> = Vec::with_capacity(raw.len());
        for (i, l) in raw.into_iter().take(MAX_LINES).enumerate() {
            let mut lr = Rules::default();
            let id = uuid(&mut lr, "order_line_id", l.order_line_id);
            let q = match l.quantity {
                None => {
                    lr.missing("quantity");
                    None
                }
                v => lr.measure("quantity", v, Decimal::new(1, 2), LEVEL_MAX, 2),
            };
            if let Some(id) = id {
                lr.check(
                    "order_line_id",
                    !lines.iter().any(|(l, _)| *l == id),
                    "Each line may appear once.",
                );
            }
            let ok = !lr.merge_into_prefixed(r, &format!("lines.{i}."));
            if let (true, Some(id), Some(q)) = (ok, id, q) {
                lines.push((id, q));
            }
        }
        Some(ShipmentInput { lines, note })
    }
}

/// POST .../post/review body.
#[derive(Deserialize, ToSchema)]
pub struct PostReviewBody {
    /// `YYYY-MM-DD`: date of the sale recognition (active books).
    pub accounting_date: Option<String>,
}

impl Validate for PostReviewBody {
    type Valid = NaiveDate;

    fn validate(self, r: &mut Rules) -> Option<NaiveDate> {
        date(r, "accounting_date", self.accounting_date)
    }
}

/// POST .../post payload.
#[derive(Deserialize, ToSchema)]
pub struct PostPayload {
    pub accounting_date: Option<String>,
    /// From `POST .../post/review`.
    pub review_fingerprint: Option<String>,
}

impl Validate for PostPayload {
    type Valid = (NaiveDate, String);

    fn validate(self, r: &mut Rules) -> Option<(NaiveDate, String)> {
        let d = date(r, "accounting_date", self.accounting_date);
        let f = r.required("review_fingerprint", self.review_fingerprint);
        Some((d?, f?))
    }
}

/// A shipped order line.
#[derive(Serialize, ToSchema, Clone)]
pub struct ShipmentLineResource {
    pub id: Uuid,
    pub order_line_id: Uuid,
    pub product_id: Uuid,
    pub title: String,
    #[schema(example = "4")]
    pub quantity: String,
}

/// A shipment of a sales order.
#[derive(Serialize, ToSchema)]
pub struct ShipmentResource {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub order_id: Uuid,
    /// draft, posted or cancelled.
    pub state: String,
    pub version: i64,
    pub note: Option<String>,
    pub lines: Vec<ShipmentLineResource>,
    pub accounting_date: Option<NaiveDate>,
    /// The sale recognition a post wrote (active books).
    pub recognition_id: Option<Uuid>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub posted_by: Option<Uuid>,
    pub posted_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancel_reason: Option<String>,
    /// Subset of update, post, cancel.
    pub allowed_actions: Vec<&'static str>,
}

/// Recognition a post would write (active books only).
#[derive(Serialize, ToSchema)]
pub struct RecognitionPreview {
    #[schema(example = "40.00")]
    pub net: String,
    #[schema(example = "8.40")]
    pub tax: String,
    #[schema(example = "48.40")]
    pub gross: String,
}

/// What a post would do, from current facts (no writes).
#[derive(Serialize, ToSchema)]
pub struct PostReview {
    pub shipment_id: Uuid,
    pub version: i64,
    pub accounting_date: NaiveDate,
    pub lines: Vec<ShipmentLineResource>,
    /// None in setup books (stock only).
    pub recognition: Option<RecognitionPreview>,
    pub review_fingerprint: String,
}
