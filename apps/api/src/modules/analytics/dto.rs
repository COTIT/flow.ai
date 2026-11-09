//! Analytics queries (period, limit, grouping) and response shapes. Money is integer minor units.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    error::AppError,
    http::json::Rules,
    modules::orders::dto::{SOURCES, SUMMARY_DAYS_MAX},
};

/// Rows per currency when `limit` is absent.
pub const LIMIT_DEFAULT: i64 = 5;
/// Most rows per currency.
pub const LIMIT_MAX: i64 = 20;

/// A period bound: an exact instant, or a calendar day starting at local midnight in the
/// workspace `general.timezone`.
// shortcut: same rule as `voice-filters`' `http::list::When`; fold into it once both merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    At(DateTime<Utc>),
    Day(NaiveDate),
}

/// `field` as RFC 3339 or `YYYY-MM-DD`; missing or blank or malformed = 422 on `field`.
fn when(r: &mut Rules, field: &str, v: Option<&str>) -> Option<When> {
    let Some(v) = v.map(str::trim).filter(|v| !v.is_empty()) else {
        r.missing(field);
        return None;
    };
    let w = DateTime::parse_from_rfc3339(v)
        .map(|t| When::At(t.with_timezone(&Utc)))
        .or_else(|_| NaiveDate::parse_from_str(v, "%Y-%m-%d").map(When::Day))
        .ok();
    let msg = format!(
        "The {} must be an RFC 3339 time or a YYYY-MM-DD date.",
        field.replace('_', " ")
    );
    r.check(field, w.is_some(), msg);
    w
}

fn source(r: &mut Rules, v: Option<&str>) -> Option<String> {
    let v = v.map(str::trim).filter(|v| !v.is_empty())?;
    r.check(
        "source",
        SOURCES.contains(&v),
        "The selected source is invalid.",
    );
    Some(v.to_owned())
}

fn limit(r: &mut Rules, v: Option<&str>) -> i64 {
    let Some(v) = v.map(str::trim).filter(|v| !v.is_empty()) else {
        return LIMIT_DEFAULT;
    };
    let n = v
        .parse::<i64>()
        .ok()
        .filter(|n| (1..=LIMIT_MAX).contains(n));
    r.check(
        "limit",
        n.is_some(),
        format!("The limit must be a whole number from 1 to {LIMIT_MAX}."),
    );
    n.unwrap_or(LIMIT_DEFAULT)
}

/// A period still in query form: `from` inclusive, `to` exclusive.
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub from: When,
    pub to: When,
}

/// A resolved period.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct Period {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

impl Period {
    /// 422 on `to_field` when `to` isn't after `from` or the span passes 366 days.
    pub fn check(self, to_field: &str) -> Result<Self, AppError> {
        if self.to <= self.from {
            return Err(AppError::field(
                to_field,
                format!(
                    "The {} must be after {}.",
                    to_field.replace('_', " "),
                    from_of(to_field)
                ),
            ));
        }
        if self.to - self.from > chrono::Duration::days(SUMMARY_DAYS_MAX) {
            return Err(AppError::field(
                to_field,
                format!("The range must not pass {SUMMARY_DAYS_MAX} days."),
            ));
        }
        Ok(self)
    }
}

fn from_of(to_field: &str) -> String {
    to_field.replacen("to", "from", 1).replace('_', " ")
}

/// GET /sales/top-customers and /sales/top-products query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TopQuery {
    /// Sold at or after: RFC 3339, or `YYYY-MM-DD` = local midnight in the workspace time zone.
    #[param(required = true, value_type = String, example = "2026-10-01")]
    pub from: Option<String>,
    /// Sold before (exclusive), same forms, at most 366 days after `from`.
    #[param(required = true, value_type = String, example = "2026-11-01")]
    pub to: Option<String>,
    /// Only `office`, `storefront` or `till` orders.
    pub source: Option<String>,
    /// Rows per currency, 1 to 20 (default 5).
    #[param(value_type = Option<i64>, example = 5)]
    pub limit: Option<String>,
    /// top-products only: `product` (default) or `variant` (each size apart).
    pub by: Option<String>,
    /// Just this product's lines (all its variants): "how much honey sold" (top-products),
    /// "who buys the most honey" (top-customers).
    #[param(value_type = Option<Uuid>)]
    pub product_id: Option<String>,
    /// Just this variant's (size's) lines.
    #[param(value_type = Option<Uuid>)]
    pub variant_id: Option<String>,
}

/// Validated top-N query.
pub struct Top {
    pub bounds: Bounds,
    pub source: Option<String>,
    pub limit: i64,
    pub by_variant: bool,
    pub product_id: Option<Uuid>,
    pub variant_id: Option<Uuid>,
}

impl TopQuery {
    /// The query; 422 for a missing or malformed bound, unknown `source` or `by`, `limit` outside 1..20.
    pub fn top(&self) -> Result<Top, AppError> {
        let mut r = Rules::default();
        let from = when(&mut r, "from", self.from.as_deref());
        let to = when(&mut r, "to", self.to.as_deref());
        let source = source(&mut r, self.source.as_deref());
        let limit = limit(&mut r, self.limit.as_deref());
        let by = self.by.as_deref().map(str::trim).filter(|v| !v.is_empty());
        r.check(
            "by",
            by.is_none_or(|b| matches!(b, "product" | "variant")),
            "The by must be product or variant.",
        );
        let product_id = uuid(&mut r, "product_id", self.product_id.as_deref());
        let variant_id = uuid(&mut r, "variant_id", self.variant_id.as_deref());
        r.into_result()?;
        let (Some(from), Some(to)) = (from, to) else {
            return Err(AppError::Internal("bounds checked above".into()));
        };
        Ok(Top {
            bounds: Bounds { from, to },
            source,
            limit,
            by_variant: by == Some("variant"),
            product_id,
            variant_id,
        })
    }
}

/// Optional UUID `field` (blank = none); 422 when not a UUID.
fn uuid(r: &mut Rules, field: &str, v: Option<&str>) -> Option<Uuid> {
    let v = v.map(str::trim).filter(|v| !v.is_empty());
    let parsed = v.and_then(|v| Uuid::parse_str(v).ok());
    r.check(
        field,
        v.is_none() || parsed.is_some(),
        format!("The {} must be a valid UUID.", field.replace('_', " ")),
    );
    parsed
}

/// GET /sales/compare query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CompareQuery {
    /// Period start: RFC 3339, or `YYYY-MM-DD` = local midnight in the workspace time zone.
    #[param(required = true, value_type = String, example = "2026-11-01")]
    pub from: Option<String>,
    /// Period end (exclusive), at most 366 days after `from`.
    #[param(required = true, value_type = String, example = "2026-12-01")]
    pub to: Option<String>,
    /// Start of the period to compare against (the base), same forms.
    #[param(required = true, value_type = String, example = "2026-10-01")]
    pub compare_from: Option<String>,
    /// End of the base period (exclusive).
    #[param(required = true, value_type = String, example = "2026-11-01")]
    pub compare_to: Option<String>,
    /// Only `office`, `storefront` or `till` orders.
    pub source: Option<String>,
}

/// Validated compare query.
pub struct Compare {
    pub current: Bounds,
    pub base: Bounds,
    pub source: Option<String>,
}

impl CompareQuery {
    /// The query; 422 for a missing or malformed bound or unknown `source`.
    pub fn compare(&self) -> Result<Compare, AppError> {
        let mut r = Rules::default();
        let from = when(&mut r, "from", self.from.as_deref());
        let to = when(&mut r, "to", self.to.as_deref());
        let compare_from = when(&mut r, "compare_from", self.compare_from.as_deref());
        let compare_to = when(&mut r, "compare_to", self.compare_to.as_deref());
        let source = source(&mut r, self.source.as_deref());
        r.into_result()?;
        let (Some(from), Some(to), Some(cf), Some(ct)) = (from, to, compare_from, compare_to)
        else {
            return Err(AppError::Internal("bounds checked above".into()));
        };
        Ok(Compare {
            current: Bounds { from, to },
            base: Bounds { from: cf, to: ct },
            source,
        })
    }
}

/// GET /invoices-export query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ExportQuery {
    /// Invoice date (`send_date`) at or after: RFC 3339, or `YYYY-MM-DD` = local midnight.
    #[param(required = true, value_type = String, example = "2026-10-01")]
    pub from: Option<String>,
    /// Invoice date before (exclusive), at most 366 days after `from`.
    #[param(required = true, value_type = String, example = "2026-11-01")]
    pub to: Option<String>,
}

impl ExportQuery {
    /// The bounds; 422 for a missing or malformed one.
    pub fn bounds(&self) -> Result<Bounds, AppError> {
        let mut r = Rules::default();
        let from = when(&mut r, "from", self.from.as_deref());
        let to = when(&mut r, "to", self.to.as_deref());
        r.into_result()?;
        let (Some(from), Some(to)) = (from, to) else {
            return Err(AppError::Internal("bounds checked above".into()));
        };
        Ok(Bounds { from, to })
    }
}

/// `{id, name}` of a relation or product.
#[derive(Debug, Serialize, ToSchema)]
pub struct Named {
    pub id: Uuid,
    pub name: String,
}

/// Orders and their gross total.
#[derive(Debug, Default, Serialize, ToSchema)]
pub struct OrderTotals {
    pub orders: i64,
    /// Totals incl VAT, minor units.
    pub gross_total: i64,
    /// Totals excl VAT.
    pub net_total: i64,
    pub vat_total: i64,
}

/// One ranked customer.
#[derive(Debug, Serialize, ToSchema)]
pub struct CustomerRow {
    pub relation: Named,
    pub orders: i64,
    /// Their orders' totals incl VAT, minor units.
    pub gross_total: i64,
    /// Totals excl VAT.
    pub net_total: i64,
    pub vat_total: i64,
}

/// Ranked customers in one currency.
#[derive(Debug, Serialize, ToSchema)]
pub struct CustomerCurrency {
    #[schema(example = "EUR")]
    pub currency: String,
    pub decimals: i16,
    /// Best first: gross total, then name.
    pub rows: Vec<CustomerRow>,
    /// Orders with no customer (till walk-ins); never ranked.
    pub without_customer: OrderTotals,
}

/// GET /sales/top-customers.
#[derive(Debug, Serialize, ToSchema)]
pub struct TopCustomers {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    /// Zone the plain dates were read in (`UTC` when unset or unknown).
    #[schema(example = "Africa/Dar_es_Salaam")]
    pub time_zone: String,
    pub source: Option<String>,
    pub currencies: Vec<CustomerCurrency>,
}

/// A sold variant.
#[derive(Debug, Serialize, ToSchema)]
pub struct VariantRef {
    pub id: Uuid,
    pub sku: Option<String>,
}

/// One ranked product (or variant).
#[derive(Debug, Serialize, ToSchema)]
pub struct ProductRow {
    pub product: Named,
    /// Set when `by=variant` and the line was for a variant.
    pub variant: Option<VariantRef>,
    /// Line title as sold (names the size).
    pub title: String,
    /// Quantity sold, 2 decimals.
    #[schema(example = "12.00")]
    pub units: String,
    pub lines: i64,
    pub orders: i64,
    /// Line totals incl VAT, minor units.
    pub gross_total: i64,
    /// Line totals excl VAT.
    pub net_total: i64,
    pub vat_total: i64,
}

/// Ranked products in one currency.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProductCurrency {
    #[schema(example = "EUR")]
    pub currency: String,
    pub decimals: i16,
    pub rows: Vec<ProductRow>,
}

/// GET /sales/top-products.
#[derive(Debug, Serialize, ToSchema)]
pub struct TopProducts {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    #[schema(example = "UTC")]
    pub time_zone: String,
    pub source: Option<String>,
    /// `product` or `variant`.
    pub by: String,
    pub currencies: Vec<ProductCurrency>,
}

/// One period's sales.
#[derive(Debug, Serialize, ToSchema)]
pub struct Sums {
    pub orders: i64,
    pub gross_total: i64,
    /// Totals excl VAT.
    pub net_total: i64,
    pub vat_total: i64,
    pub refunded_total: i64,
}

/// Both periods in one currency.
#[derive(Debug, Serialize, ToSchema)]
pub struct CompareCurrency {
    #[schema(example = "EUR")]
    pub currency: String,
    pub decimals: i16,
    pub current: Sums,
    pub base: Sums,
    /// current − base gross, minor units.
    pub gross_difference: i64,
    pub orders_difference: i64,
    /// Signed change of gross in basis points (10000 = +100 %), half away from zero. Absent
    /// when the base gross is 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gross_change_bp: Option<i64>,
    /// current − base net (excl VAT), minor units.
    pub net_difference: i64,
    /// Same as `gross_change_bp` on net totals; absent when the base net is 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_change_bp: Option<i64>,
}

/// GET /sales/compare.
#[derive(Debug, Serialize, ToSchema)]
pub struct SalesCompare {
    pub period: Period,
    pub compare_to_period: Period,
    #[schema(example = "UTC")]
    pub time_zone: String,
    pub source: Option<String>,
    pub currencies: Vec<CompareCurrency>,
}

/// Exported totals in one currency (credit notes negative).
#[derive(Debug, Serialize, ToSchema)]
pub struct ExportTotals {
    #[schema(example = "EUR")]
    pub currency: String,
    pub decimals: i16,
    pub excl: i64,
    pub vat: i64,
    pub total: i64,
}

/// GET /invoices-export.
#[derive(Debug, Serialize, ToSchema)]
pub struct InvoiceExport {
    /// Stored CSV (removed after 24 h); null when nothing was in range.
    pub file_id: Option<Uuid>,
    #[schema(example = "invoices-2026-10-01-to-2026-10-31.csv")]
    pub name: String,
    /// Download link, valid until `expires_at` (10 minutes).
    pub url: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub invoices: i64,
    pub lines: i64,
    pub currencies: Vec<ExportTotals>,
}
