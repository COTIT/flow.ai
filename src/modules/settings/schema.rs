//! Laravel StoreSettingRequest + Values/*Setting as data. Each writable section is a field
//! table (one rule per key) plus a default tree; one walker validates a sent section, keeps
//! only the listed keys and refuses credentials (stored nowhere until F2b adds encryption).

use chrono::{DateTime, NaiveDate, NaiveDateTime};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::{
    http::json::{Rules, human, is_url},
    money::MAX,
    numbering::MAX_START,
};

/// 422 text for a credential sent before F2b.
pub const SECRET_REFUSED: &str = "Integration credentials can't be stored yet.";

/// Every key that holds a credential; never stored, always printed as null.
pub const SECRET_PATHS: &[&str] = &[
    "smtp.password",
    "integrations.payment.payment_providers.mollie.api_key",
    "integrations.payment.payment_providers.pay.api_code",
    "integrations.payment.payment_providers.pay.api_token",
    "integrations.payment.payment_providers.pay.service_id",
    "integrations.fulfillment.fulfillment_providers.sendcloud.public",
    "integrations.fulfillment.fulfillment_providers.sendcloud.secret",
    "integrations.accounting.accounting_providers.jortt.client_secret",
    "integrations.export.export_providers.modjo.api_key",
    "integrations.syncProviders.picqer.api_key",
    "integrations.mail.graph.client_secret",
];

/// Laravel validated `string` without a max; keeps junk-size text out of the row.
const LONG: usize = 65_535;

/// A row another table must hold in this workspace (the service checks it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Table {
    Warehouses,
    Relations,
    /// Custom types of the `relation` domain.
    RelationTypes,
    Teams,
}

/// How one key is validated and kept.
pub enum Rule {
    /// Nullable string of at most N chars.
    Text(usize),
    /// String or number, kept as a string of at most N chars.
    TextOrNumber(usize),
    /// Integer in range.
    Int(i64, i64),
    Bool,
    Number,
    Email,
    Date,
    OneOf(&'static [&'static str]),
    /// SmtpRule: `ssl` or `tls`.
    Encryption,
    Ref(Table),
    /// A credential: null or blank is kept as null, anything else is refused.
    Secret,
    /// http(s) URL without credentials in it (an endpoint a later batch will call).
    Url,
    /// List of at most N integers.
    Ints(usize),
    /// List of at most `.1` strings of at most `.0` chars.
    Strs(usize, usize),
    /// List of at most N numbers, kept as sent.
    Numbers(usize),
    Object(&'static [Field]),
    /// List of at most N objects.
    List(&'static [Field], usize),
}

/// One key of a section.
pub struct Field {
    pub key: &'static str,
    pub rule: Rule,
    pub required: bool,
    /// False: validated like Laravel but never stored (no property on the Values class).
    pub keep: bool,
}

const fn f(key: &'static str, rule: Rule) -> Field {
    Field {
        key,
        rule,
        required: false,
        keep: true,
    }
}

const fn req(key: &'static str, rule: Rule) -> Field {
    Field {
        key,
        rule,
        required: true,
        keep: true,
    }
}

const fn checked_only(key: &'static str, rule: Rule) -> Field {
    Field {
        key,
        rule,
        required: false,
        keep: false,
    }
}

const ANY: Rule = Rule::Int(i64::MIN, i64::MAX);
const DIGITS: Rule = Rule::Int(i64::MIN, 10);
/// A start number past this would let the series overflow (Laravel took any integer).
const START: Rule = Rule::Int(0, MAX_START);
/// Payment terms and validity periods in days; later batches add them to dates.
const DAYS: Rule = Rule::Int(0, 3650);

const SMTP: &[Field] = &[
    f("host", Rule::Text(255)),
    f("username", Rule::Text(255)),
    f("password", Rule::Secret),
    f("port", ANY),
    f("encryption", Rule::Encryption),
    f("from_name", Rule::Text(255)),
    f("from_address", Rule::Text(255)),
    f("bcc_all_emails", Rule::Bool),
];

const CUSTOMER_REF: &[Field] = &[
    f("ordered_by", Rule::Bool),
    f("po_number", Rule::Bool),
    f("reference", Rule::Bool),
];

const INVOICE: &[Field] = &[
    f("prefix", Rule::Text(10)),
    f("suffix", Rule::Text(10)),
    f("start_number", START),
    f("default_payment_term", DAYS),
    f("default_payment_term_company", DAYS),
    f("default_footer_message", Rule::Text(2500)),
    f("default_note", Rule::Text(1000)),
    f("digits", DIGITS),
    f("customer_ref", Rule::Object(CUSTOMER_REF)),
];

/// Credit notes and collective invoices: invoice fields, shorter footer, no customer_ref.
const TERMS: &[Field] = &[
    f("prefix", Rule::Text(10)),
    f("suffix", Rule::Text(10)),
    f("start_number", START),
    f("default_payment_term", DAYS),
    f("default_payment_term_company", DAYS),
    f("default_footer_message", Rule::Text(255)),
    f("default_note", Rule::Text(1000)),
    f("digits", DIGITS),
];

const ESTIMATE: &[Field] = &[
    f("prefix", Rule::Text(10)),
    f("suffix", Rule::Text(10)),
    f("start_number", START),
    f("default_validity_period", DAYS),
    f("default_validity_period_company", DAYS),
    f("default_footer_message", Rule::Text(255)),
    f("default_note", Rule::Text(1000)),
    f("digits", DIGITS),
    checked_only("default_due_date_in_days", Rule::Int(0, i64::MAX)),
];

const NUMBERING: &[Field] = &[
    f("prefix", Rule::Text(10)),
    f("suffix", Rule::Text(10)),
    f("start_number", START),
    f("digits", DIGITS),
];

const WORK_ORDER: &[Field] = &[
    f("prefix", Rule::Text(10)),
    f("suffix", Rule::Text(10)),
    f("start_number", START),
    f("digits", DIGITS),
    f("fetch_users_from_team_id", Rule::Ref(Table::Teams)),
];

const PRODUCT: &[Field] = &[
    f("prefix", Rule::Text(10)),
    f("suffix", Rule::Text(10)),
    f("start_number", START),
    f("digits", DIGITS),
    f(
        "second_chance_product_default_warehouse_id",
        Rule::Ref(Table::Warehouses),
    ),
    f("set_product_nla_if_out_of_stock", Rule::Bool),
    f("disable_double_discount_for_business", Rule::Bool),
    f("warning_below_preferred_marge", Rule::Int(0, 100)),
    f("user_search_history_active", Rule::Bool),
    f("user_search_history_count", Rule::Int(1, 40)),
    f("supplier_wholesale_prices_active", Rule::Bool),
    f("advice_prices_active", Rule::Bool),
];

const SHIPPING_OPTION: &[Field] = &[
    req("type", Rule::OneOf(&["delivery", "pickup"])),
    req("name", Rule::Text(40)),
    f("value", Rule::Ref(Table::Warehouses)),
];

const CASH_REGISTER: &[Field] = &[
    f("prefix", Rule::Text(10)),
    f("suffix", Rule::Text(10)),
    f("start_number", START),
    f("digits", DIGITS),
    f("receipt_line_length", Rule::Int(20, 80)),
    f("receipt_footer", Rule::Text(1500)),
    f("receipt_header", Rule::Text(1500)),
    f("receipt_logo_id", Rule::Text(1500)),
    f("accepted_cash", Rule::Numbers(50)),
    f("use_pin_code_after_each_order", Rule::Bool),
    f("allow_individual_lines", Rule::Bool),
    f(
        "cash_drawer_link_mode",
        Rule::OneOf(&["strict", "flexible"]),
    ),
    f("create_backorder_from_other_warehouses", Rule::Bool),
    f("default_relation_id", Rule::Ref(Table::Relations)),
    f(
        "default_mutate_from_warehouse_id",
        Rule::Ref(Table::Warehouses),
    ),
    f("default_relation_type_id", Rule::Ref(Table::RelationTypes)),
    f("shipping_options", Rule::List(SHIPPING_OPTION, 50)),
];

const LEDGER: &[Field] = &[f("name", Rule::Text(LONG)), req("no", Rule::Number)];

const INCOMING_INVOICE: &[Field] = &[
    f("required_approvals", Rule::Int(0, 100)),
    f("ledger_number", Rule::TextOrNumber(50)),
    f("ignore_due_date_on_import", Rule::Bool),
    f("ledgers", Rule::List(LEDGER, 500)),
];

const GENERAL: &[Field] = &[
    f("language", Rule::Text(255)),
    f("timezone", Rule::Text(255)),
    f("measurement_system", Rule::Text(255)),
    f("datetime_format", Rule::Text(255)),
    f("thousand_separator", Rule::Text(255)),
    f("decimal_separator", Rule::Text(255)),
    f("color", Rule::Text(255)),
];

/// A PAY.nl terminal as vimdesk-web saves it (`pay/edit.vue` keeps id + name).
const TERMINAL: &[Field] = &[
    f("id", Rule::Text(255)),
    f("name", Rule::Text(255)),
    f("simulation", Rule::Bool),
];

const INTEGRATIONS: &[Field] = &[
    f(
        "fulfillment",
        Rule::Object(&[
            f("active_provider", Rule::Text(LONG)),
            f(
                "fulfillment_providers",
                Rule::Object(&[f(
                    "sendcloud",
                    Rule::Object(&[
                        f("secret", Rule::Secret),
                        f("public", Rule::Secret),
                        f("countries", Rule::Strs(2, 300)),
                    ]),
                )]),
            ),
        ]),
    ),
    f(
        "payment",
        Rule::Object(&[
            f("active_provider", Rule::Text(LONG)),
            f(
                "payment_providers",
                Rule::Object(&[
                    f("mollie", Rule::Object(&[f("api_key", Rule::Secret)])),
                    f(
                        "pay",
                        Rule::Object(&[
                            f("api_code", Rule::Secret),
                            f("api_token", Rule::Secret),
                            f("service_id", Rule::Secret),
                            f("terminals", Rule::List(TERMINAL, 100)),
                        ]),
                    ),
                ]),
            ),
        ]),
    ),
    f(
        "accounting",
        Rule::Object(&[
            f("active_provider", Rule::Text(LONG)),
            f(
                "accounting_providers",
                Rule::Object(&[f(
                    "jortt",
                    Rule::Object(&[
                        f("client_id", Rule::Text(LONG)),
                        f("client_secret", Rule::Secret),
                    ]),
                )]),
            ),
        ]),
    ),
    f(
        "export",
        Rule::Object(&[
            f("external_product_stock_sync", Rule::Bool),
            f(
                "export_providers",
                Rule::Object(&[
                    f(
                        "prestashop",
                        Rule::Object(&[
                            f("shop_url", Rule::Url),
                            f("error_email", Rule::Email),
                            f("orders_enabled", Rule::Bool),
                            f("orders_from", Rule::Text(LONG)),
                            f("products_price_sync", Rule::Bool),
                            f("products_stock_sync", Rule::Bool),
                            f("products_data_sync", Rule::Bool),
                        ]),
                    ),
                    f(
                        "hostbill",
                        Rule::Object(&[f("enabled", Rule::Bool), f("endpoint", Rule::Url)]),
                    ),
                    f(
                        "modjo",
                        Rule::Object(&[
                            f("enabled", Rule::Bool),
                            f("api_key", Rule::Secret),
                            f("custom_type_id", Rule::Text(LONG)),
                        ]),
                    ),
                ]),
            ),
        ]),
    ),
    f(
        "syncProviders",
        Rule::Object(&[f(
            "picqer",
            Rule::Object(&[
                f("base_url", Rule::Url),
                f("api_key", Rule::Secret),
                f("user_agent", Rule::Text(LONG)),
                f("order_export_warehouse_import_id", Rule::Text(LONG)),
                f("order_export_start_date", Rule::Date),
                f("order_export_tag_ids", Rule::Ints(500)),
                f("order_export_business_tag_ids", Rule::Ints(500)),
                f("order_export_pickup_tag_ids", Rule::Ints(500)),
                f("order_export_skip_import_sources", Rule::Strs(255, 100)),
                f("product_export_tag_id", ANY),
                f("auto_create_products", Rule::Bool),
                f("auto_create_second_chance_products", Rule::Bool),
                f("cancelled_order_tag_name", Rule::Text(255)),
            ]),
        )]),
    ),
    f(
        "mail",
        Rule::Object(&[
            f("driver", Rule::OneOf(&["imap", "graph"])),
            f(
                "graph",
                Rule::Object(&[
                    f("tenant_id", Rule::Text(255)),
                    f("client_id", Rule::Text(255)),
                    f("client_secret", Rule::Secret),
                    f("mailbox", Rule::Email),
                    f("processed_category", Rule::Text(255)),
                    f("delta_token", Rule::Text(LONG)),
                ]),
            ),
            f(
                "outbound",
                Rule::Object(&[
                    f("send_enabled", Rule::Bool),
                    f(
                        "default_sender",
                        Rule::Object(&[f("email", Rule::Email), f("sender_name", Rule::Text(255))]),
                    ),
                    f(
                        "senders",
                        Rule::List(
                            &[f("email", Rule::Email), f("sender_name", Rule::Text(255))],
                            100,
                        ),
                    ),
                ]),
            ),
        ]),
    ),
];

/// Sections POST may write, with their fields (`meta` is a free object, handled apart).
/// `order` and `purchase_order` had no rules in Laravel, so they stay read-only.
pub const WRITABLE: &[(&str, &[Field])] = &[
    ("smtp", SMTP),
    ("invoice", INVOICE),
    ("incoming_invoice", INCOMING_INVOICE),
    ("collective_invoice", TERMS),
    ("estimate", ESTIMATE),
    ("work_order", WORK_ORDER),
    ("credit", TERMS),
    ("relation", NUMBERING),
    ("product", PRODUCT),
    ("cash_register", CASH_REGISTER),
    ("integrations", INTEGRATIONS),
    ("general", GENERAL),
];

/// Sections GET returns (Laravel SettingResource), `meta` last and raw.
pub const READABLE: &[&str] = &[
    "smtp",
    "invoice",
    "estimate",
    "credit",
    "relation",
    "general",
    "product",
    "cash_register",
    "purchase_order",
    "order",
    "collective_invoice",
    "work_order",
    "incoming_invoice",
    "integrations",
];

/// Laravel `XSetting::empty()` / `from([])` for `section`; None for an unknown section.
pub fn defaults(section: &str) -> Option<Value> {
    let numbering = |prefix: &str, digits: i64| json!({ "prefix": prefix, "suffix": null, "start_number": 1, "digits": digits });
    let terms = |prefix: &str| {
        json!({ "prefix": prefix, "suffix": null, "start_number": 1, "default_payment_term": 14,
                "default_payment_term_company": 30, "default_footer_message": null,
                "default_note": null, "digits": 4 })
    };
    Some(match section {
        "smtp" => json!({ "host": null, "username": null, "password": null, "port": null,
                          "encryption": null, "from_name": null, "from_address": null,
                          "bcc_all_emails": null }),
        "invoice" => json!({ "prefix": null, "suffix": null, "start_number": null,
                             "default_payment_term": null, "default_payment_term_company": null,
                             "default_footer_message": null, "default_note": null, "digits": null,
                             "customer_ref": { "ordered_by": true, "po_number": false,
                                               "reference": false } }),
        "estimate" => json!({ "prefix": "EST-", "suffix": null, "start_number": 1,
                              "default_validity_period": 14, "default_validity_period_company": 30,
                              "default_footer_message": null, "default_note": null, "digits": 4 }),
        "credit" => terms("CRE-"),
        "collective_invoice" => terms("COL-INV-"),
        "relation" => numbering("RE-", 4),
        "order" => numbering("ORD-", 4),
        "purchase_order" => numbering("PO-", 5),
        "work_order" => {
            let mut v = numbering("WO-", 4);
            v["fetch_users_from_team_id"] = Value::Null;
            v
        }
        "product" => json!({ "prefix": "PRO-", "suffix": null, "start_number": 1, "digits": 5,
                             "second_chance_product_default_warehouse_id": null,
                             "set_product_nla_if_out_of_stock": null,
                             "disable_double_discount_for_business": false,
                             "warning_below_preferred_marge": null,
                             "user_search_history_active": false, "user_search_history_count": 10,
                             "supplier_wholesale_prices_active": false,
                             "advice_prices_active": false }),
        "cash_register" => json!({ "prefix": null, "suffix": null, "start_number": null,
                                   "digits": null, "default_relation_id": null,
                                   "receipt_footer": null, "receipt_header": null,
                                   "receipt_logo_id": null, "use_pin_code_after_each_order": null,
                                   "create_backorder_from_other_warehouses": null,
                                   "default_mutate_from_warehouse_id": null,
                                   "default_relation_type_id": null, "accepted_cash": null,
                                   "shipping_options": null, "cash_drawer_link_mode": null,
                                   "receipt_line_length": 34, "allow_individual_lines": true }),
        "incoming_invoice" => json!({ "ledger_number": null, "required_approvals": 1,
                                      "ignore_due_date_on_import": false, "ledgers": [],
                                      "price_tolerance_percent": 0.0,
                                      "price_tolerance_amount": 0.0,
                                      "quantity_tolerance_percent": 0.0,
                                      "auto_approve_enabled": false, "auto_approve_min_score": 95,
                                      "review_min_score": 80 }),
        "general" => json!({ "language": "en", "timezone": "UTC", "measurement_system": "metric",
                             "datetime_format": "YYYY-mm-dd", "thousand_separator": ".",
                             "decimal_separator": ",", "color": "default" }),
        "integrations" => json!({
            "fulfillment": { "active_provider": null, "fulfillment_providers": {
                "sendcloud": { "public": null, "secret": null, "countries": [] } } },
            "payment": { "active_provider": null, "payment_providers": {
                "mollie": { "api_key": null },
                "pay": { "api_code": null, "api_token": null, "service_id": null,
                         "terminals": [] } } },
            "accounting": { "active_provider": null, "accounting_providers": {
                "jortt": { "client_id": null, "client_secret": null } } },
            "export": { "external_product_stock_sync": false, "export_providers": {
                "prestashop": { "shop_url": null, "error_email": null, "orders_enabled": false,
                                "orders_from": null, "products_price_sync": null,
                                "products_stock_sync": null, "products_data_sync": false },
                "modjo": { "enabled": null, "api_key": null, "custom_type_id": null },
                "hostbill": null } },
            "syncProviders": { "picqer": {
                "base_url": null, "api_key": null, "user_agent": null,
                "order_export_warehouse_import_id": null, "order_export_start_date": null,
                "order_export_tag_ids": [], "order_export_business_tag_ids": [],
                "order_export_pickup_tag_ids": [], "order_export_skip_import_sources": [],
                "product_export_tag_id": null, "auto_create_products": false,
                "auto_create_second_chance_products": false, "cancelled_order_tag_name": null } },
            "mail": { "driver": "imap",
                "graph": { "tenant_id": null, "client_id": null, "client_secret": null,
                           "mailbox": null, "processed_category": "Vimdesk-Imported",
                           "delta_token": null },
                "outbound": { "send_enabled": false,
                              "default_sender": { "email": null, "sender_name": null },
                              "senders": [] } },
        }),
        _ => return None,
    })
}

/// `stored` over `default`, the way Laravel's Data casts filled a section: keys not stored
/// take the default, a null (or non-object) child object becomes its default object, and
/// stored keys the default doesn't list are dropped.
pub fn overlay(default: &Value, stored: &Value) -> Value {
    match (default, stored) {
        (Value::Object(d), Value::Object(s)) => d
            .iter()
            .map(|(k, dv)| {
                let v = s.get(k).map_or_else(|| dv.clone(), |sv| overlay(dv, sv));
                (k.clone(), v)
            })
            .collect::<Map<_, _>>()
            .into(),
        (Value::Object(_), _) => default.clone(),
        (_, s) => s.clone(),
    }
}

/// A uuid that must name a live row of `table` in the caller's workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub field: String,
    pub table: Table,
    pub id: Uuid,
}

/// Validate one sent section; returns what is stored (listed keys only). Uuid references
/// are collected for the service to check.
pub fn section(
    r: &mut Rules,
    name: &str,
    fields: &[Field],
    v: &Value,
    refs: &mut Vec<Reference>,
) -> Value {
    let v = laravel_input(v, "");
    if v.is_null() {
        return Value::Null;
    }
    let out = object(r, name, fields, &v, refs).map_or(Value::Null, Value::Object);
    if name == "integrations" {
        graph_required(r, &v);
    }
    out
}

/// Laravel's global TrimStrings + ConvertEmptyStringsToNull middleware, applied to the whole
/// input before validation: strings are trimmed (not `password`) and blank ones become null.
fn laravel_input(v: &Value, key: &str) -> Value {
    match v {
        Value::String(s) => {
            let t = if key == "password" {
                s.as_str()
            } else {
                s.trim()
            };
            if t.is_empty() {
                Value::Null
            } else {
                Value::String(t.to_owned())
            }
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| laravel_input(x, "")).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (k.clone(), laravel_input(x, k)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn object(
    r: &mut Rules,
    path: &str,
    fields: &[Field],
    v: &Value,
    refs: &mut Vec<Reference>,
) -> Option<Map<String, Value>> {
    let Value::Object(sent) = v else {
        r.add(path, format!("The {} must be an array.", human(path)));
        return None;
    };
    let mut out = Map::new();
    for field in fields {
        let key = format!("{path}.{}", field.key);
        let value = sent.get(field.key);
        if field.required && value.is_none_or(Value::is_null) {
            r.missing(&key);
            continue;
        }
        let kept = match value {
            None => continue,
            // A null child object takes its default (Laravel `from([])`).
            Some(Value::Null) if matches!(field.rule, Rule::Object(_)) => continue,
            Some(Value::Null) => Some(Value::Null),
            Some(v) => one(r, &key, &field.rule, v, refs),
        };
        if let (true, Some(kept)) = (field.keep, kept) {
            out.insert(field.key.to_owned(), kept);
        }
    }
    Some(out)
}

/// One non-null value; None when it failed (the error is recorded).
fn one(
    r: &mut Rules,
    key: &str,
    rule: &Rule,
    v: &Value,
    refs: &mut Vec<Reference>,
) -> Option<Value> {
    match rule {
        Rule::Text(max) => text(r, key, v, *max),
        Rule::TextOrNumber(max) => match v {
            Value::Number(n) => text(r, key, &Value::String(n.to_string()), *max),
            _ => text(r, key, v, *max),
        },
        Rule::Int(min, max) => r.integer(key, v, *min, *max).map(Value::from),
        Rule::Bool => r.boolean(key, v).map(Value::Bool),
        Rule::Number => r.decimal(key, v, -MAX, MAX).map(|_| v.clone()),
        Rule::Email => match v.as_str() {
            Some(s) => {
                r.email(key, s);
                Some(v.clone())
            }
            None => check(r, key, v, |_| false, "must be a valid email address"),
        },
        Rule::Date => check(r, key, v, is_date, "is not a valid date"),
        Rule::Url => check(r, key, v, is_url, "must be a valid URL"),
        Rule::OneOf(allowed) => {
            let ok = v.as_str().is_some_and(|s| allowed.contains(&s));
            ok.then(|| v.clone()).or_else(|| {
                r.add(key, format!("The selected {} is invalid.", human(key)));
                None
            })
        }
        Rule::Encryption => {
            let ok = matches!(v.as_str(), Some("ssl" | "tls"));
            ok.then(|| v.clone()).or_else(|| {
                r.add(key, format!("Invalid {} encryption type.", human(key)));
                None
            })
        }
        Rule::Ref(table) => reference(r, key, v, *table, refs),
        // Blank already became null (laravel_input); any other value is a credential.
        Rule::Secret => {
            r.add(key, SECRET_REFUSED);
            None
        }
        Rule::Ints(cap) => list(r, key, v, *cap, |r, k, item| {
            r.integer(k, item, i64::MIN, i64::MAX).map(Value::from)
        }),
        Rule::Strs(max, cap) => list(r, key, v, *cap, |r, k, item| text(r, k, item, *max)),
        Rule::Numbers(cap) => list(r, key, v, *cap, |r, k, item| {
            if item.is_null() {
                r.missing(k);
                return None;
            }
            r.decimal(k, item, -MAX, MAX).map(|_| item.clone())
        }),
        Rule::Object(fields) => object(r, key, fields, v, refs).map(Value::Object),
        Rule::List(fields, cap) => list(r, key, v, *cap, |r, k, item| {
            object(r, k, fields, item, refs).map(Value::Object)
        }),
    }
}

/// A string of at most `max` chars.
fn text(r: &mut Rules, key: &str, v: &Value, max: usize) -> Option<Value> {
    match v {
        Value::String(s) => {
            r.max_chars(key, s, max);
            Some(v.clone())
        }
        _ => {
            r.add(key, format!("The {} must be a string.", human(key)));
            None
        }
    }
}

/// A string passing `ok`, else "The X `what`.".
fn check(
    r: &mut Rules,
    key: &str,
    v: &Value,
    ok: impl Fn(&str) -> bool,
    what: &str,
) -> Option<Value> {
    if v.as_str().is_some_and(ok) {
        Some(v.clone())
    } else {
        r.add(key, format!("The {} {what}.", human(key)));
        None
    }
}

/// A uuid, collected for the existence check.
fn reference(
    r: &mut Rules,
    key: &str,
    v: &Value,
    table: Table,
    refs: &mut Vec<Reference>,
) -> Option<Value> {
    let Some(id) = v.as_str().and_then(|s| Uuid::parse_str(s).ok()) else {
        r.add(key, format!("The {} must be a valid UUID.", human(key)));
        return None;
    };
    refs.push(Reference {
        field: key.to_owned(),
        table,
        id,
    });
    Some(Value::String(id.to_string()))
}

/// `v` as a list of at most `cap` entries, each checked by `each` at `key.N`. An oversized
/// list is one error, never one per entry.
fn list(
    r: &mut Rules,
    key: &str,
    v: &Value,
    cap: usize,
    mut each: impl FnMut(&mut Rules, &str, &Value) -> Option<Value>,
) -> Option<Value> {
    let Value::Array(items) = v else {
        r.add(key, format!("The {} must be an array.", human(key)));
        return None;
    };
    if items.len() > cap {
        let msg = format!("The {} must not have more than {cap} items.", human(key));
        r.add(key, msg);
        return None;
    }
    let out: Vec<Value> = items
        .iter()
        .enumerate()
        .filter_map(|(n, item)| each(r, &format!("{key}.{n}"), item))
        .collect();
    Some(Value::Array(out))
}

/// Laravel `date` (strtotime): a calendar date or a date-time.
fn is_date(s: &str) -> bool {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
        || NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").is_ok()
        || NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").is_ok()
        || DateTime::parse_from_rfc3339(s).is_ok()
}

/// Laravel `required_if:integrations.mail.driver,graph` on the graph fields.
fn graph_required(r: &mut Rules, sent: &Value) {
    let mail = &sent["mail"];
    if mail["driver"].as_str() != Some("graph") {
        return;
    }
    for key in ["tenant_id", "client_id", "client_secret", "mailbox"] {
        if mail["graph"][key].is_null() {
            let field = format!("integrations.mail.graph.{key}");
            let msg = format!(
                "The {} field is required when integrations.mail.driver is graph.",
                human(&field)
            );
            r.add(&field, msg);
        }
    }
}

/// Null every credential under `value`, which sits at dotted `path` ("" = the whole tree).
pub fn redact(value: &mut Value, path: &str) {
    for secret in SECRET_PATHS {
        // The value is a credential, or lies inside one.
        if path
            .strip_prefix(secret)
            .is_some_and(|r| r.is_empty() || r.starts_with('.'))
        {
            *value = Value::Null;
            return;
        }
        let rest = if path.is_empty() {
            Some(*secret)
        } else {
            secret.strip_prefix(path).and_then(|r| r.strip_prefix('.'))
        };
        if let Some(slot) =
            rest.and_then(|rest| rest.split('.').try_fold(&mut *value, |v, k| v.get_mut(k)))
        {
            *slot = Value::Null;
        }
    }
}

/// Every credential in `data` (a stored or about-to-be-stored settings object) is null.
pub fn holds_no_secret(data: &Value) -> bool {
    SECRET_PATHS.iter().all(|path| {
        path.split('.')
            .try_fold(data, |acc, k| acc.get(k))
            .is_none_or(Value::is_null)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_fills_defaults_and_drops_unknown_keys() {
        let d = json!({ "a": 1, "b": { "c": 2, "d": 3 }, "e": null });
        let s = json!({ "a": null, "b": { "c": 9, "x": 1 }, "y": 5 });
        assert_eq!(
            overlay(&d, &s),
            json!({ "a": null, "b": { "c": 9, "d": 3 }, "e": null })
        );
        assert_eq!(overlay(&d, &Value::Null), d);
        assert_eq!(
            overlay(&json!({ "b": { "c": 1 } }), &json!({ "b": null })),
            json!({ "b": { "c": 1 } })
        );
    }

    #[test]
    fn redact_nulls_secrets_at_any_depth() {
        let mut all = json!({ "smtp": { "password": "p", "host": "h" } });
        redact(&mut all, "");
        assert_eq!(all, json!({ "smtp": { "password": null, "host": "h" } }));
        let mut smtp = json!({ "password": "p", "host": "h" });
        redact(&mut smtp, "smtp");
        assert_eq!(smtp["password"], Value::Null);
        let mut leaf = json!("p");
        redact(&mut leaf, "smtp.password");
        assert_eq!(leaf, Value::Null);
        let mut host = json!("h");
        redact(&mut host, "smtp.host");
        assert_eq!(host, json!("h"));
    }

    #[test]
    fn every_secret_sits_on_a_secret_rule() {
        // The tables and SECRET_PATHS must agree, or a credential could be stored.
        fn walk(path: &str, fields: &[Field], found: &mut Vec<String>) {
            for f in fields {
                let p = format!("{path}.{}", f.key);
                match &f.rule {
                    Rule::Secret => found.push(p),
                    Rule::Object(inner) | Rule::List(inner, _) => walk(&p, inner, found),
                    _ => {}
                }
            }
        }
        let mut found = Vec::new();
        for (name, fields) in WRITABLE {
            walk(name, fields, &mut found);
        }
        found.sort();
        let mut listed: Vec<String> = SECRET_PATHS.iter().map(|s| (*s).to_owned()).collect();
        listed.sort();
        assert_eq!(found, listed);
    }

    #[test]
    fn dates_like_strtotime() {
        assert!(is_date("2026-04-18"));
        assert!(is_date("2026-04-18 10:00:00"));
        assert!(is_date("2026-04-18T10:00:00+02:00"));
        assert!(!is_date("soon"));
        assert!(!is_date("2026-13-01"));
    }
}
