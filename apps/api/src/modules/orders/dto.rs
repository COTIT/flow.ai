//! Order request/response bodies. Money is integer minor units of the order's currency;
//! quantities are decimal text with 2 places (the stock scale).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::http::json::{Rules, Validate, present};

/// Most lines one order may have.
pub const LINES_MAX: usize = 50;
/// Largest quantity of one line.
pub const QUANTITY_MAX: Decimal = Decimal::from_parts(100_000, 0, 0, false, 0);
const TEXT_MAX: usize = 255;
const NOTE_MAX: usize = 1000;

/// Order statuses.
pub const STATUSES: &[&str] = &[
    "draft",
    "awaiting_payment",
    "open",
    "completed",
    "cancelled",
    "partially_returned",
    "returned",
];

/// One order line as sent.
#[derive(Deserialize, ToSchema)]
pub struct LineRequest {
    #[schema(value_type = Uuid)]
    product_id: Option<Value>,
    /// A variant of the product; its price wins.
    #[schema(value_type = Option<Uuid>)]
    product_variant_id: Option<Value>,
    /// Decimal text or number, more than 0, at most 2 decimals.
    #[schema(value_type = String, example = "2")]
    quantity: Option<Value>,
    /// Minor units excl VAT; default the catalog price. Office only.
    #[schema(value_type = Option<i64>, example = 1250)]
    unit_price: Option<Value>,
    /// 0..=100; default 0. Office only.
    #[schema(value_type = Option<String>, example = "10")]
    discount_percentage: Option<Value>,
    /// Default the product name. Office only.
    #[schema(max_length = 255)]
    title: Option<String>,
}

/// A validated line.
#[derive(Clone, Debug)]
pub struct LineInput {
    pub product_id: Uuid,
    pub product_variant_id: Option<Uuid>,
    pub quantity: Decimal,
    pub unit_price: Option<i64>,
    pub discount: Decimal,
    pub title: Option<String>,
}

fn uuid(r: &mut Rules, field: &str, v: &Value) -> Option<Uuid> {
    let id = v.as_str().and_then(|s| Uuid::parse_str(s).ok());
    if id.is_none() {
        r.add(field, format!("The {field} must be a valid id."));
    }
    id
}

fn opt_uuid(r: &mut Rules, field: &str, v: Option<Value>) -> Option<Uuid> {
    v.filter(|v| !v.is_null()).and_then(|v| uuid(r, field, &v))
}

/// Up to [`LINES_MAX`] lines; `office` allows price, discount and title.
pub fn lines(r: &mut Rules, v: Option<Vec<LineRequest>>, office: bool) -> Vec<LineInput> {
    let Some(rows) = v.filter(|l| !l.is_empty()) else {
        r.missing("lines");
        return Vec::new();
    };
    if rows.len() > LINES_MAX {
        r.add(
            "lines",
            format!("The lines must not have more than {LINES_MAX} items."),
        );
        return Vec::new();
    }
    let mut out = Vec::new();
    for (n, l) in rows.into_iter().enumerate() {
        let f = |k: &str| format!("lines.{n}.{k}");
        let product = match l.product_id {
            Some(v) => uuid(r, &f("product_id"), &v),
            None => {
                r.missing(&f("product_id"));
                None
            }
        };
        let variant = opt_uuid(r, &f("product_variant_id"), l.product_variant_id);
        let quantity = match l.quantity {
            Some(v) => r.decimal(&f("quantity"), &v, Decimal::new(1, 2), QUANTITY_MAX),
            None => {
                r.missing(&f("quantity"));
                None
            }
        };
        if let Some(q) = quantity {
            r.check(
                &f("quantity"),
                q.scale() <= 2 && !q.is_zero(),
                "The quantity must be more than 0 with at most 2 decimal places.",
            );
        }
        let (mut unit_price, mut discount, mut title) = (None, Decimal::ZERO, None);
        if office {
            unit_price = l
                .unit_price
                .filter(|v| !v.is_null())
                .and_then(|v| r.integer(&f("unit_price"), &v, 0, 1_000_000_000_000));
            discount = l
                .discount_percentage
                .filter(|v| !v.is_null())
                .and_then(|v| {
                    r.decimal(
                        &f("discount_percentage"),
                        &v,
                        Decimal::ZERO,
                        Decimal::ONE_HUNDRED,
                    )
                })
                .unwrap_or_default();
            title = r.opt_text(&f("title"), l.title, TEXT_MAX);
        } else if l.unit_price.is_some() || l.discount_percentage.is_some() || l.title.is_some() {
            r.add(&f("unit_price"), "Prices come from the catalog.");
        }
        if let (Some(product_id), Some(quantity)) = (product, quantity) {
            out.push(LineInput {
                product_id,
                product_variant_id: variant,
                quantity,
                unit_price,
                discount,
                title,
            });
        }
    }
    out
}

/// POST /orders: a draft.
#[derive(Deserialize, ToSchema)]
#[schema(description = "A new draft order. Prices default to the catalog.")]
pub struct CreateOrderRequest {
    #[schema(value_type = Option<Uuid>)]
    relation_id: Option<Value>,
    /// Default the workspace's main currency.
    #[schema(value_type = Option<Uuid>)]
    currency_id: Option<Value>,
    /// Where stock is reserved and taken; default the main warehouse.
    #[schema(value_type = Option<Uuid>)]
    warehouse_id: Option<Value>,
    #[schema(max_length = 255)]
    reference: Option<String>,
    #[schema(max_length = 1000)]
    note: Option<String>,
    lines: Option<Vec<LineRequest>>,
}

/// Validated create input.
pub struct OrderInput {
    pub relation_id: Option<Uuid>,
    pub currency_id: Option<Uuid>,
    pub warehouse_id: Option<Uuid>,
    pub reference: Option<String>,
    pub note: Option<String>,
    pub lines: Vec<LineInput>,
}

impl Validate for CreateOrderRequest {
    type Valid = OrderInput;

    fn validate(self, r: &mut Rules) -> Option<OrderInput> {
        Some(OrderInput {
            relation_id: opt_uuid(r, "relation_id", self.relation_id),
            currency_id: opt_uuid(r, "currency_id", self.currency_id),
            warehouse_id: opt_uuid(r, "warehouse_id", self.warehouse_id),
            reference: r.opt_text("reference", self.reference, TEXT_MAX),
            note: r.opt_text("note", self.note, NOTE_MAX),
            lines: lines(r, self.lines, true),
        })
    }
}

/// PATCH /orders/{id}: absent keeps the field. Lines and warehouse change on drafts only.
#[derive(Deserialize, ToSchema)]
#[schema(description = "Changes to an order. Leave a field out to keep it.")]
pub struct UpdateOrderRequest {
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>)]
    relation_id: Option<Option<Value>>,
    #[schema(value_type = Option<Uuid>)]
    warehouse_id: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 255)]
    reference: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 1000)]
    note: Option<Option<String>>,
    /// Replaces every line (drafts only).
    lines: Option<Vec<LineRequest>>,
}

/// Validated patch: outer None keeps the field.
pub struct OrderPatch {
    pub relation_id: Option<Option<Uuid>>,
    pub warehouse_id: Option<Uuid>,
    pub reference: Option<Option<String>>,
    pub note: Option<Option<String>>,
    pub lines: Option<Vec<LineInput>>,
}

impl Validate for UpdateOrderRequest {
    type Valid = OrderPatch;

    fn validate(self, r: &mut Rules) -> Option<OrderPatch> {
        Some(OrderPatch {
            relation_id: self.relation_id.map(|v| opt_uuid(r, "relation_id", v)),
            warehouse_id: opt_uuid(r, "warehouse_id", self.warehouse_id),
            reference: r.patch_opt_text("reference", self.reference, TEXT_MAX),
            note: r.patch_opt_text("note", self.note, NOTE_MAX),
            lines: self.lines.map(|l| lines(r, Some(l), true)),
        })
    }
}

/// One line picked.
#[derive(Deserialize, ToSchema)]
pub struct PickLineRequest {
    #[schema(value_type = Uuid)]
    line_id: Option<Value>,
    /// Total picked so far for the line (not an increment), 0..=quantity.
    #[schema(value_type = String, example = "2")]
    quantity: Option<Value>,
}

/// POST /orders/{id}/pick and /returns.
#[derive(Deserialize, ToSchema)]
#[schema(description = "Quantities per line.")]
pub struct LinesQuantityRequest {
    lines: Option<Vec<PickLineRequest>>,
    /// Returns only: put the returned quantity back on stock (default true).
    #[schema(value_type = Option<bool>)]
    restock: Option<Value>,
}

/// Validated per-line quantities.
pub struct LinesQuantity {
    pub lines: Vec<(Uuid, Decimal)>,
    // shortcut: returns read it from phase 2a step 3; drop the allow then.
    #[allow(dead_code)]
    pub restock: bool,
}

impl Validate for LinesQuantityRequest {
    type Valid = LinesQuantity;

    fn validate(self, r: &mut Rules) -> Option<LinesQuantity> {
        let mut out = Vec::new();
        match self.lines.filter(|l| (1..=LINES_MAX).contains(&l.len())) {
            None => r.add(
                "lines",
                format!("The lines must list 1 to {LINES_MAX} lines."),
            ),
            Some(rows) => {
                for (n, l) in rows.into_iter().enumerate() {
                    let id = l
                        .line_id
                        .and_then(|v| uuid(r, &format!("lines.{n}.line_id"), &v));
                    let field = format!("lines.{n}.quantity");
                    let q = l
                        .quantity
                        .and_then(|v| r.decimal(&field, &v, Decimal::ZERO, QUANTITY_MAX));
                    if let Some(q) = q {
                        r.check(
                            &field,
                            q.scale() <= 2,
                            "The quantity must have at most 2 decimal places.",
                        );
                    }
                    match (id, q) {
                        (Some(id), Some(q)) => out.push((id, q)),
                        _ => r.add(
                            &format!("lines.{n}"),
                            "Each line needs line_id and quantity.",
                        ),
                    }
                }
            }
        }
        let restock = self
            .restock
            .and_then(|v| r.boolean("restock", &v))
            .unwrap_or(true);
        Some(LinesQuantity {
            lines: out,
            restock,
        })
    }
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
    /// One status.
    pub status: Option<String>,
}

/// Create reply.
#[derive(Serialize, ToSchema)]
pub struct OrderCreated {
    pub order_id: Uuid,
    #[schema(example = "ORD-0001")]
    pub number: String,
}

/// One order line.
#[derive(Serialize, ToSchema)]
pub struct OrderLineResource {
    pub id: Uuid,
    pub product_id: Uuid,
    #[schema(required = true)]
    pub product_variant_id: Option<Uuid>,
    pub title: String,
    #[schema(example = "2.00")]
    pub quantity: String,
    /// Minor units excl VAT.
    pub unit_price: i64,
    #[schema(example = "21")]
    pub vat_rate: String,
    #[schema(example = "0")]
    pub discount_percentage: String,
    /// Minor units: line total excl VAT after discount.
    pub line_excl: i64,
    /// Minor units: this line's own VAT (display only; order VAT is per rate).
    pub line_vat: i64,
    /// Stock is reserved and taken for this line.
    pub tracked: bool,
    #[schema(example = "0.00")]
    pub picked: String,
    #[schema(example = "0.00")]
    pub deducted: String,
    #[schema(example = "0.00")]
    pub returned: String,
}

/// An order with its lines. Totals in minor units of `currency`.
#[derive(Serialize, ToSchema)]
pub struct OrderResource {
    pub id: Uuid,
    #[schema(example = "ORD-0001")]
    pub number: String,
    /// `draft`, `awaiting_payment`, `open`, `completed`, `cancelled`, `partially_returned`, `returned`.
    pub status: String,
    /// `office` or `storefront`.
    pub source: String,
    #[schema(required = true)]
    pub relation_id: Option<Uuid>,
    pub currency_id: Uuid,
    #[schema(example = "EUR")]
    pub currency: String,
    pub decimals: i16,
    pub warehouse_id: Uuid,
    /// Web customer details (office only).
    #[schema(required = true, value_type = Option<Object>)]
    pub customer: Option<Value>,
    #[schema(required = true)]
    pub reference: Option<String>,
    #[schema(required = true)]
    pub note: Option<String>,
    /// Excl VAT.
    pub subtotal: i64,
    pub vat: i64,
    pub total: i64,
    /// Per-rate VAT: `{"vats": [{"rate": "21", "base": 1000, "vat": 210}]}`.
    #[schema(value_type = Object)]
    pub summary: Value,
    #[schema(required = true)]
    pub expires_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    pub paid_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    pub completed_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    pub cancelled_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    pub invoice_id: Option<Uuid>,
    pub lines: Vec<OrderLineResource>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
