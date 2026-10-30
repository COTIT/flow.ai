//! calculate-items / calculate-marge request (lines as vimdesk-web and the POS send them) and
//! the responses (Laravel ItemsSummary / ItemsMargeCalculationSummary: JSON numbers).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, human},
    modules::pricing::{
        body::{Items, MAX_ITEMS, RawItem, Row, Scalar},
        engine::{Line, Margin, Summary},
    },
    money::{MAX, Zero, format_decimals, php_number},
};

/// Body of both routes. Unknown keys (`disable_client_discounts`, row UI state) are skipped.
#[derive(Deserialize)]
pub struct CalculateRequest {
    #[serde(default)]
    items: Items,
    #[serde(default)]
    currency_id: Scalar,
    #[serde(default)]
    relation_id: Scalar,
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
    /// Echoed through `formatDecimals`; vimdesk-web writes it back onto the row.
    pub client_default_discount: Option<Decimal>,
    /// `productDiscount` was sent (Laravel fills a relation's discount only when it wasn't).
    pub discount_sent: bool,
    pub line: Line,
}

impl Validate for CalculateRequest {
    type Valid = Calculate;

    fn validate(self, r: &mut Rules) -> Option<Calculate> {
        let rows = match self.items {
            Items::List(rows) if !rows.is_empty() => rows,
            Items::List(_) | Items::Missing => {
                r.missing("items");
                Vec::new()
            }
            Items::TooMany => {
                let msg = format!("The items must not have more than {MAX_ITEMS} items.");
                r.add("items", msg);
                Vec::new()
            }
            Items::NotList => {
                r.add("items", "The items must be an array.");
                Vec::new()
            }
        };
        let items = rows
            .into_iter()
            .enumerate()
            .filter_map(|(n, row)| {
                let key = format!("items.{n}");
                match row {
                    Row::Item(raw) => Some(item(r, &key, *raw)),
                    Row::NotObject => {
                        r.add(&key, format!("The {key} must be an array."));
                        None
                    }
                }
            })
            .collect();
        Some(Calculate {
            items,
            currency_id: ref_id(&self.currency_id),
            relation_id: ref_id(&self.relation_id),
        })
    }
}

fn ref_id(v: &Scalar) -> RefId {
    v.filled()
        .map(|v| v.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok()))
}

fn item(r: &mut Rules, key: &str, raw: RawItem) -> ItemInput {
    let mut num = |field: &str, v: &Scalar, min: Decimal, max: Decimal| {
        let f = format!("{key}.{field}");
        v.filled().and_then(|v| r.decimal(&f, &v, min, max))
    };
    let zero = Decimal::ZERO;
    let discount_sent = raw.product_discount.filled().is_some();
    let quantity = num("quantity", &raw.quantity, -MAX, MAX).unwrap_or(zero);
    let price = num("price", &raw.price, -MAX, MAX).unwrap_or(zero);
    let vat = num("vat", &raw.vat, zero, MAX).unwrap_or(zero);
    let discount = num(
        "productDiscount",
        &raw.product_discount,
        zero,
        Decimal::ONE_HUNDRED,
    )
    .unwrap_or(zero);
    let purchase_price = num("purchase_price", &raw.purchase_price, -MAX, MAX);
    let title = text(r, &format!("{key}.title"), raw.title, 500);
    let description = text(r, &format!("{key}.description"), raw.description, 5000);
    let product_id = match raw.product_id.filled() {
        None => None,
        Some(Value::String(s)) => Some(s.trim().to_owned()),
        Some(_) => {
            let f = format!("{key}.product_id");
            r.add(&f, format!("The {} must be a string.", human(&f)));
            None
        }
    };
    // Laravel never validated it; an unparsable value printed like zero.
    let client_default_discount = raw
        .client_default_discount
        .filled()
        .and_then(|v| Rules::default().decimal("x", &v, -MAX, MAX));
    ItemInput {
        id: raw.id.echo(),
        title,
        description,
        product_id,
        line: Line {
            quantity,
            price,
            vat,
            discount,
            purchase_price,
            set_child: raw.multi_set_item_id.truthy(),
        },
        multi_set_item_id: raw.multi_set_item_id.echo(),
        client_default_discount,
        discount_sent,
    }
}

/// Laravel `?string $x = ''`: absent -> "", null -> null, else a string of at most `max` chars.
fn text(r: &mut Rules, field: &str, v: Scalar, max: usize) -> Option<String> {
    match v {
        Scalar::Absent => Some(String::new()),
        Scalar::Null => None,
        Scalar::Str(s) => {
            r.max_chars(field, &s, max);
            Some(s)
        }
        _ => {
            r.add(field, format!("The {} must be a string.", human(field)));
            None
        }
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
                    // shortcut: F3 adds the relation's discount group here; until then as sent.
                    "client_default_discount":
                        format_decimals(i.client_default_discount, 2, true, Zero::Falsy),
                    "multi_set_item_id": i.multi_set_item_id,
                })
            })
            .collect();
        Self::with_discounts(s, discounts)
    }

    /// A stored document's summary: Laravel listed one null per discounted Eloquent line.
    pub fn of_document(s: &Summary, discounted: usize) -> Self {
        Self::with_discounts(s, vec![Value::Null; discounted])
    }

    fn with_discounts(s: &Summary, discounts: Vec<Value>) -> Self {
        let total_vats = s
            .vats
            .iter()
            .map(|v| {
                let entry =
                    json!({ "percentage": php_number(v.rate), "total_amount": php_number(v.vat) });
                (v.rate.normalize().to_string(), entry)
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
