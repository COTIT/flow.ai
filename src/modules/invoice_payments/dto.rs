//! Payment bodies: Laravel StorePaymentRequest (`POST /invoices/{id}/payments`) and
//! MarkInvoiceAsCreditedRequest. Unknown keys are skipped.

use chrono::{DateTime, NaiveDate, NaiveDateTime};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, human},
    modules::pricing::body::{Capped, Scalar},
    money::MAX,
};

/// How a payment was made, as stored: Laravel PaymentMethod, with its `mollie` and `bank`
/// aliases turned into a provider (`bank` stays Laravel's `afas`: the SPA keys its delete
/// button on it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Method {
    pub method: &'static str,
    pub provider: Option<&'static str>,
}

/// `payment_method` as the SPA sends it.
fn method_of(sent: &str) -> Option<Method> {
    let plain = |method| Method {
        method,
        provider: None,
    };
    Some(match sent {
        "cash" => plain("cash"),
        "pin" => plain("pin"),
        "manual" => plain("manual"),
        "mollie" => Method {
            method: "provider",
            provider: Some("mollie"),
        },
        "bank" => Method {
            method: "provider",
            provider: Some("afas"),
        },
        _ => return None,
    })
}

/// `POST /invoices/{id}/payments`.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct CreatePaymentRequest {
    date: Scalar,
    amount: Scalar,
    payment_method: Scalar,
    description: Scalar,
    provider_id: Scalar,
    settle_invoice_id: Scalar,
    meta: Capped,
}

/// How an invoice is paid: money, or a credit note (a settle).
pub enum Pay {
    Method(Method),
    Settle(Uuid),
}

/// A validated payment.
pub struct PaymentInput {
    pub date: NaiveDate,
    /// In currency units, > 0; its decimals are checked against the currency later.
    pub amount: Decimal,
    pub pay: Pay,
    pub description: Option<String>,
    pub provider_id: Option<String>,
    pub meta: Option<Value>,
}

impl Validate for CreatePaymentRequest {
    type Valid = PaymentInput;

    fn validate(self, r: &mut Rules) -> Option<PaymentInput> {
        let date = date(r, &self.date);
        let amount = match self.amount.filled() {
            None => {
                r.missing("amount");
                None
            }
            Some(v) => r.decimal("amount", &v, -MAX, MAX).and_then(|a| {
                if a > Decimal::ZERO {
                    Some(a)
                } else {
                    r.add("amount", "The amount must be greater than 0.");
                    None
                }
            }),
        };
        let settle_invoice_id = self.settle_invoice_id.filled().and_then(|v| {
            let id = v.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok());
            if id.is_none() {
                r.add(
                    "settle_invoice_id",
                    "The selected settle invoice id is invalid.",
                );
            }
            id
        });
        let pay = match (self.payment_method.filled(), settle_invoice_id) {
            (_, Some(id)) => Some(Pay::Settle(id)),
            (None, None) => {
                r.add(
                    "payment_method",
                    "The payment method field is required when settle invoice id is empty.",
                );
                None
            }
            (Some(v), None) => method(r, &v).map(Pay::Method),
        };
        let description = text(r, "description", &self.description, 180);
        let provider_id = text(r, "provider_id", &self.provider_id, 255);
        let meta = meta(r, self.meta);
        Some(PaymentInput {
            date: date?,
            amount: amount?,
            pay: pay?,
            description,
            provider_id,
            meta: meta?,
        })
    }
}

/// `POST /invoices/{id}/mark-as-credited`.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct MarkCreditedRequest {
    date: Scalar,
    payment_method: Scalar,
    description: Scalar,
    provider_id: Scalar,
    meta: Capped,
}

/// A validated mark-as-credited.
pub struct MarkCreditedInput {
    pub date: NaiveDate,
    pub method: Method,
    pub description: Option<String>,
    pub provider_id: Option<String>,
    pub meta: Option<Value>,
}

impl Validate for MarkCreditedRequest {
    type Valid = MarkCreditedInput;

    fn validate(self, r: &mut Rules) -> Option<MarkCreditedInput> {
        let date = date(r, &self.date);
        let method = match self.payment_method.filled() {
            None => {
                r.missing("payment_method");
                None
            }
            Some(v) => method(r, &v),
        };
        let description = text(r, "description", &self.description, 180);
        let provider_id = text(r, "provider_id", &self.provider_id, 255);
        let meta = meta(r, self.meta);
        Some(MarkCreditedInput {
            date: date?,
            method: method?,
            description,
            provider_id,
            meta: meta?,
        })
    }
}

fn method(r: &mut Rules, v: &Value) -> Option<Method> {
    let sent = v
        .as_str()
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_default();
    let m = method_of(&sent);
    if m.is_none() {
        r.add("payment_method", "The selected payment method is invalid.");
    }
    m
}

/// Laravel `required|date`: `Y-m-d`, `Y-m-d H:i:s` or RFC 3339; the day is kept.
fn date(r: &mut Rules, v: &Scalar) -> Option<NaiveDate> {
    let Some(v) = v.filled() else {
        r.missing("date");
        return None;
    };
    let s = v.as_str().map(str::trim).unwrap_or_default();
    let day = NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|t| t.date())
        })
        .or_else(|| DateTime::parse_from_rfc3339(s).ok().map(|t| t.date_naive()))
        .filter(|d| (1..=9999).contains(&chrono::Datelike::year(d)));
    if day.is_none() {
        r.add("date", "The date is not a valid date.");
    }
    day
}

/// An optional string (or number) of at most `max` chars; blank is null.
fn text(r: &mut Rules, field: &str, v: &Scalar, max: usize) -> Option<String> {
    let v = v.filled()?;
    let s = match v {
        Value::String(s) => s.trim().to_owned(),
        Value::Number(n) => n.to_string(),
        _ => {
            r.add(field, format!("The {} must be a string.", human(field)));
            return None;
        }
    };
    r.max_chars(field, &s, max);
    Some(s)
}

/// Largest `meta` as compact JSON. The column's CHECK measures Postgres' jsonb text (a space
/// after each `:` and `,`), at most 1.5x this, so the CHECK never turns into a 500.
const META_MAX: usize = 8 * 1024;

/// `meta`: an object or null (Laravel's MetaRule never ran; any value was stored).
fn meta(r: &mut Rules, v: Capped) -> Option<Option<Value>> {
    let too_large = |r: &mut Rules| {
        r.add("meta", "The meta is too large.");
        None
    };
    match v {
        Capped::Absent => Some(None),
        Capped::TooLarge => too_large(r),
        Capped::Value(v) if v.to_string().len() > META_MAX => too_large(r),
        Capped::Value(v) => r.meta(Some(v)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spa_methods_map_as_laravel() {
        assert_eq!(method_of("bank").unwrap().provider, Some("afas"));
        assert_eq!(method_of("mollie").unwrap().method, "provider");
        assert_eq!(method_of("cash").unwrap().provider, None);
        assert_eq!(method_of("settled"), None, "only through a settle");
        assert_eq!(method_of("giftcard"), None, "the till's, F15");
    }
}
