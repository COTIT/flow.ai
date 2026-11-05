//! Packing and transfer SQL. A transfer is read only by the two workspaces it joins.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// A transfer with the codes either side may see.
pub struct TransferRow {
    pub id: Uuid,
    pub link_id: Uuid,
    pub from_workspace_id: Uuid,
    pub to_workspace_id: Uuid,
    pub partner_name: String,
    pub partner_display_name: String,
    pub from_lot_code: String,
    pub from_lot_public_code: String,
    pub from_lot_status: String,
    pub quantity: Decimal,
    pub dispatched_at: DateTime<Utc>,
    pub to_lot_id: Option<Uuid>,
    pub to_lot_code: Option<String>,
    pub received_quantity: Option<Decimal>,
    pub received_at: Option<DateTime<Utc>>,
}

/// A link's sides and whether it is live.
pub struct LinkState {
    pub from_workspace_id: Uuid,
    pub to_workspace_id: Uuid,
    pub live: bool,
}

/// The link `id`, if `workspace_id` is a side of it.
pub async fn link(
    db: impl PgExecutor<'_>,
    id: Uuid,
    workspace_id: Uuid,
) -> Result<Option<LinkState>, sqlx::Error> {
    sqlx::query_as!(
        LinkState,
        r#"SELECT from_workspace_id, to_workspace_id,
                  (accepted_at IS NOT NULL AND revoked_at IS NULL) AS "live!"
           FROM supply_links
           WHERE id = $1 AND (from_workspace_id = $2 OR to_workspace_id = $2)"#,
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// Mark `child` as packed from `parent`.
pub async fn set_pack(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    child: Uuid,
    parent: Uuid,
    raw_kg: Decimal,
    units: Decimal,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE lots SET parent_lot_id = $3, pack_raw_kg = $4, pack_units = $5
         WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        child,
        parent,
        raw_kg,
        units,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// A new dispatched transfer; its id.
#[allow(clippy::too_many_arguments)]
pub async fn insert_transfer(
    conn: &mut PgConnection,
    link_id: Uuid,
    from_workspace_id: Uuid,
    from_lot_id: Uuid,
    from_warehouse_id: Uuid,
    quantity: Decimal,
    to_workspace_id: Uuid,
    by: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO supply_transfers (link_id, from_workspace_id, from_lot_id, from_warehouse_id,
                                       quantity, to_workspace_id, dispatched_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
        link_id,
        from_workspace_id,
        from_lot_id,
        from_warehouse_id,
        quantity,
        to_workspace_id,
        by,
    )
    .fetch_one(conn)
    .await
}

/// Transfers `workspace_id` is a side of (only `id` when given), newest first, and the count.
pub async fn list(
    db: impl PgExecutor<'_> + Copy,
    workspace_id: Uuid,
    f: Filter<'_>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<TransferRow>, i64), sqlx::Error> {
    let rows = rows(db, workspace_id, f, limit, offset).await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM supply_transfers
           WHERE (from_workspace_id = $1 OR to_workspace_id = $1)
             AND ($2::uuid IS NULL OR id = $2)
             AND ($3::text IS NULL OR ($3 = 'received') = (received_at IS NOT NULL))
             AND ($4::text IS NULL OR ($4 = 'outgoing') = (from_workspace_id = $1))"#,
        workspace_id,
        f.id,
        f.status,
        f.direction,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// Which transfers a list returns; `None` = any.
#[derive(Clone, Copy, Default)]
pub struct Filter<'a> {
    pub id: Option<Uuid>,
    /// `dispatched` (not received yet) or `received`.
    pub status: Option<&'a str>,
    /// `outgoing` (this workspace sent it) or `incoming`.
    pub direction: Option<&'a str>,
}

impl Filter<'_> {
    /// Just the transfer `id`.
    pub fn id(id: Uuid) -> Self {
        Self {
            id: Some(id),
            ..Self::default()
        }
    }
}

/// The rows of `list`, without the count (usable inside a transaction).
pub async fn rows(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: Filter<'_>,
    limit: i64,
    offset: i64,
) -> Result<Vec<TransferRow>, sqlx::Error> {
    sqlx::query_as!(
        TransferRow,
        r#"SELECT t.id, t.link_id, t.from_workspace_id, t.to_workspace_id, w.name AS partner_name,
                  coalesce(nullif(btrim(w.profile->>'company_name'), ''), w.name) AS "partner_display_name!",
                  f.code AS from_lot_code, f.public_code AS from_lot_public_code,
                  f.status AS from_lot_status, t.quantity, t.dispatched_at, t.to_lot_id,
                  r.code AS "to_lot_code?", t.received_quantity, t.received_at
           FROM supply_transfers t
           JOIN lots f ON f.workspace_id = t.from_workspace_id AND f.id = t.from_lot_id
           LEFT JOIN lots r ON r.workspace_id = t.to_workspace_id AND r.id = t.to_lot_id
           JOIN workspaces w ON w.id = CASE WHEN t.from_workspace_id = $1
                                            THEN t.to_workspace_id ELSE t.from_workspace_id END
           WHERE (t.from_workspace_id = $1 OR t.to_workspace_id = $1)
             AND ($2::uuid IS NULL OR t.id = $2)
             AND ($5::text IS NULL OR ($5 = 'received') = (t.received_at IS NOT NULL))
             AND ($6::text IS NULL OR ($6 = 'outgoing') = (t.from_workspace_id = $1))
           ORDER BY t.dispatched_at DESC, t.id DESC LIMIT $3 OFFSET $4"#,
        workspace_id,
        f.id,
        limit,
        offset,
        f.status,
        f.direction,
    )
    .fetch_all(db)
    .await
}

/// Lock a transfer addressed to `to`; (quantity, received yet, link) or None.
pub async fn lock_incoming(
    conn: &mut PgConnection,
    id: Uuid,
    to: Uuid,
) -> Result<Option<(Decimal, bool, Uuid)>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT quantity, to_lot_id IS NOT NULL AS received, link_id FROM supply_transfers
         WHERE id = $1 AND to_workspace_id = $2 FOR UPDATE",
        id,
        to,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| (r.quantity, r.received.unwrap_or(false), r.link_id)))
}

/// Record the receipt into `lot`.
pub async fn mark_received(
    conn: &mut PgConnection,
    id: Uuid,
    lot: Uuid,
    quantity: Decimal,
    by: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE supply_transfers t
         SET to_lot_id = $2, received_quantity = $3, received_by = $4, received_at = now(),
             fields = l.fields
         FROM supply_links l
         WHERE t.id = $1 AND l.id = t.link_id",
        id,
        lot,
        quantity,
        by,
    )
    .execute(conn)
    .await?;
    Ok(())
}
