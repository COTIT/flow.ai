//! Supply batch SQL. Batches and shares are insert-only (a trigger refuses the rest).

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::{http::farm::Scope, modules::farm_supply::dto::QuietFarmer};

/// A stored batch with its lot's codes.
pub struct BatchRow {
    pub id: Uuid,
    pub client_ref: Uuid,
    pub village_id: Uuid,
    pub crop_id: Uuid,
    pub lot_id: Uuid,
    pub lot_code: String,
    pub public_code: String,
    pub price_per_kg: i64,
    pub currency_id: Uuid,
    pub kg_total: Decimal,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

/// One share line.
pub struct LineRow {
    pub id: Uuid,
    pub batch_id: Uuid,
    pub harvest_report_id: Uuid,
    pub person_id: Uuid,
    pub farmer_name: String,
    pub kg: Decimal,
}

/// Who used a client_ref, and with which body.
pub struct RefRow {
    pub id: Uuid,
    pub created_by: Uuid,
    pub request_hash: Vec<u8>,
}

/// A harvest report as a batch line needs it, locked.
pub struct HarvestRow {
    pub id: Uuid,
    pub village_id: Uuid,
    pub kind: String,
    pub is_current: bool,
    pub verification: String,
    pub verified_by: Option<Uuid>,
    pub deleted: bool,
    pub crop_id: Uuid,
    /// Primary manager of the harvest's farm, if the farm has one.
    pub person_id: Option<Uuid>,
    pub quantity_kg: Decimal,
}

/// A new batch row.
pub struct NewBatchRow<'a> {
    pub client_ref: Uuid,
    pub request_hash: &'a [u8],
    pub village_id: Uuid,
    pub crop_id: Uuid,
    pub lot_id: Uuid,
    pub price_per_kg: i64,
    pub currency_id: Uuid,
    pub kg_total: Decimal,
    pub created_by: Uuid,
}

/// True when `crop_id` is a crop of the workspace.
pub async fn crop_exists(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    crop_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM farm_crops WHERE workspace_id = $1 AND id = $2) AS "ok!""#,
        workspace_id,
        crop_id,
    )
    .fetch_one(db)
    .await
}

/// The batch that used `client_ref` in this workspace.
pub async fn find_ref(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    client_ref: Uuid,
) -> Result<Option<RefRow>, sqlx::Error> {
    sqlx::query_as!(
        RefRow,
        "SELECT id, created_by, request_hash FROM farm_supply_batches
         WHERE workspace_id = $1 AND client_ref = $2",
        workspace_id,
        client_ref,
    )
    .fetch_optional(db)
    .await
}

/// Lock the workspace's harvest reports among `ids` (id order, so two batches never deadlock)
/// and read what a line checks. Unknown and foreign ids are simply missing.
pub async fn lock_harvests(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<HarvestRow>, sqlx::Error> {
    sqlx::query_as!(
        HarvestRow,
        r#"SELECT h.id, h.village_id, h.kind, h.is_current, h.verification, h.verified_by,
                  h.deleted_at IS NOT NULL AS "deleted!", c.crop_id,
                  m.person_id AS "person_id?", h.quantity_kg
           FROM farm_harvest_reports h
           JOIN farm_crop_cycles c ON c.workspace_id = h.workspace_id AND c.id = h.crop_cycle_id
           JOIN farm_plots p ON p.workspace_id = c.workspace_id AND p.id = c.plot_id
           LEFT JOIN farm_managers m ON m.farm_id = p.farm_id AND m.is_primary
           WHERE h.workspace_id = $1 AND h.id = ANY($2)
           ORDER BY h.id
           FOR UPDATE OF h"#,
        workspace_id,
        ids,
    )
    .fetch_all(conn)
    .await
}

/// Kg already committed per harvest. Its own statement, after the lock: it must see what
/// transactions that held the lock before us committed.
pub async fn committed(
    conn: &mut PgConnection,
    ids: &[Uuid],
) -> Result<Vec<(Uuid, Decimal)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id AS "id!", farm_harvest_committed_kg(id) AS "kg!"
           FROM unnest($1::uuid[]) AS id"#,
        ids,
    )
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.kg)).collect())
}

/// Insert the batch row; its id.
pub async fn insert_batch(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    b: &NewBatchRow<'_>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO farm_supply_batches (workspace_id, client_ref, request_hash, village_id,
                                          crop_id, lot_id, price_per_kg, currency_id, kg_total,
                                          created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
        workspace_id,
        b.client_ref,
        b.request_hash,
        b.village_id,
        b.crop_id,
        b.lot_id,
        b.price_per_kg,
        b.currency_id,
        b.kg_total,
        b.created_by,
    )
    .fetch_one(conn)
    .await
}

/// Insert the share lines `(harvest, person, kg)` of a batch, one statement.
pub async fn insert_lines(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    village_id: Uuid,
    batch_id: Uuid,
    lines: &[(Uuid, Uuid, Decimal)],
) -> Result<(), sqlx::Error> {
    let harvests: Vec<Uuid> = lines.iter().map(|l| l.0).collect();
    let persons: Vec<Uuid> = lines.iter().map(|l| l.1).collect();
    let kgs: Vec<Decimal> = lines.iter().map(|l| l.2).collect();
    sqlx::query!(
        "INSERT INTO farm_supply_shares (workspace_id, village_id, batch_id, harvest_report_id,
                                         person_id, kg)
         SELECT $1, $2, $3, h, p, kg FROM unnest($4::uuid[], $5::uuid[], $6::numeric[]) AS t(h, p, kg)",
        workspace_id,
        village_id,
        batch_id,
        &harvests,
        &persons,
        &kgs,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// One batch of the workspace inside `scope`.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    scope: &Scope,
) -> Result<Option<BatchRow>, sqlx::Error> {
    sqlx::query_as!(
        BatchRow,
        "SELECT b.id, b.client_ref, b.village_id, b.crop_id, b.lot_id, l.code AS lot_code,
                l.public_code, b.price_per_kg, b.currency_id, b.kg_total, b.created_by,
                b.created_at
         FROM farm_supply_batches b
         JOIN lots l ON l.workspace_id = b.workspace_id AND l.id = b.lot_id
         WHERE b.workspace_id = $1 AND b.id = $2 AND ($3::bool OR b.village_id = ANY($4))",
        workspace_id,
        id,
        scope.all,
        &scope.villages,
    )
    .fetch_optional(db)
    .await
}

/// One page of batches inside `scope`, newest first, and the count.
pub async fn list(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    scope: &Scope,
    limit: i64,
    offset: i64,
) -> Result<(Vec<BatchRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        BatchRow,
        "SELECT b.id, b.client_ref, b.village_id, b.crop_id, b.lot_id, l.code AS lot_code,
                l.public_code, b.price_per_kg, b.currency_id, b.kg_total, b.created_by,
                b.created_at
         FROM farm_supply_batches b
         JOIN lots l ON l.workspace_id = b.workspace_id AND l.id = b.lot_id
         WHERE b.workspace_id = $1 AND ($2::bool OR b.village_id = ANY($3))
         ORDER BY b.created_at DESC, b.id DESC LIMIT $4 OFFSET $5",
        workspace_id,
        scope.all,
        &scope.villages,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_supply_batches
           WHERE workspace_id = $1 AND ($2::bool OR village_id = ANY($3))"#,
        workspace_id,
        scope.all,
        &scope.villages,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// Lines of the given batches, by batch then harvest id.
pub async fn lines(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    batch_ids: &[Uuid],
) -> Result<Vec<LineRow>, sqlx::Error> {
    sqlx::query_as!(
        LineRow,
        r#"SELECT s.id, s.batch_id, s.harvest_report_id, s.person_id,
                  concat_ws(' ', p.given_name, p.family_name) AS "farmer_name!", s.kg
           FROM farm_supply_shares s
           JOIN farm_persons p ON p.workspace_id = s.workspace_id AND p.id = s.person_id
           WHERE s.workspace_id = $1 AND s.batch_id = ANY($2)
           ORDER BY s.batch_id, s.harvest_report_id"#,
        workspace_id,
        batch_ids,
    )
    .fetch_all(db)
    .await
}

/// Farmers of a supply lot per village (for the public page): count and harvest window.
pub struct GroupRow {
    pub village: String,
    pub farmers: i64,
    pub harvest_start: Option<chrono::NaiveDate>,
    pub harvest_end: Option<chrono::NaiveDate>,
}

/// The farmer groups of `lot_id`'s batch, by village name; empty for a lot without a batch.
pub async fn public_groups(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    lot_id: Uuid,
) -> Result<Vec<GroupRow>, sqlx::Error> {
    sqlx::query_as!(
        GroupRow,
        r#"SELECT v.name AS village, count(DISTINCT s.person_id) AS "farmers!",
                  min(c.harvest_start) AS harvest_start, max(c.harvest_end) AS harvest_end
           FROM farm_supply_batches b
           -- Effective kg: a farmer whose kg were all moved away no longer counts.
           JOIN (SELECT batch_id, village_id, person_id, harvest_report_id
                 FROM farm_supply_share_kg WHERE workspace_id = $1
                 GROUP BY 1, 2, 3, 4 HAVING sum(kg) > 0) s ON s.batch_id = b.id
           JOIN farm_villages v ON v.workspace_id = b.workspace_id AND v.id = s.village_id
           JOIN farm_harvest_reports h ON h.id = s.harvest_report_id
           JOIN farm_crop_cycles c ON c.id = h.crop_cycle_id
           WHERE b.workspace_id = $1 AND b.lot_id = $2
           GROUP BY v.id, v.name
           ORDER BY v.name"#,
        workspace_id,
        lot_id,
    )
    .fetch_all(db)
    .await
}

/// One step back from a lot: its packing parent, or the transfer it was received from.
pub struct HopRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub parent_lot_id: Option<Uuid>,
    pub pack_raw_kg: Option<Decimal>,
    pub pack_units: Option<Decimal>,
    pub from_lot_id: Option<Uuid>,
    /// The allow-list frozen when the transfer was received.
    pub fields: Option<Vec<String>>,
}

/// The lot `lot_id` and where it came from. Crosses workspaces only through a transfer row.
pub async fn hop(db: impl PgExecutor<'_>, lot_id: Uuid) -> Result<Option<HopRow>, sqlx::Error> {
    sqlx::query_as!(
        HopRow,
        r#"SELECT l.id, l.workspace_id, l.parent_lot_id, l.pack_raw_kg, l.pack_units,
                  t.from_lot_id AS "from_lot_id?", t.fields AS "fields?"
           FROM lots l
           LEFT JOIN supply_transfers t ON t.to_workspace_id = l.workspace_id AND t.to_lot_id = l.id
           WHERE l.id = $1"#,
        lot_id,
    )
    .fetch_optional(db)
    .await
}

/// The batch whose raw lot is `lot_id`, with its price.
pub async fn batch_of_lot(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    lot_id: Uuid,
) -> Result<Option<(Uuid, i64, Uuid)>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT id, price_per_kg, currency_id FROM farm_supply_batches
         WHERE workspace_id = $1 AND lot_id = $2",
        workspace_id,
        lot_id,
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| (r.id, r.price_per_kg, r.currency_id)))
}

/// What an order took off the shelf: per lot (None = no-lot stock) and product, by product
/// then lot (no-lot first).
pub struct TakeRow {
    pub lot_id: Option<Uuid>,
    pub product_id: Uuid,
    pub qty: Decimal,
}

/// The order's stock decreases (commit or take) in `workspace_id`, summed per lot.
pub async fn order_takes(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    order_id: Uuid,
) -> Result<Vec<TakeRow>, sqlx::Error> {
    sqlx::query_as!(
        TakeRow,
        r#"SELECT m.lot_id AS "lot_id?", m.product_id,
                  (-sum(m.quantity_change))::numeric(19,2) AS "qty!"
           FROM stock_movements m
           WHERE m.workspace_id = $1 AND m.related_domain = 'order' AND m.related_id = $2
             AND m.quantity_change < 0
           GROUP BY m.lot_id, m.product_id
           ORDER BY m.product_id, m.lot_id NULLS FIRST"#,
        workspace_id,
        order_id,
    )
    .fetch_all(conn)
    .await
}

/// A farmer's kg in a batch.
pub struct WeightRow {
    pub person_id: Uuid,
    pub village_id: Uuid,
    pub kg: Decimal,
}

/// Hold `batch_id` FOR SHARE until the transaction ends: a correction approval (FOR UPDATE)
/// can't re-split the batch between this sale's weight read and its commit.
pub async fn share_batch(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    batch_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "SELECT id FROM farm_supply_batches WHERE workspace_id = $1 AND id = $2 FOR SHARE",
        workspace_id,
        batch_id,
    )
    .fetch_optional(conn)
    .await?;
    Ok(())
}

/// Each farmer's effective kg in `batch_id` (shares plus approved corrections), by person id.
pub async fn weights(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    batch_id: Uuid,
) -> Result<Vec<WeightRow>, sqlx::Error> {
    sqlx::query_as!(
        WeightRow,
        r#"SELECT person_id AS "person_id!", village_id AS "village_id!",
                  sum(kg)::numeric(12,2) AS "kg!" FROM farm_supply_share_kg
           WHERE workspace_id = $1 AND batch_id = $2
           GROUP BY person_id, village_id HAVING sum(kg) > 0 ORDER BY person_id"#,
        workspace_id,
        batch_id,
    )
    .fetch_all(conn)
    .await
}

/// A new attribution row.
pub struct NewAttribution<'a> {
    pub workspace_id: Uuid,
    pub batch_id: Uuid,
    pub kind: &'static str,
    pub sold_workspace_id: Uuid,
    pub order_id: Uuid,
    pub order_number: &'a str,
    pub sold_lot_id: Uuid,
    pub jars: Decimal,
    pub pack_raw_kg: Decimal,
    pub pack_units: Decimal,
    pub price_per_kg: i64,
    pub currency_id: Uuid,
    pub total: i64,
}

/// Insert an attribution; its id.
pub async fn insert_attribution(
    conn: &mut PgConnection,
    a: &NewAttribution<'_>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO farm_sale_attributions (workspace_id, batch_id, kind, sold_workspace_id,
                                             order_id, order_number, sold_lot_id, jars,
                                             pack_raw_kg, pack_units, price_per_kg, currency_id,
                                             total)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) RETURNING id",
        a.workspace_id,
        a.batch_id,
        a.kind,
        a.sold_workspace_id,
        a.order_id,
        a.order_number,
        a.sold_lot_id,
        a.jars,
        a.pack_raw_kg,
        a.pack_units,
        a.price_per_kg,
        a.currency_id,
        a.total,
    )
    .fetch_one(conn)
    .await
}

/// A passbook line for an attribution (`kind` sale_credit or sale_reversal, signed amount).
#[allow(clippy::too_many_arguments)]
pub async fn attribution_line(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    village_id: Uuid,
    person_id: Uuid,
    currency_id: Uuid,
    kind: &str,
    amount: i64,
    attribution_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO farm_passbook_entries (workspace_id, village_id, person_id, currency_id,
                                            kind, amount, attribution_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
        workspace_id,
        village_id,
        person_id,
        currency_id,
        kind,
        amount,
        attribution_id,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The sale attribution of an order's lot.
pub struct SaleRow {
    pub workspace_id: Uuid,
    pub batch_id: Uuid,
    pub pack_raw_kg: Decimal,
    pub pack_units: Decimal,
    pub price_per_kg: i64,
    pub currency_id: Uuid,
}

/// The `sale` attribution of `lot_id` on the order, if its farmers were credited.
pub async fn sale_of(
    conn: &mut PgConnection,
    sold_workspace_id: Uuid,
    order_id: Uuid,
    lot_id: Uuid,
) -> Result<Option<SaleRow>, sqlx::Error> {
    sqlx::query_as!(
        SaleRow,
        "SELECT workspace_id, batch_id, pack_raw_kg, pack_units, price_per_kg, currency_id
         FROM farm_sale_attributions
         WHERE sold_workspace_id = $1 AND order_id = $2 AND sold_lot_id = $3 AND kind = 'sale'",
        sold_workspace_id,
        order_id,
        lot_id,
    )
    .fetch_optional(conn)
    .await
}

/// Units already returned on the order's lot, and each farmer's reversal lines so far.
pub async fn returned_so_far(
    conn: &mut PgConnection,
    sold_workspace_id: Uuid,
    order_id: Uuid,
    lot_id: Uuid,
) -> Result<(Decimal, Vec<(Uuid, i64)>), sqlx::Error> {
    let jars = sqlx::query_scalar!(
        r#"SELECT coalesce(sum(jars), 0)::numeric(19,2) AS "jars!" FROM farm_sale_attributions
           WHERE sold_workspace_id = $1 AND order_id = $2 AND sold_lot_id = $3
             AND kind = 'return'"#,
        sold_workspace_id,
        order_id,
        lot_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    // Corrections re-split returns too (their splits rows): count them, or the next return drifts.
    let lines = sqlx::query!(
        r#"SELECT x.person_id AS "person_id!", sum(x.amount)::bigint AS "amount!"
           FROM farm_sale_attributions a
           CROSS JOIN LATERAL (
               SELECT e.person_id, e.amount FROM farm_passbook_entries e WHERE e.attribution_id = a.id
               UNION ALL
               SELECT c.person_id, c.amount FROM farm_supply_correction_splits c
               WHERE c.attribution_id = a.id
           ) x
           WHERE a.sold_workspace_id = $1 AND a.order_id = $2 AND a.sold_lot_id = $3
             AND a.kind = 'return'
           GROUP BY x.person_id"#,
        sold_workspace_id,
        order_id,
        lot_id,
    )
    .fetch_all(conn)
    .await?;
    Ok((
        jars,
        lines.into_iter().map(|l| (l.person_id, l.amount)).collect(),
    ))
}

/// A harvest a new batch may buy.
pub struct BuyableRow {
    pub id: Uuid,
    pub farmer_name: String,
    pub plot_label: String,
    pub quantity_kg: Decimal,
    pub committed_kg: Decimal,
}

/// Current, verified, actual harvests of `village` and `crop` with a primary manager and kg
/// left, none verified by `buyer`: the lines batch create accepts. At most `LINES_MAX` rows.
pub async fn buyable(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    village_id: Uuid,
    crop_id: Uuid,
    buyer: Uuid,
) -> Result<Vec<BuyableRow>, sqlx::Error> {
    sqlx::query_as!(
        BuyableRow,
        r#"SELECT id AS "id!", farmer_name AS "farmer_name!", plot_label AS "plot_label!",
                  quantity_kg AS "quantity_kg!", committed_kg AS "committed_kg!"
           FROM (SELECT h.id, concat_ws(' ', mp.given_name, mp.family_name) AS farmer_name,
                        pl.label AS plot_label, h.quantity_kg,
                        farm_harvest_committed_kg(h.id) AS committed_kg
                 FROM farm_harvest_reports h
                 JOIN farm_crop_cycles c ON c.workspace_id = h.workspace_id AND c.id = h.crop_cycle_id
                 JOIN farm_plots pl ON pl.workspace_id = c.workspace_id AND pl.id = c.plot_id
                 JOIN farm_managers m ON m.farm_id = pl.farm_id AND m.is_primary
                 JOIN farm_persons mp ON mp.id = m.person_id
                 WHERE h.workspace_id = $1 AND h.village_id = $2 AND c.crop_id = $3
                   AND h.kind = 'actual' AND h.is_current AND h.deleted_at IS NULL
                   AND h.verification = 'verified' AND h.verified_by IS DISTINCT FROM $4) b
           WHERE committed_kg < quantity_kg
           ORDER BY farmer_name, id
           LIMIT 500"#,
        workspace_id,
        village_id,
        crop_id,
        buyer,
    )
    .fetch_all(db)
    .await
}

/// A stock-tracked product, flagged when it suits the crop.
pub struct RawProductRow {
    pub id: Uuid,
    pub name: String,
    pub unit: Option<String>,
    pub suggested: bool,
}

/// The workspace's live stock-tracked products (what `tracks_stock` accepts), suggestions for
/// `crop_id` first: used by an earlier batch of the crop, or named after it (en or sw, any case).
pub async fn raw_products(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    crop_id: Uuid,
) -> Result<Vec<RawProductRow>, sqlx::Error> {
    // shortcut: capped at 500 rows, no search; add `q` when a workspace stocks more.
    sqlx::query_as!(
        RawProductRow,
        r#"SELECT p.id, p.name, p.unit,
                  (EXISTS (SELECT 1 FROM farm_supply_batches b
                           JOIN lots l ON l.workspace_id = b.workspace_id AND l.id = b.lot_id
                           WHERE b.workspace_id = $1 AND b.crop_id = $2 AND l.product_id = p.id)
                   OR strpos(lower(p.name), lower(c.name_en)) > 0
                   OR strpos(lower(p.name), lower(c.name_sw)) > 0) AS "suggested!"
           FROM products p
           LEFT JOIN farm_crops c ON c.workspace_id = p.workspace_id AND c.id = $2
           WHERE p.workspace_id = $1 AND p.deleted_at IS NULL AND p.has_stock
             AND p.type IN ('physical', 'physical_second_chance')
           ORDER BY 4 DESC, lower(p.name), p.id
           LIMIT 500"#,
        workspace_id,
        crop_id,
    )
    .fetch_all(db)
    .await
}

/// A live link this workspace granted, with the receiving workspace's name.
pub struct DispatchLink {
    pub id: Uuid,
    pub partner_id: Uuid,
    pub partner_name: String,
    pub partner_display_name: String,
}

/// Live supply links `workspace_id` granted, by partner name.
pub async fn dispatch_links(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<Vec<DispatchLink>, sqlx::Error> {
    sqlx::query_as!(
        DispatchLink,
        r#"SELECT l.id, w.id AS partner_id, w.name AS partner_name,
                coalesce(nullif(btrim(w.profile->>'company_name'), ''), w.name) AS "partner_display_name!"
         FROM supply_links l JOIN workspaces w ON w.id = l.to_workspace_id
         WHERE l.from_workspace_id = $1 AND l.accepted_at IS NOT NULL AND l.revoked_at IS NULL
         ORDER BY w.name, l.id"#,
        workspace_id,
    )
    .fetch_all(db)
    .await
}

/// An id and its display name (lot code or warehouse name).
pub struct Named {
    pub id: Uuid,
    pub name: String,
}

/// The raw lots of the workspace's supply batches, newest first.
pub async fn batch_lots(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<Vec<Named>, sqlx::Error> {
    sqlx::query_as!(
        Named,
        "SELECT l.id, l.code AS name FROM farm_supply_batches b
         JOIN lots l ON l.workspace_id = b.workspace_id AND l.id = b.lot_id
         WHERE b.workspace_id = $1
         ORDER BY b.created_at DESC, b.id DESC",
        workspace_id,
    )
    .fetch_all(db)
    .await
}

/// Live warehouses holding free stock (level − reserved > 0) of a supply batch lot, by name.
pub async fn dispatch_warehouses(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<Vec<Named>, sqlx::Error> {
    sqlx::query_as!(
        Named,
        "SELECT w.id, w.name FROM warehouses w
         WHERE w.workspace_id = $1 AND w.deleted_at IS NULL
           AND EXISTS (SELECT 1 FROM stocks s
                       JOIN farm_supply_batches b ON b.workspace_id = s.workspace_id
                                                 AND b.lot_id = s.lot_id
                       WHERE s.workspace_id = $1 AND s.warehouse_id = w.id
                         AND s.deleted_at IS NULL AND s.stock_lvl - s.reserved > 0)
         ORDER BY w.name, w.id",
        workspace_id,
    )
    .fetch_all(db)
    .await
}

/// Live persons in `scope` (and `village`) whose last bought kg is before Tanzania day `since`
/// (default 30 days ago), or who never delivered: never first, then oldest delivery. One page
/// and the count.
pub async fn quiet_farmers(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    scope: &Scope,
    village: Option<Uuid>,
    since: Option<NaiveDate>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<QuietFarmer>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT p.id, p.given_name || ' ' || p.family_name AS "name!", p.village_id,
                  v.name AS village, s.last AS "last?: DateTime<Utc>", count(*) OVER () AS "total!"
           FROM farm_persons p
           JOIN farm_villages v ON v.workspace_id = p.workspace_id AND v.id = p.village_id
           LEFT JOIN LATERAL (
               SELECT max(created_at) AS last FROM farm_supply_shares
               WHERE person_id = p.id AND workspace_id = p.workspace_id
           ) s ON true
           WHERE p.workspace_id = $1 AND p.deleted_at IS NULL
             AND ($2 OR p.village_id = ANY($3)) AND ($4::uuid IS NULL OR p.village_id = $4)
             AND (s.last IS NULL OR (s.last AT TIME ZONE 'Africa/Dar_es_Salaam')::date
                  < coalesce($5::date, (now() AT TIME ZONE 'Africa/Dar_es_Salaam')::date - 30))
           ORDER BY s.last ASC NULLS FIRST, 2, p.id
           LIMIT $6 OFFSET $7"#,
        workspace_id,
        scope.all,
        &scope.villages,
        village,
        since,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| QuietFarmer {
            person_id: r.id,
            name: r.name,
            village_id: r.village_id,
            village: r.village,
            last_delivery_at: r.last,
        })
        .collect();
    Ok((rows, total))
}
