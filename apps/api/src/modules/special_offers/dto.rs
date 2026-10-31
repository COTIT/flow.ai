//! Special offer request/response bodies. Cross-field rules run in the service on the merged
//! row.
// Laravel Store/UpdateSpecialOfferRequest field rules, SpecialOfferResource.

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, human, present},
    modules::special_offers::repo::OfferRow,
    money::{MAX, Zero, format_decimals},
};

/// Legacy `type` values (Laravel SpecialOfferType).
pub const TYPES: [&str; 3] = ["exact", "fixed", "percentage"];
/// Laravel SpecialOfferRuleType.
pub const RULES: [&str; 8] = [
    "bogo",
    "brand_discount",
    "bundle_price",
    "category_discount",
    "cross_product_discount",
    "global_discount",
    "quantity_based",
    "supplier_discount",
];
/// Laravel DiscountType.
pub const DISCOUNT_TYPES: [&str; 4] = ["bundle_price", "fixed", "free", "percentage"];

// Laravel SpecialOfferResource.
#[derive(Serialize, ToSchema)]
#[schema(as = SpecialOfferResource, description = "A special offer on one product, or a rule over brands, suppliers or categories. Decimals are strings (4 places, trimmed).")]
pub struct OfferResource {
    id: Uuid,
    #[schema(required = true)]
    from_date: Option<NaiveDate>,
    #[schema(required = true)]
    to_date: Option<NaiveDate>,
    // Laravel left the tinyint uncast: 0/1.
    always_active: i32,
    value: String,
    #[serde(rename = "type")]
    kind: String,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    discount_rule_type: String,
    #[schema(required = true)]
    condition_quantity: Option<i32>,
    #[schema(required = true)]
    discount_quantity: Option<i32>,
    #[schema(required = true)]
    condition_brand: Option<String>,
    #[schema(required = true)]
    condition_supplier_relation_id: Option<Uuid>,
    #[schema(required = true)]
    condition_category_id: Option<Uuid>,
    #[schema(required = true)]
    target_brand: Option<String>,
    #[schema(required = true)]
    target_supplier_relation_id: Option<Uuid>,
    #[schema(required = true)]
    target_category_id: Option<Uuid>,
    #[schema(required = true)]
    target_product_id: Option<Uuid>,
    #[schema(required = true)]
    product_id: Option<Uuid>,
    discount_value: String,
    discount_type: String,
    priority: i32,
    apply_to_cheapest: bool,
    /// Always empty.
    // Laravel's relationships were never loaded: an empty array.
    #[schema(value_type = Vec<Object>, max_items = 0)]
    relationships: Value,
}

impl From<&OfferRow> for OfferResource {
    fn from(o: &OfferRow) -> Self {
        let fmt = |v: Decimal| format_decimals(Some(v), 4, true, Zero::Falsy);
        Self {
            id: o.id,
            from_date: o.from_date,
            to_date: o.to_date,
            always_active: i32::from(o.always_active),
            value: fmt(o.value),
            kind: o.kind.clone(),
            meta: o.meta.clone(),
            created_at: o.created_at,
            last_updated_at: o.updated_at,
            discount_rule_type: o
                .discount_rule_type
                .clone()
                .unwrap_or_else(|| "legacy".into()),
            condition_quantity: o.condition_quantity,
            discount_quantity: o.discount_quantity,
            condition_brand: o.condition_brand.clone(),
            condition_supplier_relation_id: o.condition_supplier_relation_id,
            condition_category_id: o.condition_category_id,
            target_brand: o.target_brand.clone(),
            target_supplier_relation_id: o.target_supplier_relation_id,
            target_category_id: o.target_category_id,
            target_product_id: o.target_product_id,
            product_id: o.product_id,
            discount_value: fmt(o.discount_value.unwrap_or(o.value)),
            discount_type: o.discount_type.clone().unwrap_or_else(|| o.kind.clone()),
            priority: o.priority,
            apply_to_cheapest: o.apply_to_cheapest,
            relationships: json!([]),
        }
    }
}

/// Create response.
#[derive(Serialize, ToSchema)]
pub struct OfferCreated {
    pub special_offer_id: Uuid,
}

/// GET list query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
    /// A rule type, or `legacy` for offers without one.
    pub discount_rule_type: Option<String>,
}

/// POST body for create and update. Every key: absent = keep, null/blank = clear.
#[derive(Deserialize, ToSchema)]
#[schema(as = SpecialOfferRequest, description = "Offer fields. Keys left out keep their value; null or blank clears. Decimals as strings (`\"12.50\"`); JSON numbers also work.")]
pub struct OfferRequest {
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>)]
    product_id: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<chrono::NaiveDate>)]
    from_date: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<chrono::NaiveDate>)]
    to_date: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<bool>)]
    always_active: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    value: Option<Option<Value>>,
    #[serde(default, rename = "type", deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    kind: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    discount_rule_type: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<i32>)]
    condition_quantity: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<i32>)]
    discount_quantity: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    condition_brand: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>)]
    condition_supplier_relation_id: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>)]
    condition_category_id: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    target_brand: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>)]
    target_supplier_relation_id: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>)]
    target_category_id: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>)]
    target_product_id: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    discount_value: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    discount_type: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<i32>)]
    priority: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<bool>)]
    apply_to_cheapest: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
    meta: Option<Option<Value>>,
}

/// Field-checked patch: `None` = not sent, `Some(None)` = sent empty.
#[derive(Default)]
pub struct OfferPatch {
    pub product_id: Option<Option<Uuid>>,
    pub from_date: Option<Option<NaiveDate>>,
    pub to_date: Option<Option<NaiveDate>>,
    pub always_active: Option<Option<bool>>,
    pub value: Option<Option<Decimal>>,
    pub kind: Option<Option<String>>,
    pub discount_rule_type: Option<Option<String>>,
    pub condition_quantity: Option<Option<i32>>,
    pub discount_quantity: Option<Option<i32>>,
    pub condition_brand: Option<Option<String>>,
    pub condition_supplier_relation_id: Option<Option<Uuid>>,
    pub condition_category_id: Option<Option<Uuid>>,
    pub target_brand: Option<Option<String>>,
    pub target_supplier_relation_id: Option<Option<Uuid>>,
    pub target_category_id: Option<Option<Uuid>>,
    pub target_product_id: Option<Option<Uuid>>,
    pub discount_value: Option<Option<Decimal>>,
    pub discount_type: Option<Option<String>>,
    pub priority: Option<Option<i32>>,
    pub apply_to_cheapest: Option<Option<bool>>,
    pub meta: Option<Option<Value>>,
}

impl OfferPatch {
    /// Laravel `hasAdvancedFields`: a filled advanced key in the request.
    pub fn has_advanced(&self) -> bool {
        matches!(self.discount_type, Some(Some(_)))
            || matches!(self.discount_value, Some(Some(_)))
            || matches!(self.condition_quantity, Some(Some(_)))
            || matches!(self.condition_brand, Some(Some(_)))
            || matches!(self.condition_supplier_relation_id, Some(Some(_)))
            || matches!(self.condition_category_id, Some(Some(_)))
            || matches!(self.target_brand, Some(Some(_)))
            || matches!(self.target_supplier_relation_id, Some(Some(_)))
            || matches!(self.target_category_id, Some(Some(_)))
            || matches!(self.target_product_id, Some(Some(_)))
            || matches!(self.priority, Some(Some(_)))
            || matches!(self.apply_to_cheapest, Some(Some(_)))
    }

    /// A field that sets the promotional price was sent (Laravel re-froze `base_price`).
    pub fn touches_price(&self) -> bool {
        self.value.is_some()
            || self.kind.is_some()
            || self.discount_value.is_some()
            || self.discount_type.is_some()
    }
}

impl Validate for OfferRequest {
    type Valid = OfferPatch;

    fn validate(self, r: &mut Rules) -> Option<OfferPatch> {
        Some(OfferPatch {
            product_id: self.product_id.map(|v| uuid(r, "product_id", v, true)),
            from_date: self.from_date.map(|v| date(r, "from_date", v)),
            to_date: self.to_date.map(|v| date(r, "to_date", v)),
            always_active: self.always_active.map(|v| boolean(r, "always_active", v)),
            value: self.value.map(|v| amount(r, "value", v)),
            kind: self.kind.map(|v| choice(r, "type", v, &TYPES)),
            discount_rule_type: self
                .discount_rule_type
                .map(|v| choice(r, "discount_rule_type", v, &RULES)),
            condition_quantity: self
                .condition_quantity
                .map(|v| int(r, "condition_quantity", v, 1)),
            discount_quantity: self
                .discount_quantity
                .map(|v| int(r, "discount_quantity", v, 1)),
            condition_brand: self.condition_brand.map(|v| text(r, "condition_brand", v)),
            condition_supplier_relation_id: self
                .condition_supplier_relation_id
                .map(|v| uuid(r, "condition_supplier_relation_id", v, false)),
            condition_category_id: self
                .condition_category_id
                .map(|v| uuid(r, "condition_category_id", v, false)),
            target_brand: self.target_brand.map(|v| text(r, "target_brand", v)),
            target_supplier_relation_id: self
                .target_supplier_relation_id
                .map(|v| uuid(r, "target_supplier_relation_id", v, false)),
            target_category_id: self
                .target_category_id
                .map(|v| uuid(r, "target_category_id", v, false)),
            target_product_id: self
                .target_product_id
                .map(|v| uuid(r, "target_product_id", v, false)),
            discount_value: self.discount_value.map(|v| amount(r, "discount_value", v)),
            discount_type: self
                .discount_type
                .map(|v| choice(r, "discount_type", v, &DISCOUNT_TYPES)),
            priority: self.priority.map(|v| int(r, "priority", v, 0)),
            apply_to_cheapest: self
                .apply_to_cheapest
                .map(|v| boolean(r, "apply_to_cheapest", v)),
            meta: self.meta.and_then(|v| r.meta(v)),
        })
    }
}

/// The product ids use Laravel ProductExistRule's text; other refs check the format here and
/// existence in the service.
fn uuid(r: &mut Rules, field: &str, v: Option<Value>, product: bool) -> Option<Uuid> {
    let v = filled(v)?;
    let id = v.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok());
    if id.is_none() {
        let msg = if product {
            format!("The {} does not exist", human(field))
        } else {
            format!("The {} must be a valid UUID.", human(field))
        };
        r.add(field, msg);
    }
    id
}

/// Laravel `date`: `Y-m-d`, `Y-m-d H:i:s` or an RFC 3339 timestamp, kept as its UTC date
/// (Laravel stored a timestamp and printed its date).
fn date(r: &mut Rules, field: &str, v: Option<Value>) -> Option<NaiveDate> {
    let v = filled(v)?;
    let s = v.as_str().map(str::trim).unwrap_or_default();
    let d = NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|t| t.date())
        })
        .or_else(|| {
            DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|t| t.with_timezone(&Utc).date_naive())
        })
        // Postgres `date` has no year 0.
        .filter(|d| (1..=9999).contains(&chrono::Datelike::year(d)));
    if d.is_none() {
        r.add(field, format!("The {} is not a valid date.", human(field)));
    }
    d
}

fn boolean(r: &mut Rules, field: &str, v: Option<Value>) -> Option<bool> {
    r.boolean(field, &filled(v)?)
}

/// Laravel `numeric|min:0`, within the column range.
fn amount(r: &mut Rules, field: &str, v: Option<Value>) -> Option<Decimal> {
    r.decimal(field, &filled(v)?, Decimal::ZERO, MAX)
}

fn int(r: &mut Rules, field: &str, v: Option<Value>, min: i64) -> Option<i32> {
    let n = r.integer(field, &filled(v)?, min, i64::from(i32::MAX))?;
    i32::try_from(n).ok()
}

/// Laravel EnumExist: matched lowercased, stored lowercase.
fn choice(r: &mut Rules, field: &str, v: Option<Value>, allowed: &[&str]) -> Option<String> {
    let v = filled(v)?;
    let raw = v.as_str().map_or_else(|| v.to_string(), str::to_owned);
    let lower = raw.trim().to_lowercase();
    if allowed.contains(&lower.as_str()) {
        Some(lower)
    } else {
        r.add(
            field,
            format!("The selected {}: {raw} is invalid.", human(field)),
        );
        None
    }
}

/// Laravel `string|max:255`.
fn text(r: &mut Rules, field: &str, v: Option<Value>) -> Option<String> {
    match filled(v)? {
        Value::String(s) => {
            let s = s.trim().to_owned();
            r.max_chars(field, &s, 255);
            Some(s)
        }
        _ => {
            r.add(field, format!("The {} must be a string.", human(field)));
            None
        }
    }
}
