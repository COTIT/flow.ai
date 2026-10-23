//! SQL for deals. Every statement takes `workspace_id` and skips soft-deleted rows. Rank
//! writers lock in one order: board row `FOR SHARE` (`lock_board`), then columns by id
//! (`lock_columns`), then the deal (`lock_deal`).

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::modules::deals::dto::{DealPatch, NewDeal};

/// Live deal.
pub struct DealRow {
    pub id: Uuid,
    pub deal_pipeline_id: Uuid,
    pub deal_pipeline_status_id: Uuid,
    pub currency_id: Uuid,
    pub relation_id: Option<Uuid>,
    pub sales_agent_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub deal_value: Decimal,
    pub deal_priority: String,
    pub expected_close_date: Option<NaiveDate>,
    pub date: Option<NaiveDate>,
    pub position: i64,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// List filters; `pattern` is an escaped ILIKE value.
pub struct ListFilter<'a> {
    pub pattern: Option<&'a str>,
    pub relation: Option<Uuid>,
    pub pipeline: Option<Uuid>,
    pub status: Option<Uuid>,
    pub limit: i64,
    pub offset: i64,
}

/// Won/lost totals of one board per currency (pipeline summary).
pub struct SummaryRow {
    pub deal_pipeline_id: Uuid,
    pub is_won: bool,
    pub asset_type: String,
    pub deals: i64,
    pub total: Decimal,
}

/// `lock_timeout` for this transaction, then the board row `FOR SHARE`: rank writers run side
/// by side, while a column write (`deal_pipelines::repo::lock`, `FOR NO KEY UPDATE`) waits for
/// them and holds them off. False = no such live board.
pub async fn lock_board(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    pipeline: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!("SET LOCAL lock_timeout = '5s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query_scalar!(
        "SELECT id FROM deal_pipelines
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL
         FOR SHARE",
        pipeline,
        workspace_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map(|row| row.is_some())
}

/// Lock these columns of the board, in id order; returns the live ones found. A deleted
/// column is not found: the composite FK alone would accept it.
pub async fn lock_columns(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    pipeline: Uuid,
    ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM deal_pipeline_statuses
         WHERE workspace_id = $1 AND deal_pipeline_id = $2 AND id = ANY($3)
           AND deleted_at IS NULL
         ORDER BY id
         FOR NO KEY UPDATE",
        workspace_id,
        pipeline,
        ids,
    )
    .fetch_all(&mut *tx)
    .await
}

/// Lock a live deal for this transaction (last in the lock order). None = 404.
pub async fn lock_deal(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<DealRow>, sqlx::Error> {
    sqlx::query!("SET LOCAL lock_timeout = '5s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query_as!(
        DealRow,
        "SELECT id, deal_pipeline_id, deal_pipeline_status_id, currency_id, relation_id,
                sales_agent_id, title, description, deal_value, deal_priority, expected_close_date,
                date, position, meta, created_at, updated_at
         FROM deals
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL
         FOR NO KEY UPDATE",
        id,
        workspace_id,
    )
    .fetch_optional(&mut *tx)
    .await
}

/// Live deal of `workspace_id`.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<DealRow>, sqlx::Error> {
    sqlx::query_as!(
        DealRow,
        "SELECT id, deal_pipeline_id, deal_pipeline_status_id, currency_id, relation_id,
                sales_agent_id, title, description, deal_value, deal_priority, expected_close_date,
                date, position, meta, created_at, updated_at
         FROM deals
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// Highest rank in a column (the deal itself left out). Run under `lock_columns`.
pub async fn last_rank(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    status: Uuid,
    except: Option<Uuid>,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT max(position) FROM deals
         WHERE workspace_id = $1 AND deal_pipeline_status_id = $2 AND deleted_at IS NULL
           AND id IS DISTINCT FROM $3",
        workspace_id,
        status,
        except,
    )
    .fetch_one(&mut *tx)
    .await
}

/// Up to two ranks of a column from `offset` on, in board order (the deal left out).
pub async fn ranks_from(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    status: Uuid,
    except: Uuid,
    offset: i64,
) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT position FROM deals
         WHERE workspace_id = $1 AND deal_pipeline_status_id = $2 AND deleted_at IS NULL
           AND id <> $3
         ORDER BY position, id
         OFFSET $4 LIMIT 2",
        workspace_id,
        status,
        except,
        offset,
    )
    .fetch_all(&mut *tx)
    .await
}

/// Respace a column to `step` apart, keeping its order (the deal left out). Run under
/// `lock_columns`; only when a gap has closed.
pub async fn renumber(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    status: Uuid,
    except: Uuid,
    step: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE deals d SET position = s.n * $4
         FROM (SELECT id, row_number() OVER (ORDER BY position, id) AS n FROM deals
               WHERE workspace_id = $1 AND deal_pipeline_status_id = $2 AND deleted_at IS NULL
                 AND id <> $3) s
         WHERE d.id = s.id",
        workspace_id,
        status,
        except,
        step,
    )
    .execute(&mut *tx)
    .await
    .map(drop)
}

/// New deal; returns its id.
pub async fn insert(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    d: &NewDeal,
    currency: Uuid,
    position: i64,
    created_by: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO deals
           (workspace_id, deal_pipeline_id, deal_pipeline_status_id, currency_id, relation_id,
            sales_agent_id, created_by, title, description, deal_value, deal_priority,
            expected_close_date, date, position, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
         RETURNING id",
        workspace_id,
        d.pipeline,
        d.status,
        currency,
        d.relation,
        d.sales_agent,
        created_by,
        d.title,
        d.description,
        d.deal_value,
        d.priority,
        d.expected_close_date,
        d.date,
        position,
        d.meta,
    )
    .fetch_one(&mut *tx)
    .await
}

/// Patch in one statement; `place` = (board, column, rank) for a move.
pub async fn update(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    p: &DealPatch,
    place: Option<(Uuid, Uuid, i64)>,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE deals
         SET title = COALESCE($3, title),
             description = CASE WHEN $4 THEN $5 ELSE description END,
             deal_value = COALESCE($6, deal_value),
             date = CASE WHEN $7 THEN $8 ELSE date END,
             sales_agent_id = CASE WHEN $9 THEN $10 ELSE sales_agent_id END,
             deal_priority = COALESCE($11, deal_priority),
             expected_close_date = CASE WHEN $12 THEN $13 ELSE expected_close_date END,
             meta = CASE WHEN $14 THEN $15 ELSE meta END,
             deal_pipeline_id = COALESCE($16, deal_pipeline_id),
             deal_pipeline_status_id = COALESCE($17, deal_pipeline_status_id),
             position = COALESCE($18, position),
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
        p.title,
        p.description.is_some(),
        p.description.as_ref().and_then(|d| d.as_deref()),
        p.deal_value,
        p.date.is_some(),
        p.date.flatten(),
        p.sales_agent.is_some(),
        p.sales_agent.flatten(),
        p.priority,
        p.expected_close_date.is_some(),
        p.expected_close_date.flatten(),
        p.meta.is_some(),
        p.meta.as_ref().and_then(|m| m.as_ref()),
        place.map(|x| x.0),
        place.map(|x| x.1),
        place.map(|x| x.2),
    )
    .execute(&mut *tx)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Set the relation only where none is set yet. False = no live deal without one.
pub async fn attach_relation(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    relation: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE deals SET relation_id = $3, updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL AND relation_id IS NULL",
        id,
        workspace_id,
        relation,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Soft delete. False = no such live deal.
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE deals SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// One page of deals, newest first, plus the total.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<(Vec<DealRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, deal_pipeline_id, deal_pipeline_status_id, currency_id, relation_id,
                  sales_agent_id, title, description, deal_value, deal_priority,
                  expected_close_date, date, position, meta, created_at, updated_at,
                  count(*) OVER () AS "total!"
           FROM deals
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR title ILIKE $2)
             AND ($3::uuid IS NULL OR relation_id = $3)
             AND ($4::uuid IS NULL OR deal_pipeline_id = $4)
             AND ($5::uuid IS NULL OR deal_pipeline_status_id = $5)
           ORDER BY id DESC
           LIMIT $6 OFFSET $7"#,
        workspace_id,
        f.pattern,
        f.relation,
        f.pipeline,
        f.status,
        f.limit,
        f.offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| DealRow {
            id: r.id,
            deal_pipeline_id: r.deal_pipeline_id,
            deal_pipeline_status_id: r.deal_pipeline_status_id,
            currency_id: r.currency_id,
            relation_id: r.relation_id,
            sales_agent_id: r.sales_agent_id,
            title: r.title,
            description: r.description,
            deal_value: r.deal_value,
            deal_priority: r.deal_priority,
            expected_close_date: r.expected_close_date,
            date: r.date,
            position: r.position,
            meta: r.meta,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect();
    Ok((rows, total))
}

/// Matching count, for a page past the end.
pub async fn count(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM deals
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR title ILIKE $2)
             AND ($3::uuid IS NULL OR relation_id = $3)
             AND ($4::uuid IS NULL OR deal_pipeline_id = $4)
             AND ($5::uuid IS NULL OR deal_pipeline_status_id = $5)"#,
        workspace_id,
        f.pattern,
        f.relation,
        f.pipeline,
        f.status,
    )
    .fetch_one(db)
    .await
}

/// One page of a board column in the board's order (Laravel SortingEnum: `a-z` / `z-a` by
/// deal date, MySQL's NULL-first ascending; `pos` by rank).
pub async fn column(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    status: Uuid,
    sorting: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<DealRow>, sqlx::Error> {
    match sorting {
        "pos" => {
            sqlx::query_as!(
                DealRow,
                "SELECT id, deal_pipeline_id, deal_pipeline_status_id, currency_id, relation_id,
                sales_agent_id, title, description, deal_value, deal_priority, expected_close_date,
                date, position, meta, created_at, updated_at
         FROM deals
                 WHERE workspace_id = $1 AND deal_pipeline_status_id = $2 AND deleted_at IS NULL
                 ORDER BY position, id LIMIT $3 OFFSET $4",
                workspace_id,
                status,
                limit,
                offset,
            )
            .fetch_all(db)
            .await
        }
        "z-a" => {
            sqlx::query_as!(
                DealRow,
                "SELECT id, deal_pipeline_id, deal_pipeline_status_id, currency_id, relation_id,
                sales_agent_id, title, description, deal_value, deal_priority, expected_close_date,
                date, position, meta, created_at, updated_at
         FROM deals
                 WHERE workspace_id = $1 AND deal_pipeline_status_id = $2 AND deleted_at IS NULL
                 ORDER BY date DESC NULLS LAST, id DESC LIMIT $3 OFFSET $4",
                workspace_id,
                status,
                limit,
                offset,
            )
            .fetch_all(db)
            .await
        }
        _ => {
            sqlx::query_as!(
                DealRow,
                "SELECT id, deal_pipeline_id, deal_pipeline_status_id, currency_id, relation_id,
                sales_agent_id, title, description, deal_value, deal_priority, expected_close_date,
                date, position, meta, created_at, updated_at
         FROM deals
                 WHERE workspace_id = $1 AND deal_pipeline_status_id = $2 AND deleted_at IS NULL
                 ORDER BY date NULLS FIRST, id LIMIT $3 OFFSET $4",
                workspace_id,
                status,
                limit,
                offset,
            )
            .fetch_all(db)
            .await
        }
    }
}

/// Live deals in a column.
pub async fn column_count(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    status: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM deals
           WHERE workspace_id = $1 AND deal_pipeline_status_id = $2 AND deleted_at IS NULL"#,
        workspace_id,
        status,
    )
    .fetch_one(db)
    .await
}

/// Move every deal of a column to the end of another column of the same board, keeping
/// their order. Run under the board's column-write lock.
pub async fn move_column(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    from: Uuid,
    to: Uuid,
    step: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE deals d
         SET deal_pipeline_status_id = $3,
             position = s.base + s.n * $4,
             updated_at = now()
         FROM (SELECT id, row_number() OVER (ORDER BY position, id) AS n,
                      (SELECT COALESCE(max(position), 0) FROM deals
                       WHERE workspace_id = $1 AND deal_pipeline_status_id = $3
                         AND deleted_at IS NULL) AS base
               FROM deals
               WHERE workspace_id = $1 AND deal_pipeline_status_id = $2 AND deleted_at IS NULL) s
         WHERE d.id = s.id",
        workspace_id,
        from,
        to,
        step,
    )
    .execute(&mut *tx)
    .await
    .map(drop)
}

/// Won and lost totals of these boards per currency, from one grouped scan.
pub async fn summaries(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    pipelines: &[Uuid],
) -> Result<Vec<SummaryRow>, sqlx::Error> {
    sqlx::query_as!(
        SummaryRow,
        r#"SELECT d.deal_pipeline_id, s.is_won, c.asset_type,
                  count(*) AS "deals!", sum(d.deal_value) AS "total!"
           FROM deals d
           JOIN deal_pipeline_statuses s
             ON s.workspace_id = d.workspace_id AND s.id = d.deal_pipeline_status_id
           JOIN currencies c ON c.workspace_id = d.workspace_id AND c.id = d.currency_id
           WHERE d.workspace_id = $1 AND d.deal_pipeline_id = ANY($2) AND d.deleted_at IS NULL
             AND (s.is_won OR s.is_lost)
           GROUP BY d.deal_pipeline_id, s.is_won, c.asset_type
           ORDER BY c.asset_type"#,
        workspace_id,
        pipelines,
    )
    .fetch_all(db)
    .await
}

/// True when `relation` is a live relation of the workspace.
pub async fn relation_exists(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    relation: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM relations
                          WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL) AS "found!""#,
        relation,
        workspace_id,
    )
    .fetch_one(db)
    .await
}

/// True when `currency` is a live currency of the workspace.
pub async fn currency_exists(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    currency: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM currencies
                          WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL) AS "found!""#,
        currency,
        workspace_id,
    )
    .fetch_one(db)
    .await
}
