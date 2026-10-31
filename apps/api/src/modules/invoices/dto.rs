//! Invoice create/update body (Laravel StoreInvoiceRequest / UpdateInvoiceRequest with
//! BuildRequestItems). Parsed allocation-bounded: item rows keep only their fields and header
//! objects are capped (`pricing::body`). References are resolved by the service.

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, human},
    modules::{
        currencies::dto::CurrencyResource,
        files::dto::FileResource,
        pricing::{
            body::{Capped, Items, MAX_ITEMS, RawItem, Row, Scalar},
            dto::{ItemLine, MarginResource, SummaryResource},
        },
        relations::address::{self, Address},
    },
    money::MAX,
};

/// A reference as sent: None = not sent or blank, Some(None) = not a uuid (never matches).
pub type RefId = Option<Option<Uuid>>;

/// A text field of an update: None = not sent (keep), Some(None) = null/blank (clear).
pub type Patch<T> = Option<Option<T>>;

/// Laravel InvoicePaymentMethod.
const PAYMENT_METHODS: &[&str] = &["bank_transfer", "mollie"];

/// Body of `POST /invoices` and `POST /invoices/{id}`; unknown keys are skipped.
#[derive(Deserialize, Default, ToSchema)]
#[schema(
    description = "Invoice header plus item lines. Create ignores `due_date` and `customer_ref`; update ignores `currency_id`, `payment_method` and `is_credit`, keeps fields left out, and changes only the lines sent (a line with `id` updates it, `deleted: true` removes it, no `id` adds one)."
)]
pub struct InvoiceBody {
    #[schema(value_type = Uuid, required = true)]
    #[serde(default)]
    relation_id: Scalar,
    /// Create only; the workspace default when left out.
    #[schema(value_type = Option<Uuid>)]
    #[serde(default)]
    currency_id: Scalar,
    #[schema(value_type = Option<Uuid>)]
    #[serde(default)]
    addressed_contact_id: Scalar,
    #[schema(value_type = Option<String>, max_length = 255)]
    #[serde(default)]
    reference: Scalar,
    #[schema(value_type = Option<String>, max_length = 1000)]
    #[serde(default)]
    note: Scalar,
    /// Create only: `bank_transfer` or `mollie`.
    #[schema(value_type = Option<String>)]
    #[serde(default)]
    payment_method: Scalar,
    /// Create only.
    #[schema(value_type = Option<bool>)]
    #[serde(default)]
    is_credit: Scalar,
    /// Update only: `YYYY-MM-DD` or a datetime.
    #[schema(value_type = Option<String>)]
    #[serde(default)]
    due_date: Scalar,
    /// Address object; create takes the relation's when left out.
    #[schema(value_type = Option<Object>)]
    #[serde(default)]
    billing_address: Capped,
    /// Update only: `ordered_by`, `po_number`, `reference`.
    #[schema(value_type = Option<Object>)]
    #[serde(default)]
    customer_ref: Capped,
    #[schema(value_type = Option<Object>)]
    #[serde(default)]
    meta: Capped,
    /// Up to 1000 lines.
    #[schema(value_type = Vec<ItemLine>, max_items = 1000)]
    #[serde(default)]
    items: Items,
}

/// `POST /invoices`.
#[derive(Deserialize, ToSchema)]
pub struct CreateInvoiceRequest(InvoiceBody);

/// `POST /invoices/{id}`.
#[derive(Deserialize, ToSchema)]
pub struct UpdateInvoiceRequest(InvoiceBody);

/// `balance.paid`: integer 0 without paid payments, else the paid sum as decimal text.
fn paid_schema() -> utoipa::openapi::Schema {
    use utoipa::openapi::schema::{ObjectBuilder, OneOfBuilder, Type};
    OneOfBuilder::new()
        .item(
            ObjectBuilder::new()
                .schema_type(Type::Integer)
                .enum_values(Some([0])),
        )
        .item(ObjectBuilder::new().schema_type(Type::String))
        .description(Some(
            "0 (integer) when nothing is paid, else the paid sum as decimal text with 8 places",
        ))
        .into()
}

// Doc-only: invoice resources are built with `json!` in `resource.rs`; these structs only
// describe them. The contract check validates real replies against them.
/// Paid, open and total amounts of an invoice.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct InvoiceBalance {
    /// Total minus paid (negative when overpaid); JSON number.
    open: f64,
    #[schema(schema_with = paid_schema)]
    paid: Value,
    /// Total including VAT; JSON number.
    total: f64,
}

/// Paid and open amounts of an invoice in a list.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct InvoiceListBalance {
    #[schema(schema_with = paid_schema)]
    paid: Value,
    /// Total minus paid; JSON number.
    open: f64,
}

/// A payment on an invoice.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct InvoicePayment {
    id: Uuid,
    #[schema(required = true)]
    description: Option<String>,
    /// The payment day at midnight UTC.
    #[schema(value_type = String, format = DateTime)]
    date: String,
    /// Decimal text with 8 places.
    amount: String,
    /// Always null.
    #[schema(value_type = Option<String>, required = true)]
    paid_amount: Option<String>,
    /// 1 or 0.
    is_paid: i32,
    #[schema(required = true)]
    provider: Option<String>,
    #[schema(required = true)]
    provider_id: Option<String>,
    #[schema(required = true)]
    status: Option<String>,
    payment_method: String,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
}

/// One invoice line. Amounts are decimal text.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct InvoiceItem {
    id: Uuid,
    title: String,
    #[schema(required = true)]
    description: Option<String>,
    quantity: String,
    price: String,
    price_incl: String,
    price_incl_vat: String,
    total_each_price_incl: String,
    marge: String,
    /// Null without a purchase price.
    #[schema(required = true)]
    marge_percentage: Option<String>,
    marge_total: String,
    vat: String,
    credited_amount: String,
    /// Always false.
    from_multi_set: bool,
    /// Always false.
    is_parent_multi_set_product: bool,
    amount_left_to_credit: String,
    /// Always empty.
    #[schema(value_type = Vec<Object>, max_items = 0)]
    returnable_serial_numbers: Vec<Value>,
    subtotal: String,
    total: String,
    total_incl: String,
    vat_amount: String,
    discount_fixed: String,
    discount_percentage: String,
    discount_amount: String,
    discount_amount_incl: String,
    purchase_price: String,
    purchase_price_total: String,
    purchase_price_incl: String,
    /// The product's available stock (0 without a product).
    available_stock: Value,
    /// Always false.
    accepted: bool,
    picked: String,
    qty_shipped: String,
    qty_cancelled: String,
    /// Always false.
    billed: bool,
    mutated_from_stock: String,
    backorder: String,
    position: i32,
    /// Always false.
    able_to_pick: bool,
    /// Always null.
    #[schema(value_type = Option<Uuid>, required = true)]
    stock_id: Option<Uuid>,
    /// Always empty.
    #[schema(value_type = Vec<Object>, max_items = 0)]
    serial_numbers: Vec<Value>,
    /// Always null.
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
    #[schema(required = true)]
    product_variant_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    relationships: InvoiceItemLinks,
}

/// Line `relationships`.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct InvoiceItemLinks {
    /// Product fields the line shows (`id`, `name`, `product_nr`, prices, stock); only on product lines.
    #[schema(value_type = Option<Object>)]
    product: Option<Value>,
    /// Compact warehouse; null when unset.
    #[schema(value_type = Option<Object>, required = true)]
    mutate_from_warehouse: Option<Value>,
}

/// An invoice in detail.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct InvoiceResource {
    id: Uuid,
    /// Formatted number; null until sent, `N/A` when cancelled.
    #[schema(required = true)]
    invoice_nr: Option<String>,
    status: String,
    #[schema(required = true)]
    reference: Option<String>,
    /// Token of the public payment page; null until sent.
    #[schema(required = true)]
    payment_hash: Option<String>,
    payment_method: String,
    balance: InvoiceBalance,
    #[schema(required = true)]
    note: Option<String>,
    #[schema(required = true)]
    send_date: Option<DateTime<Utc>>,
    #[schema(required = true)]
    due_date: Option<DateTime<Utc>>,
    items: Vec<InvoiceItem>,
    summary: SummaryResource,
    is_credit: bool,
    #[schema(required = true)]
    previous_invoice_id: Option<Uuid>,
    /// Always empty.
    #[schema(value_type = Vec<Object>, max_items = 0)]
    mollie_refunds: Vec<Value>,
    /// Contact snapshot.
    #[schema(value_type = Option<Object>, required = true)]
    addressed_contact: Option<Value>,
    /// Address snapshot.
    #[schema(value_type = Option<Object>, required = true)]
    billing_address: Option<Value>,
    marge_calculation_summary: MarginResource,
    /// Always null.
    #[schema(value_type = Option<String>, required = true)]
    import_id: Option<String>,
    /// Always null.
    #[schema(value_type = Option<String>, required = true)]
    import_source: Option<String>,
    /// `ordered_by`, `po_number`, `reference`.
    #[schema(value_type = Object)]
    customer_ref: Value,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
    relationships: InvoiceLinks,
}

/// Invoice `relationships`; the always-null and always-empty ones are kept for old clients.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct InvoiceLinks {
    /// The sent PDF; null until sent or when the file is gone.
    #[schema(required = true)]
    pdf_file: Option<FileResource>,
    /// Always null.
    #[schema(value_type = Option<Object>, required = true)]
    estimate: Option<Value>,
    /// The relation (customer) list resource.
    #[schema(value_type = Option<Object>, required = true)]
    relation: Option<Value>,
    #[schema(required = true)]
    currency: Option<CurrencyResource>,
    payments: Vec<InvoicePayment>,
    /// Always empty.
    #[schema(value_type = Vec<Object>, max_items = 0)]
    custom_sections: Vec<Value>,
    /// Always null.
    #[schema(value_type = Option<Object>, required = true)]
    parent_invoice: Option<Value>,
    /// Always empty.
    #[schema(value_type = Vec<Object>, max_items = 0)]
    child_invoices: Vec<Value>,
    /// Always null.
    #[schema(value_type = Option<Object>, required = true)]
    order: Option<Value>,
    /// Always null.
    #[schema(value_type = Option<Object>, required = true)]
    work_order: Option<Value>,
    /// Always null.
    #[schema(value_type = Option<Object>, required = true)]
    collective_invoice: Option<Value>,
    /// Always null.
    #[schema(value_type = Option<Object>, required = true)]
    cash_register: Option<Value>,
}

/// An invoice in a list.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct InvoiceIndexResource {
    id: Uuid,
    /// Formatted number; null until sent, `N/A` when cancelled.
    #[schema(required = true)]
    invoice_nr: Option<String>,
    status: String,
    balance: InvoiceListBalance,
    #[schema(required = true)]
    reference: Option<String>,
    /// Always null in lists.
    #[schema(value_type = Option<String>, required = true)]
    payment_hash: Option<String>,
    payment_method: String,
    #[schema(required = true)]
    note: Option<String>,
    /// Always false.
    has_activities: bool,
    /// Always false.
    has_note_activity: bool,
    /// `meta.on_account` as stored, else null.
    #[schema(required = true)]
    is_on_account: Option<Value>,
    #[schema(required = true)]
    send_date: Option<DateTime<Utc>>,
    #[schema(required = true)]
    due_date: Option<DateTime<Utc>>,
    is_credit: bool,
    /// Total with two decimals, e.g. `"121.00"`.
    total_amount: String,
    summary: SummaryResource,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    /// Always false.
    has_collective_invoice: bool,
    /// Always null.
    #[schema(value_type = Option<String>, required = true)]
    import_id: Option<String>,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
    relationships: InvoiceIndexLinks,
}

/// List row `relationships`.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct InvoiceIndexLinks {
    /// The relation (customer) list resource.
    #[schema(value_type = Option<Object>, required = true)]
    relation: Option<Value>,
    #[schema(required = true)]
    currency: Option<CurrencyResource>,
}

/// `GET /invoices`: a page with `meta`, or with `limit` up to that many rows and no `meta`.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct InvoiceListPage {
    data: Vec<InvoiceIndexResource>,
    /// Left out when `limit` is sent.
    #[schema(schema_with = page_meta)]
    meta: Option<Value>,
}

/// The paging details schema `Paginated` registers.
fn page_meta() -> utoipa::openapi::RefOr<utoipa::openapi::Schema> {
    utoipa::openapi::Ref::from_schema_name("Meta").into()
}

/// One line on the public payment page (no costs or stock).
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct PublicInvoiceItem {
    id: Uuid,
    title: String,
    #[schema(required = true)]
    description: Option<String>,
    quantity: String,
    price: String,
    price_incl: String,
    price_incl_vat: String,
    vat: String,
    subtotal: String,
    total: String,
    total_incl: String,
    discount_fixed: String,
    discount_percentage: String,
    /// Same as `discount_percentage`.
    discount: String,
    discount_amount: String,
    discount_amount_incl: String,
    credited_amount: String,
    amount_left_to_credit: String,
}

/// Currency on the public payment page.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct PublicCurrency {
    name: String,
    asset_type: String,
    #[schema(required = true)]
    symbol_prefix: Option<String>,
    #[schema(required = true)]
    symbol_suffix: Option<String>,
    decimals: i16,
}

/// Public payment page `relationships`.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct PublicInvoiceLinks {
    /// Always null.
    #[schema(value_type = Option<Object>, required = true)]
    order_fields: Option<Value>,
    #[schema(required = true)]
    currency: Option<PublicCurrency>,
}

/// An invoice as its public payment page shows it.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct PublicInvoice {
    /// Formatted number; null until sent.
    #[schema(required = true)]
    invoice_nr: Option<String>,
    is_credit: bool,
    status: String,
    #[schema(required = true)]
    reference: Option<String>,
    /// Open is never below 0 and is 0 once paid, cancelled or credited.
    balance: InvoiceBalance,
    /// The token in the link.
    payment_hash: String,
    payment_method: String,
    /// The workspace's company profile (no credit limit).
    #[schema(value_type = Object)]
    company_details: Value,
    #[schema(required = true)]
    due_date: Option<DateTime<Utc>>,
    items: Vec<PublicInvoiceItem>,
    summary: SummaryResource,
    relationships: PublicInvoiceLinks,
}

/// Validated header fields plus item rows. Create ignores `due_date`/`customer_ref`; update
/// ignores `currency_id`, `payment_method` and `is_credit` (Laravel's rule sets).
pub struct InvoiceInput {
    /// Required on both; Some(None) = not a uuid.
    pub relation_id: Option<Uuid>,
    pub currency_id: RefId,
    pub addressed_contact_id: Patch<Option<Uuid>>,
    pub reference: Patch<String>,
    pub note: Patch<String>,
    pub payment_method: Option<String>,
    pub is_credit: bool,
    pub due_date: Patch<DateTime<Utc>>,
    /// Patch of the billing address snapshot (create: None takes the relation's).
    pub billing_address: Patch<Address>,
    pub customer_ref: Patch<Value>,
    pub meta: Patch<Value>,
    pub items: Vec<ItemRow>,
}

/// One validated `items` entry; `index` is its place in the body (error keys).
pub struct ItemRow {
    pub index: usize,
    pub kind: RowKind,
}

/// What a row asks for. Rows not sent are left alone (Laravel: no full-set diff).
pub enum RowKind {
    /// No id: a new line.
    New(ItemFields),
    /// An existing line of this invoice (checked by the service).
    Update(Uuid, ItemFields),
    /// `deleted: true`: remove the line; an id that isn't on the invoice is skipped.
    Delete(Option<Uuid>),
    /// A set child (`multi_set_product_id`) or a row whose set parent was deleted: Laravel
    /// rebuilt those from the parent; genesy has no product sets yet.
    // shortcut: no multi-set products in genesy; port set children with the product sets.
    Skip,
}

/// Fields of a new or updated line.
pub struct ItemFields {
    pub title: String,
    /// None = not sent (an update keeps it), Some(None) = null or blank.
    pub description: Patch<String>,
    pub quantity: Decimal,
    /// None: a product line left the price out (stored as 0, Laravel did no lookup).
    pub price: Option<Decimal>,
    pub vat: Decimal,
    /// `productDiscount`; None = not sent (an update keeps the stored one, a new line has 0).
    pub discount: Option<Decimal>,
    pub purchase_price: Option<Decimal>,
    pub position: Option<i32>,
    pub product_id: RefId,
    pub product_variant_id: RefId,
    /// None = not sent (an update keeps it), Some(None) = null (clears).
    pub mutate_from_warehouse_id: Patch<Option<Uuid>>,
}

/// `{data:{invoice_id}}` of create and duplicate.
#[derive(Serialize, ToSchema)]
pub struct InvoiceCreated {
    pub invoice_id: Uuid,
}

// List query (Laravel ListInvoiceRequest + pagination).
/// List query.
#[derive(Deserialize, Default, IntoParams)]
#[serde(default)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
    /// Only invoices of this relation.
    #[param(value_type = Option<Uuid>)]
    pub relation_id: Option<String>,
    /// Search text.
    pub q: Option<String>,
    /// 1 to 100: return that many rows without `meta` instead of a page.
    #[param(value_type = Option<i64>)]
    pub limit: Option<String>,
    /// Only this status.
    pub status: Option<String>,
    /// Comma-separated statuses (`draft`, `pending`, `paid`, ...).
    pub in_status: Option<String>,
    /// `1`/`0`: only credit notes, or only invoices.
    #[param(value_type = Option<bool>)]
    pub is_credit: Option<String>,
    /// `1`: only invoices put on account.
    #[param(value_type = Option<bool>)]
    pub on_account: Option<String>,
}

impl Validate for CreateInvoiceRequest {
    type Valid = InvoiceInput;

    fn validate(self, r: &mut Rules) -> Option<InvoiceInput> {
        let mut input = header(r, &self.0, true);
        input.items = items(r, self.0.items);
        Some(input)
    }
}

impl Validate for UpdateInvoiceRequest {
    type Valid = InvoiceInput;

    fn validate(self, r: &mut Rules) -> Option<InvoiceInput> {
        let mut input = header(r, &self.0, false);
        input.items = items(r, self.0.items);
        Some(input)
    }
}

fn header(r: &mut Rules, b: &InvoiceBody, create: bool) -> InvoiceInput {
    let relation_id = match b.relation_id.filled() {
        None => {
            r.missing("relation_id");
            None
        }
        Some(v) => Some(uuid_of(&v).unwrap_or(Uuid::nil())),
    };
    let payment_method = create
        .then(|| {
            let v = b.payment_method.filled()?;
            // A non-string never matches (Laravel EnumExist lowercased strings only).
            let s = v
                .as_str()
                .map_or_else(|| "\0".to_owned(), str::to_lowercase);
            r.one_of("payment_method", Some(s), PAYMENT_METHODS, None)
        })
        .flatten();
    let is_credit = create
        && b.is_credit
            .filled()
            .and_then(|v| r.boolean("is_credit", &v))
            .unwrap_or(false);
    InvoiceInput {
        relation_id,
        currency_id: create.then(|| ref_id(&b.currency_id)).flatten(),
        addressed_contact_id: patch(&b.addressed_contact_id).map(|v| v.map(|v| uuid_of(&v))),
        reference: text(r, "reference", &b.reference, 255),
        note: text(r, "note", &b.note, 1000),
        payment_method,
        is_credit,
        due_date: (!create).then(|| due_date(r, &b.due_date)).flatten(),
        billing_address: billing_address(r, &b.billing_address),
        customer_ref: (!create)
            .then(|| {
                let v = object(r, "customer_ref", &b.customer_ref)?;
                Some(v.map(|v| customer_ref(r, &v)))
            })
            .flatten(),
        meta: object(r, "meta", &b.meta),
        items: Vec::new(),
    }
}

/// Sent as null/blank -> Some(None); absent -> None; else Some(Some(value)).
fn patch(v: &Scalar) -> Option<Option<Value>> {
    match v {
        Scalar::Absent => None,
        other => Some(other.filled()),
    }
}

fn uuid_of(v: &Value) -> Option<Uuid> {
    v.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok())
}

fn ref_id(v: &Scalar) -> RefId {
    v.filled().map(|v| uuid_of(&v))
}

/// Laravel `nullable|string|max:N` after TrimStrings (blank = null), patch semantics.
fn text(r: &mut Rules, field: &str, v: &Scalar, max: usize) -> Patch<String> {
    match patch(v)? {
        None => Some(None),
        Some(Value::String(s)) => {
            let s = s.trim().to_owned();
            r.max_chars(field, &s, max);
            Some(Some(s))
        }
        Some(_) => {
            r.add(field, format!("The {} must be a string.", human(field)));
            None
        }
    }
}

/// A capped object field (`meta`, `customer_ref`): null = clear, an object = set.
fn object(r: &mut Rules, field: &str, v: &Capped) -> Patch<Value> {
    match v {
        Capped::Absent => None,
        Capped::TooLarge => {
            r.add(field, format!("The {} is too large.", human(field)));
            None
        }
        Capped::Value(v) => r.meta_at(field, Some(v.clone())),
    }
}

/// Laravel CustomerRef value: `ordered_by`, `po_number` (sent as `po_no` or `po_number`),
/// `reference`, each a trimmed string of at most 255 chars or null.
fn customer_ref(r: &mut Rules, v: &Value) -> Value {
    let mut pick = |out: &str, sent: &[&str]| {
        let key = format!("customer_ref.{out}");
        let found = sent.iter().find_map(|k| v.get(*k).filter(|x| !x.is_null()));
        match found {
            Some(Value::String(s)) if !s.trim().is_empty() => {
                let s = s.trim();
                r.max_chars(&key, s, 255);
                Value::from(s)
            }
            Some(Value::String(_)) | None => Value::Null,
            Some(_) => {
                r.add(&key, format!("The {} must be a string.", human(&key)));
                Value::Null
            }
        }
    };
    serde_json::json!({
        "ordered_by": pick("ordered_by", &["ordered_by"]),
        "po_number": pick("po_number", &["po_number", "po_no"]),
        "reference": pick("reference", &["reference"]),
    })
}

/// Laravel `Address` rule on `billing_address` (bare field keys, as Laravel's errors).
fn billing_address(r: &mut Rules, v: &Capped) -> Patch<Address> {
    match v {
        Capped::Absent | Capped::Value(Value::Null) => {
            matches!(v, Capped::Value(_)).then_some(None)
        }
        Capped::TooLarge => {
            r.add("billing_address", "The billing address is too large.");
            None
        }
        Capped::Value(v) => address::validate_one(r, "billing_address", v).map(Some),
    }
}

/// Laravel `nullable|date|after:yesterday` on update: today or later.
fn due_date(r: &mut Rules, v: &Scalar) -> Patch<DateTime<Utc>> {
    let v = patch(v)?;
    let Some(v) = v else {
        return Some(None);
    };
    let s = v.as_str().map(str::trim).unwrap_or_default();
    let at = NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .or_else(|| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok())
        .map(|t| t.and_utc())
        .or_else(|| DateTime::parse_from_rfc3339(s).ok().map(|t| t.to_utc()))
        .filter(|t| (1..=9999).contains(&chrono::Datelike::year(t)));
    let Some(at) = at else {
        r.add("due_date", "The due date is not a valid date.");
        return None;
    };
    if at.date_naive() < Utc::now().date_naive() {
        r.add("due_date", "The due date must be a date after yesterday.");
        return None;
    }
    Some(Some(at))
}

fn items(r: &mut Rules, items: Items) -> Vec<ItemRow> {
    let rows = match items {
        Items::Missing => return Vec::new(),
        Items::List(rows) => rows,
        Items::TooMany => {
            r.add(
                "items",
                format!("The items must not have more than {MAX_ITEMS} items."),
            );
            return Vec::new();
        }
        Items::NotList => {
            r.add("items", "The items must be an array.");
            return Vec::new();
        }
    };
    rows.into_iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let key = format!("items.{index}");
            match row {
                Row::Item(raw) => Some(ItemRow {
                    index,
                    kind: item(r, &key, *raw),
                }),
                Row::NotObject => {
                    r.add(&key, format!("The {key} must be an array."));
                    None
                }
            }
        })
        .collect()
}

fn item(r: &mut Rules, key: &str, raw: RawItem) -> RowKind {
    let f = |name: &str| format!("{key}.{name}");
    let flag = |r: &mut Rules, name: &str, v: &Scalar| {
        v.filled()
            .and_then(|v| r.boolean(&f(name), &v))
            .unwrap_or(false)
    };
    let id = match raw.id.filled() {
        None => None,
        Some(v) => match uuid_of(&v) {
            Some(id) => Some(id),
            None => {
                r.add(
                    &f("id"),
                    format!("The {} must be a valid UUID.", human(&f("id"))),
                );
                return RowKind::Skip;
            }
        },
    };
    if flag(r, "deleted", &raw.deleted) {
        return RowKind::Delete(id);
    }
    let parent_deleted = flag(r, "parent_deleted", &raw.parent_deleted);
    if (id.is_some() && parent_deleted) || (id.is_none() && raw.multi_set_product_id.truthy()) {
        return RowKind::Skip;
    }
    let mut num = |name: &str, v: &Scalar, min: Decimal, max: Decimal| {
        v.filled().and_then(|v| r.decimal(&f(name), &v, min, max))
    };
    let quantity = num("quantity", &raw.quantity, -MAX, MAX);
    let price = num("price", &raw.price, -MAX, MAX);
    let vat = num("vat", &raw.vat, Decimal::ZERO, MAX);
    let discount = num(
        "productDiscount",
        &raw.product_discount,
        Decimal::ZERO,
        Decimal::ONE_HUNDRED,
    );
    let purchase_price = num("purchase_price", &raw.purchase_price, -MAX, MAX);
    if raw.quantity.filled().is_none() {
        r.missing(&f("quantity"));
    }
    if raw.vat.filled().is_none() {
        r.missing(&f("vat"));
    }
    let product_id = item_ref(r, &f("product_id"), &raw.product_id);
    if raw.price.filled().is_none() && product_id.is_none() {
        let msg = format!(
            "The {} field is required when {} is not present.",
            human(&f("price")),
            human(&f("product_id"))
        );
        r.add(&f("price"), msg);
    }
    let title = match raw.title.filled() {
        None => {
            r.missing(&f("title"));
            String::new()
        }
        Some(Value::String(s)) => {
            let s = s.trim().to_owned();
            r.max_chars(&f("title"), &s, 255);
            s
        }
        Some(_) => {
            r.add(
                &f("title"),
                format!("The {} must be a string.", human(&f("title"))),
            );
            String::new()
        }
    };
    let description = text(r, &f("description"), &raw.description, 5000);
    let position = raw
        .position
        .filled()
        .and_then(|v| r.integer(&f("position"), &v, i32::MIN.into(), i32::MAX.into()))
        .and_then(|n| i32::try_from(n).ok());
    let fields = ItemFields {
        title,
        description,
        quantity: quantity.unwrap_or_default(),
        price,
        vat: vat.unwrap_or_default(),
        discount,
        purchase_price,
        position,
        product_id,
        product_variant_id: item_ref(r, &f("product_variant_id"), &raw.product_variant_id),
        mutate_from_warehouse_id: match raw.mutate_from_warehouse_id {
            Scalar::Absent => None,
            ref v => Some(item_ref(r, &f("mutate_from_warehouse_id"), v)),
        },
    };
    match id {
        Some(id) => RowKind::Update(id, fields),
        None => RowKind::New(fields),
    }
}

/// `nullable|uuid` on an item reference.
fn item_ref(r: &mut Rules, field: &str, v: &Scalar) -> RefId {
    let v = v.filled()?;
    match uuid_of(&v) {
        Some(id) => Some(Some(id)),
        None => {
            r.add(field, format!("The {} must be a valid UUID.", human(field)));
            Some(None)
        }
    }
}

// POST /invoices/{id}/credit (Laravel CreditInvoiceRequest); unknown keys are skipped.
/// POST /invoices/{id}/credit.
#[derive(Deserialize, Default, ToSchema)]
#[schema(description = "Credit the whole invoice (`credit_all`) or the listed lines.")]
pub struct CreditInvoiceRequest {
    #[schema(value_type = Option<bool>)]
    #[serde(default)]
    credit_all: Scalar,
    /// Required unless `credit_all` is true; up to 1000 lines.
    #[schema(value_type = Option<Vec<CreditLineBody>>)]
    #[serde(default)]
    items: Capped,
}

// Doc-only: one `items` entry the credit validation reads key by key.
/// One line to credit.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct CreditLineBody {
    /// A line of the invoice.
    id: Uuid,
    /// Quantity to credit; decimal.
    credit_amount: Option<f64>,
    /// Quantity returned to stock; decimal.
    return_amount: Option<f64>,
    /// Accepted and ignored.
    disable_discount: Option<bool>,
}

/// One selected line of a credit. `disable_discount` is accepted and ignored, as Laravel's
/// credit from an invoice did (a return amount drops the discount instead).
pub struct CreditLine {
    pub index: usize,
    pub id: Uuid,
    pub credit_amount: Option<Decimal>,
    pub return_amount: Option<Decimal>,
}

/// A validated credit: every line's remaining quantity, or the selected lines.
pub struct CreditInput {
    pub credit_all: bool,
    pub items: Vec<CreditLine>,
}

impl Validate for CreditInvoiceRequest {
    type Valid = CreditInput;

    fn validate(self, r: &mut Rules) -> Option<CreditInput> {
        let sent = self
            .credit_all
            .filled()
            .and_then(|v| r.boolean("credit_all", &v));
        let credit_all = sent.unwrap_or(false);
        if credit_all {
            return Some(CreditInput {
                credit_all,
                items: Vec::new(),
            });
        }
        let list = match &self.items {
            // Laravel `required_if:credit_all,false`.
            Capped::Absent | Capped::Value(Value::Null) if sent == Some(false) => {
                r.add(
                    "items",
                    "The items field is required when credit all is false.",
                );
                return None;
            }
            Capped::Absent | Capped::Value(Value::Null) => {
                return Some(CreditInput {
                    credit_all,
                    items: Vec::new(),
                });
            }
            Capped::Value(Value::Array(list)) => list,
            Capped::TooLarge => {
                r.add("items", "The items is too large.");
                return None;
            }
            Capped::Value(_) => {
                r.add("items", "The items must be an array.");
                return None;
            }
        };
        if list.len() > MAX_ITEMS {
            r.add(
                "items",
                format!("The items must not have more than {MAX_ITEMS} items."),
            );
            return None;
        }
        let mut items = Vec::with_capacity(list.len());
        for (index, v) in list.iter().enumerate() {
            let f = |k: &str| format!("items.{index}.{k}");
            let empty = serde_json::Map::new();
            let m = v.as_object().unwrap_or(&empty);
            let field = |k: &str| m.get(k).filter(|v| !v.is_null() && v.as_str() != Some(""));
            let id = match field("id") {
                None => {
                    r.missing(&f("id"));
                    None
                }
                Some(v) => {
                    let id = v.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok());
                    if id.is_none() {
                        r.add(
                            &f("id"),
                            format!("The {} must be a valid UUID.", human(&f("id"))),
                        );
                    }
                    id
                }
            };
            let mut num = |k: &str| field(k).and_then(|v| r.decimal(&f(k), v, -MAX, MAX));
            let credit_amount = num("credit_amount");
            let return_amount = num("return_amount");
            if let Some(v) = field("disable_discount") {
                r.boolean(&f("disable_discount"), v);
            }
            if let Some(id) = id {
                items.push(CreditLine {
                    index,
                    id,
                    credit_amount,
                    return_amount,
                });
            }
        }
        Some(CreditInput { credit_all, items })
    }
}
