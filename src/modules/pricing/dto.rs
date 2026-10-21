//! calculate-items / calculate-marge request (lines as vimdesk-web and the POS send them) and
//! the calculate-items response (Laravel ItemsSummary: JSON numbers, per-line rounded).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, human},
    modules::pricing::engine::{Line, Margin, Summary},
    money::{MAX, Zero, format_decimals, php_number},
};

/// Most lines one request may carry. Laravel had no cap; a document never comes close.
pub const MAX_ITEMS: usize = 1000;

/// Body of both routes. Unknown keys (`disable_client_discounts`, row UI state) are ignored.
#[derive(Deserialize)]
pub struct CalculateRequest {
    items: Option<Value>,
    currency_id: Option<Value>,
    relation_id: Option<Value>,
}

/// A reference id as sent: None = not sent, Some(None) = sent but not a uuid (never matches).
pub type RefId = Option<Option<Uuid>>;

/// Validated body.
pub struct Calculate {
    pub items: Vec<ItemInput>,
    pub currency_id: RefId,
    pub relation_id: RefId,
}

/// One validated line plus the fields Laravel echoes back in `discounts`.
pub struct ItemInput {
    pub id: Value,
    pub title: Option<String>,
    pub description: Option<String>,
    /// Raw `product_id`; the service resolves it.
    pub product_id: Option<String>,
    pub multi_set_item_id: Value,
    pub line: Line,
}

impl Validate for CalculateRequest {
    type Valid = Calculate;

    fn validate(self, r: &mut Rules) -> Option<Calculate> {
        let items = match filled(self.items) {
            None => {
                r.missing("items");
                Vec::new()
            }
            Some(Value::Array(a)) if a.is_empty() => {
                r.missing("items");
                Vec::new()
            }
            Some(Value::Array(a)) if a.len() > MAX_ITEMS => {
                r.add(
                    "items",
                    format!("The items must not have more than {MAX_ITEMS} items."),
                );
                Vec::new()
            }
            Some(Value::Array(a)) => a,
            Some(_) => {
                r.add("items", "The items must be an array.");
                Vec::new()
            }
        };
        let items = items
            .into_iter()
            .enumerate()
            .filter_map(|(n, v)| item(r, &format!("items.{n}"), v))
            .collect();
        Some(Calculate {
            items,
            currency_id: ref_id(self.currency_id),
            relation_id: ref_id(self.relation_id),
        })
    }
}

fn ref_id(v: Option<Value>) -> RefId {
    filled(v).map(|v| v.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok()))
}

fn item(r: &mut Rules, key: &str, v: Value) -> Option<ItemInput> {
    let Value::Object(mut o) = v else {
        r.add(key, format!("The {key} must be an array."));
        return None;
    };
    let mut num = |field: &str, min: Decimal, max: Decimal| {
        let f = format!("{key}.{field}");
        filled(o.remove(field)).and_then(|v| r.decimal(&f, &v, min, max))
    };
    let zero = Decimal::ZERO;
    let quantity = num("quantity", -MAX, MAX).unwrap_or(zero);
    let price = num("price", -MAX, MAX).unwrap_or(zero);
    let vat = num("vat", zero, MAX).unwrap_or(zero);
    let discount = num("productDiscount", zero, Decimal::ONE_HUNDRED).unwrap_or(zero);
    let purchase_price = num("purchase_price", -MAX, MAX);
    let title = text(r, &format!("{key}.title"), o.remove("title"), 500);
    let description = text(
        r,
        &format!("{key}.description"),
        o.remove("description"),
        5000,
    );
    let product_id = match filled(o.remove("product_id")) {
        None => None,
        Some(Value::String(s)) => Some(s.trim().to_owned()),
        Some(_) => {
            let f = format!("{key}.product_id");
            r.add(&f, format!("The {} must be a string.", human(&f)));
            None
        }
    };
    let multi_set_item_id = o.remove("multi_set_item_id").unwrap_or(Value::Null);
    Some(ItemInput {
        id: o.remove("id").unwrap_or(Value::Null),
        title,
        description,
        product_id,
        line: Line {
            quantity,
            price,
            vat,
            discount,
            purchase_price,
            set_child: php_truthy(&multi_set_item_id),
        },
        multi_set_item_id,
    })
}

/// Laravel `?string $x = ''`: absent -> "", null -> null, else a string of at most `max` chars.
fn text(r: &mut Rules, field: &str, v: Option<Value>, max: usize) -> Option<String> {
    match v {
        None => Some(String::new()),
        Some(Value::Null) => None,
        Some(Value::String(s)) => {
            r.max_chars(field, &s, max);
            Some(s)
        }
        Some(_) => {
            r.add(field, format!("The {} must be a string.", human(field)));
            None
        }
    }
}

/// PHP truthiness of a JSON value (`if ($item->multi_set_item_id)`).
fn php_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty() && s != "0",
        Value::Array(a) => !a.is_empty(),
        Value::Object(_) => true,
    }
}

/// `{data: …}` of calculate-items (Laravel `ItemsSummary::toArray`).
#[derive(Serialize)]
pub struct SummaryResource {
    sum_total: Value,
    total_discount: Value,
    sub_total: Value,
    total_vats: Map<String, Value>,
    total_vat: Value,
    total: Value,
    total_excl: Value,
    total_excl_materials: Value,
    total_excl_hours: Value,
    total_excl_hours_users: Vec<Value>,
    discounts: Vec<Value>,
}

impl SummaryResource {
    /// Shape `s` (computed from `items`, same order) like Laravel. Hours stay 0: hour lines
    /// only exist on stored work orders, never in a calculate request.
    pub fn new(items: &[ItemInput], s: &Summary) -> Self {
        let total_vats = s
            .vats
            .iter()
            .map(|(rate, amount)| {
                let v =
                    json!({ "percentage": php_number(*rate), "total_amount": php_number(*amount) });
                (rate.normalize().to_string(), v)
            })
            .collect();
        let discounts = items
            .iter()
            .zip(&s.lines)
            .filter(|(i, a)| i.line.discount > Decimal::ZERO && a.gross > Decimal::ZERO)
            .map(|(i, a)| {
                json!({
                    "id": i.id,
                    "title": i.title,
                    "quantity": php_number(i.line.quantity),
                    "price": php_number(i.line.price),
                    "vat": php_number(i.line.vat),
                    "description": i.description,
                    "product_id": i.product_id,
                    "total": php_number(a.excl),
                    "subtotal": php_number(a.gross),
                    "productDiscount": format_decimals(Some(i.line.discount), 2, true, Zero::Falsy),
                    "purchase_price": i.line.purchase_price.map(php_number),
                    // shortcut: relation discount groups arrive with F3; until then always none.
                    "client_default_discount": "0.00",
                    "multi_set_item_id": i.multi_set_item_id,
                })
            })
            .collect();
        Self {
            sum_total: php_number(s.gross),
            total_discount: php_number(s.discount),
            sub_total: php_number(s.gross),
            total_vats,
            total_vat: php_number(s.vat),
            total: php_number(s.total),
            total_excl: php_number(s.excl),
            total_excl_materials: php_number(s.excl),
            total_excl_hours: Value::from(0),
            total_excl_hours_users: Vec::new(),
            discounts,
        }
    }
}

/// `{data: …}` of calculate-marge (Laravel `ItemsMargeCalculationSummary`). Transfer lists
/// and hours only exist on stored work orders, so their parts are always 0 here.
#[derive(Serialize)]
pub struct MarginResource {
    total_purchase_price: Value,
    marge_percentage: Value,
    marge_total_amount: Value,
    extra_data: Value,
}

impl MarginResource {
    /// Shape `m` like Laravel, legacy keys included (vimdesk-web reads both).
    pub fn new(m: &Margin) -> Self {
        let (cost, pct, amount) = (
            php_number(m.cost),
            php_number(m.percentage),
            php_number(m.amount),
        );
        let items = json!({
            "cost_excl": cost, "revenue_excl": php_number(m.revenue),
            "margin_amount": amount, "margin_percentage": pct,
        });
        let zero = json!({ "cost_excl": 0, "revenue_excl": 0, "margin_amount": 0,
                           "margin_percentage": 0 });
        Self {
            total_purchase_price: cost.clone(),
            marge_percentage: pct.clone(),
            marge_total_amount: amount.clone(),
            extra_data: json!({
                "items_percentage": pct,
                "items_purchase_price": cost,
                "items_amount": amount,
                "transfer_lists_percentage": 0,
                "transfer_lists_purchase_price": 0,
                "transfer_lists_amount": 0,
                "hours_purchase_price": 0,
                "hours_sale_price": 0,
                "components": { "items": items, "transfer_lists": zero, "hours": zero },
                "totals": items,
            }),
        }
    }
}
