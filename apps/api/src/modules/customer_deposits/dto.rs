//! Customer deposit payloads (inside the accounting C/A envelopes) and resources. Amounts
//! are exact decimal strings in major units of the functional currency.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::accounting::{
        dto::{date, uuid},
        settlement::{MoneyLocation, MoneyLocationRequest},
    },
};

/// Largest amount accepted (well inside bigint minor units).
const AMOUNT_MAX: Decimal = Decimal::from_parts(1_000_000_000, 0, 0, false, 0);

fn amount(r: &mut Rules, v: Option<Value>) -> Option<Decimal> {
    let Some(v) = v else {
        r.missing("amount");
        return None;
    };
    r.measure("amount", Some(v), Decimal::new(1, 8), AMOUNT_MAX, 8)
}

/// POST /customer-deposits/review body.
#[derive(Deserialize, ToSchema)]
pub struct DepositReviewBody {
    #[schema(value_type = Uuid)]
    pub relation_id: Option<String>,
    /// Functional currency, major units.
    #[schema(value_type = String, example = "120.00")]
    pub amount: Option<Value>,
    /// `YYYY-MM-DD`: today in the book timezone.
    pub received_on: Option<String>,
    pub location: Option<MoneyLocationRequest>,
}

/// A validated deposit.
pub struct DepositInput {
    pub relation: Uuid,
    pub amount: Decimal,
    pub received_on: NaiveDate,
    pub location: MoneyLocation,
}

fn deposit(
    r: &mut Rules,
    (relation, a, received_on, location): (
        Option<String>,
        Option<Value>,
        Option<String>,
        Option<MoneyLocationRequest>,
    ),
) -> Option<DepositInput> {
    let relation = uuid(r, "relation_id", relation);
    let a = amount(r, a);
    let received_on = date(r, "received_on", received_on);
    let location = match location {
        Some(l) => l.validate(r, "location"),
        None => {
            r.missing("location");
            None
        }
    };
    Some(DepositInput {
        relation: relation?,
        amount: a?,
        received_on: received_on?,
        location: location?,
    })
}

impl Validate for DepositReviewBody {
    type Valid = DepositInput;

    fn validate(self, r: &mut Rules) -> Option<DepositInput> {
        deposit(
            r,
            (
                self.relation_id,
                self.amount,
                self.received_on,
                self.location,
            ),
        )
    }
}

/// POST /customer-deposits payload.
#[derive(Deserialize, ToSchema)]
pub struct DepositPayload {
    #[schema(value_type = Uuid)]
    pub relation_id: Option<String>,
    #[schema(value_type = String, example = "120.00")]
    pub amount: Option<Value>,
    pub received_on: Option<String>,
    pub location: Option<MoneyLocationRequest>,
    /// From `POST /customer-deposits/review`.
    pub review_fingerprint: Option<String>,
}

impl Validate for DepositPayload {
    type Valid = (DepositInput, String);

    fn validate(self, r: &mut Rules) -> Option<(DepositInput, String)> {
        let f = r.required("review_fingerprint", self.review_fingerprint);
        let d = deposit(
            r,
            (
                self.relation_id,
                self.amount,
                self.received_on,
                self.location,
            ),
        );
        Some((d?, f?))
    }
}

/// POST /customer-deposits/{id}/apply/review body.
#[derive(Deserialize, ToSchema)]
pub struct ApplyReviewBody {
    /// A recognised, unbilled sale of the deposit's customer.
    #[schema(value_type = Uuid)]
    pub recognition_id: Option<String>,
    #[schema(value_type = String, example = "120.00")]
    pub amount: Option<Value>,
}

/// A validated application.
pub struct ApplyInput {
    pub recognition: Uuid,
    pub amount: Decimal,
}

impl Validate for ApplyReviewBody {
    type Valid = ApplyInput;

    fn validate(self, r: &mut Rules) -> Option<ApplyInput> {
        let recognition = uuid(r, "recognition_id", self.recognition_id);
        let a = amount(r, self.amount);
        Some(ApplyInput {
            recognition: recognition?,
            amount: a?,
        })
    }
}

/// POST /customer-deposits/{id}/apply payload.
#[derive(Deserialize, ToSchema)]
pub struct ApplyPayload {
    #[schema(value_type = Uuid)]
    pub recognition_id: Option<String>,
    #[schema(value_type = String, example = "120.00")]
    pub amount: Option<Value>,
    /// From `POST .../apply/review`.
    pub review_fingerprint: Option<String>,
}

impl Validate for ApplyPayload {
    type Valid = (ApplyInput, String);

    fn validate(self, r: &mut Rules) -> Option<(ApplyInput, String)> {
        let recognition = uuid(r, "recognition_id", self.recognition_id);
        let a = amount(r, self.amount);
        let f = r.required("review_fingerprint", self.review_fingerprint);
        Some((
            ApplyInput {
                recognition: recognition?,
                amount: a?,
            },
            f?,
        ))
    }
}

/// What a receive or apply would book (no writes).
#[derive(Serialize, ToSchema)]
pub struct DepositReview {
    pub relation_id: Uuid,
    /// Functional minor units as a decimal string.
    #[schema(example = "120.00")]
    pub amount: String,
    pub accounting_date: NaiveDate,
    /// Journal roles: debit then credit.
    pub debit: String,
    pub credit: String,
    pub review_fingerprint: String,
}

/// A received deposit and what is left to apply.
#[derive(Serialize, ToSchema)]
pub struct DepositResource {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub relation_id: Uuid,
    pub currency_id: Uuid,
    #[schema(example = "120.00")]
    pub amount: String,
    /// Not applied yet.
    #[schema(example = "20.00")]
    pub residual: String,
    pub received_on: NaiveDate,
    /// cash_place, bank_account or provider_clearing.
    pub location_kind: String,
    pub register_id: Option<Uuid>,
    pub vault_id: Option<Uuid>,
    pub bank_account_id: Option<Uuid>,
    pub reference: Option<String>,
    pub evidence_ref: Option<String>,
    pub confirmation_method: Option<String>,
    pub journal_entry_id: Uuid,
    /// The deposit open item's version: `expected_version` of an apply.
    pub version: i64,
    pub created_at: DateTime<Utc>,
}
