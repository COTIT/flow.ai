//! Product image SQL. Every statement filters `workspace_id`.

use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// One image of a product, in order.
pub struct ImageRow {
    pub product_id: Uuid,
    pub product_variant_id: Option<Uuid>,
    pub file_id: Uuid,
    pub position: i16,
}

/// Images of `product_ids` whose file is still stored, by product and position.
pub async fn for_products(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    product_ids: &[Uuid],
) -> Result<Vec<ImageRow>, sqlx::Error> {
    sqlx::query_as!(
        ImageRow,
        "SELECT i.product_id, i.product_variant_id, i.file_id, i.position
         FROM product_images i
         JOIN files f ON f.workspace_id = i.workspace_id AND f.id = i.file_id
                     AND f.status = 'stored'
         WHERE i.workspace_id = $1 AND i.product_id = ANY($2)
         ORDER BY i.product_id, i.position",
        workspace_id,
        product_ids,
    )
    .fetch_all(db)
    .await
}

/// Lock a live product of the workspace; false = 404.
pub async fn lock_product(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM products WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL
         FOR NO KEY UPDATE",
        workspace_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await
    .map(|r| r.is_some())
}

/// Live variants of `product_id` among `ids`.
pub async fn variants_of(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    product_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM product_variants
         WHERE workspace_id = $1 AND product_id = $2 AND id = ANY($3) AND deleted_at IS NULL",
        workspace_id,
        product_id,
        ids,
    )
    .fetch_all(db)
    .await
}

/// Replace the product's images with `images` (file, variant) at positions 1.., and mark the
/// product changed.
pub async fn replace(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    product_id: Uuid,
    images: &[(Uuid, Option<Uuid>)],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM product_images WHERE workspace_id = $1 AND product_id = $2",
        workspace_id,
        product_id,
    )
    .execute(&mut *conn)
    .await?;
    let files: Vec<Uuid> = images.iter().map(|i| i.0).collect();
    let variants: Vec<Option<Uuid>> = images.iter().map(|i| i.1).collect();
    sqlx::query!(
        "INSERT INTO product_images (workspace_id, product_id, file_id, product_variant_id,
                                     position)
         SELECT $1, $2, f, v, n::smallint
         FROM unnest($3::uuid[], $4::uuid[]) WITH ORDINALITY AS t(f, v, n)",
        workspace_id,
        product_id,
        &files,
        &variants as &[Option<Uuid>],
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "UPDATE products SET updated_at = now() WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        product_id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// A live product of the workspace exists.
pub async fn product_exists(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM products
                          WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL) AS "e!""#,
        workspace_id,
        id,
    )
    .fetch_one(db)
    .await
}
