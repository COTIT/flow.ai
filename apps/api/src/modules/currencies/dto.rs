//! Currency request/response bodies. Keys follow Laravel CurrencyResource and the
//! Store/UpdateCurrencyRequest rules (vimdesk-web currencies.store.js).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
#[derive(Serialize)]
pub struct CurrencyResource {
    id: Uuid,
    name: String,
    asset_type: String,
    symbol_prefix: Option<String>,
    symbol_suffix: Option<String>,
    decimals: i16,
    default_currency: bool,
    active: bool,
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
#[derive(Serialize)]
pub struct CurrencyCreated {
    pub currency_id: Uuid,
}

/// GET list query. Unknown params ignored (vimdesk-web sends `q` only, often empty).
#[derive(Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub page: Option<String>,
    pub page_size: Option<String>,
}

/// POST /currencies.
#[derive(Deserialize)]
pub struct CreateCurrencyRequest {
    name: Option<String>,
    asset_type: Option<String>,
    symbol_prefix: Option<String>,
    symbol_suffix: Option<String>,
    decimals: Option<Value>,
    default_currency: Option<Value>,
    active: Option<Value>,
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
#[derive(Deserialize)]
pub struct UpdateCurrencyRequest {
    #[serde(default, deserialize_with = "present")]
    name: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    asset_type: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    symbol_prefix: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    symbol_suffix: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    decimals: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    default_currency: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    active: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
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
