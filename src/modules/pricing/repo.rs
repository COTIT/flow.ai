//! Narrow reads for the price calculator: only the columns a line needs, live rows of one
//! workspace.

use rust_decimal::Decimal;
use sqlx::PgExecutor;
use uuid::Uuid;

/// What a product line takes from its product.
pub struct PricingProduct {
    pub id: Uuid,
    pub name: String,
    pub impact_on_wholesale_price: Option<Decimal>,
    pub wholesale_price: Option<Decimal>,
    pub supplier_wholesale_price: Option<Decimal>,
    pub gross_price: Decimal,
}

/// Live products of `workspace_id` among `ids`, in one query.
pub async fn products(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<PricingProduct>, sqlx::Error> {
    sqlx::query_as!(
        PricingProduct,
        "SELECT id, name, impact_on_wholesale_price, wholesale_price, supplier_wholesale_price,
                gross_price
         FROM products
         WHERE id = ANY($1) AND workspace_id = $2 AND deleted_at IS NULL",
        ids,
        workspace_id,
    )
    .fetch_all(db)
    .await
}

/// True when `id` is a live relation of `workspace_id`.
pub async fn relation_exists(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM relations
                          WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL) AS "e!""#,
        id,
        workspace_id,
    )
    .fetch_one(db)
    .await
}
