//! Purchase unit SQL. Every query filters by workspace.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgExecutor;
use uuid::Uuid;

/// Most purchase units one product may hold; keeps the list one bounded read.
pub const MAX_PER_PRODUCT: i64 = 50;

/// One stored purchase unit plus its product's unit.
pub struct PurchaseUnitRow {
    pub id: Uuid,
    pub product_id: Uuid,
    pub code: String,
    pub measurement_category: String,
    pub base_units_per_input_unit: Decimal,
    pub active: bool,
    pub version: i32,
    pub last_client_ref: Option<Uuid>,
    pub last_request_hash: Option<Vec<u8>>,
    pub base_unit: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A product's units ordered by code (at most `MAX_PER_PRODUCT`).
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    product_id: Uuid,
) -> Result<Vec<PurchaseUnitRow>, sqlx::Error> {
    sqlx::query_as!(
        PurchaseUnitRow,
        r#"SELECT u.id, u.product_id, u.code, u.measurement_category, u.base_units_per_input_unit,
                  u.active, u.version, u.last_client_ref,
                  u.last_request_hash, p.unit AS base_unit, u.created_at, u.updated_at
             FROM purchase_units u
             JOIN products p ON p.workspace_id = u.workspace_id AND p.id = u.product_id
            WHERE u.workspace_id = $1 AND u.product_id = $2
            ORDER BY lower(u.code)
            LIMIT $3"#,
        workspace_id,
        product_id,
        MAX_PER_PRODUCT,
    )
    .fetch_all(db)
    .await
}

/// One unit of this product; `lock` takes it `FOR NO KEY UPDATE`.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    product_id: Uuid,
    id: Uuid,
    lock: bool,
) -> Result<Option<PurchaseUnitRow>, sqlx::Error> {
    // Two literal statements: sqlx checks each at compile time.
    if lock {
        sqlx::query_as!(
            PurchaseUnitRow,
            r#"SELECT u.id, u.product_id, u.code, u.measurement_category,
                      u.base_units_per_input_unit, u.active, u.version,
                      u.last_client_ref, u.last_request_hash,
                      (SELECT unit FROM products p WHERE p.workspace_id = u.workspace_id
                          AND p.id = u.product_id) AS base_unit, u.created_at, u.updated_at
                 FROM purchase_units u
                WHERE u.workspace_id = $1 AND u.product_id = $2 AND u.id = $3
                FOR NO KEY UPDATE"#,
            workspace_id,
            product_id,
            id,
        )
        .fetch_optional(db)
        .await
    } else {
        sqlx::query_as!(
            PurchaseUnitRow,
            r#"SELECT u.id, u.product_id, u.code, u.measurement_category,
                      u.base_units_per_input_unit, u.active, u.version,
                      u.last_client_ref, u.last_request_hash,
                      (SELECT unit FROM products p WHERE p.workspace_id = u.workspace_id
                          AND p.id = u.product_id) AS base_unit, u.created_at, u.updated_at
                 FROM purchase_units u
                WHERE u.workspace_id = $1 AND u.product_id = $2 AND u.id = $3"#,
            workspace_id,
            product_id,
            id,
        )
        .fetch_optional(db)
        .await
    }
}

/// The unit created under `client_ref` in this workspace, if any.
pub async fn by_client_ref(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    client_ref: Uuid,
) -> Result<Option<(Uuid, Uuid, Vec<u8>)>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT id, product_id, request_hash FROM purchase_units
          WHERE workspace_id = $1 AND client_ref = $2",
        workspace_id,
        client_ref,
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| (r.id, r.product_id, r.request_hash)))
}

/// How many units the product holds.
pub async fn count(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    product_id: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM purchase_units WHERE workspace_id = $1 AND product_id = $2"#,
        workspace_id,
        product_id,
    )
    .fetch_one(db)
    .await
}

/// Fields of a new row.
pub struct NewRow<'a> {
    pub product_id: Uuid,
    pub code: &'a str,
    pub measurement_category: &'a str,
    pub ratio: Decimal,
    pub client_ref: Uuid,
    pub request_hash: &'a [u8],
    pub created_by: Uuid,
}

/// Insert; returns the id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    n: &NewRow<'_>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO purchase_units (workspace_id, product_id, code, measurement_category,
                                     base_units_per_input_unit, client_ref, request_hash,
                                     created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id",
        workspace_id,
        n.product_id,
        n.code,
        n.measurement_category,
        n.ratio,
        n.client_ref,
        n.request_hash,
        n.created_by,
    )
    .fetch_one(db)
    .await
}

/// Toggle `active`, bump the version and remember the request identity.
pub async fn set_active(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    active: bool,
    client_ref: Uuid,
    request_hash: &[u8],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE purchase_units
            SET active = $3, version = version + 1, last_client_ref = $4,
                last_request_hash = $5, updated_at = now()
          WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
        active,
        client_ref,
        request_hash,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// The live product's unit, the product row held `FOR SHARE` so the unit can't change before
/// commit; None = no such product here.
pub async fn product_unit(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    product_id: Uuid,
) -> Result<Option<Option<String>>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT unit FROM products WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL
         FOR SHARE",
        workspace_id,
        product_id,
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| r.unit))
}

/// True when a live product of this workspace has this id.
pub async fn product_exists(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    product_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM products WHERE workspace_id = $1 AND id = $2
                            AND deleted_at IS NULL) AS "e!""#,
        workspace_id,
        product_id,
    )
    .fetch_one(db)
    .await
}
