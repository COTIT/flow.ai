//! Discount request/response bodies (Laravel Store/UpdateDiscountRequest, DiscountResource).

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, human, present},
    modules::discounts::repo::DiscountRow,
    money::{Zero, format_decimals},
};

const NAME_MAX: usize = 150;
const CODE_MAX: usize = 255;
const DESCRIPTION_MAX: usize = 10_000;

/// Laravel DiscountResource.
#[derive(Serialize)]
pub struct DiscountResource {
    id: Uuid,
    name: String,
    active: bool,
    discount_product_code: String,
    percentage: String,
    valid_from: Option<String>,
    valid_to: Option<String>,
    monthly: Option<Value>,
    yearly: Option<Value>,
    seasonal: Option<Value>,
    meta: Option<Value>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
}

impl From<DiscountRow> for DiscountResource {
    fn from(d: DiscountRow) -> Self {
        Self {
            id: d.id,
            name: d.name,
            active: d.active,
            discount_product_code: d.discount_product_code,
            percentage: format_decimals(Some(d.percentage), 2, true, Zero::Falsy),
            valid_from: d.valid_from.map(laravel_date),
            valid_to: d.valid_to.map(laravel_date),
            monthly: d.monthly,
            yearly: d.yearly,
            seasonal: d.seasonal,
            meta: d.meta,
            created_at: d.created_at,
            last_updated_at: d.updated_at,
        }
    }
}

/// A date column as Laravel's `datetime` cast printed it (app timezone UTC).
fn laravel_date(d: NaiveDate) -> String {
    format!("{d}T00:00:00.000000Z")
}

/// Create response.
#[derive(Serialize)]
pub struct DiscountCreated {
    pub discount_id: Uuid,
}

/// GET list query.
#[derive(Deserialize)]
pub struct ListQuery {
    pub discount_group_id: Option<String>,
    pub page: Option<String>,
    pub page_size: Option<String>,
}

/// POST /discounts and POST /discounts/{id} (all optional on update).
#[derive(Deserialize)]
pub struct DiscountRequest {
    discount_group_id: Option<Value>,
    name: Option<String>,
    active: Option<Value>,
    discount_product_code: Option<String>,
    percentage: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    valid_from: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    valid_to: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    monthly: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    yearly: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    seasonal: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    meta: Option<Option<Value>>,
}

/// Validated fields; on update `None` keeps the stored value, `Some(None)` clears it.
pub struct DiscountInput {
    pub discount_group_id: Option<Uuid>,
    pub name: Option<String>,
    pub active: Option<bool>,
    pub discount_product_code: Option<String>,
    pub percentage: Option<Decimal>,
    pub valid_from: Option<Option<NaiveDate>>,
    pub valid_to: Option<Option<NaiveDate>>,
    pub monthly: Option<Option<Value>>,
    pub yearly: Option<Option<Value>>,
    pub seasonal: Option<Option<Value>>,
    pub meta: Option<Option<Value>>,
}

/// Create: required fields per Laravel StoreDiscountRequest.
pub struct CreateDiscountRequest(DiscountRequest);
/// Update: every field optional.
pub struct UpdateDiscountRequest(DiscountRequest);

impl<'de> Deserialize<'de> for CreateDiscountRequest {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        DiscountRequest::deserialize(d).map(Self)
    }
}

impl<'de> Deserialize<'de> for UpdateDiscountRequest {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        DiscountRequest::deserialize(d).map(Self)
    }
}

impl Validate for CreateDiscountRequest {
    type Valid = DiscountInput;

    fn validate(self, r: &mut Rules) -> Option<DiscountInput> {
        let b = self.0;
        let group = r
            .required_value("discount_group_id", b.discount_group_id)
            .map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()));
        if group == Some(None) {
            r.add("discount_group_id", GROUP_INVALID);
        }
        let name = r.text("name", b.name, NAME_MAX);
        let active = r
            .required_value("active", b.active)
            .and_then(|v| r.boolean("active", &v));
        let code = r.text("discount_product_code", b.discount_product_code, CODE_MAX);
        let percentage = r
            .required_value("percentage", b.percentage)
            .and_then(|v| r.decimal("percentage", &v, Decimal::ZERO, Decimal::ONE_HUNDRED));
        let valid_from = date(r, "valid_from", b.valid_from);
        if let Some(Some(from)) = valid_from
            && from <= Utc::now().date_naive() - chrono::Days::new(1)
        {
            r.add(
                "valid_from",
                "The valid from must be a date after yesterday.",
            );
        }
        let valid_to = date(r, "valid_to", b.valid_to);
        Some(DiscountInput {
            discount_group_id: group.flatten(),
            name: Some(name),
            active,
            discount_product_code: Some(code),
            percentage,
            valid_from,
            valid_to,
            monthly: period(r, "monthly", b.monthly, false),
            yearly: period(r, "yearly", b.yearly, false),
            seasonal: period(r, "seasonal", b.seasonal, true),
            meta: b.meta.and_then(|v| r.meta(v)),
        })
    }
}

impl Validate for UpdateDiscountRequest {
    type Valid = DiscountInput;

    fn validate(self, r: &mut Rules) -> Option<DiscountInput> {
        let b = self.0;
        Some(DiscountInput {
            discount_group_id: None,
            name: r.opt_text("name", b.name, NAME_MAX),
            active: filled(b.active).and_then(|v| r.boolean("active", &v)),
            discount_product_code: r.opt_text(
                "discount_product_code",
                b.discount_product_code,
                CODE_MAX,
            ),
            percentage: filled(b.percentage)
                .and_then(|v| r.decimal("percentage", &v, Decimal::ZERO, Decimal::ONE_HUNDRED)),
            valid_from: date(r, "valid_from", b.valid_from),
            valid_to: date(r, "valid_to", b.valid_to),
            monthly: period(r, "monthly", b.monthly, false),
            yearly: period(r, "yearly", b.yearly, false),
            seasonal: period(r, "seasonal", b.seasonal, true),
            meta: b.meta.and_then(|v| r.meta(v)),
        })
    }
}

/// Laravel DiscountGroupExist text.
pub const GROUP_INVALID: &str = "This discount group id is not a valid Discount Group";
/// Laravel `after:valid_from` text.
pub const TO_AFTER_FROM: &str = "The valid to must be a date after valid from.";

/// `date_format:Y-m-d` on a patchable field: absent = keep, null/blank = clear.
fn date(r: &mut Rules, field: &str, v: Option<Option<Value>>) -> Option<Option<NaiveDate>> {
    let v = v?;
    let Some(v) = filled(v) else {
        return Some(None);
    };
    match v
        .as_str()
        .and_then(|s| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok())
        // Postgres `date` has no year 0 and Laravel's `Y` is four digits.
        .filter(|d| (1..=9999).contains(&d.year()))
    {
        Some(d) => Some(Some(d)),
        None => {
            r.add(
                field,
                format!("The {} does not match the format Y-m-d.", human(field)),
            );
            None
        }
    }
}

/// Laravel DiscountPeriod `{percentage, valid_from, valid_to, description}`; `dated` adds the
/// seasonal window. Absent = keep, null = clear.
fn period(
    r: &mut Rules,
    key: &str,
    v: Option<Option<Value>>,
    dated: bool,
) -> Option<Option<Value>> {
    let v = v?;
    let Some(v) = filled(v) else {
        return Some(None);
    };
    let Value::Object(o) = v else {
        r.add(key, format!("The {key} must be an array."));
        return None;
    };
    let mut out = Map::new();
    let pct_key = format!("{key}.percentage");
    let pct = filled(o.get("percentage").cloned()).and_then(|p| {
        r.decimal(&pct_key, &p, Decimal::ZERO, Decimal::ONE_HUNDRED)
            .map(|d| Value::String(d.normalize().to_string()))
    });
    out.insert("percentage".into(), pct.unwrap_or(Value::Null));
    let mut window = [None, None];
    for (i, k) in ["valid_from", "valid_to"].iter().enumerate() {
        if !dated {
            out.insert((*k).into(), Value::Null);
            continue;
        }
        let field = format!("{key}.{k}");
        let d = date(r, &field, Some(o.get(*k).cloned())).flatten();
        window[i] = d;
        out.insert(
            (*k).into(),
            d.map_or(Value::Null, |d| Value::String(d.to_string())),
        );
    }
    if let [Some(from), Some(to)] = window
        && to <= from
    {
        r.add(
            &format!("{key}.valid_to"),
            "The seasonal valid to must be a date after seasonal valid from.",
        );
    }
    let desc = match o.get("description") {
        None | Some(Value::Null) => Value::Null,
        Some(Value::String(s)) => {
            r.max_chars(&format!("{key}.description"), s, DESCRIPTION_MAX);
            Value::String(s.clone())
        }
        Some(_) => {
            let f = format!("{key}.description");
            r.add(&f, format!("The {} must be a string.", human(&f)));
            Value::Null
        }
    };
    out.insert("description".into(), desc);
    Some(Some(json!(out)))
}
