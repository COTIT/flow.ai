//! Catalogue request/response bodies. Prices are integer minor units of a workspace currency;
//! power and hours are decimal text.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, optional},
    modules::{
        farm_catalogue::repo::{CategoryRow, EquipmentRow, NewCategory, NewEquipment},
        farm_surveys::dto::Money,
    },
};

const CODE_MAX: usize = 32;
const NAME_MAX: usize = 120;
/// RUAHA `equipment_offered` check, as the test names it.
const NOT_OFFERED: &str = "equipment must be offered to rent or to buy";

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
    /// `true` = active rows only, `false` = retired only; absent = all.
    #[param(value_type = Option<bool>)]
    pub active: Option<String>,
}

impl ListQuery {
    /// The `active` filter, if any.
    pub fn active(&self) -> Option<bool> {
        match self.active.as_deref() {
            Some("true" | "1") => Some(true),
            Some("false" | "0") => Some(false),
            _ => None,
        }
    }
}

/// An equipment category.
#[derive(Serialize, ToSchema)]
pub struct EquipmentCategoryResource {
    id: Uuid,
    code: String,
    name_en: String,
    name_sw: String,
    is_active: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<CategoryRow> for EquipmentCategoryResource {
    fn from(c: CategoryRow) -> Self {
        Self {
            id: c.id,
            code: c.code,
            name_en: c.name_en,
            name_sw: c.name_sw,
            is_active: c.is_active,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

/// The category a machine sits in.
#[derive(Serialize, ToSchema)]
pub struct EquipmentCategoryRef {
    id: Uuid,
    code: String,
    name_en: String,
    name_sw: String,
}

/// A machine a farmer may ask to rent and/or buy. Prices are INDICATIVE, not a quotation.
#[derive(Serialize, ToSchema)]
pub struct EquipmentResource {
    id: Uuid,
    category: EquipmentCategoryRef,
    code: String,
    name_en: String,
    name_sw: String,
    /// kW as text, 3 decimals; null = no energy estimate.
    #[schema(required = true, example = "15.000")]
    rated_power_kw: Option<String>,
    #[schema(required = true, example = "6.00")]
    typical_hours_per_day: Option<String>,
    #[schema(required = true, example = "5.0")]
    typical_days_per_week: Option<String>,
    can_rent: bool,
    can_buy: bool,
    /// Indicative purchase price.
    #[schema(required = true)]
    indicative_price: Option<Money>,
    /// Indicative rent per day.
    #[schema(required = true)]
    indicative_rent_per_day: Option<Money>,
    is_active: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<EquipmentRow> for EquipmentResource {
    fn from(e: EquipmentRow) -> Self {
        let money = |minor: Option<i64>| {
            minor.map(|minor| Money {
                minor,
                currency: e.currency.clone(),
                decimals: e.decimals,
                currency_id: e.currency_id,
            })
        };
        let indicative_price = money(e.indicative_price_minor);
        let indicative_rent_per_day = money(e.indicative_rent_per_day_minor);
        Self {
            id: e.id,
            category: EquipmentCategoryRef {
                id: e.category_id,
                code: e.category_code,
                name_en: e.category_name_en,
                name_sw: e.category_name_sw,
            },
            code: e.code,
            name_en: e.name_en,
            name_sw: e.name_sw,
            rated_power_kw: e.rated_power_kw.map(|d| d.to_string()),
            typical_hours_per_day: e.typical_hours_per_day.map(|d| d.to_string()),
            typical_days_per_week: e.typical_days_per_week.map(|d| d.to_string()),
            can_rent: e.can_rent,
            can_buy: e.can_buy,
            indicative_price,
            indicative_rent_per_day,
            is_active: e.is_active,
            created_at: e.created_at,
            updated_at: e.updated_at,
        }
    }
}

/// Optional client id: blank = server picks.
fn client_id(r: &mut Rules, value: Option<String>) -> Option<Uuid> {
    let v = optional(value)?;
    let id = Uuid::parse_str(&v).ok();
    r.check("id", id.is_some(), "The id must be a valid UUID.");
    id
}

/// Required id of a referenced row.
fn reference(r: &mut Rules, field: &str, value: Option<String>) -> Option<Uuid> {
    let Some(v) = optional(value) else {
        r.missing(field);
        return None;
    };
    let id = Uuid::parse_str(&v).ok();
    let f = field.replace('_', " ");
    r.check(
        field,
        id.is_some(),
        format!("The {f} must be a valid UUID."),
    );
    id
}

/// POST /farm/equipment-categories.
#[derive(Deserialize, ToSchema)]
#[schema(description = "New category. A client `id` makes retries safe.")]
pub struct EquipmentCategoryRequest {
    #[schema(value_type = Option<Uuid>)]
    id: Option<String>,
    #[schema(value_type = String, max_length = 32, example = "MILLING")]
    code: Option<String>,
    #[schema(value_type = String, max_length = 120)]
    name_en: Option<String>,
    #[schema(value_type = String, max_length = 120)]
    name_sw: Option<String>,
}

impl Validate for EquipmentCategoryRequest {
    type Valid = NewCategory;

    fn validate(self, r: &mut Rules) -> Option<NewCategory> {
        Some(NewCategory {
            id: client_id(r, self.id),
            code: r.text("code", self.code, CODE_MAX),
            name_en: r.text("name_en", self.name_en, NAME_MAX),
            name_sw: r.text("name_sw", self.name_sw, NAME_MAX),
        })
    }
}

/// POST /farm/equipment.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "New machine. Offered to rent, to buy, or both. A client `id` makes \
    retries safe: the same body replays, a different one is refused (409)."
)]
pub struct EquipmentRequest {
    #[schema(value_type = Option<Uuid>)]
    id: Option<String>,
    #[schema(value_type = Uuid)]
    category_id: Option<String>,
    #[schema(value_type = String, max_length = 32, example = "MILL-500")]
    code: Option<String>,
    #[schema(value_type = String, max_length = 120)]
    name_en: Option<String>,
    #[schema(value_type = String, max_length = 120)]
    name_sw: Option<String>,
    /// kW, at most 3 decimals.
    #[schema(value_type = Option<String>, example = "15")]
    rated_power_kw: Option<Value>,
    /// 0–24, at most 2 decimals.
    #[schema(value_type = Option<String>)]
    typical_hours_per_day: Option<Value>,
    /// 0–7, at most 1 decimal.
    #[schema(value_type = Option<String>)]
    typical_days_per_week: Option<Value>,
    /// Default false.
    #[schema(value_type = Option<bool>)]
    can_rent: Option<Value>,
    /// Default true.
    #[schema(value_type = Option<bool>)]
    can_buy: Option<Value>,
    /// Minor units of `currency_id`.
    #[schema(value_type = Option<i64>)]
    indicative_price_minor: Option<Value>,
    /// Minor units of `currency_id`.
    #[schema(value_type = Option<i64>)]
    indicative_rent_per_day_minor: Option<Value>,
    #[schema(value_type = Uuid)]
    currency_id: Option<String>,
}

fn flag(r: &mut Rules, field: &str, value: Option<Value>, default: bool) -> bool {
    filled(value)
        .and_then(|v| r.boolean(field, &v))
        .unwrap_or(default)
}

fn minor(r: &mut Rules, field: &str, value: Option<Value>) -> Option<i64> {
    r.integer(field, &filled(value)?, 0, i64::MAX)
}

impl Validate for EquipmentRequest {
    type Valid = NewEquipment;

    fn validate(self, r: &mut Rules) -> Option<NewEquipment> {
        let id = client_id(r, self.id);
        let category_id = reference(r, "category_id", self.category_id);
        let currency_id = reference(r, "currency_id", self.currency_id);
        let code = r.text("code", self.code, CODE_MAX);
        let name_en = r.text("name_en", self.name_en, NAME_MAX);
        let name_sw = r.text("name_sw", self.name_sw, NAME_MAX);
        let rated_power_kw = r.measure(
            "rated_power_kw",
            self.rated_power_kw,
            Decimal::ZERO,
            Decimal::new(99_999_999, 3),
            3,
        );
        let typical_hours_per_day = r.measure(
            "typical_hours_per_day",
            self.typical_hours_per_day,
            Decimal::ZERO,
            Decimal::from(24),
            2,
        );
        let typical_days_per_week = r.measure(
            "typical_days_per_week",
            self.typical_days_per_week,
            Decimal::ZERO,
            Decimal::from(7),
            1,
        );
        let can_rent = flag(r, "can_rent", self.can_rent, false);
        let can_buy = flag(r, "can_buy", self.can_buy, true);
        r.check("can_rent", can_rent || can_buy, NOT_OFFERED);
        Some(NewEquipment {
            id,
            category_id: category_id?,
            code,
            name_en,
            name_sw,
            rated_power_kw,
            typical_hours_per_day,
            typical_days_per_week,
            can_rent,
            can_buy,
            indicative_price_minor: minor(r, "indicative_price_minor", self.indicative_price_minor),
            indicative_rent_per_day_minor: minor(
                r,
                "indicative_rent_per_day_minor",
                self.indicative_rent_per_day_minor,
            ),
            currency_id: currency_id?,
        })
    }
}
