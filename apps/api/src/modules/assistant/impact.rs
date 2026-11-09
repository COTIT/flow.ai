//! Destructive-operation preflight: native refusal first, then named target and bounded impact counts.

use super::preconditions_repo as snapshot_repo;
use super::{dto::ProposalInput, preconditions, repo::Owner};
use crate::{
    AppError,
    modules::{invoices, permissions},
};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

/// Refuse a delete the native rules would refuse; otherwise review its target and impact.
#[tracing::instrument(name = "impact_capture", level = "debug", skip_all)]
pub async fn capture(
    conn: &mut PgConnection,
    owner: Owner,
    input: &ProposalInput,
) -> Result<Option<Value>, AppError> {
    let (permission, kind) = match input.operation_id.as_str() {
        "relations_delete" => ("relation.delete", "relation"),
        "products_delete" => ("product.delete", "product"),
        _ => return Ok(None),
    };
    let access = permissions::repo::access(&mut *conn, owner.workspace_id, owner.user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !access.active
        || !permissions::model::Granted::new(access.owner, access.permissions).allows(permission)
    {
        return Err(AppError::Forbidden(
            "You do not have permission for this action.",
        ));
    }
    let workspace = owner.workspace_id;
    let id = input
        .path
        .split('/')
        .nth(5)
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or(AppError::NotFound)?;
    let facts = facts(conn, workspace, kind, id).await?;
    if kind == "relation" {
        invoices::service::relation_deletable(conn, workspace, id).await?;
    } else {
        invoices::service::product_deletable(conn, workspace, id).await?;
    }
    Ok(Some(
        json!({"version":1,"operation_id":input.operation_id,"facts":facts}),
    ))
}

/// Named target plus counts of what the soft delete hides; 404 outside the workspace.
async fn facts(
    conn: &mut PgConnection,
    workspace: Uuid,
    kind: &str,
    id: Uuid,
) -> Result<Value, AppError> {
    let (locked, impact) = if kind == "relation" {
        snapshot_repo::relation_impact(conn, workspace, id).await?
    } else {
        snapshot_repo::product_impact(conn, workspace, id).await?
    }
    .ok_or(AppError::NotFound)?;
    Ok(json!({"kind":kind,"locked":locked,"impact":impact}))
}

/// Inside the delete transaction, after its row lock: reviewed impact must still hold.
pub async fn check(
    conn: &mut PgConnection,
    workspace: Uuid,
    kind: &str,
    id: Uuid,
) -> Result<(), AppError> {
    if preconditions::expects(kind) {
        let actual = facts(conn, workspace, kind, id).await?;
        preconditions::check_impact(workspace, kind, &actual["impact"])?;
    }
    Ok(())
}
