//! Till sale bodies: catalog lines at the price the till charged, one payment (cash with the
//! tendered amount, or card), the offline `client_ref`. Money is integer minor units.

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::{
        orders::dto::{LineInput, LineRequest, lines},
        till_sales::repo::SaleRow,
    },
};

const REF_MAX: usize = 64;
const MONEY_MAX: i64 = 1_000_000_000_000;
const NEEDS_PRICE: &str = "The unit price the till charged is required.";
const TILL_ONLY_PRICE: &str = "The till sends only the unit price it charged.";
/// Offline sales older than this are refused: a queue that old needs a person.
const OFFLINE_MAX_DAYS: i64 = 7;
/// Clock skew a till may have.
const SKEW_MINUTES: i64 = 5;

/// The payment of a sale.
#[derive(Deserialize, ToSchema)]
#[allow(dead_code)]
pub struct TillPayment {
    /// `cash` or `card` (an external terminal, recorded by hand).
    #[schema(example = "cash")]
    method: String,
    /// Cash handed over, minor units; required for cash.
    #[schema(example = 5000)]
    tendered: Option<i64>,
    /// Card slip or terminal reference.
    #[schema(max_length = 64)]
    reference: Option<String>,
}

/// POST /till/sales.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "A till sale. Lines carry the unit price the till charged (minor units excl VAT). Send the same body again under the same `client_ref` to replay it from an offline queue."
)]
pub struct SaleRequest {
    /// The till's own id for this sale, unique per register; reuse it on every retry.
    #[schema(value_type = String, max_length = 64, example = "0190f3c2-offline-17")]
    client_ref: Option<String>,
    /// When the sale was made (RFC 3339), for an offline replay; default now. At most 7 days old.
    #[schema(value_type = Option<DateTime<Utc>>)]
    sold_at: Option<String>,
    /// 1 to 50 lines; `unit_price` is required, `discount_percentage` and `title` are not taken.
    lines: Option<Vec<LineRequest>>,
    #[schema(value_type = TillPayment)]
    payment: Option<Value>,
}

/// How the sale was paid. `tendered` is always set on a sale; a quote may leave it out.
pub enum Payment {
    Cash { tendered: Option<i64> },
    Card { reference: Option<String> },
}

/// A validated sale.
pub struct NewSale {
    pub client_ref: String,
    pub sold_at: Option<DateTime<Utc>>,
    pub lines: Vec<LineInput>,
    pub payment: Payment,
    /// sha256 of the sale as validated: a replay must match it.
    pub hash: [u8; 32],
}

/// The payment; `need_tendered` makes cash's `tendered` required (a sale, not a quote).
fn payment(r: &mut Rules, v: Option<Value>, need_tendered: bool) -> Option<Payment> {
    let v = r.required_value("payment", v)?;
    let method = r.one_of(
        "payment.method",
        v["method"].as_str().map(str::to_owned),
        &["cash", "card"],
        None,
    );
    match method.as_deref() {
        Some("cash") => match &v["tendered"] {
            Value::Null if need_tendered => {
                r.missing("payment.tendered");
                None
            }
            Value::Null => Some(Payment::Cash { tendered: None }),
            t => r
                .integer("payment.tendered", t, 0, MONEY_MAX)
                .map(|t| Payment::Cash { tendered: Some(t) }),
        },
        Some(_) => {
            let reference = r.opt_text(
                "payment.reference",
                v["reference"].as_str().map(str::to_owned),
                REF_MAX,
            );
            Some(Payment::Card { reference })
        }
        None => {
            if v["method"].is_null() {
                r.missing("payment.method");
            }
            None
        }
    }
}

fn sold_at(r: &mut Rules, v: Option<String>) -> Option<DateTime<Utc>> {
    let v = crate::http::json::optional(v)?;
    let Ok(at) = DateTime::parse_from_rfc3339(&v).map(|d| d.with_timezone(&Utc)) else {
        r.add("sold_at", "The sold at must be an RFC 3339 date-time.");
        return None;
    };
    let now = Utc::now();
    r.check(
        "sold_at",
        at <= now + Duration::minutes(SKEW_MINUTES) && at >= now - Duration::days(OFFLINE_MAX_DAYS),
        "The sold at must be within the last 7 days.",
    );
    Some(at)
}

/// Till lines: catalog lines that carry only the unit price the till charged.
fn till_lines(r: &mut Rules, rows: Option<Vec<LineRequest>>) -> Vec<LineInput> {
    for (n, l) in rows.iter().flatten().enumerate() {
        if l.unit_price.as_ref().is_none_or(Value::is_null) {
            r.add(&format!("lines.{n}.unit_price"), NEEDS_PRICE);
        }
        if l.discount_percentage.is_some() || l.title.is_some() {
            r.add(&format!("lines.{n}.unit_price"), TILL_ONLY_PRICE);
        }
    }
    lines(r, rows, true)
}

impl Validate for SaleRequest {
    type Valid = NewSale;

    fn validate(self, r: &mut Rules) -> Option<NewSale> {
        let client_ref = r.text("client_ref", self.client_ref, REF_MAX);
        let sold_at = sold_at(r, self.sold_at);
        let lines = till_lines(r, self.lines);
        let payment = payment(r, self.payment, true)?;
        let canonical = json!({
            "client_ref": client_ref,
            // The instant, not its spelling: `Z` and `+00:00` are one sale.
            "sold_at": sold_at.map(|t| t.to_rfc3339_opts(SecondsFormat::Micros, true)),
            "lines": lines.iter().map(|l| json!([
                l.product_id, l.product_variant_id, l.quantity.normalize().to_string(), l.unit_price,
            ])).collect::<Vec<_>>(),
            "payment": match &payment {
                Payment::Cash { tendered } => json!(["cash", tendered]),
                Payment::Card { reference } => json!(["card", reference]),
            },
        });
        let hash = Sha256::digest(canonical.to_string().as_bytes()).into();
        Some(NewSale {
            client_ref,
            sold_at,
            lines,
            payment,
            hash,
        })
    }
}

/// POST /till/sales/quote.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "A basket to price as `POST /till/sales` would, without booking it. Same lines \
    and payment; cash `tendered` is optional (gives `change`)."
)]
pub struct QuoteRequest {
    /// 1 to 50 lines; `unit_price` is required, `discount_percentage` and `title` are not taken.
    lines: Option<Vec<LineRequest>>,
    #[schema(value_type = TillPayment)]
    payment: Option<Value>,
}

/// A validated quote.
pub struct NewQuote {
    pub lines: Vec<LineInput>,
    pub payment: Payment,
}

impl Validate for QuoteRequest {
    type Valid = NewQuote;

    fn validate(self, r: &mut Rules) -> Option<NewQuote> {
        let lines = till_lines(r, self.lines);
        let payment = payment(r, self.payment, false)?;
        Some(NewQuote { lines, payment })
    }
}

/// One quoted line, as the sale would book it.
#[derive(Serialize, ToSchema)]
pub struct QuoteLine {
    pub product_id: Uuid,
    #[schema(required = true)]
    pub product_variant_id: Option<Uuid>,
    /// Decimal string.
    #[schema(example = "2.00")]
    pub quantity: String,
    /// Minor units excl VAT.
    pub unit_price: i64,
    pub line_excl: i64,
    pub line_vat: i64,
    /// line_excl + line_vat.
    pub line_total: i64,
}

/// What `POST /till/sales` would book for this basket now. Minor units.
#[derive(Serialize, ToSchema)]
pub struct QuoteResource {
    pub lines: Vec<QuoteLine>,
    /// Excl VAT.
    pub subtotal: i64,
    pub vat: i64,
    /// Incl VAT: the order total.
    pub total: i64,
    /// The same lines at today's catalog prices.
    pub server_total: i64,
    /// total − server_total.
    pub price_mismatch: i64,
    /// Lines at another unit price than the catalog's.
    pub mismatched_lines: i32,
    /// To pay: cash rounded to the currency's step, card = total.
    pub due: i64,
    /// due − total.
    pub rounding: i64,
    /// tendered − due; null for card or without `tendered`.
    #[schema(required = true)]
    pub change: Option<i64>,
}

/// A till sale as booked.
#[derive(Serialize, ToSchema)]
pub struct SaleResource {
    order_id: Uuid,
    /// Order number (the order series).
    number: String,
    client_ref: String,
    register_id: Uuid,
    till_user_id: Uuid,
    #[schema(example = "cash")]
    method: String,
    /// What the till charged incl VAT: the order total.
    total: i64,
    /// The same lines at today's catalog prices.
    server_total: i64,
    /// total − server_total; not 0 = the till charged other prices (e.g. offline).
    price_mismatch: i64,
    /// Lines charged at another unit price than the catalog's; counts even when they offset.
    mismatched_lines: i32,
    /// Paid: cash rounded to the currency's step, card = total.
    due: i64,
    /// due − total.
    rounding: i64,
    #[schema(required = true)]
    tendered: Option<i64>,
    #[schema(required = true)]
    change: Option<i64>,
    #[schema(required = true)]
    card_reference: Option<String>,
    sold_at: DateTime<Utc>,
    /// The booked lines, in order, for the receipt.
    lines: Vec<SaleLine>,
}

/// One booked line of a till sale. Minor units.
#[derive(Serialize, ToSchema)]
pub struct SaleLine {
    #[schema(example = "Honey 500 g")]
    title: String,
    product_id: Uuid,
    #[schema(required = true)]
    product_variant_id: Option<Uuid>,
    /// Decimal string.
    #[schema(example = "2.00")]
    quantity: String,
    /// What the till charged per unit, excl VAT.
    unit_price: i64,
    line_excl: i64,
    line_vat: i64,
    /// line_excl + line_vat.
    line_total: i64,
}

impl SaleResource {
    /// `s` with its order's `lines`.
    pub fn new(s: SaleRow, lines: &[crate::modules::orders::repo::LineRow]) -> Self {
        Self {
            lines: lines
                .iter()
                .map(|l| SaleLine {
                    title: l.title.clone(),
                    product_id: l.product_id,
                    product_variant_id: l.product_variant_id,
                    quantity: format!("{:.2}", l.quantity),
                    unit_price: l.unit_price,
                    line_excl: l.line_excl,
                    line_vat: l.line_vat,
                    line_total: l.line_excl + l.line_vat,
                })
                .collect(),
            order_id: s.order_id,
            number: s.number,
            client_ref: s.client_ref,
            register_id: s.register_id,
            till_user_id: s.till_user_id,
            method: s.method,
            total: s.total,
            server_total: s.server_total,
            price_mismatch: s.price_mismatch,
            mismatched_lines: s.mismatched_lines,
            due: s.due,
            rounding: s.rounding,
            tendered: s.tendered,
            change: s.change,
            card_reference: s.card_reference,
            sold_at: s.sold_at,
        }
    }
}

/// GET /till/sales query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SaleQuery {
    /// `true`: only sales with a line charged at another price than the catalog's.
    #[param(value_type = Option<bool>)]
    pub mismatch: Option<String>,
    /// Only this register's sales.
    #[param(value_type = Option<Uuid>)]
    pub register_id: Option<String>,
    /// Sold at or after this time (RFC 3339).
    #[param(value_type = Option<String>, format = DateTime, example = "2026-11-02T00:00:00+01:00")]
    pub from: Option<String>,
    /// Sold before this time (RFC 3339).
    #[param(value_type = Option<String>, format = DateTime)]
    pub to: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

impl SaleQuery {
    /// The filter; 422 on a malformed `register_id`, `from` or `to`. A `mismatch` other than
    /// `true`/`1` means no filter, as before.
    pub fn filter(
        &self,
    ) -> Result<crate::modules::till_sales::repo::ListFilter, crate::error::AppError> {
        use crate::http::list;
        let mut r = Rules::default();
        let f = crate::modules::till_sales::repo::ListFilter {
            mismatch: matches!(self.mismatch.as_deref(), Some("true" | "1")),
            register_id: list::id(&mut r, "register_id", self.register_id.as_deref()),
            from: list::time(&mut r, "from", self.from.as_deref()),
            to: list::time(&mut r, "to", self.to.as_deref()),
        };
        r.into_result()?;
        Ok(f)
    }
}
