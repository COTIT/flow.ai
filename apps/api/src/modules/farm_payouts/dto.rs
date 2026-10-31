//! Payout and cash request/response bodies.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::{
        farm_payouts::repo::{DayRow, PayoutRow},
        farm_surveys::dto::Money,
        farm_vouchers::service::ID_TYPES,
    },
};

const REASON_MAX: usize = 500;
const CASH_KINDS: &[&str] = &["float_out", "float_back", "count"];

fn money(minor: i64, currency: &str, decimals: i16, currency_id: Uuid) -> Money {
    Money {
        minor,
        currency: currency.to_owned(),
        decimals,
        currency_id,
    }
}

/// A payout.
#[derive(Serialize, ToSchema)]
#[schema(description = "Cash paid to a farmer from their passbook balance.")]
pub struct PayoutResource {
    id: Uuid,
    client_ref: Uuid,
    person_id: Uuid,
    village_id: Uuid,
    amount: Money,
    /// The ID document checked: `nida`, `voter`, `driving_licence` or `village_letter`.
    id_type: String,
    paid_by: Uuid,
    paid_at: DateTime<Utc>,
    /// Set once voided; the passbook and cash ledger then hold reversing lines.
    #[schema(required = true)]
    void: Option<PayoutVoid>,
}

/// Why and by whom a payout was voided.
#[derive(Serialize, ToSchema)]
pub struct PayoutVoid {
    reason: String,
    voided_by: Uuid,
    voided_at: DateTime<Utc>,
}

impl From<PayoutRow> for PayoutResource {
    fn from(p: PayoutRow) -> Self {
        let void = match (p.void_reason, p.voided_by, p.voided_at) {
            (Some(reason), Some(voided_by), Some(voided_at)) => Some(PayoutVoid {
                reason,
                voided_by,
                voided_at,
            }),
            _ => None,
        };
        Self {
            id: p.id,
            client_ref: p.client_ref,
            person_id: p.person_id,
            village_id: p.village_id,
            amount: money(p.amount, &p.currency, p.decimals, p.currency_id),
            id_type: p.id_type,
            paid_by: p.paid_by,
            paid_at: p.paid_at,
            void,
        }
    }
}

/// POST /farm/payouts.
#[derive(Deserialize, ToSchema)]
#[schema(description = "Pay a farmer cash from their passbook balance.")]
pub struct CreatePayoutRequest {
    /// Retry key (offline desk): the same key and body replay; another body is a 409.
    #[schema(value_type = Uuid)]
    client_ref: Option<String>,
    #[schema(value_type = Uuid)]
    person_id: Option<String>,
    /// Minor units, at least 1, at most the balance.
    #[schema(value_type = i64, minimum = 1)]
    amount: Option<Value>,
    #[schema(value_type = Uuid)]
    currency_id: Option<String>,
    /// `nida`, `voter`, `driving_licence` or `village_letter`.
    #[schema(value_type = String)]
    id_type: Option<String>,
    /// The payer checked the name on the ID against the farmer; must be true.
    #[schema(value_type = bool)]
    name_confirmed: Option<Value>,
}

/// Validated payout input.
pub struct NewPayoutInput {
    pub client_ref: Uuid,
    pub person_id: Uuid,
    pub amount: i64,
    pub currency_id: Uuid,
    pub id_type: String,
}

impl NewPayoutInput {
    /// sha256 of the body a replay must match.
    pub fn hash(&self) -> [u8; 32] {
        let canonical = json!([self.person_id, self.amount, self.currency_id, self.id_type]);
        Sha256::digest(canonical.to_string()).into()
    }
}

fn uuid(r: &mut Rules, field: &str, v: Option<String>) -> Uuid {
    let Some(v) = r.required(field, v) else {
        return Uuid::nil();
    };
    let id = Uuid::parse_str(v.trim()).ok();
    r.check(
        field,
        id.is_some(),
        format!("The {field} must be a valid UUID."),
    );
    id.unwrap_or_default()
}

impl Validate for CreatePayoutRequest {
    type Valid = NewPayoutInput;

    fn validate(self, r: &mut Rules) -> Option<NewPayoutInput> {
        let amount = r
            .required_value("amount", self.amount)
            .and_then(|v| r.integer("amount", &v, 1, i64::MAX))
            .unwrap_or_default();
        let id_type = r
            .required("id_type", self.id_type.clone())
            .and_then(|_| r.one_of("id_type", self.id_type, ID_TYPES, None))
            .unwrap_or_default();
        let confirmed = self
            .name_confirmed
            .as_ref()
            .and_then(|v| r.boolean("name_confirmed", v));
        r.check(
            "name_confirmed",
            confirmed == Some(true),
            "The name confirmed field must be accepted.",
        );
        Some(NewPayoutInput {
            client_ref: uuid(r, "client_ref", self.client_ref),
            person_id: uuid(r, "person_id", self.person_id),
            amount,
            currency_id: uuid(r, "currency_id", self.currency_id),
            id_type,
        })
    }
}

/// POST /farm/payouts/{payout}/void.
#[derive(Deserialize, ToSchema)]
pub struct VoidPayoutRequest {
    /// 1-500 chars.
    #[schema(value_type = String, max_length = 500)]
    reason: Option<String>,
}

impl Validate for VoidPayoutRequest {
    type Valid = String;

    fn validate(self, r: &mut Rules) -> Option<String> {
        Some(r.text("reason", self.reason, REASON_MAX))
    }
}

/// POST /farm/cash/movements.
#[derive(Deserialize, ToSchema)]
#[schema(description = "Cash handed to or back from an officer, or the officer's count.")]
pub struct CashMovementRequest {
    /// `float_out` (ops hands cash out), `float_back` (officer hands it back), `count` (what the officer counted).
    #[schema(value_type = String)]
    kind: Option<String>,
    #[schema(value_type = Uuid)]
    staff_user_id: Option<String>,
    #[schema(value_type = Uuid)]
    village_id: Option<String>,
    #[schema(value_type = Uuid)]
    currency_id: Option<String>,
    /// Minor units, positive (a count may be 0).
    #[schema(value_type = i64, minimum = 0)]
    amount: Option<Value>,
}

/// Validated cash movement.
pub struct CashInput {
    pub kind: String,
    pub staff_user_id: Uuid,
    pub village_id: Uuid,
    pub currency_id: Uuid,
    pub amount: i64,
}

impl Validate for CashMovementRequest {
    type Valid = CashInput;

    fn validate(self, r: &mut Rules) -> Option<CashInput> {
        let kind = r
            .required("kind", self.kind.clone())
            .and_then(|_| r.one_of("kind", self.kind, CASH_KINDS, None))
            .unwrap_or_default();
        let min = if kind == "count" { 0 } else { 1 };
        let amount = r
            .required_value("amount", self.amount)
            .and_then(|v| r.integer("amount", &v, min, i64::MAX))
            .unwrap_or_default();
        Some(CashInput {
            kind,
            staff_user_id: uuid(r, "staff_user_id", self.staff_user_id),
            village_id: uuid(r, "village_id", self.village_id),
            currency_id: uuid(r, "currency_id", self.currency_id),
            amount,
        })
    }
}

/// A cash ledger row as stored.
#[derive(Serialize, ToSchema)]
pub struct CashMovementResource {
    pub id: Uuid,
    pub kind: String,
    pub staff_user_id: Uuid,
    pub village_id: Uuid,
    pub currency_id: Uuid,
    /// Signed minor units as booked (a count: the amount counted).
    pub amount: i64,
    /// Tanzania (UTC+3) date.
    pub day: NaiveDate,
}

/// GET /farm/cash/reconciliation query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DayQuery {
    /// Tanzania date `YYYY-MM-DD`.
    #[param(value_type = String, format = Date)]
    pub day: Option<String>,
    /// Only this village.
    #[param(value_type = Option<Uuid>)]
    pub village_id: Option<String>,
}

/// One officer's day in one currency.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "expected = opening + floats - paid + voided - returned; variance = counted - expected."
)]
pub struct CashDayResource {
    staff_user_id: Uuid,
    /// Cash held at the start of the day (all earlier movements).
    opening: Money,
    floats: Money,
    /// Number of payouts made that day.
    payouts: i64,
    paid: Money,
    /// Payouts voided that day (cash back with the officer).
    voided: Money,
    /// Float handed back.
    returned: Money,
    expected: Money,
    /// The day's last count; null when not counted.
    #[schema(required = true)]
    counted: Option<Money>,
    /// counted − expected; null when not counted.
    #[schema(required = true)]
    variance: Option<Money>,
}

impl From<DayRow> for CashDayResource {
    fn from(d: DayRow) -> Self {
        let m = |minor| money(minor, &d.currency, d.decimals, d.currency_id);
        let expected = d.opening + d.floats - d.paid + d.voided - d.returned;
        Self {
            staff_user_id: d.staff_user_id,
            opening: m(d.opening),
            floats: m(d.floats),
            payouts: d.payouts,
            paid: m(d.paid),
            voided: m(d.voided),
            returned: m(d.returned),
            expected: m(expected),
            counted: d.counted.map(m),
            variance: d.counted.map(|c| m(c - expected)),
        }
    }
}
