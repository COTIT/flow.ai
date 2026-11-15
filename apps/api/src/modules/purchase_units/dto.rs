//! Purchase unit request envelopes and resource.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::http::json::{Rules, Validate};

/// Allowed measurement categories, as stored.
pub const CATEGORIES: [&str; 6] = ["mass", "length", "area", "volume", "count", "packaging"];
/// Smallest and largest ratio a `numeric(20,8)` column holds exactly.
const MIN_RATIO: Decimal = Decimal::from_parts(1, 0, 0, false, 8);
const MAX_RATIO: Decimal = Decimal::from_parts(1661992959, 1808227885, 5, false, 8);

/// Base-unit category per product unit (masterplan §5). Box needs a content unit first.
pub fn category(unit: &str) -> Option<&'static str> {
    Some(match unit {
        "kg" | "gram" => "mass",
        "cm" | "m" => "length",
        "m2" => "area",
        "liter" | "m3" => "volume",
        "pc" | "single" => "count",
        "box" => "packaging",
        _ => return None,
    })
}

/// `C` envelope: POST /products/{product}/purchase-units.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatePurchaseUnitRequest {
    /// Client-made UUID; the permanent identity of this create.
    #[schema(value_type = Uuid)]
    client_ref: Option<String>,
    #[schema(value_type = CreatePurchaseUnitPayload)]
    payload: Option<Value>,
}

/// Fields of a new purchase unit.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatePurchaseUnitPayload {
    /// Supplier unit name, unique per product ignoring case, 1-50 characters.
    #[schema(example = "bag")]
    code: Option<String>,
    /// Must match the product unit's category: mass, length, area, volume or count.
    #[schema(example = "mass")]
    measurement_category: Option<String>,
    /// Base units in one input unit: exact decimal string, above 0, at most 8 decimals.
    #[schema(value_type = String, example = "25")]
    base_units_per_input_unit: Option<Value>,
}

/// Validated create.
pub struct NewPurchaseUnit {
    pub client_ref: Uuid,
    pub code: String,
    pub measurement_category: String,
    pub ratio: Decimal,
}

/// `A` envelope: PATCH /products/{product}/purchase-units/{unit}.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdatePurchaseUnitRequest {
    #[schema(value_type = Uuid)]
    client_ref: Option<String>,
    /// The version the client last read; another one is 409.
    #[schema(value_type = i32, minimum = 1)]
    expected_version: Option<Value>,
    #[schema(value_type = UpdatePurchaseUnitPayload)]
    payload: Option<Value>,
}

/// The only editable field.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdatePurchaseUnitPayload {
    active: Option<Value>,
}

/// Validated update.
pub struct PurchaseUnitChange {
    pub client_ref: Uuid,
    pub expected_version: i32,
    pub active: bool,
}

fn client_ref(r: &mut Rules, value: Option<String>) -> Uuid {
    r.required("client_ref", value)
        .and_then(|v| {
            let id = Uuid::parse_str(&v).ok();
            r.check(
                "client_ref",
                id.is_some(),
                "The client ref must be a valid UUID.",
            );
            id
        })
        .unwrap_or_default()
}

/// Decode a nested payload object, recording its errors under `payload.<field>`.
fn payload<T: serde::de::DeserializeOwned>(r: &mut Rules, value: Option<Value>) -> Option<T> {
    match value {
        Some(v @ Value::Object(_)) => match serde_json::from_value(v) {
            Ok(p) => Some(p),
            Err(e) => {
                r.add("payload", format!("The payload is invalid: {e}."));
                None
            }
        },
        _ => {
            r.missing("payload");
            None
        }
    }
}

impl Validate for CreatePurchaseUnitRequest {
    type Valid = NewPurchaseUnit;

    fn validate(self, r: &mut Rules) -> Option<NewPurchaseUnit> {
        let client_ref = client_ref(r, self.client_ref);
        let p: CreatePurchaseUnitPayload = payload(r, self.payload)?;
        let code = r.required("payload.code", p.code).unwrap_or_default();
        r.max_chars("payload.code", &code, 50);
        let measurement_category = r
            .required("payload.measurement_category", p.measurement_category)
            .unwrap_or_default();
        if !measurement_category.is_empty() {
            r.check(
                "payload.measurement_category",
                CATEGORIES.contains(&measurement_category.as_str()),
                "The selected measurement category is invalid.",
            );
        }
        let ratio = match p.base_units_per_input_unit {
            Some(v @ Value::String(_)) => r.decimal(
                "payload.base_units_per_input_unit",
                &v,
                MIN_RATIO,
                MAX_RATIO,
            ),
            _ => {
                r.add(
                    "payload.base_units_per_input_unit",
                    "The base units per input unit must be a decimal string.",
                );
                None
            }
        };
        Some(NewPurchaseUnit {
            client_ref,
            code,
            measurement_category,
            ratio: ratio.unwrap_or_default(),
        })
    }
}

impl Validate for UpdatePurchaseUnitRequest {
    type Valid = PurchaseUnitChange;

    fn validate(self, r: &mut Rules) -> Option<PurchaseUnitChange> {
        let client_ref = client_ref(r, self.client_ref);
        let expected_version = match self.expected_version {
            Some(v) => r.integer("expected_version", &v, 1, i64::from(i32::MAX)),
            None => {
                r.missing("expected_version");
                None
            }
        };
        let p: UpdatePurchaseUnitPayload = payload(r, self.payload)?;
        let active = match p.active {
            Some(Value::Bool(b)) => Some(b),
            _ => {
                r.add("payload.active", "The active field must be true or false.");
                None
            }
        };
        Some(PurchaseUnitChange {
            client_ref,
            expected_version: expected_version
                .and_then(|v| i32::try_from(v).ok())
                .unwrap_or(1),
            active: active.unwrap_or_default(),
        })
    }
}

/// One purchase unit.
#[derive(Serialize, ToSchema, Clone)]
pub struct PurchaseUnitResource {
    pub id: Uuid,
    pub product_id: Uuid,
    pub code: String,
    /// mass, length, area, volume or count.
    pub measurement_category: String,
    /// The product's unit this converts into.
    pub base_unit: Option<String>,
    /// Exact decimal string, at most 8 decimals.
    #[schema(value_type = String, example = "25")]
    pub base_units_per_input_unit: String,
    pub active: bool,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_product_unit_maps_to_one_category() {
        let map = [
            ("kg", "mass"),
            ("gram", "mass"),
            ("cm", "length"),
            ("m", "length"),
            ("m2", "area"),
            ("liter", "volume"),
            ("m3", "volume"),
            ("pc", "count"),
            ("single", "count"),
            ("box", "packaging"),
        ];
        for (unit, cat) in map {
            assert_eq!(category(unit), Some(cat), "{unit}");
        }
        assert_eq!(category("ton"), None);
    }

    #[test]
    fn ratio_bounds_fit_numeric_20_8() {
        assert_eq!(MIN_RATIO.to_string(), "0.00000001");
        assert_eq!(MAX_RATIO.to_string(), "999999999999.99999999");
    }
}
