//! Relation bodies: create/update rules (Laravel Store/UpdateRelationRequest) and the
//! RelationResource / RelationIndexResource shapes vimdesk-web and the POS read.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, human, is_url},
    modules::{
        contacts::{
            dto::{ContactFields, ContactResource, contact_fields, invalid_choice, opt_str},
            repo::ContactRow,
        },
        custom_types::{dto::CustomTypeResource, repo::CustomTypeRow},
        relations::{
            address::{self, Address, AddressView},
            countries,
            repo::RelationRow,
            vat_number,
        },
        vat_groups::{dto::VatGroupResource, repo::VatGroupRow, service::rates_for_country},
        workspaces::dto::account,
    },
    money::{Zero, format_decimals},
    numbering::Appendix,
    php::truthy,
};

/// Laravel `rules.CustomTypeNotValid` for `relation_type_id`.
pub const TYPE_NOT_VALID: &str = "The relation type id is not valid";
const POS_NAME: &str =
    "A POS customer needs a company name, alias, or contact first and last name.";

fn get<'a>(v: &'a Map<String, Value>, k: &str) -> Option<&'a Value> {
    v.get(k)
}

/// `business` | `personal`, any case (Laravel EnumExist), stored lowercase.
fn kind(r: &mut Rules, v: Option<&Value>) -> Option<String> {
    let raw = match filled(v.cloned()) {
        None => {
            r.missing("type");
            return None;
        }
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(other) => other.to_string(),
    };
    let lower = raw.to_ascii_lowercase();
    if lower == "business" || lower == "personal" {
        Some(lower)
    } else {
        invalid_choice(r, "type", &raw);
        None
    }
}

fn flag(r: &mut Rules, key: &str, v: Option<&Value>) -> Option<bool> {
    filled(v.cloned()).and_then(|b| r.boolean(key, &b))
}

fn email(r: &mut Rules, key: &str, v: Option<&Value>) -> Option<String> {
    opt_str(r, key, v, 255).map(|e| {
        r.email(key, &e);
        e.to_lowercase()
    })
}

fn website(r: &mut Rules, v: Option<&Value>) -> Option<String> {
    opt_str(r, "website", v, 255).inspect(|w| {
        r.check("website", is_url(w), "The website must be a valid URL.");
    })
}

fn language(r: &mut Rules, v: Option<&Value>) -> Option<String> {
    let l = opt_str(r, "default_language", v, usize::MAX)?;
    if l.chars().count() > 2 {
        r.add(
            "default_language",
            "The default language must not be greater than 2 characters.",
        );
    }
    let ok = l == "en" || l == "nl";
    r.check(
        "default_language",
        ok,
        "The selected default language is invalid.",
    );
    ok.then_some(l)
}

/// `relation_type_id` / `source_type_id`: uuid string; existence is checked by the service.
fn type_ref(r: &mut Rules, key: &str, v: Option<&Value>) -> Option<Uuid> {
    let s = opt_str(r, key, v, usize::MAX)?;
    let id = Uuid::parse_str(&s).ok();
    if id.is_none() {
        r.add(key, format!("The {} is not valid", human(key)));
    }
    id
}

/// Laravel `nullable|integer` / `numeric|min:0|max:100` fields that are validated but never
/// stored (`default_payment_term`, `default_vat_rate`: no column / overridden accessor).
fn checked_only(r: &mut Rules, v: &Map<String, Value>) {
    if let Some(n) = filled(get(v, "default_payment_term").cloned()) {
        r.integer("default_payment_term", &n, i64::MIN, i64::MAX);
    }
    if let Some(n) = filled(get(v, "default_vat_rate").cloned()) {
        r.decimal("default_vat_rate", &n, Decimal::ZERO, Decimal::ONE_HUNDRED);
    }
}

/// Validated create input. `relation_type_id` None = use the workspace default.
pub struct NewRelation {
    pub kind: String,
    pub relation_type_id: Option<Uuid>,
    pub source_type_id: Option<Uuid>,
    pub alias: Option<String>,
    pub company_name: Option<String>,
    pub coc_number: Option<String>,
    pub vat_number: Option<String>,
    pub email: Option<String>,
    pub website: Option<String>,
    pub phone_number: Option<String>,
    pub default_language: Option<String>,
    pub addresses: Vec<Address>,
    pub financial_account: Option<Value>,
    pub allow_invoicing_on_account: bool,
    pub disable_order_workflow_emails: bool,
    pub meta: Option<Value>,
    /// Inline contact, created as the default contact (Laravel: whenever `contact` is an object).
    pub contact: Option<ContactFields>,
}

/// POST /relations. Raw object so unknown keys (`address`, `default_validity_period`) pass.
#[derive(Deserialize)]
pub struct CreateRelationRequest(Map<String, Value>);

impl Validate for CreateRelationRequest {
    type Valid = NewRelation;

    fn validate(self, r: &mut Rules) -> Option<NewRelation> {
        let v = self.0;
        checked_only(r, &v);
        r.custom_sections("custom_sections", get(&v, "custom_sections"));
        r.custom_sections("custom_section_forms", get(&v, "custom_section_forms"));
        let vat = opt_str(r, "vat_number", get(&v, "vat_number"), 255);
        if let Some(vat) = &vat {
            r.check(
                "vat_number",
                vat_number::is_valid_format(vat),
                "The Vat number must be a valid number issued by any Member State.",
            );
        }
        let contact = match get(&v, "contact") {
            None | Some(Value::Null) => None,
            Some(Value::Object(c)) => Some(contact_fields(r, "contact.", c, false)),
            Some(_) => {
                r.add("contact", "The contact must be an array.");
                None
            }
        };
        let new = NewRelation {
            kind: kind(r, get(&v, "type")).unwrap_or_default(),
            relation_type_id: type_ref(r, "relation_type_id", get(&v, "relation_type_id")),
            source_type_id: type_ref(r, "source_type_id", get(&v, "source_type_id")),
            alias: opt_str(r, "alias", get(&v, "alias"), 255),
            company_name: opt_str(r, "company_name", get(&v, "company_name"), 255),
            coc_number: opt_str(r, "coc_number", get(&v, "coc_number"), 200),
            vat_number: vat,
            email: email(r, "email", get(&v, "email")),
            website: website(r, get(&v, "website")),
            phone_number: opt_str(r, "phone_number", get(&v, "phone_number"), 55),
            default_language: language(r, get(&v, "default_language")),
            addresses: match get(&v, "addresses") {
                None => Vec::new(),
                Some(a) => address::validate_list(r, a.clone()),
            },
            financial_account: filled(get(&v, "financial_account").cloned())
                .map(|a| account(r, "financial_account", "", a))
                .filter(|a| !a.is_null()),
            allow_invoicing_on_account: flag(
                r,
                "allow_invoicing_on_account",
                get(&v, "allow_invoicing_on_account"),
            )
            .unwrap_or_default(),
            disable_order_workflow_emails: flag(
                r,
                "disable_order_workflow_emails",
                get(&v, "disable_order_workflow_emails"),
            )
            .unwrap_or_default(),
            meta: r.meta(get(&v, "meta").cloned()).flatten(),
            contact,
        };
        if flag(r, "created_from_pos", get(&v, "created_from_pos")).unwrap_or_default() {
            let named = new
                .contact
                .as_ref()
                .is_some_and(|c| c.first_name.is_some() && c.last_name.is_some());
            if new.company_name.is_none() && new.alias.is_none() && !named {
                r.add("contact.first_name", POS_NAME);
                r.add("contact.last_name", POS_NAME);
            }
        }
        Some(new)
    }
}

/// `terms_and_periods`: four optional whole numbers 1..=365, keys `terms_and_periods.<key>`.
/// Laravel's rule named a validator that doesn't exist (`int`) and answered 500 for any value.
fn terms_in(r: &mut Rules, v: Value) -> Option<Value> {
    let map = match v {
        Value::Null => return None,
        Value::Object(map) => map,
        _ => {
            r.add(
                "terms_and_periods",
                "The terms and periods must be an array.",
            );
            return None;
        }
    };
    let mut out = Map::new();
    for k in TERM_KEYS {
        let n = filled(map.get(k).cloned()).and_then(|n| {
            let mut own = Rules::default();
            let n = own.integer(k, &n, 1, 365);
            // Keys carry the prefix, messages the bare name (Laravel Data::validate wrapping).
            own.merge_into_prefixed(r, "terms_and_periods.");
            n
        });
        out.insert(k.into(), n.map_or(Value::Null, Value::from));
    }
    Some(Value::Object(out))
}

/// Laravel SupplierPaymentTermsEnum values.
const PAYMENT_TERMS: [&str; 8] = [
    "end_of_next_month",
    "fifteen_days",
    "fob",
    "forty_five_days",
    "immediate_payment",
    "ninety_days",
    "thirty_days",
    "twenty_one_days",
];

/// `supplier_data` (Laravel RelationSupplierData): an object stored as sent, after checking
/// its enums, numbers and the fixed shipping cost rule. Keys `supplier_data.<field>`.
// shortcut: no field-by-field port of the nested Data classes (purchase orders read them in
// the last batch); vimdesk-web fills their defaults itself.
fn supplier_in(r: &mut Rules, v: Value) -> Option<Value> {
    let map = match &v {
        Value::Null => return None,
        Value::Object(map) => map,
        _ => {
            r.add("supplier_data", "The supplier data is not valid");
            return None;
        }
    };
    let key = |f: &str| format!("supplier_data.{f}");
    let pick = |r: &mut Rules, f: &str, allowed: &[&str]| match filled(map.get(f).cloned()) {
        None => {}
        Some(Value::String(x)) if allowed.contains(&x.as_str()) => {}
        Some(_) => r.add(&key(f), format!("The selected {} is invalid.", human(f))),
    };
    pick(r, "payment_term", &PAYMENT_TERMS);
    pick(r, "order_type", &["email", "ftp"]);
    if let Some(n) = filled(map.get("lead_time").cloned()) {
        let mut own = Rules::default();
        own.integer("lead_time", &n, i64::MIN, i64::MAX);
        own.merge_into_prefixed(r, "supplier_data.");
    }
    for f in ["min_order_weight", "min_order_value"] {
        if let Some(n) = filled(map.get(f).cloned()) {
            let mut own = Rules::default();
            own.decimal(f, &n, Decimal::MIN, Decimal::MAX);
            own.merge_into_prefixed(r, "supplier_data.");
        }
    }
    if let Some(Value::Object(fixed)) = map.get("fixed_shipping_cost") {
        let mut own = Rules::default();
        let enabled =
            filled(fixed.get("enabled").cloned()).and_then(|b| own.boolean("enabled", &b));
        match filled(fixed.get("amount_base").cloned()) {
            None if enabled == Some(true) => own.missing("amount_base"),
            None => {}
            Some(n) => {
                if let Some(a) = own.decimal("amount_base", &n, Decimal::MIN, Decimal::MAX)
                    && a <= Decimal::ZERO
                {
                    own.add("amount_base", "The amount base must be greater than 0.");
                }
            }
        }
        if let Some(n) = filled(fixed.get("vat").cloned()) {
            own.decimal("vat", &n, Decimal::ZERO, Decimal::MAX);
        }
        for f in ["line_name", "reference"] {
            opt_str(&mut own, f, fixed.get(f), 255);
        }
        own.merge_into_prefixed(r, "supplier_data.fixed_shipping_cost.");
    }
    Some(v)
}

/// POST /relations/{id}: `type` and `relation_type_id` required (Laravel update rules), every
/// other known key is a patch (absent = keep, null = clear). Raw object: the SPA sends its
/// whole form, stray keys included.
#[derive(Deserialize)]
pub struct UpdateRelationRequest(Map<String, Value>);

/// Validated update; `None` keeps the stored value, `Some(None)` clears it.
pub struct RelationPatch {
    pub kind: String,
    pub relation_type_id: Uuid,
    pub alias: Option<Option<String>>,
    pub company_name: Option<Option<String>>,
    pub coc_number: Option<Option<String>>,
    pub vat_number: Option<Option<String>>,
    pub email: Option<Option<String>>,
    pub website: Option<Option<String>>,
    pub phone_number: Option<Option<String>>,
    pub default_language: Option<Option<String>>,
    pub import_source: Option<Option<String>>,
    pub import_id: Option<Option<String>>,
    pub addresses: Option<Vec<Address>>,
    pub financial_account: Option<Option<Value>>,
    pub terms_and_periods: Option<Option<Value>>,
    pub supplier_data: Option<Option<Value>>,
    pub allow_invoicing_on_account: Option<bool>,
    pub disable_order_workflow_emails: Option<bool>,
    pub meta: Option<Option<Value>>,
}

impl Validate for UpdateRelationRequest {
    type Valid = RelationPatch;

    fn validate(self, r: &mut Rules) -> Option<RelationPatch> {
        let v = self.0;
        checked_only(r, &v);
        r.custom_sections("custom_sections", get(&v, "custom_sections"));
        r.custom_sections("custom_section_forms", get(&v, "custom_section_forms"));
        let sent = |k: &str| v.contains_key(k);
        let text =
            |r: &mut Rules, k: &str, max: usize| sent(k).then(|| opt_str(r, k, get(&v, k), max));
        let relation_type_id = type_ref(r, "relation_type_id", get(&v, "relation_type_id"));
        if relation_type_id.is_none() && filled(get(&v, "relation_type_id").cloned()).is_none() {
            r.missing("relation_type_id");
        }
        let flag_patch = |r: &mut Rules, k: &str| {
            sent(k).then(|| match filled(get(&v, k).cloned()) {
                None => {
                    r.missing(k);
                    false
                }
                Some(b) => r.boolean(k, &b).unwrap_or_default(),
            })
        };
        Some(RelationPatch {
            kind: kind(r, get(&v, "type")).unwrap_or_default(),
            alias: text(r, "alias", 255),
            company_name: text(r, "company_name", 255),
            coc_number: text(r, "coc_number", 200),
            vat_number: text(r, "vat_number", 255),
            email: sent("email").then(|| email(r, "email", get(&v, "email"))),
            website: sent("website").then(|| website(r, get(&v, "website"))),
            phone_number: text(r, "phone_number", 55),
            default_language: sent("default_language")
                .then(|| language(r, get(&v, "default_language"))),
            import_source: text(r, "import_source", 255),
            import_id: text(r, "import_id", 255),
            addresses: get(&v, "addresses").map(|a| address::validate_list(r, a.clone())),
            financial_account: sent("financial_account").then(|| {
                filled(get(&v, "financial_account").cloned())
                    .map(|a| account(r, "financial_account", "", a))
                    .filter(|a| !a.is_null())
            }),
            terms_and_periods: sent("terms_and_periods")
                .then(|| terms_in(r, get(&v, "terms_and_periods").cloned().unwrap_or_default())),
            supplier_data: sent("supplier_data")
                .then(|| supplier_in(r, get(&v, "supplier_data").cloned().unwrap_or_default())),
            allow_invoicing_on_account: flag_patch(r, "allow_invoicing_on_account"),
            disable_order_workflow_emails: flag_patch(r, "disable_order_workflow_emails"),
            meta: sent("meta").then(|| r.meta(get(&v, "meta").cloned()).flatten()),
            // Last: every other rule has run, so all errors are reported before this ends it.
            relation_type_id: relation_type_id?,
        })
    }
}

/// True when an update's FTP config leaves the password blank: keep the stored one, since
/// responses never show it and the SPA posts back what it loaded.
pub fn keeps_ftp_password(supplier: &Value) -> bool {
    supplier
        .get("order_ftp")
        .and_then(Value::as_object)
        .is_some_and(|ftp| {
            ftp.get("password")
                .is_none_or(|p| p.is_null() || p.as_str().is_some_and(|s| s.trim().is_empty()))
        })
}

/// Create response.
#[derive(Serialize)]
pub struct RelationCreated {
    pub relation_id: Uuid,
}

/// Laravel RelationNr::format: prefix + number zero-padded to `digits` (default 1) + suffix.
pub fn relation_nr(number: Option<i64>, appendix: Option<&Value>) -> Option<String> {
    let n = number.filter(|n| *n != 0)?;
    Some(Appendix::from_section(appendix.unwrap_or(&Value::Null)).format(n))
}

/// Workspace facts the resource needs: profile `address.country` and `language`.
pub struct WorkspaceCtx {
    pub country: Option<String>,
    pub language: Option<String>,
}

impl WorkspaceCtx {
    fn country(&self) -> String {
        self.country.as_deref().unwrap_or("").to_ascii_uppercase()
    }
}

fn eu_member(code: &str) -> bool {
    countries::find(code).is_some_and(|c| c.eu_member)
}

/// Laravel Relation::isCompany.
pub fn is_company(r: &RelationRow) -> bool {
    r.kind == "business" || truthy(r.company_name.as_deref())
}

const TERM_KEYS: [&str; 4] = [
    "invoice_payment_term",
    "credit_invoice_payment_term",
    "collective_invoice_payment_term",
    "estimate_validity_period",
];

/// Stored terms with all four keys (Laravel RelationTermsAndPeriods, nulls when unset).
fn terms(stored: Option<&Value>) -> Value {
    let mut out = Map::new();
    for k in TERM_KEYS {
        let v = stored
            .and_then(|t| t.get(k))
            .cloned()
            .unwrap_or(Value::Null);
        out.insert(k.into(), v);
    }
    Value::Object(out)
}

/// Terms with blanks filled from workspace defaults (Laravel `fromRelation`).
// shortcut: Laravel's empty-settings defaults (invoice none, credit/collective/estimate 14
// personal, 30 company), not the workspace's stored terms; wire settings::service in F21
// (relation financial views), reading the sections once per request, not per row.
fn used_terms(row: &RelationRow) -> Value {
    let company = is_company(row);
    let fallback = |k: &str| match k {
        "invoice_payment_term" => Value::Null,
        _ => json!(if company { 30 } else { 14 }),
    };
    let mut t = terms(row.terms_and_periods.as_ref());
    if let Value::Object(map) = &mut t {
        for k in TERM_KEYS {
            let unset = map
                .get(k)
                .is_none_or(|v| v.is_null() || v.as_i64() == Some(0));
            if unset {
                map.insert(k.into(), fallback(k));
            }
        }
    }
    t
}

/// `supplier_data` as returned: stored object with the FTP password blanked (write-only).
pub fn supplier_out(stored: &Value) -> Value {
    let mut v = stored.clone();
    if let Some(ftp) = v.get_mut("order_ftp").and_then(Value::as_object_mut)
        && ftp.contains_key("password")
    {
        ftp.insert("password".into(), Value::Null);
    }
    v
}

/// Unpaid invoice counters; zero until invoices are ported.
#[derive(Serialize)]
struct UnpaidInvoices {
    count: i64,
    allowed_for_collective: i64,
    allowed_on_account: bool,
}

#[derive(Serialize)]
struct Relationships {
    relation_type: Option<CustomTypeResource>,
    source_type: Option<CustomTypeResource>,
    contacts: Vec<ContactResource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    supplier_discount_groups: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    relation_discount_groups: Option<Vec<Value>>,
    custom_sections: [(); 0],
    custom_section_forms: [(); 0],
    #[serde(skip_serializing_if = "Option::is_none")]
    unpaid_invoices: Option<UnpaidInvoices>,
    counts: Value,
}

/// Laravel RelationResource (GET /relations/{id}). Keys in Laravel order; `Option<Option<_>>`
/// fields are Laravel `when()` keys: absent, or present (possibly null).
#[derive(Serialize)]
pub struct RelationResource {
    id: Uuid,
    alias: Option<String>,
    company_name: Option<String>,
    coc_number: Option<String>,
    vat_number: Option<String>,
    #[serde(rename = "type")]
    kind: String,
    email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email_validation: Option<Option<Value>>,
    website: Option<String>,
    phone_number: Option<String>,
    relation_nr: Option<String>,
    import_id: Option<String>,
    import_source: Option<String>,
    allow_invoicing_on_account: bool,
    disable_order_workflow_emails: bool,
    terms_and_periods: Value,
    used_terms_and_periods: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_vat_rate: Option<String>,
    default_language: Option<String>,
    has_reverse_charge: bool,
    addresses: Vec<AddressView>,
    financial_account: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    supplier_data: Option<Value>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    vat_groups: Vec<VatGroupResource>,
    meta: Option<Value>,
    relationships: Relationships,
}

/// Inputs for one relation's detail view.
pub struct Detail {
    pub row: RelationRow,
    pub relation_type: Option<CustomTypeRow>,
    pub source_type: Option<CustomTypeRow>,
    pub contacts: Vec<ContactRow>,
    pub vat_groups: Vec<VatGroupRow>,
    pub workspace: WorkspaceCtx,
    /// `slim=1`: no discount groups or unpaid invoice counters (Laravel `$withForGetSlim`).
    pub slim: bool,
    pub include_unpaid_invoices: bool,
    /// Discount groups this relation supplies (DiscountGroupForRelationResource).
    pub supplier_discount_groups: Vec<Value>,
    /// Discount groups attached to this relation as a customer.
    pub relation_discount_groups: Vec<Value>,
}

impl From<Detail> for RelationResource {
    fn from(d: Detail) -> Self {
        let r = &d.row;
        let ws_country = d.workspace.country();
        let company = is_company(r);
        let vat = truthy(r.vat_number.as_deref());
        let default_address = address::default_of(&r.addresses);
        let default_country = default_address.and_then(|a| a.country.clone());
        let foreign = |c: &str| c.to_ascii_uppercase() != ws_country;
        let has_reverse_charge = company
            && vat
            && default_country
                .as_deref()
                .is_some_and(|c| !c.is_empty() && foreign(c));
        // Laravel vat_groups attribute: groups of the default address country (null = none set).
        let groups: Vec<&VatGroupRow> = d
            .vat_groups
            .iter()
            .filter(|g| g.country == default_country)
            .collect();
        let default_vat_rate = if company
            && default_country
                .as_deref()
                .is_some_and(|c| !c.is_empty() && foreign(c))
        {
            // Laravel returned the string '0.00', which formatDecimals prints as "0".
            Some(format_decimals(Some(Decimal::ZERO), 2, true, Zero::Column))
        } else {
            groups
                .iter()
                .find(|g| g.default_group)
                .map(|g| g.vat_amount)
                .filter(|a| !a.is_zero())
                .map(|a| format_decimals(Some(a), 2, true, Zero::Falsy))
        };
        let addresses = r
            .addresses
            .iter()
            .map(|a| {
                let mut view = AddressView::from(a);
                let country = a.country.as_deref().unwrap_or("");
                let reverse = foreign(country) && vat;
                view.reverse_charge = Some(reverse);
                view.shows_reverse_charge_notice =
                    Some(company && reverse && eu_member(country) && eu_member(&ws_country));
                view.vat_rates = Some(rates_for_country(
                    &d.vat_groups,
                    country,
                    d.workspace.language.as_deref(),
                ));
                view
            })
            .collect();
        let full = !d.slim;
        let contacts = d.contacts.into_iter().map(Into::into).collect();
        let vat_groups = groups.into_iter().cloned().map(Into::into).collect();
        let r = d.row;
        Self {
            id: r.id,
            email_validation: truthy(r.email.as_deref()).then_some(None),
            terms_and_periods: terms(r.terms_and_periods.as_ref()),
            used_terms_and_periods: used_terms(&r),
            relation_nr: relation_nr(r.relation_number, r.relation_appendix.as_ref()),
            supplier_data: r.supplier_data.as_ref().map(supplier_out),
            alias: r.alias,
            company_name: r.company_name,
            coc_number: r.coc_number,
            vat_number: r.vat_number,
            kind: r.kind,
            email: r.email,
            website: r.website,
            phone_number: r.phone_number,
            import_id: r.import_id,
            import_source: r.import_source,
            allow_invoicing_on_account: r.allow_invoicing_on_account,
            disable_order_workflow_emails: r.disable_order_workflow_emails,
            default_vat_rate,
            default_language: r.default_language,
            has_reverse_charge,
            addresses,
            financial_account: r.financial_account,
            created_at: r.created_at,
            last_updated_at: r.updated_at,
            vat_groups,
            meta: r.meta,
            relationships: Relationships {
                relation_type: d.relation_type.map(Into::into),
                source_type: d.source_type.map(Into::into),
                contacts,
                supplier_discount_groups: full.then_some(d.supplier_discount_groups),
                relation_discount_groups: full.then_some(d.relation_discount_groups),
                custom_sections: [],
                custom_section_forms: [],
                unpaid_invoices: (full && d.include_unpaid_invoices).then_some(UnpaidInvoices {
                    count: 0,
                    allowed_for_collective: 0,
                    allowed_on_account: r.allow_invoicing_on_account,
                }),
                counts: json!({ "files": 0 }),
            },
        }
    }
}

/// Laravel RelationIndexResource::normalizeSupplierData: null when every top-level value is
/// null, blank or an empty list; else the stored data (FTP password blanked).
fn index_supplier(stored: Option<&Value>) -> Option<Value> {
    let stored = stored?;
    let map = stored.as_object()?;
    let empty = |v: &Value| match v {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => false,
    };
    (!map.values().all(empty)).then(|| supplier_out(stored))
}

#[derive(Serialize)]
struct IndexRelationships {
    #[serde(skip_serializing_if = "Option::is_none")]
    relation_type: Option<Option<CustomTypeResource>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_type: Option<Option<CustomTypeResource>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    contacts: Option<Vec<ContactResource>>,
    tags: [(); 0],
}

/// Laravel RelationIndexResource (list rows, `/searches?domain=relation`). Activity and tag
/// counters are zero until those modules are ported; addresses are the stored ones (no VAT keys).
#[derive(Serialize)]
pub struct RelationIndexResource {
    id: Uuid,
    alias: Option<String>,
    company_name: Option<String>,
    #[serde(rename = "type")]
    kind: String,
    email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email_validation: Option<Option<Value>>,
    website: Option<String>,
    phone_number: Option<String>,
    relation_nr: Option<String>,
    default_language: Option<String>,
    addresses: Vec<AddressView>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    allow_invoicing_on_account: bool,
    disable_order_workflow_emails: bool,
    supplier_data: Option<Value>,
    activities_count: i64,
    has_activities: bool,
    has_note_activity: bool,
    tags_count: i64,
    relationships: IndexRelationships,
    /// Laravel `purchase_orders_count`: only loaded on the supplier list.
    open_orders: Option<i64>,
    open_order_value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    has_advice: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    advice_count: Option<i64>,
}

/// List flags that change the row shape.
#[derive(Clone, Copy)]
pub struct IndexFlags {
    pub suppliers: bool,
    pub include_advice: bool,
}

impl RelationIndexResource {
    /// One list row. `suppliers` + `include_advice` loads no relations in Laravel, so only
    /// `relationships.tags` is left.
    pub fn new(
        (r, relation_type, source_type): (
            RelationRow,
            Option<CustomTypeRow>,
            Option<CustomTypeRow>,
        ),
        contacts: Vec<ContactRow>,
        flags: IndexFlags,
    ) -> Self {
        let bare = flags.suppliers && flags.include_advice;
        Self {
            id: r.id,
            email_validation: truthy(r.email.as_deref()).then_some(None),
            relation_nr: relation_nr(r.relation_number, r.relation_appendix.as_ref()),
            addresses: r.addresses.iter().map(AddressView::from).collect(),
            supplier_data: index_supplier(r.supplier_data.as_ref()),
            alias: r.alias,
            company_name: r.company_name,
            kind: r.kind,
            email: r.email,
            website: r.website,
            phone_number: r.phone_number,
            default_language: r.default_language,
            created_at: r.created_at,
            last_updated_at: r.updated_at,
            allow_invoicing_on_account: r.allow_invoicing_on_account,
            disable_order_workflow_emails: r.disable_order_workflow_emails,
            activities_count: 0,
            has_activities: false,
            has_note_activity: false,
            tags_count: 0,
            relationships: IndexRelationships {
                relation_type: (!bare).then(|| relation_type.map(Into::into)),
                source_type: (!bare).then(|| source_type.map(Into::into)),
                contacts: (!bare).then(|| contacts.into_iter().map(Into::into).collect()),
                tags: [],
            },
            // shortcut: purchase orders not ported; counts are 0 / "0.00" until they are.
            open_orders: flags.suppliers.then_some(0),
            open_order_value: format_decimals(None, 2, true, Zero::Falsy),
            has_advice: flags.include_advice.then_some(false),
            advice_count: flags.include_advice.then_some(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relation_nr_pads_like_str_pad() {
        let appendix = json!({ "prefix": "RE-", "digits": 4, "suffix": null });
        assert_eq!(
            relation_nr(Some(1), Some(&appendix)).as_deref(),
            Some("RE-0001")
        );
        assert_eq!(
            relation_nr(Some(12345), Some(&appendix)).as_deref(),
            Some("RE-12345")
        );
        assert_eq!(relation_nr(Some(7), None).as_deref(), Some("7"));
        let suffixed = json!({ "prefix": null, "digits": 3, "suffix": "-X" });
        assert_eq!(
            relation_nr(Some(7), Some(&suffixed)).as_deref(),
            Some("007-X")
        );
        assert_eq!(relation_nr(Some(0), Some(&appendix)), None);
        assert_eq!(relation_nr(None, Some(&appendix)), None);
    }

    #[test]
    fn ftp_password_blanked_only_when_present() {
        let v = json!({ "order_ftp": { "host": "h", "password": "p" }, "notes": "n" });
        assert_eq!(
            supplier_out(&v),
            json!({ "order_ftp": { "host": "h", "password": null }, "notes": "n" })
        );
        let no_ftp = json!({ "notes": "n" });
        assert_eq!(supplier_out(&no_ftp), no_ftp);
    }
}
