//! Currency request/response bodies. Keys follow Laravel CurrencyResource and the
//! Store/UpdateCurrencyRequest rules (vimdesk-web currencies.store.js).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, present},
    modules::currencies::repo::CurrencyRow,
};

const NAME_MAX: usize = 255;
const SYMBOL_MAX: usize = 10;
/// Laravel IsoCurrencyRule message for attribute `asset type`.
const BAD_ASSET: &str = "Invalid asset type.";

/// ISO 4217 codes moneyphp's ISOCurrencies accepted (active codes, funds and metals), sorted.
const ISO_4217: [&str; 183] = [
    "AED", "AFN", "ALL", "AMD", "ANG", "AOA", "ARS", "AUD", "AWG", "AZN", "BAM", "BBD", "BDT",
    "BGN", "BHD", "BIF", "BMD", "BND", "BOB", "BOV", "BRL", "BSD", "BTN", "BWP", "BYN", "BZD",
    "CAD", "CDF", "CHE", "CHF", "CHW", "CLF", "CLP", "CNY", "COP", "COU", "CRC", "CUC", "CUP",
    "CVE", "CZK", "DJF", "DKK", "DOP", "DZD", "EGP", "ERN", "ETB", "EUR", "FJD", "FKP", "GBP",
    "GEL", "GHS", "GIP", "GMD", "GNF", "GTQ", "GYD", "HKD", "HNL", "HRK", "HTG", "HUF", "IDR",
    "ILS", "INR", "IQD", "IRR", "ISK", "JMD", "JOD", "JPY", "KES", "KGS", "KHR", "KMF", "KPW",
    "KRW", "KWD", "KYD", "KZT", "LAK", "LBP", "LKR", "LRD", "LSL", "LYD", "MAD", "MDL", "MGA",
    "MKD", "MMK", "MNT", "MOP", "MRU", "MUR", "MVR", "MWK", "MXN", "MXV", "MYR", "MZN", "NAD",
    "NGN", "NIO", "NOK", "NPR", "NZD", "OMR", "PAB", "PEN", "PGK", "PHP", "PKR", "PLN", "PYG",
    "QAR", "RON", "RSD", "RUB", "RWF", "SAR", "SBD", "SCR", "SDG", "SEK", "SGD", "SHP", "SLE",
    "SLL", "SOS", "SRD", "SSP", "STN", "SVC", "SYP", "SZL", "THB", "TJS", "TMT", "TND", "TOP",
    "TRY", "TTD", "TWD", "TZS", "UAH", "UGX", "USD", "USN", "UYI", "UYU", "UYW", "UZS", "VED",
    "VES", "VND", "VUV", "WST", "XAF", "XAG", "XAU", "XBA", "XBB", "XBC", "XBD", "XCD", "XCG",
    "XDR", "XOF", "XPD", "XPF", "XPT", "XSU", "XTS", "XUA", "XXX", "YER", "ZAR", "ZMW", "ZWG",
    "ZWL",
];

fn is_iso_4217(code: &str) -> bool {
    ISO_4217
        .binary_search(&code.to_ascii_uppercase().as_str())
        .is_ok()
}

/// Laravel CurrencyResource. `default_currency` must be a JSON bool: vimdesk-web and the POS
/// find the default with `=== true`.
#[derive(Serialize, ToSchema)]
#[schema(description = "A currency the workspace can price in. Exactly one is the default.")]
pub struct CurrencyResource {
    id: Uuid,
    name: String,
    asset_type: String,
    #[schema(required = true)]
    symbol_prefix: Option<String>,
    #[schema(required = true)]
    symbol_suffix: Option<String>,
    decimals: i16,
    default_currency: bool,
    active: bool,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
}

impl From<CurrencyRow> for CurrencyResource {
    fn from(c: CurrencyRow) -> Self {
        Self {
            id: c.id,
            name: c.name,
            asset_type: c.asset_type,
            symbol_prefix: c.symbol_prefix,
            symbol_suffix: c.symbol_suffix,
            decimals: c.decimals,
            default_currency: c.default_currency,
            active: c.active,
            meta: c.meta,
            created_at: c.created_at,
            last_updated_at: c.updated_at,
        }
    }
}

/// Create response: the new row's id.
#[derive(Serialize, ToSchema)]
pub struct CurrencyCreated {
    pub currency_id: Uuid,
}

/// GET list query. Unknown params ignored (vimdesk-web sends `q` only, often empty).
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// Search on name and asset type.
    pub q: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

/// POST /currencies. Raw `Value`s keep Laravel's lenient parsing ("2", "true");
/// the schema documents the clean types clients should send.
#[derive(Deserialize, ToSchema)]
#[schema(description = "New currency. `asset_type` is an ISO 4217 code.")]
pub struct CreateCurrencyRequest {
    #[schema(value_type = String, max_length = 255)]
    name: Option<String>,
    /// ISO 4217 code, any case.
    #[schema(value_type = String, example = "EUR")]
    asset_type: Option<String>,
    #[schema(max_length = 10)]
    symbol_prefix: Option<String>,
    #[schema(max_length = 10)]
    symbol_suffix: Option<String>,
    #[schema(value_type = i16, minimum = 0, maximum = 8)]
    decimals: Option<Value>,
    #[schema(value_type = bool)]
    default_currency: Option<Value>,
    #[schema(value_type = bool)]
    active: Option<Value>,
    #[schema(value_type = Option<Object>)]
    meta: Option<Value>,
}

/// Validated create input.
pub struct NewCurrency {
    pub name: String,
    pub asset_type: String,
    pub symbol_prefix: Option<String>,
    pub symbol_suffix: Option<String>,
    pub decimals: i16,
    pub default_currency: bool,
    pub active: bool,
    pub meta: Option<Value>,
}

fn asset(r: &mut Rules, code: &str) {
    if !code.is_empty() {
        r.check("asset_type", is_iso_4217(code), BAD_ASSET);
    }
}

fn decimals(r: &mut Rules, v: Option<Value>) -> Option<i16> {
    let v = r.required_value("decimals", v)?;
    r.integer("decimals", &v, 0, 8).map(|n| n as i16)
}

fn flag(r: &mut Rules, field: &str, v: Option<Value>) -> Option<bool> {
    let v = r.required_value(field, v)?;
    r.boolean(field, &v)
}

impl Validate for CreateCurrencyRequest {
    type Valid = NewCurrency;

    fn validate(self, r: &mut Rules) -> Option<NewCurrency> {
        let name = r.text("name", self.name, NAME_MAX);
        // Stored as sent (Laravel); checked case-insensitively.
        let asset_type = r.text("asset_type", self.asset_type, NAME_MAX);
        asset(r, &asset_type);
        Some(NewCurrency {
            name,
            asset_type,
            symbol_prefix: r.opt_text("symbol_prefix", self.symbol_prefix, SYMBOL_MAX),
            symbol_suffix: r.opt_text("symbol_suffix", self.symbol_suffix, SYMBOL_MAX),
            decimals: decimals(r, self.decimals).unwrap_or_default(),
            default_currency: flag(r, "default_currency", self.default_currency)
                .unwrap_or_default(),
            active: flag(r, "active", self.active).unwrap_or_default(),
            meta: r.meta(self.meta).flatten(),
        })
    }
}

/// POST /currencies/{id}: a patch. Absent = keep; the settings form echoes read-only keys
/// (`id`, `created_at`), which are ignored.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Changes to a currency. Leave a field out to keep it; send null to clear it."
)]
pub struct UpdateCurrencyRequest {
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 255)]
    name: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, example = "EUR")]
    asset_type: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 10)]
    symbol_prefix: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 10)]
    symbol_suffix: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<i16>, minimum = 0, maximum = 8)]
    decimals: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<bool>)]
    default_currency: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<bool>)]
    active: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
    meta: Option<Option<Value>>,
}

/// None = keep. `Some(None)` on a nullable column clears it.
pub struct CurrencyPatch {
    pub name: Option<String>,
    pub asset_type: Option<String>,
    pub symbol_prefix: Option<Option<String>>,
    pub symbol_suffix: Option<Option<String>>,
    pub decimals: Option<i16>,
    pub default_currency: Option<bool>,
    pub active: Option<bool>,
    pub meta: Option<Option<Value>>,
}

impl Validate for UpdateCurrencyRequest {
    type Valid = CurrencyPatch;

    fn validate(self, r: &mut Rules) -> Option<CurrencyPatch> {
        let asset_type = r.patch_text("asset_type", self.asset_type, NAME_MAX);
        if let Some(code) = &asset_type {
            asset(r, code);
        }
        Some(CurrencyPatch {
            name: r.patch_text("name", self.name, NAME_MAX),
            asset_type,
            symbol_prefix: r.patch_opt_text("symbol_prefix", self.symbol_prefix, SYMBOL_MAX),
            symbol_suffix: r.patch_opt_text("symbol_suffix", self.symbol_suffix, SYMBOL_MAX),
            decimals: self.decimals.and_then(|v| decimals(r, v)),
            // null = false: Laravel's FilterValidateBoolean cast stored it that way.
            default_currency: self.default_currency.map(|v| match filled(v) {
                None => false,
                Some(v) => r.boolean("default_currency", &v).unwrap_or_default(),
            }),
            active: self.active.and_then(|v| flag(r, "active", v)),
            meta: self.meta.and_then(|v| r.meta(v)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_list_sorted_and_case_blind() {
        assert!(ISO_4217.windows(2).all(|w| w[0] < w[1]));
        for ok in ["EUR", "eur", "Usd", "JPY", "XAU"] {
            assert!(is_iso_4217(ok), "{ok}");
        }
        for bad in ["xyz", "EU", "EURO", ""] {
            assert!(!is_iso_4217(bad), "{bad}");
        }
    }
}
