//! SQL for deal_pipelines and deal_pipeline_statuses. Every statement takes `workspace_id`
//! and skips soft-deleted rows.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::modules::{
    custom_statuses::repo::CustomStatusRow,
    deal_pipelines::dto::{
        LaneProps, LaneRules, NewPipeline, NewStatus, PipelinePatch, StatusPatch,
    },
};

/// Live pipeline.
pub struct PipelineRow {
    pub id: Uuid,
    pub name: String,
    pub kind: Option<String>,
    pub sorting: String,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Live column of a pipeline, with its custom status when that is still live.
pub struct StatusRow {
    pub id: Uuid,
    pub deal_pipeline_id: Uuid,
    pub position: i32,
    pub rules: LaneRules,
    pub props: LaneProps,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub custom_status: Option<CustomStatusRow>,
}

/// New pipeline; returns its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    p: &NewPipeline,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO deal_pipelines (workspace_id, name, type, sorting, meta)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
        workspace_id,
        p.name,
        p.kind,
        p.sorting,
        p.meta,
    )
    .fetch_one(db)
    .await
}

/// Live pipeline of `workspace_id`. None = unknown, deleted or another workspace's.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<PipelineRow>, sqlx::Error> {
    sqlx::query_as!(
        PipelineRow,
        r#"SELECT id, name, type AS kind, sorting, meta, created_at, updated_at
           FROM deal_pipelines
           WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL"#,
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// Live pipelines of `workspace_id` among `ids`, in id order (resources that embed one).
pub async fn find_many(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<PipelineRow>, sqlx::Error> {
    sqlx::query_as!(
        PipelineRow,
        r#"SELECT id, name, type AS kind, sorting, meta, created_at, updated_at
           FROM deal_pipelines
           WHERE workspace_id = $1 AND id = ANY($2) AND deleted_at IS NULL
           ORDER BY id"#,
        workspace_id,
        ids,
    )
    .fetch_all(db)
    .await
}

/// Where a deleted column's deals go: the board's first live ordinary column other than
/// `except` (position order), else its first won or lost one. Open deals don't turn into won
/// ones while an ordinary column is left.
pub async fn fallback_status(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    pipeline: Uuid,
    except: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM deal_pipeline_statuses
         WHERE workspace_id = $1 AND deal_pipeline_id = $2 AND deleted_at IS NULL AND id <> $3
         ORDER BY is_won OR is_lost, position, id
         LIMIT 1",
        workspace_id,
        pipeline,
        except,
    )
    .fetch_optional(&mut *tx)
    .await
}

/// One page of pipelines in insertion order, plus the total.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<PipelineRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, name, type AS kind, sorting, meta, created_at, updated_at,
                  count(*) OVER () AS "total!"
           FROM deal_pipelines
           WHERE workspace_id = $1 AND deleted_at IS NULL
           ORDER BY id
           LIMIT $2 OFFSET $3"#,
        workspace_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| PipelineRow {
            id: r.id,
            name: r.name,
            kind: r.kind,
            sorting: r.sorting,
            meta: r.meta,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect();
    Ok((rows, total))
}

/// Live pipeline count, for a page past the end.
pub async fn count(db: impl PgExecutor<'_>, workspace_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM deal_pipelines
           WHERE workspace_id = $1 AND deleted_at IS NULL"#,
        workspace_id,
    )
    .fetch_one(db)
    .await
}

/// Patch in one statement: None keeps the column. False = no such live row.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    p: &PipelinePatch,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE deal_pipelines
         SET name = COALESCE($3, name),
             type = CASE WHEN $4 THEN $5 ELSE type END,
             sorting = COALESCE($6, sorting),
             meta = CASE WHEN $7 THEN $8 ELSE meta END,
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
        p.name,
        p.kind.is_some(),
        p.kind.as_ref().and_then(|k| k.as_deref()),
        p.sorting,
        p.meta.is_some(),
        p.meta.as_ref().and_then(|m| m.as_ref()),
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Lock a live pipeline for the rest of the transaction: every column write of one board
/// goes through this lock, so position shifts and the won/lost checks see each other.
/// False = no such live pipeline.
pub async fn lock(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!("SET LOCAL lock_timeout = '5s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query_scalar!(
        "SELECT id FROM deal_pipelines
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL
         FOR NO KEY UPDATE",
        id,
        workspace_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map(|row| row.is_some())
}

/// Live columns of these pipelines, each board's in position order. One query per page.
pub async fn statuses_for(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    pipelines: &[Uuid],
) -> Result<Vec<StatusRow>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT s.id, s.deal_pipeline_id, s.position, s.max_items_per_lane,
                  s.max_days_in_lane, s.auto_archive_in_days, s.lane_color,
                  s.deal_probability, s.is_won, s.is_lost, s.meta, s.created_at, s.updated_at,
                  c.id AS "cs_id?", c.name AS "cs_name?", c.domain AS "cs_domain?",
                  c.archived AS "cs_archived?", c.meta AS cs_meta,
                  c.created_at AS "cs_created_at?", c.updated_at AS "cs_updated_at?"
           FROM deal_pipeline_statuses s
           LEFT JOIN custom_statuses c
             ON c.workspace_id = s.workspace_id AND c.id = s.custom_status_id
            AND c.deleted_at IS NULL
           WHERE s.workspace_id = $1 AND s.deal_pipeline_id = ANY($2) AND s.deleted_at IS NULL
           ORDER BY s.deal_pipeline_id, s.position, s.id"#,
        workspace_id,
        pipelines,
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| StatusRow {
            id: r.id,
            deal_pipeline_id: r.deal_pipeline_id,
            position: r.position,
            rules: LaneRules {
                max_items_per_lane: r.max_items_per_lane,
                max_days_in_lane: r.max_days_in_lane,
                auto_archive_in_days: r.auto_archive_in_days,
            },
            props: LaneProps {
                lane_color: r.lane_color,
                deal_probability: r.deal_probability,
                is_won: r.is_won,
                is_lost: r.is_lost,
            },
            meta: r.meta,
            created_at: r.created_at,
            updated_at: r.updated_at,
            custom_status: match (
                r.cs_id,
                r.cs_name,
                r.cs_domain,
                r.cs_archived,
                r.cs_created_at,
                r.cs_updated_at,
            ) {
                (
                    Some(id),
                    Some(name),
                    Some(domain),
                    Some(archived),
                    Some(created_at),
                    Some(updated_at),
                ) => Some(CustomStatusRow {
                    id,
                    name,
                    domain,
                    archived,
                    meta: r.cs_meta,
                    created_at,
                    updated_at,
                }),
                _ => None,
            },
        })
        .collect())
}

/// True when `id` is a live `deal_pipeline` custom status of the workspace (archived ones
/// count, as in Laravel ExistCustomStatus).
pub async fn custom_status_exists(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
             SELECT 1 FROM custom_statuses
             WHERE id = $1 AND workspace_id = $2 AND domain = 'deal_pipeline'
               AND deleted_at IS NULL
           ) AS "found!""#,
        id,
        workspace_id,
    )
    .fetch_one(db)
    .await
}

/// True when a live column of the pipeline already uses the custom status.
pub async fn custom_status_used(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    pipeline: Uuid,
    custom_status: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
             SELECT 1 FROM deal_pipeline_statuses
             WHERE workspace_id = $1 AND deal_pipeline_id = $2 AND custom_status_id = $3
               AND deleted_at IS NULL
           ) AS "used!""#,
        workspace_id,
        pipeline,
        custom_status,
    )
    .fetch_one(db)
    .await
}

/// True when another live column of the pipeline already holds a flag these props set.
pub async fn flag_taken(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    pipeline: Uuid,
    props: &LaneProps,
    except: Option<Uuid>,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
             SELECT 1 FROM deal_pipeline_statuses
             WHERE workspace_id = $5 AND deal_pipeline_id = $1 AND deleted_at IS NULL
               AND id IS DISTINCT FROM $4 AND ((is_won AND $2) OR (is_lost AND $3))
           ) AS "taken!""#,
        pipeline,
        props.is_won,
        props.is_lost,
        except,
        workspace_id,
    )
    .fetch_one(db)
    .await
}

/// Make room at `position`: when another live column holds it, every column from there on
/// moves one to the right (Laravel's checkPosition cascade, as one statement). Run under
/// `lock`.
pub async fn shift_from(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    pipeline: Uuid,
    position: i32,
    except: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE deal_pipeline_statuses SET position = position + 1, updated_at = now()
         WHERE workspace_id = $4 AND deal_pipeline_id = $1 AND deleted_at IS NULL
           AND position >= $2 AND id IS DISTINCT FROM $3
           AND EXISTS (SELECT 1 FROM deal_pipeline_statuses
                       WHERE workspace_id = $4 AND deal_pipeline_id = $1
                         AND deleted_at IS NULL AND position = $2
                         AND id IS DISTINCT FROM $3)",
        pipeline,
        position,
        except,
        workspace_id,
    )
    .execute(&mut *tx)
    .await
    .map(drop)
}

/// New column; returns its id.
pub async fn insert_status(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    pipeline: Uuid,
    s: &NewStatus,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO deal_pipeline_statuses
           (workspace_id, deal_pipeline_id, custom_status_id, position, max_items_per_lane,
            max_days_in_lane, auto_archive_in_days, lane_color, deal_probability, is_won,
            is_lost, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         RETURNING id",
        workspace_id,
        pipeline,
        s.custom_status_id,
        s.position,
        s.rules.max_items_per_lane,
        s.rules.max_days_in_lane,
        s.rules.auto_archive_in_days,
        s.props.lane_color,
        s.props.deal_probability,
        s.props.is_won,
        s.props.is_lost,
        s.meta,
    )
    .fetch_one(&mut *tx)
    .await
}

/// True when `id` is a live column of this pipeline.
pub async fn status_exists(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    pipeline: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
             SELECT 1 FROM deal_pipeline_statuses
             WHERE id = $1 AND workspace_id = $2 AND deal_pipeline_id = $3
               AND deleted_at IS NULL
           ) AS "found!""#,
        id,
        workspace_id,
        pipeline,
    )
    .fetch_one(db)
    .await
}

/// Patch a column in one statement: None keeps the stored values.
pub async fn update_status(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    pipeline: Uuid,
    id: Uuid,
    p: &StatusPatch,
) -> Result<(), sqlx::Error> {
    let rules = p.rules.as_ref();
    let props = p.props.as_ref();
    sqlx::query!(
        "UPDATE deal_pipeline_statuses
         SET position = COALESCE($4, position),
             max_items_per_lane = COALESCE($5, max_items_per_lane),
             max_days_in_lane = COALESCE($6, max_days_in_lane),
             auto_archive_in_days = COALESCE($7, auto_archive_in_days),
             lane_color = COALESCE($8, lane_color),
             deal_probability = COALESCE($9, deal_probability),
             is_won = COALESCE($10, is_won),
             is_lost = COALESCE($11, is_lost),
             meta = CASE WHEN $12 THEN $13 ELSE meta END,
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deal_pipeline_id = $3 AND deleted_at IS NULL",
        id,
        workspace_id,
        pipeline,
        p.position,
        rules.map(|r| r.max_items_per_lane),
        rules.map(|r| r.max_days_in_lane),
        rules.map(|r| r.auto_archive_in_days),
        props.map(|x| x.lane_color.as_str()),
        props.map(|x| x.deal_probability.as_str()),
        props.map(|x| x.is_won),
        props.map(|x| x.is_lost),
        p.meta.is_some(),
        p.meta.as_ref().and_then(|m| m.as_ref()),
    )
    .execute(&mut *tx)
    .await
    .map(drop)
}

/// Soft delete a column of this pipeline; frees its custom status. False = no such live row.
pub async fn soft_delete_status(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    pipeline: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE deal_pipeline_statuses SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deal_pipeline_id = $3 AND deleted_at IS NULL",
        id,
        workspace_id,
        pipeline,
    )
    .execute(&mut *tx)
    .await
    .map(|done| done.rows_affected() == 1)
}
