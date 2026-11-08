//! Order request/response bodies. Money is integer minor units of the order's currency;
//! quantities are decimal text with 2 places (the stock scale).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, present},
    modules::lots::public::Link,
    money,
};

/// Most lines one order may have.
pub const LINES_MAX: usize = 50;
/// Largest quantity of one line.
pub const QUANTITY_MAX: Decimal = Decimal::from_parts(100_000, 0, 0, false, 0);
/// Largest quantity of one web shop line (a fake checkout can't hold a warehouse).
pub const WEB_QUANTITY_MAX: Decimal = Decimal::from_parts(1_000, 0, 0, false, 0);
const URL_MAX: usize = 2048;
const ADDRESS_MAX: usize = 4096;
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
    #[schema(value_type = Uuid, required = true, nullable = false)]
    product_id: Option<Value>,
    /// A variant of the product; its price wins.
    #[schema(value_type = Option<Uuid>)]
    product_variant_id: Option<Value>,
    /// Decimal text or number, more than 0, at most 2 decimals.
    #[schema(value_type = String, required = true, nullable = false, example = "2")]
    quantity: Option<Value>,
    /// Minor units excl VAT; default the catalog price. Office only.
    #[schema(value_type = Option<i64>, example = 1250)]
    pub(crate) unit_price: Option<Value>,
    /// 0..=100; default 0. Office only.
    #[schema(value_type = Option<String>, example = "10")]
    pub(crate) discount_percentage: Option<Value>,
    /// Default the product name. Office only.
    #[schema(max_length = 255)]
    pub(crate) title: Option<String>,
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
            if !office {
                r.check(
                    &f("quantity"),
                    q <= WEB_QUANTITY_MAX,
                    format!("The quantity must not be greater than {WEB_QUANTITY_MAX}."),
                );
            }
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
    #[schema(value_type=Vec<LineRequest>,required=true,nullable=false,min_items=1,max_items=50)]
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
    /// Created at or after (inclusive). RFC 3339 time, or `YYYY-MM-DD` = local midnight
    /// starting that day in the workspace `general.timezone` (UTC when unset). Yesterday's
    /// orders: `created_from=<yesterday>&created_to=<today>`.
    #[param(value_type = Option<String>, example = "2026-11-07")]
    pub created_from: Option<String>,
    /// Created before (exclusive), same forms as `created_from`; must be after it. Today so
    /// far: `created_from=<today>`.
    #[param(value_type = Option<String>, example = "2026-11-08")]
    pub created_to: Option<String>,
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
    /// Lots the order holds or took units of for this line's product, by lot code. Stock is
    /// held and taken per product, so lines of the same product show the same list.
    pub lots: Vec<OrderLineLot>,
    /// The variant's first image, else the product's, as a signed link; null without one.
    #[schema(required = true)]
    pub image: Option<Link>,
}

/// A lot an order line's product came from.
#[derive(Serialize, ToSchema)]
pub struct OrderLineLot {
    pub id: Uuid,
    pub code: String,
    /// Units held or taken from it, 2 decimals.
    #[schema(example = "2.00")]
    pub quantity: String,
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
    /// First time it became `open` (office confirm, web payment) or `completed` (till sale).
    /// Null on orders from before the trail existed.
    #[schema(required = true)]
    pub confirmed_at: Option<DateTime<Utc>>,
    /// When every line was first picked in full; null until then (and on older orders).
    #[schema(required = true)]
    pub picked_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    pub invoice_id: Option<Uuid>,
    /// The linked invoice's status (`draft`, `pending`, `partially_paid`, `paid`, …); null
    /// without an invoice. Read with the order; the order's own status doesn't follow it.
    #[schema(required = true)]
    pub invoice_status: Option<String>,
    /// What the API would accept on this order now, from the rules its action routes enforce.
    /// Status and payment rules only, not the caller's permissions.
    pub allowed_actions: Vec<OrderAction>,
    /// Sum of the lines' quantities, 2 decimals.
    #[schema(example = "4.00")]
    pub units: String,
    /// `cash` or `card` (till sale), `online` (a paid web payment); null otherwise.
    #[schema(required = true, example = "card")]
    pub payment_method: Option<String>,
    pub lines: Vec<OrderLineResource>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// An office action on an order (`POST /orders/{order}/<action>`; `return` is `/returns`,
/// `refund` is `/refunds`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrderAction {
    Confirm,
    Pick,
    Complete,
    Cancel,
    Return,
    Refund,
    Invoice,
}

impl OrderAction {
    /// Every action, in the order `allowed_actions` lists them.
    pub const ALL: [Self; 7] = [
        Self::Confirm,
        Self::Pick,
        Self::Complete,
        Self::Cancel,
        Self::Return,
        Self::Refund,
        Self::Invoice,
    ];
}

/// The web customer, kept on the order for the office; never sent back to the shop or in
/// events.
#[derive(Deserialize, ToSchema)]
pub struct CustomerRequest {
    #[schema(value_type = String, example = "kim@example.com")]
    email: Option<String>,
    #[schema(value_type = String, max_length = 255)]
    name: Option<String>,
    #[schema(max_length = 50)]
    phone: Option<String>,
    /// Free-form address object (street, city, country, ...), at most 4 KiB.
    #[schema(value_type = Option<Object>)]
    shipping_address: Option<Value>,
    #[schema(value_type = Option<Object>)]
    billing_address: Option<Value>,
}

/// POST /storefront/orders.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "A web order. Prices come from the catalog; stock is held until paid or expired."
)]
pub struct CheckoutRequest {
    /// 1 to 50 lines: `product_id`, optional `product_variant_id`, `quantity` (at most 1000).
    #[schema(value_type=Vec<LineRequest>,required=true,nullable=false,min_items=1,max_items=50)]
    lines: Option<Vec<LineRequest>>,
    customer: Option<CustomerRequest>,
}

/// Validated checkout.
pub struct CheckoutInput {
    pub lines: Vec<LineInput>,
    pub customer: Value,
}

fn address(r: &mut Rules, field: &str, v: Option<Value>) -> Value {
    match v {
        None | Some(Value::Null) => Value::Null,
        Some(v @ Value::Object(_)) if v.to_string().len() <= ADDRESS_MAX => v,
        Some(_) => {
            r.add(
                field,
                format!("The {field} must be an object of at most 4 KiB."),
            );
            Value::Null
        }
    }
}

impl Validate for CheckoutRequest {
    type Valid = CheckoutInput;

    fn validate(self, r: &mut Rules) -> Option<CheckoutInput> {
        let lines = lines(r, self.lines, false);
        let Some(c) = self.customer else {
            r.missing("customer");
            return None;
        };
        let email = r.text("customer.email", c.email, TEXT_MAX);
        if !email.is_empty() {
            r.email("customer.email", &email);
        }
        let name = r.text("customer.name", c.name, TEXT_MAX);
        let phone = r.opt_text("customer.phone", c.phone, 50);
        let shipping = address(r, "customer.shipping_address", c.shipping_address);
        let billing = address(r, "customer.billing_address", c.billing_address);
        Some(CheckoutInput {
            lines,
            customer: serde_json::json!({
                "email": email, "name": name, "phone": phone,
                "shipping_address": shipping, "billing_address": billing,
            }),
        })
    }
}

/// POST /storefront/orders/{order}/payment.
#[derive(Deserialize, ToSchema)]
#[schema(description = "Where the customer returns after paying.")]
pub struct PaymentRequest {
    /// https (http only for localhost), at most 2048 characters.
    #[schema(value_type = String, example = "https://shop.example.com/thanks")]
    redirect_url: Option<String>,
}

/// Validated redirect.
pub struct PaymentInput {
    pub redirect_url: String,
}

/// https, or http to localhost / 127.0.0.1 (a shop in development).
fn redirect(v: &str) -> bool {
    let clean = !v
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || "@\"'<>`\\".contains(c));
    let local = clean
        && ["http://localhost", "http://127.0.0.1"].iter().any(|p| {
            v.strip_prefix(p)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with([':', '/']))
        });
    (v.starts_with("https://") && crate::http::json::is_url(v)) || local
}

impl Validate for PaymentRequest {
    type Valid = PaymentInput;

    fn validate(self, r: &mut Rules) -> Option<PaymentInput> {
        let url = r.text("redirect_url", self.redirect_url, URL_MAX);
        if !url.is_empty() {
            r.check(
                "redirect_url",
                redirect(&url),
                "The redirect url must be an https URL (http only for localhost).",
            );
        }
        Some(PaymentInput { redirect_url: url })
    }
}

/// What the shop sees of its order: ids, number, status, totals and currency; never the
/// customer.
#[derive(Serialize, ToSchema)]
pub struct StorefrontOrder {
    pub id: Uuid,
    #[schema(example = "ORD-0001")]
    pub number: String,
    /// `awaiting_payment`, `open` (paid), `cancelled` (expired), `completed`, ...
    pub status: String,
    #[schema(example = "EUR")]
    pub currency: String,
    pub decimals: i16,
    /// Minor units, excl VAT.
    pub subtotal: i64,
    pub vat: i64,
    pub total: i64,
    /// When an unpaid order gives its stock back.
    #[schema(required = true)]
    pub expires_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    pub paid_at: Option<DateTime<Utc>>,
    /// What was ordered, in order.
    pub lines: Vec<StorefrontOrderLine>,
}

/// One line of a web order, as the customer may see it.
#[derive(Serialize, ToSchema)]
pub struct StorefrontOrderLine {
    #[schema(example = "Raw honey 500 g")]
    pub title: String,
    pub product_id: Uuid,
    #[schema(required = true)]
    pub product_variant_id: Option<Uuid>,
    #[schema(example = "2.00")]
    pub quantity: String,
    /// One unit incl VAT, minor units; null only when out of range.
    #[schema(required = true, example = 1363)]
    pub unit_price_gross: Option<i64>,
    /// The line incl VAT, minor units (the line's own VAT; the order's VAT is per rate).
    #[schema(example = 2725)]
    pub total_gross: i64,
}

/// A payment intent.
#[derive(Serialize, ToSchema)]
pub struct PaymentIntentResource {
    pub payment_id: Uuid,
    /// `open` until the provider confirms.
    pub status: String,
    /// Send the customer here.
    #[schema(required = true)]
    pub checkout_url: Option<String>,
}

/// POST /orders/{order}/refunds.
#[derive(Deserialize, ToSchema)]
#[schema(description = "Refund part of what was paid online.")]
pub struct RefundRequest {
    /// Decimal text in the order's currency as typed (`"12.50"`), greater than 0, within the
    /// currency's decimals and at most paid − refunds not failed. The API converts it to minor
    /// units.
    #[schema(value_type = String, example = "5.00")]
    amount: Option<Value>,
}

/// Validated refund amount, in currency units until the order's decimals are known.
pub struct RefundInput {
    pub amount: Decimal,
}

impl Validate for RefundRequest {
    type Valid = RefundInput;

    fn validate(self, r: &mut Rules) -> Option<RefundInput> {
        let amount = match self.amount {
            Some(v) => r
                .decimal("amount", &v, -money::MAX, money::MAX)
                .and_then(|a| {
                    if a > Decimal::ZERO {
                        Some(a)
                    } else {
                        r.add("amount", "The amount must be greater than 0.");
                        None
                    }
                }),
            None => {
                r.missing("amount");
                None
            }
        };
        Some(RefundInput {
            amount: amount.unwrap_or_default(),
        })
    }
}

/// Refund reply.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct RefundCreated {
    pub refund_id: Uuid,
    pub amount: i64,
    /// `creating` until the provider confirms; its status follows by webhook.
    #[schema(example = "creating")]
    pub status: String,
}

/// Trusted native refund admission; settlement requires authoritative payment evidence.
#[derive(Clone, Serialize)]
pub struct RefundReceipt {
    pub refund_id: Uuid,
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirects_are_https_or_local_http() {
        for ok in [
            "https://shop.example.com/x",
            "http://localhost",
            "http://localhost:3000/t",
            "http://127.0.0.1/t",
        ] {
            assert!(redirect(ok), "{ok}");
        }
        for bad in [
            "http://shop.example.com",
            "http://localhost.evil.com/",
            "http://localhost:3000@evil.com/",
            "ftp://x",
            "https://",
            "javascript:alert(1)",
        ] {
            assert!(!redirect(bad), "{bad}");
        }
    }
}

/// Who did an order action.
#[derive(Serialize, ToSchema)]
pub struct EventActor {
    /// `user`, `till_user` or `app_key`.
    #[schema(example = "user")]
    pub r#type: &'static str,
    pub id: Uuid,
    /// Null once a user left the workspace.
    #[schema(required = true)]
    pub name: Option<String>,
}

/// One action on an order.
#[derive(Serialize, ToSchema)]
pub struct OrderEventResource {
    pub id: Uuid,
    /// `created`, `confirmed`, `paid`, `picked`, `completed`, `cancelled`, `expired`,
    /// `returned`, `refunded` or `invoiced`.
    #[schema(example = "confirmed")]
    pub kind: String,
    pub at: DateTime<Utc>,
    /// Null when the system did it (expiry job, payment provider).
    #[schema(required = true)]
    pub actor: Option<EventActor>,
    /// `refunded`: `{amount}` in minor units, plus `auto: true` for a refund the system owed;
    /// else null.
    #[schema(required = true, value_type = Option<Object>)]
    pub meta: Option<Value>,
}

impl From<crate::modules::orders::trail::EventRow> for OrderEventResource {
    fn from(e: crate::modules::orders::trail::EventRow) -> Self {
        let user_name = match (&e.first_name, &e.last_name) {
            (Some(f), Some(l)) => Some(crate::modules::users::dto::full_name(
                f,
                e.middle_name.as_deref(),
                l,
            )),
            _ => None,
        };
        let actor = match (e.user_id, e.till_user_id, e.app_key_id) {
            (Some(id), _, _) => Some(EventActor {
                r#type: "user",
                id,
                name: user_name,
            }),
            (_, Some(id), _) => Some(EventActor {
                r#type: "till_user",
                id,
                name: e.till_user_name,
            }),
            (_, _, Some(id)) => Some(EventActor {
                r#type: "app_key",
                id,
                name: e.app_key_name,
            }),
            _ => None,
        };
        Self {
            id: e.id,
            kind: e.kind,
            at: e.created_at,
            actor,
            meta: e.meta,
        }
    }
}

/// Order sources (`orders.source`).
pub const SOURCES: &[&str] = &["office", "storefront", "till"];
/// Longest sales summary range.
pub const SUMMARY_DAYS_MAX: i64 = 366;

/// GET /sales-summary query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SummaryQuery {
    /// Sold at or after (RFC 3339). Required.
    #[param(required = true, value_type = String, format = DateTime, example = "2026-11-02T00:00:00+01:00")]
    pub from: Option<String>,
    /// Sold before (RFC 3339), at most 366 days after `from`. Required.
    #[param(required = true, value_type = String, format = DateTime, example = "2026-11-09T00:00:00+01:00")]
    pub to: Option<String>,
    /// Only `office`, `storefront` or `till` orders.
    pub source: Option<String>,
}

/// Validated summary range.
pub struct SummaryRange {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub source: Option<String>,
}

impl SummaryQuery {
    /// The range; 422 when `from`/`to` are missing or malformed, `to` isn't after `from`, the
    /// span passes 366 days, or `source` is unknown.
    pub fn range(&self) -> Result<SummaryRange, crate::error::AppError> {
        use crate::http::list::time;
        let mut r = Rules::default();
        let from = time(&mut r, "from", self.from.as_deref());
        let to = time(&mut r, "to", self.to.as_deref());
        if self.from.as_deref().is_none_or(|v| v.trim().is_empty()) {
            r.missing("from");
        }
        if self.to.as_deref().is_none_or(|v| v.trim().is_empty()) {
            r.missing("to");
        }
        if let (Some(f), Some(t)) = (from, to) {
            r.check("to", t > f, "The to must be after from.");
            r.check(
                "to",
                t - f <= chrono::Duration::days(SUMMARY_DAYS_MAX),
                format!("The range must not pass {SUMMARY_DAYS_MAX} days."),
            );
        }
        let source = r.one_of("source", self.source.clone(), SOURCES, None);
        r.into_result()?;
        match (from, to) {
            (Some(from), Some(to)) => Ok(SummaryRange { from, to, source }),
            _ => Err(crate::error::AppError::Internal(
                "summary range lost".into(),
            )),
        }
    }
}

/// Sales in a range, one row per currency (never summed across currencies).
#[derive(Serialize, ToSchema)]
pub struct SalesSummary {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    /// The `source` asked for; null = every source.
    #[schema(required = true)]
    pub source: Option<String>,
    pub currencies: Vec<CurrencySales>,
}

/// Sales in one currency.
#[derive(Serialize, ToSchema)]
pub struct CurrencySales {
    #[schema(example = "EUR")]
    pub currency: String,
    pub decimals: i16,
    /// Orders sold in the range.
    pub orders: i64,
    /// Their totals incl VAT, minor units, before any refund or return.
    pub gross_total: i64,
    /// Provider refunds of those orders that went through (status `refunded`), minor units.
    /// Credit notes and cash handed back outside the API are not in it.
    pub refunded_total: i64,
}
