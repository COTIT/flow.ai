//! Custom type request/response bodies. Keys follow Laravel CustomTypeResource and the
//! Store/Update/ListCustomTypeRequest rules, including ValidateCustomTypeProperties.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    error::AppError,
    http::{
        json::{Rules, Validate, present},
        list::flag,
    },
    modules::custom_types::{domains, repo::CustomTypeRow},
};

const NAME_MAX: usize = 55;
/// `icon` / `color` on activity types.
const TEXT_MAX: usize = 255;
/// Laravel `rules.CustomTypeUnique` / `rules.CustomStatusUnique` for attribute `name`.
pub const NOT_UNIQUE: &str = "The name is not unique";

// Laravel CustomTypeResource.
/// A user-defined type (relation type, activity type, ...) for one domain.
#[derive(Serialize, ToSchema)]
pub struct CustomTypeResource {
    id: Uuid,
    name: String,
    /// Domain the type belongs to, lowercase (`relation`, `activity`, ...).
    #[schema(example = "relation")]
    domain: String,
    /// Domain-specific settings, e.g. `{"is_supplier": false}` for `relation`.
    #[schema(value_type = Option<Object>, required = true)]
    properties: Option<Value>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
}

impl From<CustomTypeRow> for CustomTypeResource {
    fn from(t: CustomTypeRow) -> Self {
        Self {
            id: t.id,
            name: t.name,
            domain: t.domain,
            properties: t.properties,
            created_at: t.created_at,
            last_updated_at: t.updated_at,
            meta: t.meta,
        }
    }
}

/// Create response: the new row's id.
#[derive(Serialize, ToSchema)]
pub struct CustomTypeCreated {
    pub custom_type_id: Uuid,
}

/// Laravel EnumExist(Domains): any case in, lowercase stored. Shared with custom statuses.
pub fn domain(r: &mut Rules, value: Option<String>) -> String {
    let d = r.text("domain", value, usize::MAX);
    if !d.is_empty() && !domains::known(&d) {
        r.add("domain", format!("The selected domain: {d} is invalid."));
    }
    d.to_ascii_lowercase()
}

/// Optional `domain` query filter, lowercased; 422 when unknown.
pub fn domain_filter(value: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(d) = value.map(str::trim).filter(|d| !d.is_empty()) else {
        return Ok(None);
    };
    let mut r = Rules::default();
    let d = domain(&mut r, Some(d.to_owned()));
    r.into_result()?;
    Ok(Some(d))
}

// GET list query (Laravel ListCustomTypeRequest). Unknown params ignored.
/// GET list query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// Search on name.
    pub q: Option<String>,
    /// Only this domain, any case; unknown domain is 422.
    pub domain: Option<String>,
    /// `hour_registration_type` only: filter on that property.
    #[param(value_type = Option<bool>)]
    pub default_for_work_order: Option<String>,
    /// `hour_registration_type` only: filter on that property.
    #[param(value_type = Option<bool>)]
    pub is_leave_hours_type: Option<String>,
    /// `relation` only: filter on that property.
    #[param(value_type = Option<bool>)]
    pub is_supplier: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

/// A flag filter as the `properties->>key` text it must equal; only for its own domain.
pub fn flag_filter(value: Option<&str>, applies: bool) -> Option<&'static str> {
    value
        .filter(|_| applies)
        .map(|v| if flag(v) { "true" } else { "false" })
}

/// Strict JSON bool at `properties.<key>`, else Laravel's "<key words> must be true or false".
fn strict_bool(r: &mut Rules, key: &str, v: &Value) -> Option<bool> {
    let b = v.as_bool();
    if b.is_none() {
        let words = key.replace('_', " ");
        r.add(
            &format!("properties.{key}"),
            format!("{words} must be true or false"),
        );
    }
    b
}

/// Laravel ValidateCustomTypeProperties + the request's `validated()` override: which
/// `properties` a domain keeps. `given` None = key absent from the body.
pub fn properties(r: &mut Rules, domain: &str, given: Option<Value>) -> Option<Value> {
    let given = given.filter(|v| !v.is_null());
    let obj = match &given {
        None => None,
        Some(Value::Object(o)) => Some(o),
        Some(_) => {
            if matches!(
                domain,
                "relation" | "activity" | "product_return_reason" | "hour_registration_type"
            ) {
                r.add("properties", "The properties must be an array.");
            }
            return None;
        }
    };
    let get = |key: &str| obj.and_then(|o| o.get(key)).filter(|v| !v.is_null());
    match domain {
        "relation" => {
            let supplier = get("is_supplier").and_then(|v| strict_bool(r, "is_supplier", v));
            Some(json!({ "is_supplier": supplier.unwrap_or(false) }))
        }
        "activity" => {
            let ticket = get("is_ticket_type").and_then(|v| strict_bool(r, "is_ticket_type", v));
            let mut text = |key: &str| match get(key).and_then(Value::as_str) {
                Some(s) => {
                    r.max_chars(&format!("properties.{key}"), s.trim(), TEXT_MAX);
                    Value::String(s.trim().to_owned())
                }
                None => Value::Bool(false),
            };
            let (icon, color) = (text("icon"), text("color"));
            Some(json!({
                "is_ticket_type": ticket.unwrap_or(false),
                "icon": icon,
                "color": color,
            }))
        }
        "product_return_reason" => {
            obj?;
            let field = "properties.discount_percentage";
            let Some(v) = get("discount_percentage") else {
                r.add(field, "discount percentage is required");
                return None;
            };
            // Plain decimals only, like PHP is_numeric (rust_decimal alone takes "1_0", "1e2").
            let plain = |t: &str| {
                !t.is_empty()
                    && t.bytes()
                        .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'+'))
            };
            let n = match v {
                Value::Number(n) => Some(n.to_string()),
                Value::String(s) => Some(s.trim().to_owned()),
                _ => None,
            }
            .filter(|t| plain(t))
            .and_then(|t| Decimal::from_str(&t).ok());
            match n {
                None => r.add(field, "discount percentage must be numeric"),
                Some(n) if n < Decimal::ZERO || n > Decimal::ONE_HUNDRED => {
                    r.add(field, "discount percentage must be between 0 and 100")
                }
                Some(_) => {}
            }
            // A number stays a number; text is stored normalized (" 12.50 " -> "12.5").
            let kept = match (v, n) {
                (Value::String(_), Some(n)) => Value::String(n.normalize().to_string()),
                (v, _) => v.clone(),
            };
            Some(json!({ "discount_percentage": kept }))
        }
        "hour_registration_type" => {
            obj?;
            let mut kept = Map::new();
            for key in [
                "default_for_work_order",
                "is_leave_hours_type",
                "is_sick_hours_type",
            ] {
                if let Some(b) = get(key).and_then(|v| strict_bool(r, key, v)) {
                    kept.insert(key.into(), Value::Bool(b));
                }
            }
            Some(Value::Object(kept))
        }
        _ => None,
    }
}

/// POST /custom-types.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "New custom type. `name` is unique within its domain. `properties` keys \
        depend on the domain: `relation` {is_supplier}, `activity` {is_ticket_type, icon, \
        color}, `product_return_reason` {discount_percentage} (required), \
        `hour_registration_type` {default_for_work_order, is_leave_hours_type, \
        is_sick_hours_type}; other domains ignore it."
)]
pub struct CreateCustomTypeRequest {
    #[schema(value_type = String, max_length = 55)]
    name: Option<String>,
    /// Domain, any case.
    #[schema(value_type = String, example = "relation")]
    domain: Option<String>,
    #[schema(value_type = Option<Object>)]
    properties: Option<Value>,
    #[schema(value_type = Option<Object>)]
    meta: Option<Value>,
}

/// Validated create input.
pub struct NewCustomType {
    pub name: String,
    pub domain: String,
    pub properties: Option<Value>,
    pub meta: Option<Value>,
}

impl Validate for CreateCustomTypeRequest {
    type Valid = NewCustomType;

    fn validate(self, r: &mut Rules) -> Option<NewCustomType> {
        let domain = domain(r, self.domain);
        Some(NewCustomType {
            name: r.text("name", self.name, NAME_MAX),
            properties: properties(r, &domain, self.properties),
            domain,
            meta: r.meta(self.meta).flatten(),
        })
    }
}

// POST /custom-types/{id}. The stored domain is kept (Laravel forced it); `properties` is
// checked against that domain by the service. Absent = keep.
/// POST /custom-types/{id}.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Changes to a custom type. The domain can't change; `properties` follows \
        the same per-domain rules as create. Leave a field out to keep it."
)]
pub struct UpdateCustomTypeRequest {
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = String, required = false, max_length = 55)]
    name: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
    properties: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
    meta: Option<Option<Value>>,
}

/// Validated update; `None` keeps the stored value.
pub struct CustomTypePatch {
    pub name: Option<String>,
    /// Raw; Some(None) = sent as null.
    pub properties: Option<Option<Value>>,
    pub meta: Option<Option<Value>>,
}

impl Validate for UpdateCustomTypeRequest {
    type Valid = CustomTypePatch;

    fn validate(self, r: &mut Rules) -> Option<CustomTypePatch> {
        Some(CustomTypePatch {
            name: r.patch_text("name", self.name, NAME_MAX),
            properties: self.properties,
            meta: self.meta.and_then(|v| r.meta(v)),
        })
    }
}
