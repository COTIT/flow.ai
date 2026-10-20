//! Shipment method request/response bodies. Keys follow Laravel ShipmentMethodResource and the
//! Store/UpdateShipmentMethodRequest rules (vimdesk-web shipment-method.store.js).

use chrono::{DateTime, Utc};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, present},
    modules::shipment_methods::repo::ShipmentMethodRow,
    money,
};

const TEXT_MAX: usize = 255;
/// More than the 249 ISO countries, so one price per country always fits.
const COUNTRIES_MAX: usize = 300;

/// Laravel `gt:min_weight`: ":attribute must be greater than :value" with the minimum.
pub fn weight_message(min: Decimal) -> String {
    format!("The max weight must be greater than {}.", min.normalize())
}

/// Laravel ShipmentMethodResource. Uncast decimal columns came out as 8-decimal strings and
/// the double weights as numbers; `can_be_selected` is a JSON bool here (Laravel sent 0/1).
#[derive(Serialize)]
pub struct ShipmentMethodResource {
    id: Uuid,
    name: String,
    carrier: String,
    shipping_costs: String,
    min_weight: f64,
    max_weight: f64,
    provider: Option<String>,
    price_impact_business: String,
    price_impact_consumer: String,
    provider_id: Option<String>,
    countries: Option<Value>,
    can_be_selected: bool,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    meta: Option<Value>,
}

fn column(mut d: Decimal) -> String {
    d.rescale(8);
    d.to_string()
}

impl From<ShipmentMethodRow> for ShipmentMethodResource {
    fn from(m: ShipmentMethodRow) -> Self {
        Self {
            id: m.id,
            name: m.name,
            carrier: m.carrier,
            shipping_costs: column(m.shipping_costs),
            min_weight: m.min_weight.to_f64().unwrap_or_default(),
            max_weight: m.max_weight.to_f64().unwrap_or_default(),
            provider: m.provider,
            price_impact_business: column(m.price_impact_business),
            price_impact_consumer: column(m.price_impact_consumer),
            provider_id: m.provider_id,
            countries: m.countries,
            can_be_selected: m.can_be_selected,
            created_at: m.created_at,
            last_updated_at: m.updated_at,
            meta: m.meta,
        }
    }
}

/// Create response: the new row's id.
#[derive(Serialize)]
pub struct ShipmentMethodCreated {
    pub shipment_method_id: Uuid,
}

/// GET list query. Unknown params (`sort_by`, `sort_direction`) ignored, like Laravel.
#[derive(Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub page: Option<String>,
    pub page_size: Option<String>,
}

fn amount(r: &mut Rules, field: &str, v: Option<Value>) -> Option<Decimal> {
    let v = r.required_value(field, v)?;
    r.decimal(field, &v, Decimal::ZERO, money::MAX)
}

fn impact(r: &mut Rules, field: &str, v: Option<Value>) -> Option<Decimal> {
    let v = r.required_value(field, v)?;
    r.decimal(field, &v, -Decimal::ONE_HUNDRED, Decimal::ONE_HUNDRED)
}

/// `[{name, country, price, lead_time_hours}]`, rebuilt from the validated keys only (others
/// dropped): name text <=255, country ISO alpha-2 uppercased, price a number >= 0 (kept as
/// sent, trimmed), lead time a whole number >= 0.
fn countries(r: &mut Rules, v: Value) -> Option<Value> {
    let Value::Array(items) = v else {
        r.add("countries", "The countries must be an array.");
        return None;
    };
    if items.len() > COUNTRIES_MAX {
        let msg = format!("The countries must not have more than {COUNTRIES_MAX} items.");
        r.add("countries", msg);
        return None;
    }
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.into_iter().enumerate() {
        let Value::Object(mut c) = item else {
            r.add(
                &format!("countries.{i}"),
                format!("The countries.{i} must be an array."),
            );
            continue;
        };
        let mut kept = Map::new();
        if let Some(name) = filled(c.remove("name")) {
            let field = format!("countries.{i}.name");
            match name {
                Value::String(s) => {
                    r.max_chars(&field, s.trim(), TEXT_MAX);
                    kept.insert("name".into(), Value::String(s.trim().to_owned()));
                }
                _ => r.add(&field, format!("The {field} must be a string.")),
            }
        }
        if let Some(code) = filled(c.remove("country")) {
            let field = format!("countries.{i}.country");
            if let Some(code) = r.country(&field, code.as_str().unwrap_or_default()) {
                kept.insert("country".into(), Value::String(code));
            }
        }
        if let Some(price) = filled(c.remove("price")) {
            let field = format!("countries.{i}.price");
            if r.decimal(&field, &price, Decimal::ZERO, money::MAX)
                .is_some()
            {
                let price = match price {
                    Value::String(s) => Value::String(s.trim().to_owned()),
                    other => other,
                };
                kept.insert("price".into(), price);
            }
        }
        if let Some(hours) = filled(c.remove("lead_time_hours")) {
            let field = format!("countries.{i}.lead_time_hours");
            if let Some(h) = r.integer(&field, &hours, 0, i64::MAX) {
                kept.insert("lead_time_hours".into(), Value::from(h));
            }
        }
        out.push(Value::Object(kept));
    }
    Some(Value::Array(out))
}

fn selectable(r: &mut Rules, v: Option<Value>) -> bool {
    filled(v)
        .and_then(|v| r.boolean("can_be_selected", &v))
        .unwrap_or_default()
}

/// POST /shipment-methods. Unlike Laravel, price impacts, countries and `can_be_selected`
/// are kept on create (Laravel's store rules silently dropped them).
#[derive(Deserialize)]
pub struct CreateShipmentMethodRequest {
    name: Option<String>,
    carrier: Option<String>,
    shipping_costs: Option<Value>,
    min_weight: Option<Value>,
    max_weight: Option<Value>,
    price_impact_business: Option<Value>,
    price_impact_consumer: Option<Value>,
    countries: Option<Value>,
    can_be_selected: Option<Value>,
    meta: Option<Value>,
}

/// Validated create input.
pub struct NewShipmentMethod {
    pub name: String,
    pub carrier: String,
    pub shipping_costs: Decimal,
    pub min_weight: Decimal,
    pub max_weight: Decimal,
    pub price_impact_business: Decimal,
    pub price_impact_consumer: Decimal,
    pub countries: Option<Value>,
    pub can_be_selected: bool,
    pub meta: Option<Value>,
}

impl Validate for CreateShipmentMethodRequest {
    type Valid = NewShipmentMethod;

    fn validate(self, r: &mut Rules) -> Option<NewShipmentMethod> {
        let min_weight = amount(r, "min_weight", self.min_weight);
        let max_weight = amount(r, "max_weight", self.max_weight);
        if let (Some(min), Some(max)) = (min_weight, max_weight) {
            r.check("max_weight", max > min, weight_message(min));
        }
        let optional_impact = |r: &mut Rules, field: &str, v: Option<Value>| {
            filled(v).map_or(Some(Decimal::ZERO), |v| impact(r, field, Some(v)))
        };
        Some(NewShipmentMethod {
            name: r.text("name", self.name, TEXT_MAX),
            carrier: r.text("carrier", self.carrier, TEXT_MAX),
            shipping_costs: amount(r, "shipping_costs", self.shipping_costs).unwrap_or_default(),
            min_weight: min_weight.unwrap_or_default(),
            max_weight: max_weight.unwrap_or_default(),
            price_impact_business: optional_impact(
                r,
                "price_impact_business",
                self.price_impact_business,
            )
            .unwrap_or_default(),
            price_impact_consumer: optional_impact(
                r,
                "price_impact_consumer",
                self.price_impact_consumer,
            )
            .unwrap_or_default(),
            countries: filled(self.countries).and_then(|v| countries(r, v)),
            can_be_selected: selectable(r, self.can_be_selected),
            meta: r.meta(self.meta).flatten(),
        })
    }
}

/// POST /shipment-methods/{id}. Laravel update rules: price impacts and `countries` required
/// (an empty array is accepted here; Laravel's `required` refused it), the rest patches.
#[derive(Deserialize)]
pub struct UpdateShipmentMethodRequest {
    #[serde(default, deserialize_with = "present")]
    name: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    carrier: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    shipping_costs: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    min_weight: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    max_weight: Option<Option<Value>>,
    price_impact_business: Option<Value>,
    price_impact_consumer: Option<Value>,
    countries: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    can_be_selected: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    meta: Option<Option<Value>>,
}

/// Validated update; `None` keeps the stored value.
pub struct ShipmentMethodPatch {
    pub name: Option<String>,
    pub carrier: Option<String>,
    pub shipping_costs: Option<Decimal>,
    pub min_weight: Option<Decimal>,
    pub max_weight: Option<Decimal>,
    pub price_impact_business: Decimal,
    pub price_impact_consumer: Decimal,
    pub countries: Value,
    pub can_be_selected: Option<bool>,
    pub meta: Option<Option<Value>>,
}

impl Validate for UpdateShipmentMethodRequest {
    type Valid = ShipmentMethodPatch;

    fn validate(self, r: &mut Rules) -> Option<ShipmentMethodPatch> {
        let min_weight = self.min_weight.and_then(|v| amount(r, "min_weight", v));
        let max_weight = self.max_weight.and_then(|v| amount(r, "max_weight", v));
        if let (Some(min), Some(max)) = (min_weight, max_weight) {
            r.check("max_weight", max > min, weight_message(min));
        }
        let countries = match self.countries {
            Some(v @ Value::Array(_)) => countries(r, v),
            other => r
                .required_value("countries", other)
                .and_then(|v| countries(r, v)),
        };
        Some(ShipmentMethodPatch {
            name: r.patch_text("name", self.name, TEXT_MAX),
            carrier: r.patch_text("carrier", self.carrier, TEXT_MAX),
            shipping_costs: self
                .shipping_costs
                .and_then(|v| amount(r, "shipping_costs", v)),
            min_weight,
            max_weight,
            price_impact_business: impact(r, "price_impact_business", self.price_impact_business)
                .unwrap_or_default(),
            price_impact_consumer: impact(r, "price_impact_consumer", self.price_impact_consumer)
                .unwrap_or_default(),
            countries: countries.unwrap_or_default(),
            can_be_selected: self.can_be_selected.map(|v| selectable(r, v)),
            meta: self.meta.and_then(|v| r.meta(v)),
        })
    }
}
