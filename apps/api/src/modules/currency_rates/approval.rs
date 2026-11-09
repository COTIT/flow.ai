//! Review card for `currency_rates_create`: the pair by code, the rate in force on that day
//! (before) and the new one (after). Display only; the native handler rechecks every rule.

use serde_json::{Value, json};
use sqlx::PgConnection;

use super::{dto::CreateRateRequest, repo, service};
use crate::{
    error::AppError,
    modules::{
        assistant::{dto::ProposalInput, preconditions, repo::Owner},
        permissions,
    },
};

/// Facts for a rate proposal, `None` for any other operation. 403 without `currency.update`
/// before any currency is read; the native create rules (422) run before the card exists.
pub async fn capture(
    conn: &mut PgConnection,
    owner: Owner,
    input: &ProposalInput,
) -> Result<Option<Value>, AppError> {
    if input.operation_id != "currency_rates_create" {
        return Ok(None);
    }
    let ws = owner.workspace_id;
    let allowed = permissions::repo::access(&mut *conn, ws, owner.user_id)
        .await?
        .is_some_and(|a| {
            a.active
                && permissions::model::Granted::new(a.owner, a.permissions)
                    .allows("currency.update")
        });
    if !allowed {
        return Err(AppError::Forbidden(
            "You do not have permission for this action.",
        ));
    }
    let rate =
        preconditions::validated::<CreateRateRequest>(input.body.clone().unwrap_or(json!({})))?;
    let (from, to) = service::checked(conn, ws, &rate).await?;
    let previous = repo::in_force(&mut *conn, ws, from.id, to.id, rate.effective_on).await?;
    let before = previous.as_ref().map(|p| p.rate.normalize().to_string());
    let facts = json!({
        "kind": "currency_rate",
        "locked": {
            "label": format!("{} → {}", from.code, to.code),
            // `id` + `name` let the card show "EUR" for the body's currency ids.
            "from": {"id": from.id, "name": from.code, "code": from.code, "decimals": from.decimals},
            "to": {"id": to.id, "name": to.code, "code": to.code, "decimals": to.decimals},
            "previous": previous.map(|p| json!({"rate": before, "effective_on": p.effective_on})),
        },
        "changes": [{"field": "rate", "before": before, "after": rate.rate.normalize().to_string()}],
        "effective_on": rate.effective_on,
        "note": rate.note,
        "source": "workspace",
    });
    Ok(Some(
        json!({"version": 1, "operation_id": input.operation_id, "facts": facts}),
    ))
}
