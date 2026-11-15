//! Purchase unit rules: category must match the product unit, exact immutable ratio,
//! client_ref replay, versioned activation.

use axum::http::StatusCode;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::workspace::WorkspaceAccess,
    idempotency::{self, Actor, Answer, Key},
    modules::purchase_units::{
        dto::{NewPurchaseUnit, PurchaseUnitChange, PurchaseUnitResource, category},
        repo::{self, MAX_PER_PRODUCT, NewRow, PurchaseUnitRow},
    },
};

const REPLAY_CONFLICT: &str = "This client_ref was already used for a different request.";

fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

fn resource(r: PurchaseUnitRow) -> PurchaseUnitResource {
    PurchaseUnitResource {
        id: r.id,
        product_id: r.product_id,
        code: r.code,
        measurement_category: r.measurement_category,
        base_unit: r.base_unit,
        base_units_per_input_unit: r.base_units_per_input_unit.normalize().to_string(),
        active: r.active,
        version: r.version,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }
}

/// Canonical request fingerprint: equal meaning hashes equal, whatever the JSON spelling.
fn fingerprint(parts: &[&str]) -> Vec<u8> {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update(b"\n");
    }
    h.finalize().to_vec()
}

/// A product's purchase units, 404 when the product isn't a live one of this workspace.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    product_id: &str,
) -> Result<Vec<PurchaseUnitResource>, AppError> {
    let product_id = parse(product_id)?;
    if !repo::product_exists(&state.db, workspace_id, product_id).await? {
        return Err(AppError::NotFound);
    }
    let rows = repo::list(&state.db, workspace_id, product_id).await?;
    Ok(rows.into_iter().map(resource).collect())
}

/// Create (201), or replay the unit made under the same client_ref (200).
pub async fn create(
    state: &AppState,
    access: &WorkspaceAccess,
    product_id: &str,
    input: NewPurchaseUnit,
    key: Key,
) -> Result<Answer, AppError> {
    let ws = access.workspace_id;
    let product_id = parse(product_id)?;
    let mut tx = state.db.begin().await?;
    if let Some(answer) =
        idempotency::replay(&mut tx, ws, Actor::User(access.user_id), Some(&key)).await?
    {
        return Ok(answer);
    }
    let unit = repo::product_unit(&mut *tx, ws, product_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let ratio = input.ratio.normalize().to_string();
    let hash = fingerprint(&[
        &product_id.to_string(),
        &input.code.to_lowercase(),
        &input.measurement_category,
        &ratio,
    ]);
    // Serialize same-client_ref creates, then decide replay vs conflict.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('purchase_unit:' || $1::text, 0))")
        .bind(format!("{ws}:{}", input.client_ref))
        .execute(&mut *tx)
        .await?;
    if let Some((id, _, stored)) = repo::by_client_ref(&mut *tx, ws, input.client_ref).await? {
        if stored != hash {
            return Err(AppError::Conflict(REPLAY_CONFLICT));
        }
        let row = repo::find(&mut *tx, ws, product_id, id, false)
            .await?
            .ok_or(AppError::NotFound)?;
        let answer = Answer {
            status: 200,
            body: serde_json::json!({ "data": resource(row) })
                .to_string()
                .into_bytes(),
        };
        idempotency::finish(&mut tx, ws, Some(&key), &answer).await?;
        tx.commit().await?;
        return Ok(answer);
    }
    let expected = match unit.as_deref() {
        None => {
            return Err(AppError::Reason {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "purchase_unit_base_unsupported",
                message: "Set the product's unit before adding purchase units.".into(),
            });
        }
        Some("box") => {
            return Err(AppError::Reason {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "purchase_unit_base_unsupported",
                message: "Box products need a content unit before purchase units.".into(),
            });
        }
        Some(u) => category(u).ok_or_else(|| AppError::Internal(format!("unit {u}")))?,
    };
    if input.measurement_category != expected {
        return Err(AppError::field(
            "payload.measurement_category",
            format!(
                "The measurement category must be {expected} for a product sold in {}.",
                unit.unwrap_or_default()
            ),
        ));
    }
    if repo::count(&mut *tx, ws, product_id).await? >= MAX_PER_PRODUCT {
        return Err(AppError::field(
            "payload.code",
            format!("A product holds at most {MAX_PER_PRODUCT} purchase units."),
        ));
    }
    let id = repo::insert(
        &mut *tx,
        ws,
        &NewRow {
            product_id,
            code: &input.code,
            measurement_category: &input.measurement_category,
            ratio: input.ratio,
            client_ref: input.client_ref,
            request_hash: &hash,
            created_by: access.user_id,
        },
    )
    .await
    .map_err(
        |e| match e.as_database_error().and_then(|d| d.constraint()) {
            Some("purchase_units_code_key") => {
                AppError::field("payload.code", "The code has already been taken.")
            }
            _ => e.into(),
        },
    )?;
    let row = repo::find(&mut *tx, ws, product_id, id, false)
        .await?
        .ok_or(AppError::NotFound)?;
    let answer = Answer::created(&serde_json::json!({ "data": resource(row) }));
    idempotency::finish(&mut tx, ws, Some(&key), &answer).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Turn a unit on or off. Stale `expected_version` is 409; no change keeps the version.
pub async fn update(
    state: &AppState,
    access: &WorkspaceAccess,
    product_id: &str,
    id: &str,
    input: PurchaseUnitChange,
    key: Key,
) -> Result<Answer, AppError> {
    let ws = access.workspace_id;
    let (product_id, id) = (parse(product_id)?, parse(id)?);
    let mut tx = state.db.begin().await?;
    if let Some(answer) =
        idempotency::replay(&mut tx, ws, Actor::User(access.user_id), Some(&key)).await?
    {
        return Ok(answer);
    }
    let row = repo::find(&mut *tx, ws, product_id, id, true)
        .await?
        .ok_or(AppError::NotFound)?;
    let hash = fingerprint(&[
        &id.to_string(),
        &input.expected_version.to_string(),
        if input.active { "1" } else { "0" },
    ]);
    let replayed = row.last_client_ref == Some(input.client_ref);
    if replayed && row.last_request_hash.as_deref() != Some(&hash[..]) {
        return Err(AppError::Conflict(REPLAY_CONFLICT));
    }
    if !replayed {
        if row.version != input.expected_version {
            return Err(AppError::Reason {
                status: StatusCode::CONFLICT,
                code: "stale_document",
                message: "This purchase unit changed since you read it. Reload and try again."
                    .into(),
            });
        }
        if row.active != input.active {
            repo::set_active(&mut *tx, ws, id, input.active, input.client_ref, &hash).await?;
        }
    }
    let row = repo::find(&mut *tx, ws, product_id, id, false)
        .await?
        .ok_or(AppError::NotFound)?;
    let answer = Answer {
        status: 200,
        body: serde_json::json!({ "data": resource(row) })
            .to_string()
            .into_bytes(),
    };
    idempotency::finish(&mut tx, ws, Some(&key), &answer).await?;
    tx.commit().await?;
    Ok(answer)
}
