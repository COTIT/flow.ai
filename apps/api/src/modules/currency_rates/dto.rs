//! Currency rate request/response bodies.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, human, optional},
    modules::currency_rates::repo::{RateRow, Side},
};

const NOTE_MAX: usize = 500;
/// Largest rate either way (1 unit = one billion of the other).
const RATE_MAX: i64 = 1_000_000_000;

/// A currency inside a rate or a conversion.
#[derive(Serialize, ToSchema)]
pub struct SideResource {
    id: Uuid,
    /// ISO 4217 code, upper case.
    code: String,
    decimals: i16,
}

impl From<&Side> for SideResource {
    fn from(s: &Side) -> Self {
        Self {
            id: s.id,
            code: s.code.clone(),
            decimals: s.decimals,
        }
    }
}

/// One rate of the workspace's history.
#[derive(Serialize, ToSchema)]
#[schema(description = "A workspace exchange rate: 1 `from` = `rate` `to` from `effective_on` on.")]
pub struct RateResource {
    id: Uuid,
    from: SideResource,
    to: SideResource,
    /// Exact decimal text.
    #[schema(example = "2950.5")]
    rate: String,
    effective_on: NaiveDate,
    #[schema(required = true)]
    note: Option<String>,
    created_at: DateTime<Utc>,
}

impl From<RateRow> for RateResource {
    fn from(r: RateRow) -> Self {
        Self {
            id: r.id,
            from: SideResource {
                id: r.from_id,
                code: r.from_code,
                decimals: r.from_decimals,
            },
            to: SideResource {
                id: r.to_id,
                code: r.to_code,
                decimals: r.to_decimals,
            },
            rate: r.rate.normalize().to_string(),
            effective_on: r.effective_on,
            note: r.note,
            created_at: r.created_at,
        }
    }
}

/// Create response: the new row's id.
#[derive(Serialize, ToSchema)]
pub struct RateCreated {
    pub currency_rate_id: Uuid,
}

/// POST /currency-rates.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "New rate: 1 major unit of `from` = `rate` major units of `to`. \
                        Rates are never edited; a newer rate for the same day replaces it."
)]
pub struct CreateRateRequest {
    #[schema(value_type = Uuid)]
    from_currency_id: Option<String>,
    #[schema(value_type = Uuid)]
    to_currency_id: Option<String>,
    /// Positive decimal, at most 8 decimals, at most 1000000000.
    #[schema(value_type = String, example = "2950.5")]
    rate: Option<Value>,
    /// First day the rate applies (`YYYY-MM-DD`); not after today.
    #[schema(value_type = String, format = Date)]
    effective_on: Option<String>,
    #[schema(max_length = 500)]
    note: Option<String>,
}

/// Validated create input.
pub struct NewRate {
    pub from: Uuid,
    pub to: Uuid,
    pub rate: Decimal,
    pub effective_on: NaiveDate,
    pub note: Option<String>,
}

/// Required currency id; malformed = "invalid" (an unknown one fails the same way later).
pub(crate) fn currency_id(r: &mut Rules, field: &str, v: Option<String>) -> Option<Uuid> {
    let v = r.required(field, v)?;
    let id = Uuid::parse_str(&v).ok();
    if id.is_none() {
        invalid(r, field);
    }
    id
}

/// Laravel `exists` wording.
pub(crate) fn invalid(r: &mut Rules, field: &str) {
    r.add(field, format!("The selected {} is invalid.", human(field)));
}

/// `YYYY-MM-DD`, else a 422 on `field`.
pub(crate) fn day(r: &mut Rules, field: &str, v: String) -> Option<NaiveDate> {
    let d = NaiveDate::parse_from_str(&v, "%Y-%m-%d").ok();
    if d.is_none() {
        r.add(
            field,
            format!("The {} must be a date (YYYY-MM-DD).", human(field)),
        );
    }
    d
}

impl Validate for CreateRateRequest {
    type Valid = NewRate;

    fn validate(self, r: &mut Rules) -> Option<NewRate> {
        let from = currency_id(r, "from_currency_id", self.from_currency_id);
        let to = currency_id(r, "to_currency_id", self.to_currency_id);
        if from.is_some() && from == to {
            r.add(
                "to_currency_id",
                "The to currency must differ from the from currency.",
            );
        }
        let rate = r.required_value("rate", self.rate).and_then(|v| {
            let max = Decimal::from(RATE_MAX);
            r.decimal("rate", &v, -max, max)
        });
        if rate.is_some_and(|v| v <= Decimal::ZERO) {
            r.add("rate", "The rate must be greater than 0.");
        }
        let effective_on = r
            .required("effective_on", self.effective_on)
            .and_then(|v| day(r, "effective_on", v));
        let note = r.opt_text("note", self.note, NOTE_MAX);
        Some(NewRate {
            from: from?,
            to: to?,
            rate: rate?,
            effective_on: effective_on?,
            note,
        })
    }
}

/// GET /currency-rates query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// Only rates from this currency.
    #[param(value_type = Option<Uuid>)]
    pub from_currency_id: Option<String>,
    /// Only rates to this currency.
    #[param(value_type = Option<Uuid>)]
    pub to_currency_id: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

/// GET /currency-rates/convert query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ConvertQuery {
    #[param(value_type = Uuid)]
    pub from_currency_id: Option<String>,
    #[param(value_type = Uuid)]
    pub to_currency_id: Option<String>,
    /// Signed whole minor units of the `from` currency (EUR 12.34 = 1234).
    #[param(value_type = i64)]
    pub amount_minor: Option<String>,
    /// Day whose rate applies (`YYYY-MM-DD`); default today in the workspace time zone.
    #[param(value_type = Option<String>, format = Date)]
    pub on: Option<String>,
}

/// Validated convert input.
pub struct Convert {
    pub from: Uuid,
    pub to: Uuid,
    pub amount: i64,
    pub on: Option<NaiveDate>,
}

impl Validate for ConvertQuery {
    type Valid = Convert;

    fn validate(self, r: &mut Rules) -> Option<Convert> {
        let from = currency_id(r, "from_currency_id", self.from_currency_id);
        let to = currency_id(r, "to_currency_id", self.to_currency_id);
        let amount = r.required("amount_minor", self.amount_minor).and_then(|v| {
            let n = v.parse::<i64>().ok();
            if n.is_none() {
                r.add(
                    "amount_minor",
                    "The amount minor must be a whole number of minor units.",
                );
            }
            n
        });
        let on = match optional(self.on) {
            Some(v) => Some(day(r, "on", v)?),
            None => None,
        };
        Some(Convert {
            from: from?,
            to: to?,
            amount: amount?,
            on,
        })
    }
}

/// How the rate was applied.
#[derive(Serialize, ToSchema, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Same currency: amount unchanged, no rate needed.
    Same,
    /// A `from` → `to` rate.
    Direct,
    /// Only a `to` → `from` rate exists: divided by it.
    Inverse,
}

/// A conversion at the workspace's own rate.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "A converted amount. Always say it uses the workspace's own rate as of \
                        `rate_effective_on`; Genesy fetches no market rates."
)]
pub struct Conversion {
    pub amount_minor: i64,
    pub from: SideResource,
    pub to: SideResource,
    /// Minor units of `to`, rounded half up once.
    pub converted_minor: i64,
    /// The stored rate used (exact decimal text; for `inverse` it is the `to` → `from` rate).
    #[schema(example = "2950.5")]
    pub rate: String,
    /// Day the rate took effect; null for `same`.
    #[schema(required = true)]
    pub rate_effective_on: Option<NaiveDate>,
    pub direction: Direction,
    /// Always `half_up`.
    #[schema(example = "half_up")]
    pub rounding: &'static str,
    /// Always `workspace`: a rate staff entered.
    #[schema(example = "workspace")]
    pub source: &'static str,
}
