//! Contact bodies: ContactResource (embedded in relations and returned alone), the shared
//! contact field rules, phones. Keys follow Laravel ContactResource / StoreContactRequest.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::{
    ToSchema,
    openapi::{ArrayBuilder, Schema},
};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, human, present},
    modules::contacts::repo::ContactRow,
    php::{truthy, ucfirst},
};

/// Most phone numbers one contact may hold (Laravel had no cap).
const MAX_PHONES: usize = 20;

/// Laravel ContactType values for `default_for`.
pub const DEFAULT_FOR: [&str; 5] = [
    "default_for_estimate",
    "default_for_invoice",
    "default_for_order",
    "default_for_purchase_order",
    "default_for_work_order",
];

/// Laravel PhoneNumberType values for `phones[].type`.
const PHONE_TYPES: [&str; 5] = ["home", "mobile", "phone", "phone2", "work"];

/// Laravel Contact::full_name: first name (else salutation), ` middle `, last name.
pub fn full_name(c: &ContactRow) -> String {
    let first = match &c.first_name {
        Some(f) if truthy(c.first_name.as_deref()) => ucfirst(f),
        _ => c.salutation.clone().unwrap_or_default(),
    };
    let middle = match &c.middle_name {
        Some(m) if truthy(c.middle_name.as_deref()) => format!(" {} ", m.to_ascii_lowercase()),
        _ => " ".to_owned(),
    };
    let last = if truthy(c.last_name.as_deref()) {
        c.last_name.as_deref().unwrap_or_default()
    } else {
        ""
    };
    format!("{first}{middle}{last}")
}

// Laravel ContactResource. `prefix`/`attn`/`salutation` only when set; `email_validation`
// only with an email, always null until email checks are ported.
/// A person at a relation.
#[derive(Serialize, ToSchema)]
pub struct ContactResource {
    id: Uuid,
    /// First name (else salutation), lowercase middle name, last name.
    full_name: String,
    /// Only when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String)]
    prefix: Option<String>,
    /// Only when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String)]
    attn: Option<String>,
    /// Only when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String)]
    salutation: Option<String>,
    #[schema(required = true)]
    initials: Option<String>,
    #[schema(required = true)]
    job_title: Option<String>,
    #[schema(required = true)]
    first_name: Option<String>,
    /// Lowercase.
    #[schema(required = true)]
    middle_name: Option<String>,
    #[schema(required = true)]
    last_name: Option<String>,
    #[schema(required = true)]
    email: Option<String>,
    /// Only when `email` is set; null until email checks exist.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>, required = false)]
    email_validation: Option<Option<Value>>,
    #[schema(value_type = Vec<ContactPhone>)]
    phones: Value,
    default_contact: bool,
    /// One of `default_for_estimate`, `default_for_invoice`, `default_for_order`,
    /// `default_for_purchase_order`, `default_for_work_order`.
    #[schema(required = true)]
    default_for: Option<String>,
    relation_id: Uuid,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
    #[schema(schema_with = empty_array)]
    relationships: [(); 0],
}

/// One stored phone number.
// Doc-only: describes the `phones` items; never constructed.
#[derive(ToSchema)]
#[allow(dead_code)]
pub struct ContactPhone {
    /// `home`, `mobile`, `phone`, `phone2` or `work`.
    #[schema(rename = "type", example = "mobile")]
    kind: String,
    /// Digits, with `+<country code>` when known; null when nothing usable was sent.
    #[schema(required = true, example = "+31612345678")]
    number: Option<String>,
}

/// Wire shape of a `[(); 0]` field: always `[]`.
pub fn empty_array() -> Schema {
    ArrayBuilder::new()
        .max_items(Some(0))
        .description(Some("Always empty"))
        .into()
}

impl From<ContactRow> for ContactResource {
    fn from(c: ContactRow) -> Self {
        let when = |s: Option<String>| s.filter(|v| truthy(Some(v)));
        Self {
            id: c.id,
            full_name: full_name(&c),
            email_validation: truthy(c.email.as_deref()).then_some(None),
            middle_name: c.middle_name.map(|m| m.to_ascii_lowercase()),
            prefix: when(c.prefix),
            attn: when(c.attn),
            salutation: when(c.salutation),
            initials: c.initials,
            job_title: c.job_title,
            first_name: c.first_name,
            last_name: c.last_name,
            email: c.email,
            phones: c.phones,
            default_contact: c.default_contact,
            default_for: c.default_for,
            relation_id: c.relation_id,
            created_at: c.created_at,
            last_updated_at: c.updated_at,
            meta: c.meta,
            relationships: [],
        }
    }
}

/// Create response: the new contact's id.
#[derive(Serialize, ToSchema)]
pub struct ContactCreated {
    pub contact_id: Uuid,
}

/// Optional string: trimmed, blank = None, non-string = "must be a string", `max` chars.
pub fn opt_str(r: &mut Rules, key: &str, v: Option<&Value>, max: usize) -> Option<String> {
    match v {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => {
            let s = s.trim();
            if s.is_empty() {
                return None;
            }
            r.max_chars(key, s, max);
            Some(s.to_owned())
        }
        Some(_) => {
            r.add(key, format!("The {} must be a string.", human(key)));
            None
        }
    }
}

/// Laravel EnumExist wording.
pub fn invalid_choice(r: &mut Rules, key: &str, value: &str) {
    let name = human(key.rsplit('.').next().unwrap_or(key));
    r.add(key, format!("The selected {name}: {value} is invalid."));
}

/// `phones`: array of `{type, number}` (Laravel PhoneNumber rules, errors at
/// `<prefix>phones.N.type`). Null items are dropped.
pub fn phones(r: &mut Rules, prefix: &str, v: Option<&Value>) -> Vec<Value> {
    let key = format!("{prefix}phones");
    let items = match v {
        None | Some(Value::Null) => return Vec::new(),
        Some(Value::Array(items)) if items.len() > MAX_PHONES => {
            let msg = format!("The {key} must not have more than {MAX_PHONES} items.");
            r.add(&key, msg);
            return Vec::new();
        }
        Some(Value::Array(items)) => items,
        Some(_) => {
            r.add(&key, format!("The {} must be an array.", human(&key)));
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let item_key = format!("{key}.{i}");
        let Value::Object(p) = item else {
            if !item.is_null() {
                r.add(&item_key, format!("The {item_key} must be an array."));
            }
            continue;
        };
        let kind_key = format!("{item_key}.type");
        let kind = opt_str(r, &kind_key, p.get("type"), 255);
        match kind.as_deref().map(str::to_ascii_lowercase) {
            None => r.add(&kind_key, "The type field is required."),
            Some(k) if !PHONE_TYPES.contains(&k.as_str()) => {
                invalid_choice(r, &kind_key, kind.as_deref().unwrap_or_default())
            }
            Some(_) => {}
        }
        let number_key = format!("{item_key}.number");
        let number = opt_str(r, &number_key, p.get("number"), 30);
        if number.is_none()
            && !matches!(
                p.get("number"),
                Some(Value::Number(_)) | Some(Value::Bool(_))
            )
        {
            r.add(&number_key, "The number field is required.");
        }
        let mut phone = Map::new();
        phone.insert(
            "type".into(),
            kind.map_or(Value::Null, |k| Value::String(k.to_ascii_lowercase())),
        );
        phone.insert("number".into(), number.map_or(Value::Null, Value::String));
        out.push(Value::Object(phone));
    }
    out
}

/// Contact fields shared by the inline relation contact and the contact routes. `prefix` is
/// the error-key prefix (`contact.` inline, empty on the contact routes).
pub struct ContactFields {
    pub first_name: Option<String>,
    pub middle_name: Option<String>,
    pub last_name: Option<String>,
    pub prefix: Option<String>,
    pub initials: Option<String>,
    pub job_title: Option<String>,
    pub salutation: Option<String>,
    pub email: Option<String>,
    pub phone_number: Option<String>,
    /// Raw `phones`; numbers are normalized by the service once the relation country is known.
    pub phones: Vec<Value>,
    pub default_contact: bool,
    pub default_for: Option<String>,
    pub meta: Option<Value>,
}

/// Laravel StoreContactRequest rules on an object (names required only on the contact routes).
pub fn contact_fields(
    r: &mut Rules,
    p: &str,
    v: &Map<String, Value>,
    names_required: bool,
) -> ContactFields {
    let key = |f: &str| format!("{p}{f}");
    let name = |r: &mut Rules, f: &str| {
        let s = opt_str(r, &key(f), v.get(f), 55);
        if s.is_none() && names_required {
            r.missing(&key(f));
        }
        s
    };
    let first_name = name(r, "first_name").map(|s| ucfirst(&s));
    let last_name = name(r, "last_name").map(|s| ucfirst(&s));
    let email = opt_str(r, &key("email"), v.get("email"), 255).map(|e| {
        r.email(&key("email"), &e);
        e.to_lowercase()
    });
    let default_contact = match v.get("default_contact") {
        None | Some(Value::Null) => false,
        Some(b) => r.boolean(&key("default_contact"), b).unwrap_or_default(),
    };
    let default_for = opt_str(r, &key("default_for"), v.get("default_for"), 255).and_then(|d| {
        let lower = d.to_ascii_lowercase();
        if DEFAULT_FOR.contains(&lower.as_str()) {
            Some(lower)
        } else {
            invalid_choice(r, &key("default_for"), &d);
            None
        }
    });
    let meta = r.meta_at(&key("meta"), v.get("meta").cloned()).flatten();
    ContactFields {
        first_name,
        middle_name: opt_str(r, &key("middle_name"), v.get("middle_name"), 55)
            .map(|s| s.to_ascii_lowercase()),
        last_name,
        prefix: opt_str(r, &key("prefix"), v.get("prefix"), 55),
        initials: opt_str(r, &key("initials"), v.get("initials"), 55),
        job_title: opt_str(r, &key("job_title"), v.get("job_title"), 100),
        salutation: opt_str(r, &key("salutation"), v.get("salutation"), 55),
        email,
        phone_number: opt_str(r, &key("phone_number"), v.get("phone_number"), 55),
        phones: phones(r, p, v.get("phones")),
        default_contact,
        default_for,
        meta,
    }
}

/// POST /relations/{relation}/contacts (Laravel StoreContactRequest).
#[derive(Deserialize)]
pub struct CreateContactRequest(Map<String, Value>);

/// Contact fields. Text is trimmed; blank = not set.
// Doc-only: request_body schema for the raw-object create body (and a relation's inline
// `contact`); never constructed.
#[derive(ToSchema)]
#[allow(dead_code)]
pub struct ContactInput {
    /// Required on the contact routes, optional inline on a relation.
    #[schema(max_length = 55)]
    first_name: Option<String>,
    #[schema(max_length = 55)]
    middle_name: Option<String>,
    /// Required on the contact routes, optional inline on a relation.
    #[schema(max_length = 55)]
    last_name: Option<String>,
    #[schema(max_length = 55)]
    prefix: Option<String>,
    #[schema(max_length = 55)]
    initials: Option<String>,
    #[schema(max_length = 100)]
    job_title: Option<String>,
    #[schema(max_length = 55)]
    salutation: Option<String>,
    #[schema(max_length = 255)]
    email: Option<String>,
    #[schema(max_length = 55)]
    phone_number: Option<String>,
    /// Up to 20; numbers are normalized to `+<country code>` from the relation's address.
    #[schema(max_items = 20)]
    phones: Option<Vec<PhoneInput>>,
    default_contact: Option<bool>,
    /// One of `default_for_estimate`, `default_for_invoice`, `default_for_order`,
    /// `default_for_purchase_order`, `default_for_work_order`.
    default_for: Option<String>,
    #[schema(value_type = Option<Object>)]
    meta: Option<Value>,
}

/// One phone number to store.
// Doc-only: `phones` item schema; never constructed.
#[derive(ToSchema)]
#[allow(dead_code)]
pub struct PhoneInput {
    /// `home`, `mobile`, `phone`, `phone2` or `work`, any case.
    #[schema(rename = "type", example = "mobile")]
    kind: String,
    #[schema(max_length = 30, example = "06 12345678")]
    number: String,
}

impl Validate for CreateContactRequest {
    type Valid = ContactFields;

    fn validate(self, r: &mut Rules) -> Option<ContactFields> {
        Some(contact_fields(r, "", &self.0, true))
    }
}

// POST /relations/{relation}/contacts/{contact}: every key is a patch (absent = keep).
// Laravel nulled `email`/`phone_number` whenever they were left out; genesy keeps them.
/// POST /relations/{relation}/contacts/{contact}.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Changes to a contact, same rules as create. Leave a field out to keep it, \
        send null to clear it (`first_name` and `last_name` can't be cleared)."
)]
pub struct UpdateContactRequest {
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 55)]
    first_name: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 55)]
    middle_name: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 55)]
    last_name: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 55)]
    prefix: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 55)]
    initials: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 100)]
    job_title: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 55)]
    salutation: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 255)]
    email: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 55)]
    phone_number: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Vec<PhoneInput>>, max_items = 20)]
    phones: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<bool>)]
    default_contact: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    default_for: Option<Option<Value>>,
}

/// Validated contact patch; `None` keeps the stored value, `Some(None)` clears it.
pub struct ContactPatch {
    pub first_name: Option<Option<String>>,
    pub middle_name: Option<Option<String>>,
    pub last_name: Option<Option<String>>,
    pub prefix: Option<Option<String>>,
    pub initials: Option<Option<String>>,
    pub job_title: Option<Option<String>>,
    pub salutation: Option<Option<String>>,
    pub email: Option<Option<String>>,
    pub phone_number: Option<Option<String>>,
    pub phones: Option<Vec<Value>>,
    pub default_contact: Option<bool>,
    pub default_for: Option<Option<String>>,
}

impl Validate for UpdateContactRequest {
    type Valid = ContactPatch;

    fn validate(self, r: &mut Rules) -> Option<ContactPatch> {
        // Run each sent key through the create rules, one key at a time.
        let mut sent = Map::new();
        let mut keys = Vec::new();
        for (k, v) in [
            ("first_name", self.first_name),
            ("middle_name", self.middle_name),
            ("last_name", self.last_name),
            ("prefix", self.prefix),
            ("initials", self.initials),
            ("job_title", self.job_title),
            ("salutation", self.salutation),
            ("email", self.email),
            ("phone_number", self.phone_number),
            ("phones", self.phones),
            ("default_contact", self.default_contact),
            ("default_for", self.default_for),
        ] {
            if let Some(v) = v {
                sent.insert(k.to_owned(), v.unwrap_or(Value::Null));
                keys.push(k);
            }
        }
        let f = contact_fields(r, "", &sent, false);
        let has = |k: &str| keys.contains(&k);
        Some(ContactPatch {
            first_name: has("first_name").then_some(f.first_name),
            middle_name: has("middle_name").then_some(f.middle_name),
            last_name: has("last_name").then_some(f.last_name),
            prefix: has("prefix").then_some(f.prefix),
            initials: has("initials").then_some(f.initials),
            job_title: has("job_title").then_some(f.job_title),
            salutation: has("salutation").then_some(f.salutation),
            email: has("email").then_some(f.email),
            phone_number: has("phone_number").then_some(f.phone_number),
            phones: has("phones").then_some(f.phones),
            default_contact: has("default_contact").then_some(f.default_contact),
            default_for: has("default_for").then_some(f.default_for),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(
        first: Option<&str>,
        middle: Option<&str>,
        last: Option<&str>,
        salutation: Option<&str>,
    ) -> ContactRow {
        ContactRow {
            id: Uuid::nil(),
            relation_id: Uuid::nil(),
            first_name: first.map(Into::into),
            middle_name: middle.map(Into::into),
            last_name: last.map(Into::into),
            email: None,
            phones: json!([]),
            job_title: None,
            initials: None,
            prefix: None,
            attn: None,
            salutation: salutation.map(Into::into),
            default_contact: false,
            default_for: None,
            meta: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn full_name_like_laravel_accessor() {
        assert_eq!(
            full_name(&row(Some("john"), None, Some("Doe"), None)),
            "John Doe"
        );
        assert_eq!(
            full_name(&row(Some("jan"), Some("Van"), Some("Dijk"), None)),
            "Jan van Dijk"
        );
        assert_eq!(
            full_name(&row(None, None, Some("Doe"), Some("Mr"))),
            "Mr Doe"
        );
        assert_eq!(full_name(&row(None, None, Some("Doe"), None)), " Doe");
        assert_eq!(full_name(&row(None, None, None, None)), " ");
    }

    #[test]
    fn resource_hides_empty_optional_keys() {
        let mut c = row(Some("a"), None, Some("b"), None);
        c.prefix = Some("Mr".into());
        c.email = Some("a@b.co".into());
        let v = serde_json::to_value(ContactResource::from(c)).unwrap();

        assert_eq!(v["prefix"], "Mr");
        assert!(v.get("attn").is_none() && v.get("salutation").is_none());
        assert_eq!(v["email_validation"], Value::Null);
        assert_eq!(v["relationships"], json!([]));
        let none =
            serde_json::to_value(ContactResource::from(row(None, None, None, None))).unwrap();
        assert!(none.get("email_validation").is_none());
    }

    fn errors(r: Rules) -> std::collections::BTreeMap<String, Vec<String>> {
        match r.into_result() {
            Ok(()) => Default::default(),
            Err(crate::error::AppError::Validation(e)) => e,
            Err(other) => panic!("{other:?}"),
        }
    }

    #[test]
    fn phones_validated_with_prefixed_keys() {
        let mut r = Rules::default();
        let out = phones(
            &mut r,
            "contact.",
            Some(&json!([{ "type": "Mobile", "number": " 06 1 " }, null, { "type": "fax" }, 5])),
        );

        assert_eq!(out[0], json!({ "type": "mobile", "number": "06 1" }));
        let e = errors(r);
        assert_eq!(
            e["contact.phones.2.type"],
            ["The selected type: fax is invalid."]
        );
        assert_eq!(
            e["contact.phones.2.number"],
            ["The number field is required."]
        );
        assert_eq!(
            e["contact.phones.3"],
            ["The contact.phones.3 must be an array."]
        );
    }
}
