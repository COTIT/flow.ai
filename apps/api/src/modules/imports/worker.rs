//! Apply job: rechecks the applier's grants, then one native write per row transaction.
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use super::{profiles, service::native_permission};
use crate::{
    error::AppError,
    jobs::{kinds::Ctx, repo::Claimed, runner::Step},
    modules::permissions,
};

/// Rows per job step; the runner requeues until none are left.
const STEP_ROWS: usize = 25;

/// Run one bounded step of an import apply.
pub async fn run(ctx: &Ctx, job: &Claimed) -> Result<Step, AppError> {
    let Some(id) = job.payload["batch_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
    else {
        return Ok(Step::Done);
    };
    let Some(b) = sqlx::query(
        "SELECT workspace_id, state, profile, mapping, applied_by, revision FROM import_batches WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&ctx.db)
    .await?
    else {
        return Ok(Step::Done);
    };
    let (ws, state, profile, mapping, actor, revision): (
        Uuid,
        String,
        String,
        Value,
        Option<Uuid>,
        i32,
    ) = (b.get(0), b.get(1), b.get(2), b.get(3), b.get(4), b.get(5));
    if state != "applying" {
        return Ok(Step::Done);
    }
    let permitted = match actor {
        Some(user) => permissions::repo::access(&ctx.db, ws, user)
            .await?
            .is_some_and(|a| {
                let g = permissions::model::Granted::new(a.owner, a.permissions);
                a.active
                    && g.allows("import.apply")
                    && g.allows(native_permission(&profile, &mapping))
            }),
        None => false,
    };
    if !permitted {
        sqlx::query("UPDATE import_batches SET state = 'failed', issues = $2, updated_at = now() WHERE id = $1 AND state = 'applying'")
            .bind(id)
            .bind(json!([profiles::issue(&AppError::Forbidden(""))]))
            .execute(&ctx.db)
            .await?;
        return Ok(Step::Done);
    }
    let actor = actor.unwrap_or_default();
    for _ in 0..STEP_ROWS {
        let mut tx = ctx.db.begin().await?;
        let Some(row) = sqlx::query_scalar::<_, Value>(
            "SELECT jsonb_build_object('id', id, 'fields', fields, 'external_key', external_key, 'target_id', target_id)
             FROM import_rows WHERE batch_id = $1 AND revision = $2 AND status = 'valid'
             ORDER BY row_number LIMIT 1 FOR UPDATE SKIP LOCKED",
        )
        .bind(id)
        .bind(revision)
        .fetch_optional(&mut *tx)
        .await?
        else {
            sqlx::query("UPDATE import_batches SET state = 'completed', updated_at = now() WHERE id = $1 AND state = 'applying'")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(Step::Done);
        };
        let row_id = row["id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .unwrap_or_default();
        match profiles::apply(&mut tx, ws, actor, &profile, &mapping, &row).await {
            Ok(result) => {
                sqlx::query("UPDATE import_rows SET status = 'applied', result_id = $2, applied_at = now() WHERE id = $1")
                    .bind(row_id)
                    .bind(result)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
            }
            Err(error) => {
                tx.rollback().await?;
                sqlx::query("UPDATE import_rows SET status = 'failed', issues = $2 WHERE id = $1 AND status = 'valid'")
                    .bind(row_id)
                    .bind(json!([profiles::issue(&error)]))
                    .execute(&ctx.db)
                    .await?;
            }
        }
    }
    Ok(Step::More)
}
