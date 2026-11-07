//! Mutable invoice input defaults held steady before native resolution and insertion.

use sqlx::PgConnection;
use uuid::Uuid;

/// Lock relation, selected contact and requested product defaults in native lookup order.
pub(super) async fn share_defaults(
    conn: &mut PgConnection,
    workspace: Uuid,
    relation: Uuid,
    contact: Option<Uuid>,
    products: &[Uuid],
    warehouses: &[Uuid],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "SELECT id FROM relations WHERE workspace_id=$1 AND id=$2 AND deleted_at IS NULL FOR SHARE",
        workspace,
        relation
    )
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(contact) = contact {
        sqlx::query!("SELECT id FROM contacts WHERE workspace_id=$1 AND relation_id=$2 AND id=$3 AND deleted_at IS NULL FOR SHARE",workspace,relation,contact).fetch_optional(&mut *conn).await?;
    }
    if !products.is_empty() {
        sqlx::query!("SELECT id FROM products WHERE workspace_id=$1 AND id=ANY($2) AND deleted_at IS NULL ORDER BY id FOR SHARE",workspace,products).fetch_all(&mut *conn).await?;
    }
    if !products.is_empty() || !warehouses.is_empty() {
        sqlx::query!("SELECT w.id FROM warehouses w WHERE w.workspace_id=$1 AND w.deleted_at IS NULL AND (w.id=ANY($3) OR w.id IN (SELECT p.default_warehouse_id FROM products p WHERE p.workspace_id=$1 AND p.id=ANY($2) AND p.deleted_at IS NULL)) ORDER BY w.id FOR SHARE OF w",workspace,products,warehouses).fetch_all(conn).await?;
    }
    Ok(())
}
