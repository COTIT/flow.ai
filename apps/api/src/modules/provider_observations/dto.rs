//! Accounting exception DTOs: resource, list query, resolve payload and review.
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::accounting::dto::opt_uuid,
};

/// One exception with its provider observation and accounting state.
#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct ExceptionResource {
    pub id: Uuid,
    /// `pending_posting` (retry) or `discrepancy` (resolve).
    pub kind: String,
    pub reason_code: String,
    pub message: String,
    /// `pending` or `resolved`.
    pub state: String,
    /// `posted`, `assign_receivable` or `write_off`.
    pub resolution: Option<String>,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub relation_id: Option<Uuid>,
    /// Discrepancy minor units sent to recovery.
    pub amount: Option<i64>,
    /// Recovery item left open, minor units.
    pub recovery_residual: Option<i64>,
    pub observation_id: Uuid,
    pub provider: String,
    /// `collection`, `refund` or `chargeback`.
    pub economic_kind: String,
    pub external_id: String,
    /// Observed minor units.
    pub observed_amount: i64,
    pub currency: String,
    pub decimals: i16,
    pub invoice_id: Option<Uuid>,
    pub credit_note_id: Option<Uuid>,
    pub order_id: Option<Uuid>,
    pub operation_id: Uuid,
    pub occurred_at: DateTime<Utc>,
    /// Source link: `posted` or `pending_observation`.
    pub disposition: Option<String>,
    /// From kind and state: `retry` (pending posting), `resolve` (pending discrepancy), else none.
    pub allowed_actions: Vec<String>,
}

/// List filters.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// `pending` (default) or `all`.
    pub state: Option<String>,
    /// Keyset: exceptions older than this id.
    pub before: Option<Uuid>,
    /// 1–100, default 50.
    pub limit: Option<i64>,
}

/// Validated resolve.
pub struct ResolveInput {
    pub resolution: &'static str,
    pub relation_id: Option<Uuid>,
    pub reason: String,
    pub evidence_ref: String,
}

fn fields(
    r: &mut Rules,
    resolution: Option<String>,
    relation_id: Option<String>,
    reason: Option<String>,
    evidence_ref: Option<String>,
) -> Option<ResolveInput> {
    let resolution = match r.text("resolution", resolution, 32).as_str() {
        "assign_receivable" => Some("assign_receivable"),
        "write_off" => Some("write_off"),
        _ => {
            r.add(
                "resolution",
                "The resolution must be assign_receivable or write_off.",
            );
            None
        }
    };
    let relation_id = opt_uuid(r, "relation_id", relation_id);
    let reason = r.text("reason", reason, 1000);
    let evidence_ref = r.text("evidence_ref", evidence_ref, 255);
    Some(ResolveInput {
        resolution: resolution?,
        relation_id: relation_id?,
        reason,
        evidence_ref,
    })
}

/// POST .../resolve/review body.
#[derive(Deserialize, ToSchema)]
pub struct ResolveReviewBody {
    /// `assign_receivable` or `write_off`.
    pub resolution: Option<String>,
    #[schema(value_type = Option<Uuid>)]
    pub relation_id: Option<String>,
    pub reason: Option<String>,
    pub evidence_ref: Option<String>,
}

impl Validate for ResolveReviewBody {
    type Valid = ResolveInput;

    fn validate(self, r: &mut Rules) -> Option<ResolveInput> {
        fields(
            r,
            self.resolution,
            self.relation_id,
            self.reason,
            self.evidence_ref,
        )
    }
}

/// POST .../resolve payload.
#[derive(Deserialize, ToSchema)]
pub struct ResolvePayload {
    /// `assign_receivable` or `write_off`.
    pub resolution: Option<String>,
    /// assign_receivable: customer who owes it (default: the observation's customer).
    #[schema(value_type = Option<Uuid>)]
    pub relation_id: Option<String>,
    pub reason: Option<String>,
    pub evidence_ref: Option<String>,
    /// From `POST .../resolve/review`.
    pub review_fingerprint: Option<String>,
}

impl Validate for ResolvePayload {
    type Valid = (ResolveInput, String);

    fn validate(self, r: &mut Rules) -> Option<(ResolveInput, String)> {
        let i = fields(
            r,
            self.resolution,
            self.relation_id,
            self.reason,
            self.evidence_ref,
        );
        let f = r.required("review_fingerprint", self.review_fingerprint);
        Some((i?, f?))
    }
}

/// POST .../retry payload (no fields).
#[derive(Deserialize, ToSchema, Default)]
pub struct RetryPayload {}

impl Validate for RetryPayload {
    type Valid = ();

    fn validate(self, _: &mut Rules) -> Option<()> {
        Some(())
    }
}

/// What a resolve would book (no writes).
#[derive(Serialize, ToSchema)]
pub struct ResolveReview {
    pub resolution: String,
    pub relation_id: Option<Uuid>,
    #[schema(example = "20.00")]
    pub amount: String,
    pub accounting_date: NaiveDate,
    /// Journal roles: debit then credit.
    pub debit: String,
    pub credit: String,
    pub review_fingerprint: String,
}
