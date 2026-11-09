//! Supply correction SQL. Every table here is insert-only (a trigger refuses the rest).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::http::farm::Scope;

/// A batch locked for a correction, with its raw lot.
pub struct BatchLock {
    pub id: Uuid,
    pub village_id: Uuid,
    pub lot_id: Uuid,
    pub lot_code: String,
    pub product_id: Uuid,
    pub currency_id: Uuid,
}

/// Lock the batch row of the workspace inside `scope` (serialises corrections of one batch).
pub async fn lock_batch(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    batch_id: Uuid,
    scope: &Scope,
) -> Result<Option<BatchLock>, sqlx::Error> {
    sqlx::query_as!(
        BatchLock,
        r#"SELECT b.id, b.village_id, b.lot_id, l.code AS lot_code, l.product_id, b.currency_id
           FROM farm_supply_batches b
           JOIN lots l ON l.workspace_id = b.workspace_id AND l.id = b.lot_id
           WHERE b.workspace_id = $1 AND b.id = $2 AND ($3::bool OR b.village_id = ANY($4))
           FOR UPDATE OF b"#,
        workspace_id,
        batch_id,
        scope.all,
        &scope.villages,
    )
    .fetch_optional(conn)
    .await
}

/// The batch of the workspace inside `scope`, unlocked (approval locks stock before the batch).
pub async fn find_batch(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    batch_id: Uuid,
    scope: &Scope,
) -> Result<Option<BatchLock>, sqlx::Error> {
    sqlx::query_as!(
        BatchLock,
        r#"SELECT b.id, b.village_id, b.lot_id, l.code AS lot_code, l.product_id, b.currency_id
           FROM farm_supply_batches b
           JOIN lots l ON l.workspace_id = b.workspace_id AND l.id = b.lot_id
           WHERE b.workspace_id = $1 AND b.id = $2 AND ($3::bool OR b.village_id = ANY($4))"#,
        workspace_id,
        batch_id,
        scope.all,
        &scope.villages,
    )
    .fetch_optional(conn)
    .await
}

/// The batch's currency `(id, code, decimals)`.
pub async fn currency(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    batch_id: Uuid,
) -> Result<Option<(Uuid, String, i16)>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT c.id, c.asset_type, c.decimals FROM farm_supply_batches b
         JOIN currencies c ON c.workspace_id = b.workspace_id AND c.id = b.currency_id
         WHERE b.workspace_id = $1 AND b.id = $2",
        workspace_id,
        batch_id,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| (r.id, r.asset_type, r.decimals)))
}

/// A share line of a batch: its farmer when bought and its harvest.
pub struct ShareRow {
    pub id: Uuid,
    pub person_id: Uuid,
    pub harvest_report_id: Uuid,
}

/// The batch's share lines.
pub async fn shares(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    batch_id: Uuid,
) -> Result<Vec<ShareRow>, sqlx::Error> {
    sqlx::query_as!(
        ShareRow,
        "SELECT id, person_id, harvest_report_id FROM farm_supply_shares
         WHERE workspace_id = $1 AND batch_id = $2",
        workspace_id,
        batch_id,
    )
    .fetch_all(conn)
    .await
}

/// Effective kg per (share, person) of the batch, approved corrections included.
pub async fn share_kg(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    batch_id: Uuid,
) -> Result<Vec<(Uuid, Uuid, Decimal)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT share_id AS "share_id!", person_id AS "person_id!",
                  sum(kg)::numeric(12,2) AS "kg!"
           FROM farm_supply_share_kg WHERE workspace_id = $1 AND batch_id = $2
           GROUP BY share_id, person_id"#,
        workspace_id,
        batch_id,
    )
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.share_id, r.person_id, r.kg))
        .collect())
}

/// True when a (share, person) of the batch is below 0 kg.
pub async fn below_zero(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    batch_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM farm_supply_share_kg
                          WHERE workspace_id = $1 AND batch_id = $2
                          GROUP BY share_id, person_id HAVING sum(kg) < 0) AS "x!""#,
        workspace_id,
        batch_id,
    )
    .fetch_one(conn)
    .await
}

/// True when a live person of the village.
pub async fn person_in_village(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    village_id: Uuid,
    person_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM farm_persons WHERE workspace_id = $1 AND village_id = $2
                          AND id = $3 AND deleted_at IS NULL) AS "x!""#,
        workspace_id,
        village_id,
        person_id,
    )
    .fetch_one(conn)
    .await
}

/// The raw lot's stock: total over every row, and the warehouse holding most (with its free kg).
pub struct LotStock {
    pub total: Decimal,
    pub warehouse: Option<(Uuid, Decimal)>,
}

/// Stock of `lot_id` in the workspace.
pub async fn lot_stock(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    lot_id: Uuid,
) -> Result<LotStock, sqlx::Error> {
    let total = sqlx::query_scalar!(
        r#"SELECT coalesce(sum(stock_lvl), 0)::numeric(19,2) AS "n!" FROM stocks
           WHERE workspace_id = $1 AND lot_id = $2 AND deleted_at IS NULL"#,
        workspace_id,
        lot_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    let row = sqlx::query!(
        "SELECT warehouse_id, stock_lvl - reserved AS free FROM stocks
         WHERE workspace_id = $1 AND lot_id = $2 AND bin_id IS NULL AND deleted_at IS NULL
         ORDER BY stock_lvl DESC, warehouse_id LIMIT 1",
        workspace_id,
        lot_id,
    )
    .fetch_optional(conn)
    .await?;
    Ok(LotStock {
        total,
        warehouse: row.map(|r| (r.warehouse_id, r.free.unwrap_or_default())),
    })
}

/// True when the raw lot, or a lot packed from it, left over a supply link.
pub async fn dispatched(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    lot_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
               SELECT 1 FROM supply_transfers t
               JOIN lots l ON l.workspace_id = t.from_workspace_id AND l.id = t.from_lot_id
               WHERE t.from_workspace_id = $1 AND (l.id = $2 OR l.parent_lot_id = $2)) AS "x!""#,
        workspace_id,
        lot_id,
    )
    .fetch_one(conn)
    .await
}

/// Who used a client_ref, and with which body.
pub struct RefRow {
    pub id: Uuid,
    pub requested_by: Uuid,
    pub request_hash: Vec<u8>,
}

/// The correction filed under `client_ref`, if any.
pub async fn find_ref(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    client_ref: Uuid,
) -> Result<Option<RefRow>, sqlx::Error> {
    sqlx::query_as!(
        RefRow,
        "SELECT id, requested_by, request_hash FROM farm_supply_corrections
         WHERE workspace_id = $1 AND client_ref = $2",
        workspace_id,
        client_ref,
    )
    .fetch_optional(db)
    .await
}

/// A new correction header.
pub struct NewCorrection<'a> {
    pub village_id: Uuid,
    pub batch_id: Uuid,
    pub reason: &'a str,
    pub client_ref: Uuid,
    pub request_hash: &'a [u8],
    pub requested_by: Uuid,
}

/// Insert the header and its `(share, person, kg_delta)` lines; the correction id.
pub async fn insert(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    c: &NewCorrection<'_>,
    lines: &[(Uuid, Uuid, Decimal)],
) -> Result<Uuid, sqlx::Error> {
    let id = sqlx::query_scalar!(
        "INSERT INTO farm_supply_corrections (workspace_id, village_id, batch_id, reason,
                                              client_ref, request_hash, requested_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
        workspace_id,
        c.village_id,
        c.batch_id,
        c.reason,
        c.client_ref,
        c.request_hash,
        c.requested_by,
    )
    .fetch_one(&mut *conn)
    .await?;
    let shares: Vec<Uuid> = lines.iter().map(|l| l.0).collect();
    let persons: Vec<Uuid> = lines.iter().map(|l| l.1).collect();
    let kgs: Vec<Decimal> = lines.iter().map(|l| l.2).collect();
    sqlx::query!(
        "INSERT INTO farm_supply_correction_lines (workspace_id, village_id, correction_id,
                                                   share_id, person_id, kg_delta)
         SELECT $1, $2, $3, s, p, kg FROM unnest($4::uuid[], $5::uuid[], $6::numeric[]) AS t(s, p, kg)",
        workspace_id,
        c.village_id,
        id,
        &shares,
        &persons,
        &kgs,
    )
    .execute(conn)
    .await?;
    Ok(id)
}

/// A stored correction with its decision.
pub struct CorrectionRow {
    pub id: Uuid,
    pub village_id: Uuid,
    pub batch_id: Uuid,
    pub reason: String,
    pub requested_by: Uuid,
    pub requested_at: DateTime<Utc>,
    pub decision: Option<String>,
    pub decided_by: Option<Uuid>,
    pub decided_at: Option<DateTime<Utc>>,
}

/// One correction of the workspace inside `scope`; `lock` takes its row for a decision.
pub async fn find(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    scope: &Scope,
    lock: bool,
) -> Result<Option<CorrectionRow>, sqlx::Error> {
    // Two statements: FOR UPDATE can't sit on the nullable side of the decision join.
    if lock {
        sqlx::query!(
            "SELECT id FROM farm_supply_corrections WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
            workspace_id,
            id,
        )
        .fetch_optional(&mut *conn)
        .await?;
    }
    sqlx::query_as!(
        CorrectionRow,
        r#"SELECT c.id, c.village_id, c.batch_id, c.reason, c.requested_by, c.requested_at,
                  d.decision AS "decision?", d.decided_by AS "decided_by?",
                  d.decided_at AS "decided_at?"
           FROM farm_supply_corrections c
           LEFT JOIN farm_supply_correction_decisions d ON d.correction_id = c.id
           WHERE c.workspace_id = $1 AND c.id = $2 AND ($3::bool OR c.village_id = ANY($4))"#,
        workspace_id,
        id,
        scope.all,
        &scope.villages,
    )
    .fetch_optional(conn)
    .await
}

/// One page of corrections inside `scope`, newest first, and the count. `status`: pending,
/// approved or rejected (None: all).
#[allow(clippy::too_many_arguments)]
pub async fn list(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    scope: &Scope,
    status: Option<&str>,
    batch_id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<CorrectionRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        CorrectionRow,
        r#"SELECT c.id, c.village_id, c.batch_id, c.reason, c.requested_by, c.requested_at,
                  d.decision AS "decision?", d.decided_by AS "decided_by?",
                  d.decided_at AS "decided_at?"
           FROM farm_supply_corrections c
           LEFT JOIN farm_supply_correction_decisions d ON d.correction_id = c.id
           WHERE c.workspace_id = $1 AND ($2::bool OR c.village_id = ANY($3))
             AND ($4::text IS NULL OR coalesce(d.decision, 'pending') = $4)
             AND ($5::uuid IS NULL OR c.batch_id = $5)
           ORDER BY c.requested_at DESC, c.id DESC LIMIT $6 OFFSET $7"#,
        workspace_id,
        scope.all,
        &scope.villages,
        status,
        batch_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_supply_corrections c
           LEFT JOIN farm_supply_correction_decisions d ON d.correction_id = c.id
           WHERE c.workspace_id = $1 AND ($2::bool OR c.village_id = ANY($3))
             AND ($4::text IS NULL OR coalesce(d.decision, 'pending') = $4)
             AND ($5::uuid IS NULL OR c.batch_id = $5)"#,
        workspace_id,
        scope.all,
        &scope.villages,
        status,
        batch_id,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// A correction line with the farmer's name.
pub struct LineRow {
    pub correction_id: Uuid,
    pub share_id: Uuid,
    pub person_id: Uuid,
    pub name: String,
    pub kg_delta: Decimal,
}

/// Lines of the given corrections.
pub async fn lines(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<LineRow>, sqlx::Error> {
    sqlx::query_as!(
        LineRow,
        r#"SELECT l.correction_id, l.share_id, l.person_id,
                  concat_ws(' ', p.given_name, p.family_name) AS "name!", l.kg_delta
           FROM farm_supply_correction_lines l
           JOIN farm_persons p ON p.workspace_id = l.workspace_id AND p.id = l.person_id
           WHERE l.workspace_id = $1 AND l.correction_id = ANY($2)
           ORDER BY l.correction_id, l.id"#,
        workspace_id,
        ids,
    )
    .fetch_all(conn)
    .await
}

/// Record the decision (the schema refuses the requester).
pub async fn decide(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    c: &CorrectionRow,
    decision: &str,
    decided_by: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO farm_supply_correction_decisions (correction_id, workspace_id, decision,
                                                       requested_by, decided_by)
         VALUES ($1, $2, $3, $4, $5)",
        c.id,
        workspace_id,
        decision,
        c.requested_by,
        decided_by,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Most attributions one approval re-splits; past it, 422.
pub const ATTRIBUTIONS_MAX: i64 = 5_000;

/// The batch's sale and return attributions `(id, total)`, at most `ATTRIBUTIONS_MAX + 1`.
pub async fn attributions(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    batch_id: Uuid,
) -> Result<Vec<(Uuid, i64)>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT id, total FROM farm_sale_attributions
         WHERE workspace_id = $1 AND batch_id = $2 ORDER BY id LIMIT $3",
        workspace_id,
        batch_id,
        ATTRIBUTIONS_MAX + 1,
    )
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.total)).collect())
}

/// What each farmer holds per attribution: its passbook lines plus earlier correction splits.
pub async fn held(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    attributions: &[Uuid],
) -> Result<Vec<(Uuid, Uuid, i64)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT attribution_id AS "attribution_id!", person_id AS "person_id!",
                  sum(amount)::bigint AS "amount!"
           FROM (SELECT attribution_id, person_id, amount FROM farm_passbook_entries
                 WHERE workspace_id = $1 AND attribution_id = ANY($2)
                 UNION ALL
                 SELECT attribution_id, person_id, amount FROM farm_supply_correction_splits
                 WHERE workspace_id = $1 AND attribution_id = ANY($2)) x
           GROUP BY attribution_id, person_id"#,
        workspace_id,
        attributions,
    )
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.attribution_id, r.person_id, r.amount))
        .collect())
}

/// Insert the re-split rows `(attribution, person, amount)`, one statement.
pub async fn insert_splits(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    correction_id: Uuid,
    rows: &[(Uuid, Uuid, i64)],
) -> Result<(), sqlx::Error> {
    let a: Vec<Uuid> = rows.iter().map(|r| r.0).collect();
    let p: Vec<Uuid> = rows.iter().map(|r| r.1).collect();
    let m: Vec<i64> = rows.iter().map(|r| r.2).collect();
    sqlx::query!(
        "INSERT INTO farm_supply_correction_splits (workspace_id, correction_id, attribution_id,
                                                    person_id, amount)
         SELECT $1, $2, a, p, m FROM unnest($3::uuid[], $4::uuid[], $5::bigint[]) AS t(a, p, m)",
        workspace_id,
        correction_id,
        &a,
        &p,
        &m,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// One `supply_correction` passbook line per `(person, amount)`, one statement.
pub async fn insert_lines(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    village_id: Uuid,
    currency_id: Uuid,
    correction_id: Uuid,
    rows: &[(Uuid, i64)],
) -> Result<(), sqlx::Error> {
    let p: Vec<Uuid> = rows.iter().map(|r| r.0).collect();
    let m: Vec<i64> = rows.iter().map(|r| r.1).collect();
    sqlx::query!(
        "INSERT INTO farm_passbook_entries (workspace_id, village_id, person_id, currency_id,
                                            kind, amount, correction_id)
         SELECT $1, $2, p, $3, 'supply_correction', m, $4
         FROM unnest($5::uuid[], $6::bigint[]) AS t(p, m)",
        workspace_id,
        village_id,
        currency_id,
        correction_id,
        &p,
        &m,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Passbook balance per person in one currency.
pub async fn balances(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    currency_id: Uuid,
    persons: &[Uuid],
) -> Result<Vec<(Uuid, i64)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT person_id, sum(amount)::bigint AS "amount!" FROM farm_passbook_entries
           WHERE workspace_id = $1 AND currency_id = $2 AND person_id = ANY($3)
           GROUP BY person_id"#,
        workspace_id,
        currency_id,
        persons,
    )
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(|r| (r.person_id, r.amount)).collect())
}

/// Posted lines of an approved correction `(person, name, amount)`.
pub async fn posted(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    correction_id: Uuid,
) -> Result<Vec<(Uuid, String, i64)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT e.person_id, concat_ws(' ', p.given_name, p.family_name) AS "name!", e.amount
           FROM farm_passbook_entries e
           JOIN farm_persons p ON p.workspace_id = e.workspace_id AND p.id = e.person_id
           WHERE e.workspace_id = $1 AND e.correction_id = $2 ORDER BY e.person_id"#,
        workspace_id,
        correction_id,
    )
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.person_id, r.name, r.amount))
        .collect())
}

/// Names of persons of the workspace.
pub async fn names(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    persons: &[Uuid],
) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, concat_ws(' ', given_name, family_name) AS "name!" FROM farm_persons
           WHERE workspace_id = $1 AND id = ANY($2)"#,
        workspace_id,
        persons,
    )
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.name)).collect())
}
