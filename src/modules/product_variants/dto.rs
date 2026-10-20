//! Variant bodies: Laravel Create/UpdateProductVariantRequest and GenerateProductVariantsRequest
//! (vimdesk-web EditVariantModal.vue, variant-setup ReviewStep.vue).

use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{
    http::json::{Rules, Validate, filled, human},
    money,
};

/// Combinations one generate call may create.
const MAX_COMBINATIONS: usize = 500;
/// Option values one generate call may upsert, over all its types.
const MAX_VALUES: usize = 200;

/// Sent columns; `None` = not sent (update keeps), `Some(None)` = clear.
#[derive(Default)]
pub struct Fields {
    pub sku: Option<Option<String>>,
    pub barcode: Option<Option<String>>,
    pub ean13: Option<Option<String>>,
    pub price: Option<Option<Decimal>>,
    pub impact_on_price: Option<Option<Decimal>>,
    pub original_price: Option<Option<Decimal>>,
    pub wholesale_price: Option<Option<Decimal>>,
    pub original_wholesale_price: Option<Option<Decimal>>,
    pub gross_price: Option<Option<Decimal>>,
    pub supplier_wholesale_price: Option<Option<Decimal>>,
    pub impact_on_wholesale_price: Option<Option<Decimal>>,
    pub weight: Option<Option<Decimal>>,
    pub reference_vendor: Option<Option<String>>,
    pub reference_vendor2: Option<Option<String>>,
    pub reference_vendor3: Option<Option<String>>,
    pub is_default: Option<bool>,
    pub meta: Option<Option<Value>>,
}

/// POST and PUT body (Laravel used the same rules for both).
#[derive(Deserialize)]
pub struct VariantBody(Map<String, Value>);

impl Validate for VariantBody {
    type Valid = Fields;

    fn validate(self, r: &mut Rules) -> Option<Fields> {
        let m = &self.0;
        let zero = Decimal::ZERO;
        let max = money::MAX;
        let price_sent = filled(m.get("price").cloned()).is_some();
        let impact_sent = filled(m.get("impact_on_price").cloned()).is_some();
        if !price_sent && !impact_sent {
            r.add(
                "price",
                "The price field is required when impact on price is not present.",
            );
            r.add(
                "impact_on_price",
                "The impact on price field is required when price is not present.",
            );
        }
        let text = |r: &mut Rules, k: &str| {
            m.get(k).map(|v| match filled(Some(v.clone())) {
                None => None,
                Some(Value::String(s)) => {
                    let s = s.trim().to_owned();
                    r.max_chars(k, &s, 255);
                    Some(s)
                }
                Some(Value::Number(n)) => Some(n.to_string()),
                Some(_) => {
                    r.add(k, format!("The {} must be a string.", human(k)));
                    None
                }
            })
        };
        let num = |r: &mut Rules, k: &str, min: Decimal| {
            m.get(k)
                .map(|v| filled(Some(v.clone())).and_then(|v| r.decimal(k, &v, min, max)))
        };
        Some(Fields {
            sku: text(r, "sku"),
            barcode: text(r, "barcode"),
            ean13: text(r, "ean13"),
            price: num(r, "price", zero),
            impact_on_price: num(r, "impact_on_price", -max),
            original_price: num(r, "original_price", zero),
            wholesale_price: num(r, "wholesale_price", zero),
            original_wholesale_price: num(r, "original_wholesale_price", zero),
            gross_price: num(r, "gross_price", zero),
            supplier_wholesale_price: num(r, "supplier_wholesale_price", zero),
            impact_on_wholesale_price: num(r, "impact_on_wholesale_price", -max),
            weight: num(r, "weight", zero),
            reference_vendor: text(r, "reference_vendor"),
            reference_vendor2: text(r, "reference_vendor2"),
            reference_vendor3: text(r, "reference_vendor3"),
            is_default: m.get("is_default").map(|v| {
                filled(Some(v.clone()))
                    .and_then(|v| r.boolean("is_default", &v))
                    .unwrap_or_default()
            }),
            meta: m.get("meta").and_then(|v| r.meta(Some(v.clone()))),
        })
    }
}

/// One option type to generate from: typed name and its values (with sort orders).
pub struct OptionTypeInput {
    pub name: String,
    pub sort_order: Option<i32>,
    pub values: Vec<(String, Option<i32>)>,
}

/// One combination: raw option value ids (synthetic keys or values) and its overrides.
pub struct Combination {
    pub ids: Vec<String>,
    pub sku: Option<String>,
    pub price: Option<Decimal>,
    pub stock: Option<Decimal>,
}

/// Validated generate request.
pub struct Generate {
    pub option_types: Vec<OptionTypeInput>,
    pub combinations: Vec<Combination>,
    pub sku_pattern: Option<String>,
    pub default_price: Option<Decimal>,
    pub default_stock: Option<Decimal>,
}

/// POST /variants/generate.
#[derive(Deserialize)]
pub struct GenerateBody(Map<String, Value>);

impl Validate for GenerateBody {
    type Valid = Generate;

    fn validate(self, r: &mut Rules) -> Option<Generate> {
        let m = &self.0;
        let max = money::MAX;
        let sort = |r: &mut Rules, k: &str, v: Option<&Value>| {
            filled(v.cloned())
                .and_then(|v| r.integer(k, &v, i32::MIN.into(), i32::MAX.into()))
                .and_then(|n| i32::try_from(n).ok())
        };
        let mut option_types = Vec::new();
        match m.get("option_types") {
            Some(Value::Array(items)) if !items.is_empty() && items.len() <= 20 => {
                for (i, t) in items.iter().enumerate() {
                    let k = format!("option_types.{i}");
                    let name = t
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty());
                    if name.is_none() {
                        r.missing(&format!("{k}.name"));
                    }
                    let mut values = Vec::new();
                    match t.get("values") {
                        Some(Value::Array(vs)) if !vs.is_empty() && vs.len() <= 100 => {
                            for (j, v) in vs.iter().enumerate() {
                                let vk = format!("{k}.values.{j}");
                                match v
                                    .get("value")
                                    .and_then(Value::as_str)
                                    .map(str::trim)
                                    .filter(|s| !s.is_empty())
                                {
                                    Some(val) => {
                                        r.max_chars(&format!("{vk}.value"), val, 255);
                                        values.push((
                                            val.to_owned(),
                                            sort(
                                                r,
                                                &format!("{vk}.sort_order"),
                                                v.get("sort_order"),
                                            ),
                                        ));
                                    }
                                    None => r.missing(&format!("{vk}.value")),
                                }
                            }
                        }
                        _ => r.add(
                            &format!("{k}.values"),
                            format!("The {k}.values must have between 1 and 100 items."),
                        ),
                    }
                    if let Some(name) = name {
                        r.max_chars(&format!("{k}.name"), name, 255);
                        option_types.push(OptionTypeInput {
                            name: name.to_owned(),
                            sort_order: sort(r, &format!("{k}.sort_order"), t.get("sort_order")),
                            values,
                        });
                    }
                }
            }
            _ => r.add(
                "option_types",
                "The option types must have between 1 and 20 items.",
            ),
        }
        if option_types.iter().map(|t| t.values.len()).sum::<usize>() > MAX_VALUES {
            r.add(
                "option_types",
                format!("The option types must not have more than {MAX_VALUES} values in total."),
            );
        }
        let mut combinations = Vec::new();
        match m.get("combinations") {
            None | Some(Value::Null) => {}
            Some(Value::Array(items)) if items.len() <= MAX_COMBINATIONS => {
                for (i, c) in items.iter().enumerate() {
                    let k = format!("combinations.{i}");
                    let raw = c
                        .get("option_value_ids")
                        .or_else(|| c.get("optionValueIds"));
                    let ids: Vec<String> = match raw {
                        Some(Value::Array(ids)) if !ids.is_empty() => ids
                            .iter()
                            .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned))
                            .collect(),
                        _ => {
                            r.missing(&format!("{k}.option_value_ids"));
                            Vec::new()
                        }
                    };
                    let price = filled(c.get("price").cloned())
                        .and_then(|v| r.decimal(&format!("{k}.price"), &v, Decimal::ZERO, max));
                    let stock = filled(c.get("stock").cloned())
                        .and_then(|v| r.decimal(&format!("{k}.stock"), &v, Decimal::ZERO, max));
                    let sku = c
                        .get("sku")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned);
                    if let Some(sku) = &sku {
                        r.max_chars(&format!("{k}.sku"), sku, 255);
                    }
                    combinations.push(Combination {
                        ids,
                        sku,
                        price,
                        stock,
                    });
                }
            }
            Some(_) => r.add(
                "combinations",
                format!("The combinations must not have more than {MAX_COMBINATIONS} items."),
            ),
        }
        let dec = |r: &mut Rules, k: &str| {
            filled(m.get(k).cloned()).and_then(|v| r.decimal(k, &v, Decimal::ZERO, max))
        };
        // `{index}` (7 chars) expands to at most 3 digits, so a fitting pattern gives a fitting sku.
        let sku_pattern = m
            .get("sku_pattern")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if let Some(pat) = &sku_pattern {
            r.max_chars("sku_pattern", pat, 255);
        }
        Some(Generate {
            option_types,
            combinations,
            sku_pattern,
            default_price: dec(r, "default_price"),
            default_stock: dec(r, "default_stock"),
        })
    }
}

/// Laravel ProductVariantResource. `product` is the parent in list shape.
pub fn render(
    v: &crate::modules::product_variants::repo::VariantRow,
    product: &Value,
    values: &[&crate::modules::product_variants::repo::OptionValueRow],
    workspace_id: uuid::Uuid,
) -> Value {
    use crate::modules::products::pricing::column;
    let opt = |d: Option<Decimal>| d.map(|d| column(Some(d), 4));
    serde_json::json!({
        "id": v.id,
        "product_id": v.product_id,
        "is_default": v.is_default,
        "sku": v.sku,
        "barcode": v.barcode,
        "ean13": v.ean13,
        "price": column(Some(v.price), 4),
        "original_price": opt(v.original_price),
        "wholesale_price": opt(v.wholesale_price),
        "original_wholesale_price": opt(v.original_wholesale_price),
        "gross_price": column(Some(v.gross_price), 4),
        "supplier_wholesale_price": opt(v.supplier_wholesale_price),
        "impact_on_price": opt(v.impact_on_price),
        "impact_on_wholesale_price": opt(v.impact_on_wholesale_price),
        "weight": v.weight.map(|w| column(Some(w), 2)),
        "reference_vendor": v.reference_vendor,
        "reference_vendor2": v.reference_vendor2,
        "reference_vendor3": v.reference_vendor3,
        "meta": v.meta,
        "created_at": v.created_at,
        "updated_at": v.updated_at,
        "relationships": {
            "product": product,
            "option_values": values.iter().map(|o| serde_json::json!({
                "id": o.id,
                "workspace_id": workspace_id,
                "option_type_id": o.option_type_id,
                "value": o.value,
                "display_value": o.display_value,
                "sort_order": o.sort_order,
                "created_at": o.created_at,
                "updated_at": o.updated_at,
                "relationships": { "option_type": {
                    "id": o.option_type_id, "name": o.type_name, "display_name": o.type_display_name,
                } },
            })).collect::<Vec<_>>(),
            "images": [],
        },
    })
}
