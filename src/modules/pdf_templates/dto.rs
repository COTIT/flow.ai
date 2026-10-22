//! PDF template bodies (Laravel Store/UpdatePdfTemplateRequest) and PdfTemplateResource.
//! Columns, conditions, extra fields and meta are capped while parsing (`pricing::body`).

use rust_decimal::{Decimal, RoundingStrategy, prelude::ToPrimitive};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, human},
    modules::{
        pdf_templates::fields,
        pricing::body::{Capped, Scalar},
    },
};

/// Domains vimdesk-web offers templates for.
pub const DOMAINS: &[&str] = &[
    "invoice",
    "estimate",
    "order",
    "purchase_order",
    "collective_invoice",
    "work_order",
    "cash_register",
    "relation",
];

/// Laravel `extra_fields` keys (all booleans but the template id).
const EXTRA_FLAGS: &[&str] = &[
    "display_procedures",
    "display_signature",
    "display_hours_with_tariff",
    "display_hours",
    "display_planned_dates",
    "display_description",
    "display_invoice_specifications",
];

/// A text field of an update: None = not sent (keep), Some(None) = null/blank (clear).
pub type Patch<T> = Option<Option<T>>;

/// Body of create and update; unknown keys are skipped.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct TemplateBody {
    domain: Scalar,
    domain_action: Scalar,
    name: Scalar,
    columns: Capped,
    header_left: Scalar,
    header_right: Scalar,
    header_top: Scalar,
    footer: Scalar,
    padding_top: Scalar,
    padding_bottom: Scalar,
    padding_x: Scalar,
    enable_summary: Scalar,
    default: Scalar,
    background_file_id: Scalar,
    logo_file_id: Scalar,
    conditions: Capped,
    extra_fields: Capped,
    meta: Capped,
}

/// `POST /settings/pdf-templates`.
#[derive(Deserialize)]
pub struct CreateTemplateRequest(TemplateBody);

/// `POST /settings/pdf-templates/{id}`.
#[derive(Deserialize)]
pub struct UpdateTemplateRequest(TemplateBody);

/// A validated body. On update the domain is the stored one (checked by the service) and
/// every optional field is a patch; on create absent optional fields are their defaults.
pub struct TemplateInput {
    pub domain: Option<String>,
    pub domain_action: Patch<String>,
    pub name: Option<String>,
    /// Columns, with their field keys still to check against the domain.
    pub columns: Option<Value>,
    pub header_left: Patch<String>,
    pub header_right: Patch<String>,
    pub header_top: Patch<String>,
    pub footer: Patch<String>,
    pub padding_top: Option<i32>,
    pub padding_bottom: Option<i32>,
    pub padding_x: Option<i32>,
    pub enable_summary: Option<bool>,
    pub is_default: Option<bool>,
    /// Some(None) = cleared; Some(Some(None)) = not a uuid (never matches).
    pub background_file_id: Patch<Option<Uuid>>,
    pub logo_file_id: Patch<Option<Uuid>>,
    pub conditions: Patch<Conditions>,
    pub extra_fields: Patch<Value>,
    pub meta: Patch<Value>,
}

/// Laravel Conditions value.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Conditions {
    pub language: Option<String>,
    pub relation_type: Option<String>,
    /// A custom relation type; Some(nil) = not a uuid (never matches).
    pub relation_custom_type_id: Option<Uuid>,
}

/// `{data:{pdf_template_id}}`.
#[derive(Serialize)]
pub struct TemplateCreated {
    pub pdf_template_id: Uuid,
}

/// List query.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ListQuery {
    pub page: Option<String>,
    pub page_size: Option<String>,
    pub domain: Option<String>,
    pub q: Option<String>,
    pub relation_type: Option<String>,
    pub language: Option<String>,
    pub relation_custom_type_id: Option<String>,
}

impl Validate for CreateTemplateRequest {
    type Valid = TemplateInput;

    fn validate(self, r: &mut Rules) -> Option<TemplateInput> {
        let b = self.0;
        let domain = match b.domain.filled() {
            None => {
                r.missing("domain");
                None
            }
            Some(v) => {
                let d = v.as_str().map(str::to_lowercase).unwrap_or_default();
                r.one_of("domain", Some(d), DOMAINS, None)
            }
        };
        let mut input = fields_of(r, b);
        if let (Some(d), Some(cols)) = (&domain, &input.columns) {
            check_column_fields(r, d, cols);
        }
        input.domain = domain;
        Some(input)
    }
}

impl Validate for UpdateTemplateRequest {
    type Valid = TemplateInput;

    fn validate(self, r: &mut Rules) -> Option<TemplateInput> {
        // Laravel's update rules have no domain/domain_action: they can't change.
        let mut b = self.0;
        b.domain = Scalar::Absent;
        b.domain_action = Scalar::Absent;
        Some(fields_of(r, b))
    }
}

/// Every field but `domain`; name, columns and paddings are required on create and update.
fn fields_of(r: &mut Rules, b: TemplateBody) -> TemplateInput {
    let name = match b.name.filled() {
        None => {
            r.missing("name");
            None
        }
        Some(Value::String(s)) => {
            let s = s.trim().to_owned();
            r.max_chars("name", &s, 255);
            Some(s)
        }
        Some(_) => {
            r.add("name", "The name must be a string.");
            None
        }
    };
    TemplateInput {
        domain: None,
        domain_action: text(r, "domain_action", &b.domain_action, 255),
        name,
        columns: columns(r, &b.columns),
        header_left: text(r, "header_left", &b.header_left, 65_535),
        header_right: text(r, "header_right", &b.header_right, 65_535),
        header_top: text(r, "header_top", &b.header_top, 65_535),
        footer: text(r, "footer", &b.footer, 65_535),
        padding_top: padding(r, "padding_top", &b.padding_top),
        padding_bottom: padding(r, "padding_bottom", &b.padding_bottom),
        padding_x: padding(r, "padding_x", &b.padding_x),
        enable_summary: flag(r, "enable_summary", &b.enable_summary),
        is_default: flag(r, "default", &b.default),
        background_file_id: file_ref(r, "background_file_id", &b.background_file_id),
        logo_file_id: file_ref(r, "logo_file_id", &b.logo_file_id),
        conditions: conditions(r, &b.conditions),
        extra_fields: extra_fields(r, &b.extra_fields),
        meta: object(r, "meta", &b.meta),
    }
}

/// Sent as null/blank -> Some(None); absent -> None; else Some(Some(value)).
fn patch(v: &Scalar) -> Option<Option<Value>> {
    match v {
        Scalar::Absent => None,
        other => Some(other.filled()),
    }
}

/// `nullable|string|max:N` (trimmed, blank = null), patch semantics.
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

/// `required|numeric|min:0|max:40` stored as a whole number (Laravel's int column rounded).
fn padding(r: &mut Rules, field: &str, v: &Scalar) -> Option<i32> {
    match v.filled() {
        None => {
            r.missing(field);
            None
        }
        Some(v) => r
            .decimal(field, &v, Decimal::ZERO, Decimal::from(40))
            .map(|d| d.round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero))
            .and_then(|d| d.to_i32()),
    }
}

/// `nullable|boolean`; null reads as not sent.
fn flag(r: &mut Rules, field: &str, v: &Scalar) -> Option<bool> {
    v.filled().and_then(|v| r.boolean(field, &v))
}

/// `nullable` uuid of a file (checked by the service).
fn file_ref(r: &mut Rules, field: &str, v: &Scalar) -> Patch<Option<Uuid>> {
    match patch(v)? {
        None => Some(None),
        Some(v) => {
            let id = v.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok());
            if id.is_none() {
                r.add(field, format!("The selected {} is invalid.", human(field)));
            }
            Some(Some(id))
        }
    }
}

/// `columns`: a list of `{label?, alignment, fields[]}`; field keys are checked against the
/// domain separately.
fn columns(r: &mut Rules, v: &Capped) -> Option<Value> {
    let list = match v {
        Capped::Absent | Capped::Value(Value::Null) => {
            r.missing("columns");
            return None;
        }
        Capped::TooLarge => {
            r.add("columns", "The columns is too large.");
            return None;
        }
        Capped::Value(Value::Array(list)) if list.is_empty() => {
            r.missing("columns");
            return None;
        }
        Capped::Value(Value::Array(list)) => list,
        Capped::Value(_) => {
            r.add("columns", "The columns must be an array.");
            return None;
        }
    };
    let mut out = Vec::with_capacity(list.len());
    for (i, col) in list.iter().enumerate() {
        let key = |k: &str| format!("columns.{i}.{k}");
        let label = match col.get("label") {
            None | Some(Value::Null) => Value::Null,
            Some(Value::String(s)) => {
                r.max_chars(&key("label"), s, 55);
                Value::from(s.as_str())
            }
            Some(_) => {
                r.add(
                    &key("label"),
                    format!("The {} must be a string.", human(&key("label"))),
                );
                Value::Null
            }
        };
        let alignment = match col.get("alignment").and_then(Value::as_str) {
            Some(a @ ("left" | "right")) => a,
            None if col.get("alignment").is_none_or(Value::is_null) => {
                r.missing(&key("alignment"));
                "left"
            }
            _ => {
                r.add(
                    &key("alignment"),
                    format!("The selected {} is invalid.", human(&key("alignment"))),
                );
                "left"
            }
        };
        let fields = match col.get("fields") {
            Some(Value::Array(f)) if !f.is_empty() => f.clone(),
            Some(Value::Array(_)) | None | Some(Value::Null) => {
                r.missing(&key("fields"));
                Vec::new()
            }
            Some(_) => {
                r.add(
                    &key("fields"),
                    format!("The {} must be an array.", human(&key("fields"))),
                );
                Vec::new()
            }
        };
        out.push(json!({ "label": label, "alignment": alignment, "fields": fields }));
    }
    Some(Value::Array(out))
}

/// Every field key of every column must be an item field of `domain` (Laravel checked them
/// against every domain's fields at once, so a key a domain can't print passed).
pub fn check_column_fields(r: &mut Rules, domain: &str, columns: &Value) {
    let allowed = fields::keys(domain);
    for (i, col) in columns.as_array().into_iter().flatten().enumerate() {
        for (j, f) in col["fields"].as_array().into_iter().flatten().enumerate() {
            if !f.as_str().is_some_and(|k| allowed.contains(&k)) {
                let key = format!("columns.{i}.fields.{j}");
                r.add(&key, format!("The selected {} is invalid.", human(&key)));
            }
        }
    }
}

/// Laravel Conditions: `language` (≤ 3), `relation_type` (business/personal),
/// `relation_custom_type_id` (a relation custom type, checked by the service).
fn conditions(r: &mut Rules, v: &Capped) -> Patch<Conditions> {
    let obj = match v {
        Capped::Absent => return None,
        Capped::Value(Value::Null) => return Some(None),
        Capped::TooLarge => {
            r.add("conditions", "The conditions is too large.");
            return None;
        }
        Capped::Value(Value::Object(o)) => o,
        Capped::Value(_) => {
            r.add("conditions", "The conditions must be an array.");
            return None;
        }
    };
    let text = |r: &mut Rules, k: &str| match obj.get(k) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.trim().is_empty() => None,
        Some(Value::String(s)) => Some(s.trim().to_owned()),
        Some(_) => {
            let f = format!("conditions.{k}");
            r.add(&f, format!("The {} must be a string.", human(&f)));
            None
        }
    };
    let language = text(r, "language");
    if let Some(l) = &language {
        r.max_chars("conditions.language", l, 3);
    }
    let relation_type = text(r, "relation_type");
    if let Some(t) = &relation_type
        && !matches!(t.as_str(), "business" | "personal")
    {
        r.add(
            "conditions.relation_type",
            "The selected conditions.relation type is invalid.",
        );
    }
    let relation_custom_type_id =
        text(r, "relation_custom_type_id").map(|s| Uuid::parse_str(&s).unwrap_or(Uuid::nil()));
    Some(Some(Conditions {
        language,
        relation_type,
        relation_custom_type_id,
    }))
}

/// Laravel `extra_fields`: the display flags (booleans) and the specification template id.
fn extra_fields(r: &mut Rules, v: &Capped) -> Patch<Value> {
    let obj = match v {
        Capped::Absent => return None,
        Capped::Value(Value::Null) => return Some(None),
        Capped::TooLarge => {
            r.add("extra_fields", "The extra fields is too large.");
            return None;
        }
        Capped::Value(Value::Object(o)) => o,
        Capped::Value(_) => {
            r.add("extra_fields", "The extra fields must be an array.");
            return None;
        }
    };
    let mut out = Map::new();
    for k in EXTRA_FLAGS {
        if let Some(v) = obj.get(*k).filter(|v| !v.is_null()) {
            let f = format!("extra_fields.{k}");
            if let Some(b) = r.boolean(&f, v) {
                out.insert((*k).to_owned(), Value::Bool(b));
            }
        }
    }
    let spec = "pdf_template_id_for_invoice_specifications";
    match obj.get(spec) {
        None | Some(Value::Null) => {}
        Some(v) => match v.as_str().and_then(|s| Uuid::parse_str(s).ok()) {
            Some(id) => {
                out.insert(spec.to_owned(), Value::from(id.to_string()));
            }
            None => {
                let f = format!("extra_fields.{spec}");
                r.add(&f, format!("The {} must be a valid UUID.", human(&f)));
            }
        },
    }
    Some(Some(Value::Object(out)))
}

/// A capped object (`meta`): null clears, an object sets.
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
