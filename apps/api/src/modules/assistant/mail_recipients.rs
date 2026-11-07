//! Mail recipient approval snapshots; dispatch consumes the same resolved address vector.

use super::{dto::ProposalInput, preconditions, repo::Owner};
use crate::{
    AppError,
    http::json::{Rules, Validate},
    modules::{messages, permissions},
};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

/// Capture direct mail under native write authority before reading its contact.
pub async fn capture(
    conn: &mut PgConnection,
    owner: Owner,
    input: &ProposalInput,
) -> Result<Option<Value>, AppError> {
    if input.operation_id != "messages_send" {
        return Ok(None);
    }
    let access = permissions::repo::access(&mut *conn, owner.workspace_id, owner.user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !access.active
        || !permissions::model::Granted::new(access.owner, access.permissions)
            .allows("workspace.send-message")
    {
        return Err(AppError::Forbidden(
            "You do not have permission for this action.",
        ));
    }
    let request: messages::dto::SendMessageRequest =
        serde_json::from_value(input.body.clone().unwrap_or(Value::Null))
            .map_err(|_| AppError::field("body", "Invalid business draft."))?;
    let mut rules = Rules::default();
    let normalized = request.validate(&mut rules);
    rules.into_result()?;
    let normalized =
        normalized.ok_or_else(|| AppError::field("body", "Invalid business draft."))?;
    let contact = messages::service::contact_email_from(
        conn,
        owner.workspace_id,
        normalized.addressed_contact_id,
    )
    .await?;
    let to = if normalized.manual {
        Vec::new()
    } else if normalized.to.is_empty() {
        contact.into_iter().collect()
    } else {
        normalized.to
    };
    Ok(Some(
        json!({"version":1,"operation_id":input.operation_id,"facts":{"kind":"mail","locked":{"to":to}}}),
    ))
}

/// Compare the already resolved vector; the sender moves these exact addresses into SMTP.
pub fn check(workspace: Uuid, to: &[String]) -> Result<(), AppError> {
    preconditions::check_locked(workspace, "mail", &json!({"to":to}))
}
