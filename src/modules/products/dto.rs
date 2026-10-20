//! Product request bodies: Laravel Store/UpdateProductRequest and SearchProductRequest
//! (vimdesk-web product.store.ts, ProductPrices.vue). The resource lives in `resource.rs`.

use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde::Deserialize;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, human, is_url},
    money,
};

pub const TYPES: [&str; 7] = [
    "digital",
    "multi_product_group",
    "multi_product_set",
    "physical",
    "physical_second_chance",
    "service",
    "giftcard",
];
pub const STATUSES: [&str; 3] = ["active", "disabled", "nla"];
const UNITS: [&str; 10] = [
    "box", "cm", "gram", "kg", "liter", "m", "m2", "m3", "pc", "single",
];
const WARRANTY_MODES: [&str; 3] = ["receipt", "sale", "manual"];
const WARRANTY_UNITS: [&str; 6] = ["day", "days", "month", "months", "year", "years"];
/// Bundle types: need `multi_set_product_ids`.
pub const SET_TYPES: [&str; 2] = ["multi_product_set", "multi_product_group"];
/// Types that may carry dimensions, `has_stock` and `is_serialized` (Laravel exclude_unless).
pub const PHYSICAL: [&str; 2] = ["physical", "physical_second_chance"];

/// Every column a request may set. `None` = not sent (update keeps), `Some(None)` = clear.
#[derive(Default, Clone)]
pub struct Fields {
    pub name: Option<String>,
    pub brand: Option<Option<String>>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub has_stock: Option<bool>,
    pub is_serialized: Option<bool>,
    pub warranty_default_mode: Option<Option<String>>,
    pub warranty_duration_value: Option<Option<i32>>,
    pub warranty_duration_unit: Option<Option<String>>,
    pub reference: Option<Option<String>>,
    pub reference_vendor: Option<Option<String>>,
    pub reference_vendor2: Option<Option<String>>,
    pub reference_vendor3: Option<Option<String>>,
    pub ean13: Option<Option<String>>,
    pub hs_code: Option<Option<String>>,
    pub barcode: Option<Option<String>>,
    pub barcode_vendor: Option<Option<String>>,
    pub supplier_discount_code: Option<Option<String>>,
    pub short_description: Option<Option<String>>,
    pub description: Option<Option<String>>,
    pub stock_level_supplier: Option<Option<i32>>,
    pub supplier_relation_id: Option<Option<Uuid>>,
    pub currency_id: Option<Uuid>,
    pub vat_group_id: Option<Option<Uuid>>,
    pub sales_group_id: Option<Option<Uuid>>,
    pub default_warehouse_id: Option<Option<Uuid>>,
    pub price: Option<Decimal>,
    pub gross_price: Option<Decimal>,
    pub wholesale_price: Option<Option<Decimal>>,
    pub supplier_wholesale_price: Option<Option<Decimal>>,
    pub impact_on_price: Option<Option<Decimal>>,
    pub impact_on_wholesale_price: Option<Option<Decimal>>,
    pub order_unit: Option<Decimal>,
    pub sale_unit: Option<Decimal>,
    pub weight: Option<Option<Decimal>>,
    pub unit: Option<Option<String>>,
    pub dimensions: Option<Option<Value>>,
    pub country_of_origin: Option<Option<String>>,
    pub url_vendor: Option<Option<String>>,
    pub url_vendor_override: Option<Option<String>>,
    pub exclude_price_corrections: Option<bool>,
    pub import_id: Option<Option<String>>,
    pub import_source: Option<Option<String>>,
    pub export_enabled: Option<bool>,
    pub export_price_sync: Option<bool>,
    pub export_product_data: Option<bool>,
    pub export_data: Option<Option<Value>>,
    pub meta: Option<Option<Value>>,
    /// Bundle children: (product id, quantity).
    pub set_children: Option<Vec<(Uuid, Decimal)>>,
}

/// POST /products and POST /products/{id}: the raw object, read key by key so absent and null
/// stay apart (Laravel `sometimes` vs `nullable`).
#[derive(Deserialize)]
pub struct ProductBody(Map<String, Value>);

/// Create: `name` + `price` required.
#[derive(Deserialize)]
pub struct CreateProductRequest(ProductBody);

/// Update: everything optional; `name` stays required when sent.
#[derive(Deserialize)]
pub struct UpdateProductRequest(ProductBody);

impl Validate for CreateProductRequest {
    type Valid = Fields;

    fn validate(self, r: &mut Rules) -> Option<Fields> {
        let m = &self.0.0;
        let mut f = fields(r, m, true);
        // "required" only for a missing or blank key; a bad value already has its own error.
        for k in ["name", "price"] {
            if filled(m.get(k).cloned()).is_none() {
                r.missing(k);
            }
        }
        if f.kind.as_deref().is_some_and(|t| SET_TYPES.contains(&t))
            && f.set_children.as_ref().is_none_or(Vec::is_empty)
        {
            r.missing("multi_set_product_ids");
        }
        // Laravel CreateProduct defaults.
        f.kind.get_or_insert_with(|| "physical".into());
        f.status.get_or_insert_with(|| "active".into());
        f.wholesale_price.get_or_insert(Some(Decimal::ZERO));
        f.supplier_wholesale_price.get_or_insert(Some(Decimal::ZERO));
        Some(f)
    }
}

impl Validate for UpdateProductRequest {
    type Valid = Fields;

    fn validate(self, r: &mut Rules) -> Option<Fields> {
        let m = &self.0.0;
        if m.contains_key("name") && filled(m.get("name").cloned()).is_none() {
            r.missing("name");
        }
        Some(fields(r, m, false))
    }
}

/// One key: absent = None, null/blank = Some(None), else Some(Some(v)).
fn key(m: &Map<String, Value>, k: &str) -> Option<Option<Value>> {
    m.get(k).map(|v| filled(Some(v.clone())))
}

fn text(r: &mut Rules, m: &Map<String, Value>, k: &str, max: usize) -> Option<Option<String>> {
    key(m, k).map(|v| {
        let v = v?;
        let s = match v {
            Value::String(s) => s.trim().to_owned(),
            Value::Number(n) => n.to_string(),
            _ => {
                r.add(k, format!("The {} must be a string.", human(k)));
                return None;
            }
        };
        r.max_chars(k, &s, max);
        Some(s)
    })
}

fn decimal(
    r: &mut Rules,
    m: &Map<String, Value>,
    k: &str,
    min: Decimal,
    max: Decimal,
) -> Option<Option<Decimal>> {
    key(m, k).map(|v| v.and_then(|v| r.decimal(k, &v, min, max)))
}

fn flag(r: &mut Rules, m: &Map<String, Value>, k: &str) -> Option<bool> {
    key(m, k).map(|v| v.and_then(|v| r.boolean(k, &v)).unwrap_or_default())
}

fn int(r: &mut Rules, m: &Map<String, Value>, k: &str, min: i64) -> Option<Option<i32>> {
    key(m, k).map(|v| {
        v.and_then(|v| r.integer(k, &v, min, i32::MAX.into()))
            .and_then(|n| i32::try_from(n).ok())
    })
}

/// Laravel EnumExist: lowercased input, "The selected {attr}: {value} is invalid."
fn enum_of(r: &mut Rules, m: &Map<String, Value>, k: &str, allowed: &[&str]) -> Option<String> {
    let v = key(m, k)??;
    let s = v.as_str().map(str::to_lowercase).unwrap_or_else(|| v.to_string());
    if allowed.contains(&s.as_str()) {
        Some(s)
    } else {
        r.add(k, format!("The selected {}: {s} is invalid.", human(k)));
        None
    }
}

/// `nullable|in:` on a plain list.
fn choice(r: &mut Rules, m: &Map<String, Value>, k: &str, allowed: &[&str]) -> Option<Option<String>> {
    text(r, m, k, usize::MAX).map(|v| v.and_then(|s| r.one_of(k, Some(s), allowed, None)))
}

/// A uuid the service checks later; malformed = the ValidateId text right away.
fn id(r: &mut Rules, m: &Map<String, Value>, k: &str, bad: &str) -> Option<Option<Uuid>> {
    key(m, k).map(|v| {
        let v = v?;
        let id = v.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok());
        if id.is_none() {
            r.add(k, bad);
        }
        id
    })
}

fn url(r: &mut Rules, m: &Map<String, Value>, k: &str, check: bool) -> Option<Option<String>> {
    text(r, m, k, 2048).map(|v| {
        let v = v?;
        if check && !is_url(&v) {
            r.add(k, format!("The {} must be a valid URL.", human(k)));
        }
        Some(v)
    })
}

/// Laravel ValidateId message for `field` in `domain`.
pub fn bad_id(field: &str, domain: &str) -> String {
    format!("The {} is an invalid id for the {domain} domain.", human(field))
}

/// The CurrencyExist text.
pub const BAD_CURRENCY: &str = "This currency id does not exist";

fn fields(r: &mut Rules, m: &Map<String, Value>, create: bool) -> Fields {
    let zero = Decimal::ZERO;
    let max = money::MAX;
    let any = -money::MAX;
    r.custom_sections("custom_sections", m.get("custom_sections"));
    r.custom_sections("custom_section_forms", m.get("custom_section_forms"));
    let kind = enum_of(r, m, "type", &TYPES);
    let status = enum_of(r, m, "status", &STATUSES);
    // NOT NULL column: on update a null price keeps the stored one (Laravel then failed).
    let price = decimal(r, m, "price", zero, max).flatten();
    let set_children = m
        .get("multi_set_product_ids")
        .and_then(|v| set_children(r, v));
    let currency_id = id(r, m, "currency_id", BAD_CURRENCY).flatten();
    Fields {
        name: text(r, m, "name", 255).flatten(),
        brand: text(r, m, "brand", 255),
        kind,
        status,
        has_stock: flag(r, m, "has_stock"),
        is_serialized: flag(r, m, "is_serialized"),
        warranty_default_mode: choice(r, m, "warranty_default_mode", &WARRANTY_MODES),
        warranty_duration_value: int(r, m, "warranty_duration_value", 0),
        warranty_duration_unit: choice(r, m, "warranty_duration_unit", &WARRANTY_UNITS),
        reference: text(r, m, "reference", 100),
        reference_vendor: text(r, m, "reference_vendor", 100),
        reference_vendor2: text(r, m, "reference_vendor2", 100),
        reference_vendor3: text(r, m, "reference_vendor3", 100),
        ean13: text(r, m, "ean13", 100),
        hs_code: text(r, m, "hs_code", 55),
        barcode: text(r, m, "barcode", 55),
        barcode_vendor: text(r, m, "barcode_vendor", 255),
        supplier_discount_code: text(r, m, "supplier_discount_code", 55),
        short_description: text(r, m, "short_description", 255),
        description: text(r, m, "description", 1500),
        // Laravel `numeric` into an int column: MySQL rounded.
        stock_level_supplier: decimal(r, m, "stock_level_supplier", any, max)
            .map(|v| v.and_then(|d| money::round(d, 0).to_i32())),
        supplier_relation_id: id(r, m, "supplier_relation_id", &bad_id("supplier_relation_id", "relation")),
        currency_id,
        vat_group_id: id(r, m, "vat_group_id", &bad_id("vat_group_id", "vat_group")),
        sales_group_id: id(r, m, "sales_group_id", &bad_id("sales_group_id", "sales_group")),
        default_warehouse_id: id(
            r,
            m,
            "default_warehouse_id",
            &bad_id("default_warehouse_id", "warehouse"),
        ),
        price,
        gross_price: decimal(r, m, "gross_price", zero, max).map(Option::unwrap_or_default),
        wholesale_price: decimal(r, m, "wholesale_price", zero, max),
        supplier_wholesale_price: decimal(r, m, "supplier_wholesale_price", zero, max),
        impact_on_price: decimal(r, m, "impact_on_price", any, max),
        impact_on_wholesale_price: decimal(r, m, "impact_on_wholesale_price", any, Decimal::ONE_HUNDRED),
        order_unit: decimal(r, m, "order_unit", zero, max).flatten(),
        sale_unit: decimal(r, m, "sale_unit", zero, max).flatten(),
        weight: decimal(r, m, "weight", zero, max),
        unit: choice(r, m, "unit", &UNITS),
        dimensions: key(m, "dimensions").map(|v| v.and_then(|v| dimensions(r, &v))),
        country_of_origin: text(r, m, "country_of_origin", 100),
        // Laravel's update rules took any string here; create checked URLs.
        url_vendor: url(r, m, "url_vendor", create),
        url_vendor_override: url(r, m, "url_vendor_override", create),
        exclude_price_corrections: flag(r, m, "exclude_price_corrections"),
        import_id: text(r, m, "import_id", 255),
        import_source: text(r, m, "import_source", 255),
        export_enabled: flag(r, m, "export_enabled"),
        export_price_sync: flag(r, m, "export_price_sync"),
        export_product_data: flag(r, m, "export_product_data"),
        export_data: key(m, "export_data").map(|v| v.and_then(|v| object(r, "export_data", v))),
        meta: m.get("meta").and_then(|v| r.meta(Some(v.clone()))),
        set_children,
    }
}

/// `dimensions` {length, width, height}: numbers >= 0, kept as JSON numbers.
fn dimensions(r: &mut Rules, v: &Value) -> Option<Value> {
    let Value::Object(m) = v else {
        r.add("dimensions", "The dimensions must be an array.");
        return None;
    };
    let mut out = Map::new();
    for k in ["length", "width", "height"] {
        let n = filled(m.get(k).cloned())
            .and_then(|n| r.number(&format!("dimensions.{k}"), &n, Decimal::ZERO));
        out.insert(k.into(), n.map_or(Value::Null, Value::from));
    }
    Some(Value::Object(out))
}

fn object(r: &mut Rules, k: &str, v: Value) -> Option<Value> {
    if v.is_object() {
        Some(v)
    } else {
        r.add(k, format!("The {} must be an array.", human(k)));
        None
    }
}

/// `multi_set_product_ids`: [{id: uuid, quantity: numeric >= 1}], at most 100.
fn set_children(r: &mut Rules, v: &Value) -> Option<Vec<(Uuid, Decimal)>> {
    let Value::Array(items) = v else {
        r.add("multi_set_product_ids", "The multi set product ids must be an array.");
        return None;
    };
    if items.len() > 100 {
        r.add(
            "multi_set_product_ids",
            "The multi set product ids must not have more than 100 items.",
        );
        return None;
    }
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let k = format!("multi_set_product_ids.{i}");
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .and_then(|s| Uuid::parse_str(s).ok());
        if id.is_none() {
            r.add(&format!("{k}.id"), format!("The {k}.id must be a valid UUID."));
        }
        let qty = filled(item.get("quantity").cloned())
            .and_then(|q| r.decimal(&format!("{k}.quantity"), &q, Decimal::ONE, money::MAX));
        if let (Some(id), Some(q)) = (id, qty) {
            out.push((id, q));
        }
    }
    Some(out)
}

/// POST /products/search.
#[derive(Deserialize)]
pub struct SearchBody {
    query: Option<String>,
}

impl Validate for SearchBody {
    type Valid = String;

    fn validate(self, r: &mut Rules) -> Option<String> {
        // Laravel strips the literal "Shift" some barcode scanners type.
        let q = self.query.map(|q| q.replace("Shift", ""));
        Some(r.text("query", q, 155))
    }
}

/// GET list query (Laravel ListProductRequest plus the RN/web params Laravel ignored).
#[derive(Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub search: Option<String>,
    pub status: Option<String>,
    pub page: Option<String>,
    pub page_size: Option<String>,
    pub per_page: Option<String>,
    pub limit: Option<String>,
    pub supplier_id: Option<String>,
    pub supplier_relation_id: Option<String>,
    pub ean13: Option<String>,
    pub exclude_ids: Option<String>,
}

/// GET price-history query.
#[derive(Deserialize)]
pub struct HistoryQuery {
    pub page: Option<String>,
    pub per_page: Option<String>,
}
