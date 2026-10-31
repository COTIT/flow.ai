//! Workspace record bodies: Laravel WorkspaceResource + UpdateWorkspaceRequest (company profile).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, human, is_bic, is_iban, is_phone, is_url, normalize_iban},
    modules::workspaces::repo::Record,
};

// Stored profile as rendered. Missing keys come out null, `address`/`account` always objects:
// settings.store.js replaces its whole form with this and binds `address.city` etc.
// Fields are raw `Value`s (stored jsonb); the schema documents what validation stores.
/// Company profile. Every key is always present; unset values are null.
#[derive(Serialize, Deserialize, Default, ToSchema)]
#[serde(default)]
#[schema(as = WorkspaceProfile)]
pub struct Profile {
    #[schema(value_type = Option<String>, required = true)]
    company_name: Option<Value>,
    #[schema(value_type = Option<String>, required = true)]
    vat_number: Option<Value>,
    #[schema(value_type = Option<String>, required = true)]
    coc_number: Option<Value>,
    address: Address,
    // None only in `public()` (any member reads /me/workspaces).
    /// Bank details; left out where any member can read the profile.
    #[serde(skip_serializing_if = "Option::is_none")]
    account: Option<Account>,
    #[schema(value_type = Option<String>, required = true)]
    phone_number: Option<Value>,
    #[schema(value_type = Option<String>, required = true)]
    email: Option<Value>,
    #[schema(value_type = Option<String>, required = true)]
    website: Option<Value>,
    // Files not ported: always null.
    /// Not supported yet, always null.
    #[schema(value_type = Option<String>, required = true)]
    logo_file_id: Option<Value>,
    /// Not supported yet, always null.
    #[schema(value_type = Option<String>, required = true)]
    logo: Option<Value>,
    /// `en` or `nl`.
    #[schema(value_type = Option<String>, required = true)]
    language: Option<Value>,
}

/// Company address. Unset values are null.
#[derive(Serialize, Deserialize, Default, ToSchema)]
#[serde(default)]
#[schema(as = WorkspaceAddress)]
struct Address {
    #[schema(value_type = Option<String>, required = true)]
    address: Option<Value>,
    /// Numeric, sent as a string or a number; returned as sent.
    #[schema(value_type = Option<Value>, required = true)]
    house_number: Option<Value>,
    #[schema(value_type = Option<String>, required = true)]
    house_number_addition: Option<Value>,
    #[schema(value_type = Option<String>, required = true)]
    address2: Option<Value>,
    #[schema(value_type = Option<String>, required = true)]
    zipcode: Option<Value>,
    #[schema(value_type = Option<String>, required = true)]
    city: Option<Value>,
    #[schema(value_type = Option<String>, required = true)]
    state: Option<Value>,
    #[schema(value_type = Option<String>, required = true)]
    country: Option<Value>,
}

/// Bank details. Unset values are null.
#[derive(Serialize, Deserialize, Default, ToSchema)]
#[serde(default)]
#[schema(as = WorkspaceAccount)]
struct Account {
    #[schema(value_type = Option<String>, required = true)]
    account_name: Option<Value>,
    /// IBAN, without spaces, uppercase.
    #[schema(value_type = Option<String>, required = true)]
    account_number: Option<Value>,
    /// BIC / SWIFT code.
    #[schema(value_type = Option<String>, required = true)]
    bic_swift: Option<Value>,
    #[schema(value_type = Option<f64>, required = true, minimum = 0)]
    credit_limit: Option<Value>,
}

impl Profile {
    /// From the jsonb column. A null column renders empty; a field of the wrong shape
    /// (legacy/imported row) renders empty instead of blanking the whole profile.
    pub fn from_stored(stored: Option<Value>) -> Self {
        let Some(Value::Object(map)) = stored else {
            return Self::with_account();
        };
        let mut p = Self::with_account();
        let field = |key: &str| map.get(key).filter(|v| !v.is_null()).cloned();
        p.company_name = field("company_name");
        p.vat_number = field("vat_number");
        p.coc_number = field("coc_number");
        p.phone_number = field("phone_number");
        p.email = field("email");
        p.website = field("website");
        p.language = field("language");
        if let Some(a) = field("address").and_then(|v| serde_json::from_value(v).ok()) {
            p.address = a;
        }
        if let Some(a) = field("account").and_then(|v| serde_json::from_value(v).ok()) {
            p.account = Some(a);
        }
        p
    }

    /// Without bank details: for views any member can read.
    pub fn public(mut self) -> Self {
        self.account = None;
        self
    }

    fn with_account() -> Self {
        Self {
            account: Some(Account::default()),
            ..Self::default()
        }
    }
}

// GET /v1/workspace/{workspace} body (Laravel WorkspaceResource).
/// The workspace record with its full company profile.
#[derive(Serialize, ToSchema)]
pub struct WorkspaceRecordResource {
    id: Uuid,
    workspace_name: String,
    workspace_type: String,
    pending_invites_count: i64,
    users_count: i64,
    workspace_profile: Profile,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
    /// Always empty.
    #[schema(value_type = Vec<Object>, max_items = 0)]
    relationships: [(); 0],
}

impl From<Record> for WorkspaceRecordResource {
    fn from(r: Record) -> Self {
        Self {
            id: r.id,
            workspace_name: r.name,
            workspace_type: r.workspace_type,
            pending_invites_count: r.pending_invites,
            users_count: r.users,
            workspace_profile: Profile::from_stored(r.profile),
            created_at: r.created_at,
            last_updated_at: r.updated_at,
            meta: r.meta,
            relationships: [],
        }
    }
}

/// POST /v1/workspace/{workspace}/update: any subset of the profile keys. Unknown keys
/// (`workspace_type`, ...) are dropped; Laravel only wrote `workspace_profile` + `meta`.
#[derive(Deserialize)]
#[serde(transparent)]
pub struct UpdateWorkspaceRequest(Map<String, Value>);

/// Wire shape of [`UpdateWorkspaceRequest`] for the spec; never built (the handler parses a map).
#[derive(ToSchema)]
#[schema(
    description = "Company profile changes. Leave a key out to keep it; send null (or a blank \
        string) to clear it. A sent `address` or `account` replaces the stored one."
)]
#[allow(dead_code)]
pub struct WorkspaceProfilePatch {
    #[schema(max_length = 55)]
    company_name: Option<String>,
    #[schema(max_length = 55)]
    vat_number: Option<String>,
    #[schema(max_length = 55)]
    coc_number: Option<String>,
    #[schema(max_length = 55)]
    phone_number: Option<String>,
    /// `en` or `nl`.
    #[schema(example = "en")]
    language: Option<String>,
    #[schema(format = Email, max_length = 255)]
    email: Option<String>,
    #[schema(max_length = 255, example = "https://example.com")]
    website: Option<String>,
    /// Must be null for now.
    logo_file_id: Option<Uuid>,
    address: Option<AddressInput>,
    account: Option<AccountInput>,
    #[schema(value_type = Option<Object>)]
    meta: Option<Value>,
}

/// New company address.
#[derive(ToSchema)]
#[schema(as = WorkspaceAddressInput)]
#[allow(dead_code)]
struct AddressInput {
    #[schema(max_length = 255)]
    address: String,
    /// Numeric; a string or a number.
    #[schema(value_type = String, example = "12")]
    house_number: String,
    #[schema(max_length = 10)]
    house_number_addition: Option<String>,
    #[schema(max_length = 255)]
    address2: Option<String>,
    #[schema(max_length = 20)]
    zipcode: String,
    #[schema(max_length = 255)]
    city: String,
    #[schema(max_length = 255)]
    state: Option<String>,
    #[schema(max_length = 255)]
    country: String,
}

/// New bank details.
#[derive(ToSchema)]
#[schema(as = WorkspaceAccountInput)]
#[allow(dead_code)]
struct AccountInput {
    #[schema(max_length = 255)]
    account_name: Option<String>,
    /// IBAN; spaces and case are normalized.
    account_number: Option<String>,
    /// BIC / SWIFT code.
    bic_swift: Option<String>,
    #[schema(minimum = 0)]
    credit_limit: Option<f64>,
}

/// Validated patch: profile keys to overwrite (null = clear), and meta when sent.
pub struct ProfilePatch {
    pub profile: Value,
    pub meta: Option<Option<Value>>,
}

const TOP_TEXT: [(&str, usize); 3] = [("company_name", 55), ("vat_number", 55), ("coc_number", 55)];

impl Validate for UpdateWorkspaceRequest {
    type Valid = ProfilePatch;

    fn validate(self, r: &mut Rules) -> Option<ProfilePatch> {
        let mut body = self.0;
        let mut out = Map::new();
        for (key, max) in TOP_TEXT {
            if let Some(v) = body.remove(key) {
                out.insert(key.into(), text_or_null(r, key, v, max));
            }
        }
        if let Some(v) = body.remove("phone_number") {
            let v = text_or_null(r, "phone_number", v, 55);
            if let Value::String(s) = &v {
                r.check("phone_number", is_phone(s), "Phone number is not valid");
            }
            out.insert("phone_number".into(), v);
        }
        if let Some(v) = body.remove("language") {
            let v = text_or_null(r, "language", v, 2);
            if let Value::String(s) = &v {
                r.check(
                    "language",
                    s == "en" || s == "nl",
                    "The selected language is invalid.",
                );
            }
            out.insert("language".into(), v);
        }
        if let Some(v) = body.remove("email") {
            let v = text_or_null(r, "email", v, 255);
            if let Value::String(s) = &v {
                r.email("email", s);
            }
            out.insert("email".into(), v);
        }
        if let Some(v) = body.remove("website") {
            let v = text_or_null(r, "website", v, 255);
            if let Value::String(s) = &v {
                r.check("website", is_url(s), "The website must be a valid URL.");
            }
            out.insert("website".into(), v);
        }
        if let Some(v) = body.remove("logo_file_id") {
            // shortcut: files not ported, so no id can exist yet; accept once File is ported.
            r.check(
                "logo_file_id",
                v.is_null(),
                "The selected logo file id is invalid.",
            );
            out.insert("logo_file_id".into(), Value::Null);
        }
        if let Some(v) = body.remove("address") {
            out.insert("address".into(), address(r, v));
        }
        if let Some(v) = body.remove("account") {
            out.insert("account".into(), account(r, "account", "account.", v));
        }
        let meta = body.remove("meta").map(|v| match v {
            Value::Null => None,
            Value::Object(_) => Some(v),
            _ => {
                r.add("meta", "The meta must be an array.");
                None
            }
        });
        Some(ProfilePatch {
            profile: Value::Object(out),
            meta,
        })
    }
}

/// Nullable string ≤ `max` chars, trimmed, blank = null. None = wrong type (error recorded).
fn text(r: &mut Rules, field: &str, v: Value, max: usize) -> Option<Value> {
    let name = field.rsplit('.').next().unwrap_or(field);
    match v {
        Value::Null => Some(Value::Null),
        Value::String(s) if s.trim().is_empty() => Some(Value::Null),
        Value::String(s) => {
            let s = s.trim().to_owned();
            if s.chars().count() > max {
                let msg = format!(
                    "The {} must not be greater than {max} characters.",
                    human(name)
                );
                r.add(field, msg);
            }
            Some(Value::String(s))
        }
        _ => {
            r.add(field, format!("The {} must be a string.", human(name)));
            None
        }
    }
}

/// `text` where a wrong type stores null (the error is already recorded).
fn text_or_null(r: &mut Rules, field: &str, v: Value, max: usize) -> Value {
    text(r, field, v, max).unwrap_or(Value::Null)
}

/// Number or numeric string (Laravel `numeric`). None = missing/null, or invalid (error recorded).
fn numeric(r: &mut Rules, field: &str, v: Option<Value>) -> Option<f64> {
    let name = field.rsplit('.').next().unwrap_or(field);
    let n = match v? {
        Value::Null => return None,
        Value::Number(n) => n.as_f64(),
        // Length cap first: a 60 KB digit string is not a number anyone means.
        Value::String(s) if s.trim().len() <= 20 => {
            s.trim().parse::<f64>().ok().filter(|f| f.is_finite())
        }
        _ => None,
    };
    if n.is_none() {
        r.add(field, format!("The {} must be a number.", human(name)));
    }
    n
}

/// Laravel Address::rules: address/house_number/zipcode/city/country required.
fn address(r: &mut Rules, v: Value) -> Value {
    let Value::Object(mut a) = v else {
        if !v.is_null() {
            r.add("address", "The address is not valid");
        }
        return Value::Null;
    };
    let mut out = Map::new();
    for (key, max, required) in [
        ("address", 255, true),
        ("house_number_addition", 10, false),
        ("address2", 255, false),
        ("zipcode", 20, true),
        ("city", 255, true),
        ("state", 255, false),
        ("country", 255, true),
    ] {
        let field = format!("address.{key}");
        let value = text(r, &field, a.remove(key).unwrap_or(Value::Null), max);
        if required && value.as_ref().is_some_and(Value::is_null) {
            r.add(&field, format!("The {} field is required.", human(key)));
        }
        out.insert(key.into(), value.unwrap_or(Value::Null));
    }
    // Stored as the trimmed text (or the number) the client sent, after a numeric check.
    let house = match a.remove("house_number") {
        None | Some(Value::Null) => {
            r.add(
                "address.house_number",
                "The house number field is required.",
            );
            Value::Null
        }
        Some(v) => match numeric(r, "address.house_number", Some(v.clone())) {
            None => Value::Null,
            Some(_) => match v {
                Value::String(s) => Value::String(s.trim().to_owned()),
                other => other,
            },
        },
    };
    out.insert("house_number".into(), house);
    Value::Object(out)
}

/// Laravel Account::rules: iban, bic, credit_limit numeric ≥ 0, all nullable. IBAN stored
/// normalized (no spaces, uppercase), credit_limit as a JSON number. `key` names the whole
/// object, `prefix` goes before inner error keys (relations report them bare, as Laravel did).
pub(crate) fn account(r: &mut Rules, key: &str, prefix: &str, v: Value) -> Value {
    let Value::Object(mut a) = v else {
        if !v.is_null() {
            r.add(key, format!("The {} is not valid", human(key)));
        }
        return Value::Null;
    };
    let k = |field: &str| format!("{prefix}{field}");
    let mut take = |name: &str| a.remove(name).unwrap_or(Value::Null);
    let (name, number, bic, limit) = (
        take("account_name"),
        take("account_number"),
        take("bic_swift"),
        take("credit_limit"),
    );
    let name = text_or_null(r, &k("account_name"), name, 255);
    let number = match text_or_null(r, &k("account_number"), number, 255) {
        Value::String(s) if is_iban(&s) => Value::String(normalize_iban(&s)),
        Value::String(s) => {
            r.add(
                &k("account_number"),
                "The account number must be a valid IBAN.",
            );
            Value::String(s)
        }
        other => other,
    };
    let bic = text_or_null(r, &k("bic_swift"), bic, 255);
    if let Value::String(s) = &bic {
        r.check(
            &k("bic_swift"),
            is_bic(s),
            "The bic swift must be a valid BIC.",
        );
    }
    let limit = match numeric(r, &k("credit_limit"), Some(limit)) {
        Some(n) => {
            r.check(
                &k("credit_limit"),
                n >= 0.0,
                "The credit limit must be at least 0.",
            );
            serde_json::Number::from_f64(n).map_or(Value::Null, Value::Number)
        }
        None => Value::Null,
    };
    let mut out = Map::new();
    out.insert("account_name".into(), name);
    out.insert("account_number".into(), number);
    out.insert("bic_swift".into(), bic);
    out.insert("credit_limit".into(), limit);
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_or_null_profile_renders_objects() {
        for stored in [None, Some(json!({ "address": null, "account": null }))] {
            let v = serde_json::to_value(Profile::from_stored(stored)).unwrap();
            assert!(v["address"].is_object() && v["account"].is_object());
            assert_eq!(v["company_name"], Value::Null);
        }
    }

    #[test]
    fn wrong_shaped_field_renders_empty_keeps_the_rest() {
        let stored = json!({ "company_name": "Acme", "address": "Damrak 1", "account": [1] });

        let v = serde_json::to_value(Profile::from_stored(Some(stored))).unwrap();

        assert_eq!(v["company_name"], "Acme");
        assert!(v["address"].is_object() && v["account"].is_object());
    }

    #[test]
    fn public_profile_hides_account() {
        let v = serde_json::to_value(Profile::from_stored(None).public()).unwrap();
        assert!(v.get("account").is_none() && v["address"].is_object());
    }

    #[test]
    fn unknown_keys_dropped_from_patch() {
        let mut r = Rules::default();
        let req = UpdateWorkspaceRequest(
            json!({ "workspace_type": "x", "company_name": " Acme " })
                .as_object()
                .unwrap()
                .clone(),
        );
        let patch = req.validate(&mut r).unwrap();
        assert!(r.into_result().is_ok());
        assert_eq!(patch.profile, json!({ "company_name": "Acme" }));
        assert!(patch.meta.is_none());
    }
}
