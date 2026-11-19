//! Per-profile rules: allowed columns, native decoding, exact matching and the native write.
use serde_json::{Map, Value, json};
use sqlx::{PgConnection, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    error::AppError,
    http::json::{Rules, Validate},
    modules::{
        accounting::{banking, opening},
        product_bulk_runs::service::{lock_products, resolve_unlocked},
        products::{self, service::Actor},
        relations::{
            self,
            dto::{CreateRelationRequest, NewRelation},
        },
    },
};

/// Product columns an import may set.
const PRODUCT_FIELDS: &[&str] = &[
    "name",
    "price",
    "barcode",
    "ean13",
    "reference",
    "description",
    "unit",
];
/// Relation columns an import may set; the address ones form the first address.
const RELATION_FIELDS: &[&str] = &[
    "type",
    "company_name",
    "alias",
    "coc_number",
    "vat_number",
    "email",
    "website",
    "phone_number",
    "default_language",
];
const ADDRESS_FIELDS: &[&str] = &[
    "address",
    "address2",
    "house_number",
    "zipcode",
    "city",
    "state",
    "country",
];

/// Whether `profile` accepts column `name`.
pub fn allows(profile: &str, name: &str) -> bool {
    match profile {
        "products" => PRODUCT_FIELDS.contains(&name),
        "relations" => RELATION_FIELDS.contains(&name) || ADDRESS_FIELDS.contains(&name),
        banking::PROFILE => banking::ROW_FIELDS.contains(&name),
        p if opening::is_profile(p) => opening::allows(p, name),
        _ => false,
    }
}

/// Native relation create input from flat import columns.
pub fn relation_input(fields: &Value) -> Result<NewRelation, AppError> {
    let mut body = Map::new();
    let mut address = Map::new();
    for (k, v) in fields.as_object().into_iter().flatten() {
        if ADDRESS_FIELDS.contains(&k.as_str()) {
            address.insert(k.clone(), v.clone());
        } else {
            body.insert(k.clone(), v.clone());
        }
    }
    if !address.is_empty() {
        body.insert("addresses".into(), json!([address]));
    }
    let request: CreateRelationRequest = serde_json::from_value(Value::Object(body))
        .map_err(|_| AppError::field("fields", "Use a relation object."))?;
    let mut rules = Rules::default();
    let input = request.validate(&mut rules);
    rules.into_result()?;
    input.ok_or_else(|| AppError::field("fields", "Use valid relation values."))
}

/// The exact identity a products row uses: the match column on create, `external_key` on update.
pub fn product_key<'a>(
    mapping: &Value,
    fields: &'a Value,
    external_key: Option<&'a str>,
) -> Option<&'a str> {
    let key = if mapping["operation"] == "update" {
        external_key
    } else {
        fields[mapping["match_key"].as_str().unwrap_or_default()].as_str()
    };
    key.filter(|k| !k.is_empty())
}

/// Validation outcome for one row: matched product (update) or the refusal.
pub async fn check(
    conn: &mut PgConnection,
    workspace: Uuid,
    profile: &str,
    mapping: &Value,
    fields: &Value,
    external_key: Option<&str>,
) -> Result<Option<Uuid>, AppError> {
    if profile == "relations" {
        relation_input(fields)?;
        return Ok(None);
    }
    let update = mapping["operation"] == "update";
    let match_key = mapping["match_key"].as_str().unwrap_or_default();
    products::dto::decode_values(fields.clone(), !update)?;
    let Some(key) = product_key(mapping, fields, external_key) else {
        if update {
            return Err(AppError::field(
                "external_key",
                format!("Give the exact {match_key} of the product to update."),
            ));
        }
        return Ok(None);
    };
    let found =
        resolve_unlocked(conn, workspace, match_key, &json!({"key": key}), &json!({})).await?;
    match (update, found.as_slice()) {
        (false, []) => Ok(None),
        (false, _) => Err(AppError::field(
            match_key,
            format!("A product with this exact {match_key} already exists."),
        )),
        (true, [id]) => Ok(Some(*id)),
        (true, []) => Err(AppError::field(
            "external_key",
            format!("No product has exactly this {match_key}."),
        )),
        (true, _) => Err(AppError::field(
            "external_key",
            format!("Several products share this {match_key}; make it unique first."),
        )),
    }
}

/// Native write for one valid row inside its own transaction; returns the record id.
pub async fn apply(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    actor: Uuid,
    profile: &str,
    mapping: &Value,
    row: &Value,
) -> Result<Uuid, AppError> {
    let fields = &row["fields"];
    if profile == "relations" {
        return relations::service::create_in(tx, workspace, actor, relation_input(fields)?).await;
    }
    let external_key = row["external_key"].as_str();
    let actor = Actor {
        user_id: actor,
        flow: "manual",
    };
    products::service::lock_identity(tx, workspace).await?;
    // Recheck the exact match under the identity lock: validation may be stale.
    let target = check(tx, workspace, profile, mapping, fields, external_key)
        .await
        .map_err(|_| AppError::Conflict("This row no longer matches; validate a new import."))?;
    let expected = row["target_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok());
    if target != expected {
        return Err(AppError::Conflict(
            "This row no longer matches; validate a new import.",
        ));
    }
    match target {
        None => {
            products::service::create_in(
                tx,
                workspace,
                products::dto::decode_values(fields.clone(), true)?,
                actor,
            )
            .await
        }
        Some(id) => {
            lock_products(tx, workspace, &[id]).await?;
            products::service::update_in(
                tx,
                workspace,
                id,
                products::dto::decode_values(fields.clone(), false)?,
                actor,
            )
            .await?;
            Ok(id)
        }
    }
}

/// Readable refusal for a row: `{code, message, fields?}`; never a raw error string.
pub fn issue(error: &AppError) -> Value {
    match error {
        AppError::Validation(v) => {
            let message = v.values().flatten().cloned().collect::<Vec<_>>().join(" ");
            json!({"code": "validation", "message": message, "fields": v})
        }
        AppError::Reason { code, message, .. } => json!({"code": code, "message": message}),
        AppError::Conflict(m) => json!({"code": "stale", "message": m}),
        AppError::NotFound => {
            json!({"code": "not_found", "message": "A referenced record no longer exists."})
        }
        AppError::Forbidden(_) => {
            json!({"code": "permission", "message": "The person who applied this import no longer has permission."})
        }
        AppError::Unprocessable(m) => json!({"code": "invalid", "message": m}),
        _ => {
            json!({"code": "failed", "message": "This row could not be saved. Check it and import it again."})
        }
    }
}
