//! Relation addresses (Laravel Infrastructure\Values\Address): stored fields, postal format
//! (rinvex template per country), validation of the `addresses` array.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{
    http::json::{Rules, human},
    modules::relations::countries,
    php::{truthy, ucfirst},
};

/// rinvex fallback when a known country has no template (Laravel Address::format).
const DEFAULT_FORMAT: &str =
    "{{recipient}}\n{{street}}\n{{postalcode}} {{city}}\n{{region}}\n{{country}}";

/// One address as stored in `relations.addresses`. Country uppercase, phone trimmed.
/// Reads leniently (numbers as text, null flags as false): Laravel rows held both.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Address {
    #[serde(deserialize_with = "lenient_text")]
    pub address: Option<String>,
    #[serde(deserialize_with = "lenient_text")]
    pub house_number: Option<String>,
    #[serde(deserialize_with = "lenient_text")]
    pub zipcode: Option<String>,
    #[serde(deserialize_with = "lenient_text")]
    pub city: Option<String>,
    #[serde(deserialize_with = "lenient_text")]
    pub country: Option<String>,
    #[serde(deserialize_with = "lenient_text")]
    pub house_number_addition: Option<String>,
    #[serde(deserialize_with = "lenient_text")]
    pub address2: Option<String>,
    #[serde(deserialize_with = "lenient_text")]
    pub state: Option<String>,
    #[serde(deserialize_with = "lenient_text")]
    pub label: Option<String>,
    #[serde(deserialize_with = "lenient_flag")]
    pub main_address: bool,
    #[serde(rename = "type", deserialize_with = "lenient_text")]
    pub kind: Option<String>,
    #[serde(deserialize_with = "lenient_text")]
    pub attn: Option<String>,
    #[serde(deserialize_with = "lenient_text")]
    pub phone_number: Option<String>,
}

/// Stored text field: string, number (as its text) or null; anything else reads as null.
fn lenient_text<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => Some(s),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

/// Stored flag: true, 1 or "1" are true; anything else (null included) is false.
fn lenient_flag<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let v = Value::deserialize(d)?;
    Ok(v == Value::Bool(true) || v.as_i64() == Some(1) || v.as_str() == Some("1"))
}

/// Address as Laravel serialized it: stored fields in constructor order plus the computed
/// `formatted`/`search_format` and the three per-address VAT keys (null outside detail).
#[derive(Serialize)]
pub struct AddressView {
    address: Option<String>,
    house_number: Option<String>,
    zipcode: Option<String>,
    city: Option<String>,
    country: Option<String>,
    house_number_addition: Option<String>,
    address2: Option<String>,
    state: Option<String>,
    label: Option<String>,
    main_address: bool,
    #[serde(rename = "type")]
    kind: Option<String>,
    formatted: String,
    search_format: String,
    attn: Option<String>,
    phone_number: Option<String>,
    pub reverse_charge: Option<bool>,
    pub shows_reverse_charge_notice: Option<bool>,
    pub vat_rates: Option<Value>,
}

impl From<&Address> for AddressView {
    fn from(a: &Address) -> Self {
        Self {
            formatted: a.formatted(),
            search_format: a.search_format(),
            address: a.address.clone(),
            house_number: a.house_number.clone(),
            zipcode: a.zipcode.clone(),
            city: a.city.clone(),
            country: a.country.clone(),
            house_number_addition: a.house_number_addition.clone(),
            address2: a.address2.clone(),
            state: a.state.clone(),
            label: a.label.clone(),
            main_address: a.main_address,
            kind: a.kind.clone(),
            attn: a.attn.clone(),
            phone_number: a.phone_number.clone(),
            reverse_charge: None,
            shows_reverse_charge_notice: None,
            vat_rates: None,
        }
    }
}

fn text(s: &Option<String>) -> &str {
    s.as_deref().unwrap_or("")
}

impl Address {
    /// Postal block: the country's rinvex template, else Laravel's non-region layout.
    pub fn formatted(&self) -> String {
        let country = text(&self.country);
        let Some(c) = (!country.is_empty())
            .then(|| countries::find(country))
            .flatten()
        else {
            return self.non_region_format();
        };
        let mut f = c.address_format.unwrap_or(DEFAULT_FORMAT).to_owned();
        if !text(&self.attn).is_empty() {
            f = f.replace("{{recipient}}", &ucfirst(text(&self.attn)));
        }
        let street = format!(
            "{} {}{} {}",
            ucfirst(text(&self.address)),
            text(&self.house_number),
            text(&self.house_number_addition),
            ucfirst(text(&self.address2)),
        );
        f.replace("{{recipient}}\n", "")
            .replace("{{street}}", street.trim_matches(PHP_TRIM))
            .replace("{{postalcode}}", &text(&self.zipcode).to_ascii_uppercase())
            .replace("{{region_short}}", &ucfirst(text(&self.state)))
            .replace("{{region}}", &ucfirst(text(&self.state)))
            .replace("{{city}}", &ucfirst(text(&self.city)))
            .replace("{{country}}", c.name)
    }

    fn non_region_format(&self) -> String {
        let mut out = String::new();
        if truthy(self.address.as_deref()) {
            out += &format!("{} ", text(&self.address));
        }
        if truthy(self.house_number.as_deref()) {
            out += &format!(
                "{}{}\n",
                text(&self.house_number),
                text(&self.house_number_addition)
            );
        }
        if truthy(self.address2.as_deref()) {
            out += &format!("{}\n", text(&self.address2));
        }
        if truthy(self.zipcode.as_deref()) {
            out += &format!("{}, ", text(&self.zipcode).to_ascii_uppercase());
        }
        if truthy(self.city.as_deref()) {
            out += &format!("{}\n", text(&self.city));
        }
        if truthy(self.country.as_deref()) {
            out += text(&self.country);
        }
        out
    }

    /// `1339nm 11`: zipcode lowercased without spaces + house number (zipcode lookups).
    pub fn search_format(&self) -> String {
        let zipcode = if truthy(self.zipcode.as_deref()) {
            text(&self.zipcode).to_ascii_lowercase().replace(' ', "")
        } else {
            String::new()
        };
        format!("{zipcode} {}", text(&self.house_number))
    }
}

/// Most addresses one relation may hold.
pub const MAX_ADDRESSES: usize = 50;

/// Characters PHP `trim()` strips.
const PHP_TRIM: &[char] = &[' ', '\t', '\n', '\r', '\0', '\x0B'];

/// One string field: trimmed, blank = None, non-string = "must be a string", over `max`
/// chars = Laravel max wording.
fn field(
    r: &mut Rules,
    key: &str,
    v: Option<&Value>,
    max: usize,
    required: bool,
) -> Option<String> {
    let s = match v {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.trim().to_owned()).filter(|s| !s.is_empty()),
        Some(_) => {
            r.add(key, format!("The {} must be a string.", human(key)));
            return None;
        }
    };
    match &s {
        None if required => r.missing(key),
        Some(s) => r.max_chars(key, s, max),
        None => {}
    }
    s
}

/// PHP `is_numeric` on a sent house number: JSON number or plain decimal string.
fn house_number(r: &mut Rules, v: Option<&Value>) -> Option<String> {
    let s = match v {
        None | Some(Value::Null) => None,
        Some(Value::Number(n)) => Some(n.to_string()),
        Some(Value::String(s)) => Some(s.trim().to_owned()).filter(|s| !s.is_empty()),
        Some(_) => Some(String::new()),
    };
    let Some(s) = s else {
        r.missing("house_number");
        return None;
    };
    if s.len() > 20 || s.parse::<f64>().map_or(true, |n| !n.is_finite()) {
        r.add("house_number", "The house number must be a number.");
        return None;
    }
    Some(s)
}

/// Laravel Address::rules on one item. Errors use the bare field key (`zipcode`): Laravel
/// threw them out of the rule unprefixed and vimdesk-web's address form reads them so.
fn item(r: &mut Rules, v: &Map<String, Value>) -> Address {
    let main_address = match v.get("main_address") {
        None | Some(Value::Null) => false,
        Some(b) => r.boolean("main_address", b).unwrap_or_default(),
    };
    Address {
        address: field(r, "address", v.get("address"), 255, true),
        house_number: house_number(r, v.get("house_number")),
        house_number_addition: field(
            r,
            "house_number_addition",
            v.get("house_number_addition"),
            10,
            false,
        ),
        address2: field(r, "address2", v.get("address2"), 255, false),
        country: field(r, "country", v.get("country"), 255, true).map(|c| c.to_ascii_uppercase()),
        zipcode: field(r, "zipcode", v.get("zipcode"), 20, true),
        city: field(r, "city", v.get("city"), 255, true),
        state: field(r, "state", v.get("state"), 255, false),
        label: field(r, "label", v.get("label"), 55, false),
        main_address,
        kind: field(r, "type", v.get("type"), 255, false).or_else(|| Some("other".into())),
        attn: field(r, "attn", v.get("attn"), 255, false),
        phone_number: field(r, "phone_number", v.get("phone_number"), 255, false),
    }
}

/// `addresses` (Laravel MainAddressRule + `addresses.*` Address rule): an array of address
/// objects, at most one main. Like Laravel, only the first invalid item reports its errors.
pub fn validate_list(r: &mut Rules, value: Value) -> Vec<Address> {
    let Value::Array(items) = value else {
        r.add("addresses", "The addresses is not valid");
        return Vec::new();
    };
    // Laravel had no cap; 50 bounds row size and every response that embeds them.
    if items.len() > MAX_ADDRESSES {
        r.add(
            "addresses",
            format!("The addresses must not have more than {MAX_ADDRESSES} items."),
        );
        return Vec::new();
    }
    let mut out = Vec::with_capacity(items.len());
    for (i, v) in items.iter().enumerate() {
        let Value::Object(map) = v else {
            r.add(
                &format!("addresses.{i}"),
                format!("The addresses.{i} field is required."),
            );
            return out;
        };
        let mut own = Rules::default();
        let a = item(&mut own, map);
        if own.merge_into(r) {
            return out;
        }
        out.push(a);
    }
    if out.iter().filter(|a| a.main_address).count() > 1 {
        r.add("addresses", "There should be one main address");
    }
    out
}

/// Address Laravel's `defaultAddress()` picks: main, else first.
pub fn default_of(addresses: &[Address]) -> Option<&Address> {
    addresses
        .iter()
        .find(|a| a.main_address)
        .or(addresses.first())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn nl() -> Address {
        Address {
            address: Some("midwaystraat".into()),
            house_number: Some("11".into()),
            house_number_addition: Some("a".into()),
            zipcode: Some("1339 nm".into()),
            city: Some("almere".into()),
            country: Some("NL".into()),
            ..Address::default()
        }
    }

    #[test]
    fn known_country_uses_its_template() {
        assert_eq!(
            nl().formatted(),
            "Midwaystraat 11a\n1339 NM Almere\nNetherlands"
        );
    }

    #[test]
    fn attn_fills_recipient_line() {
        let a = Address {
            attn: Some("finance".into()),
            ..nl()
        };
        assert_eq!(
            a.formatted(),
            "Finance\nMidwaystraat 11a\n1339 NM Almere\nNetherlands"
        );
    }

    #[test]
    fn region_placeholders_and_country_name() {
        let us = Address {
            address: Some("main st".into()),
            house_number: Some("5".into()),
            zipcode: Some("10001".into()),
            city: Some("new york".into()),
            state: Some("ny".into()),
            country: Some("US".into()),
            ..Address::default()
        };
        assert_eq!(
            us.formatted(),
            "Main st 5\nNew york Ny 10001\nUnited States"
        );
    }

    #[test]
    fn known_country_without_template_uses_default_with_empty_region_line() {
        let xk = Address {
            country: Some("XK".into()),
            ..nl()
        };
        assert_eq!(xk.formatted(), "Midwaystraat 11a\n1339 NM Almere\n\nKosovo");
    }

    #[test]
    fn unknown_country_falls_back_to_non_region_layout() {
        let a = Address {
            country: Some("ZZ".into()),
            address2: Some("unit 4".into()),
            ..nl()
        };
        assert_eq!(
            a.formatted(),
            "midwaystraat 11a\nunit 4\n1339 NM, almere\nZZ"
        );
        let bare = Address {
            country: None,
            house_number: Some("0".into()),
            ..nl()
        };
        assert_eq!(bare.formatted(), "midwaystraat 1339 NM, almere\n");
    }

    #[test]
    fn street_trimmed_when_parts_missing() {
        let a = Address {
            house_number: None,
            house_number_addition: None,
            ..nl()
        };
        assert_eq!(a.formatted(), "Midwaystraat\n1339 NM Almere\nNetherlands");
    }

    #[test]
    fn search_format_is_compact_zipcode_and_number() {
        assert_eq!(nl().search_format(), "1339nm 11");
        assert_eq!(Address::default().search_format(), " ");
    }

    fn validate(
        v: Value,
    ) -> (
        Vec<Address>,
        std::collections::BTreeMap<String, Vec<String>>,
    ) {
        let mut r = Rules::default();
        let out = validate_list(&mut r, v);
        match r.into_result() {
            Ok(()) => (out, Default::default()),
            Err(crate::error::AppError::Validation(e)) => (out, e),
            Err(other) => panic!("{other:?}"),
        }
    }

    #[test]
    fn valid_list_normalized() {
        let (out, e) = validate(json!([{
            "address": " Midwaystraat ", "house_number": 11, "zipcode": "1339NM", "city": "Almere",
            "country": "nl", "main_address": "1", "phone_number": " 06 1 ", "state": ""
        }]));

        assert!(e.is_empty(), "{e:?}");
        let a = &out[0];
        assert_eq!(a.address.as_deref(), Some("Midwaystraat"));
        assert_eq!(a.house_number.as_deref(), Some("11"));
        assert_eq!(a.country.as_deref(), Some("NL"));
        assert_eq!(a.kind.as_deref(), Some("other"));
        assert_eq!(a.phone_number.as_deref(), Some("06 1"));
        assert_eq!(a.state, None);
        assert!(a.main_address);
    }

    #[test]
    fn item_errors_use_bare_keys_first_bad_item_only() {
        let (_, e) = validate(json!([
            {"address": "A", "house_number": "1", "zipcode": "1", "city": "C", "country": "NL"},
            {"house_number": "12a", "zipcode": "x".repeat(21), "country": "NL", "label": 5},
            {"house_number": "x"}
        ]));

        assert_eq!(e["address"], ["The address field is required."]);
        assert_eq!(e["house_number"], ["The house number must be a number."]);
        assert_eq!(
            e["zipcode"],
            ["The zipcode must not be greater than 20 characters."]
        );
        assert_eq!(e["city"], ["The city field is required."]);
        assert_eq!(e["label"], ["The label must be a string."]);
        assert_eq!(e.len(), 5, "{e:?}");
    }

    #[test]
    fn list_shape_and_single_main() {
        let ok = json!({"address": "A", "house_number": "1", "zipcode": "1", "city": "C", "country": "NL", "main_address": true});
        assert_eq!(
            validate(json!({})).1["addresses"],
            ["The addresses is not valid"]
        );
        assert_eq!(
            validate(json!([null])).1["addresses.0"],
            ["The addresses.0 field is required."]
        );
        assert_eq!(
            validate(json!([ok.clone(), ok])).1["addresses"],
            ["There should be one main address"]
        );
        assert!(validate(json!([])).1.is_empty());
    }

    #[test]
    fn default_is_main_else_first() {
        let main = Address {
            main_address: true,
            city: Some("B".into()),
            ..Address::default()
        };
        let first = Address {
            city: Some("A".into()),
            ..Address::default()
        };
        assert_eq!(default_of(&[first.clone(), main.clone()]), Some(&main));
        assert_eq!(default_of(std::slice::from_ref(&first)), Some(&first));
        assert_eq!(default_of(&[]), None);
    }

    #[test]
    fn stored_json_round_trips_and_view_adds_computed_keys() {
        let stored = serde_json::to_value(nl()).unwrap();
        let back: Address = serde_json::from_value(stored).unwrap();
        let view = serde_json::to_value(AddressView::from(&back)).unwrap();

        assert_eq!(view["type"], Value::Null);
        assert_eq!(
            view["formatted"],
            "Midwaystraat 11a\n1339 NM Almere\nNetherlands"
        );
        assert_eq!(view["search_format"], "1339nm 11");
        for key in ["reverse_charge", "shows_reverse_charge_notice", "vat_rates"] {
            assert_eq!(view[key], Value::Null, "{key}");
        }
    }
}
