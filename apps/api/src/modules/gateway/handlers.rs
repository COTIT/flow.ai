//! Workspace payment evidence, explicit refresh and audited operator resolutions.
use crate::{
    AppError, AppState,
    http::{
        json::{Rules, Validate, ValidatedJson},
        response::{Data, Envelope, MessageBody},
        workspace::WorkspaceAccess,
    },
    modules::gateway::{
        accounts,
        repo::{self, Operation},
        service,
    },
};
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

/// Routes requiring workspace membership and explicit owner permissions.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list))
        .routes(routes!(detail))
        .routes(routes!(refresh))
        .routes(routes!(resolve))
        .routes(routes!(retire))
}
/// Callback scope includes immutable workspace/environment account slot.
pub fn public() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(callback))
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in=Query)]
pub struct Paging {
    page: Option<i64>,
    per_page: Option<i64>,
}

fn permitted(access: &WorkspaceAccess, op: &Operation) -> bool {
    (op.order_id.is_some() && access.allows("order.view"))
        || (op.invoice_id.is_some() && access.allows("invoice.view"))
}
async fn scoped(
    state: &AppState,
    access: &WorkspaceAccess,
    id: &str,
) -> Result<Operation, AppError> {
    if !access.allows("order.view") && !access.allows("invoice.view") {
        return Err(AppError::Forbidden(
            "Payment owner view permission is required.",
        ));
    }
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    let op = repo::find(&state.db, access.workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !permitted(access, &op) {
        return Err(AppError::Forbidden("Payment owner permission is required."));
    }
    Ok(op)
}

#[utoipa::path(get,path="/workspace/{workspace}/payments",tag="payments",operation_id="payments_gateway_list",summary="List bound payment operations",params(("workspace"=Uuid,Path),Paging),responses((status=200,body=Value, description = "Payment operation data."),(status=403,body=MessageBody, description = "Required owner permission is missing."),(status=404,body=MessageBody, description = "Unknown workspace or inactive workspace membership.")),security(("bearer"=[])))]
pub async fn list(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Query(query): Query<Paging>,
) -> Result<axum::Json<Value>, AppError> {
    let orders = access.allows("order.list");
    let invoices = access.allows("invoice.list");
    if !orders && !invoices {
        return Err(AppError::Forbidden("Payment owner permission is required."));
    }
    let page = query.page.unwrap_or(1).clamp(1, 1_000_000);
    let per = query.per_page.unwrap_or(25).clamp(1, 100);
    let total:i64=sqlx::query_scalar("SELECT count(*) FROM order_payments WHERE workspace_id=$1 AND provider='globadyme' AND (($2 AND order_id IS NOT NULL) OR ($3 AND invoice_id IS NOT NULL))")
        .bind(access.workspace_id).bind(orders).bind(invoices).fetch_one(&state.db).await?;
    let ops:Vec<Operation>=sqlx::query_as("SELECT * FROM order_payments WHERE workspace_id=$1 AND provider='globadyme' AND (($2 AND order_id IS NOT NULL) OR ($3 AND invoice_id IS NOT NULL)) ORDER BY created_at DESC,id DESC LIMIT $4 OFFSET $5")
        .bind(access.workspace_id).bind(orders).bind(invoices).bind(per).bind((page-1)*per).fetch_all(&state.db).await?;
    Ok(axum::Json(
        json!({"data":ops.iter().map(service::view).collect::<Vec<_>>(),"meta":{"current_page":page,"last_page":((total+per-1)/per).max(1),"per_page":per,"total":total}}),
    ))
}

#[utoipa::path(get,path="/workspace/{workspace}/payments/{payment}",tag="payments",operation_id="payments_gateway_get",summary="Inspect owned transaction and chargeback evidence",params(("workspace"=Uuid,Path),("payment"=Uuid,Path)),responses((status=200,body=Envelope<Value>, description = "Payment operation data."),(status=403,body=MessageBody, description = "Required owner permission is missing."),(status=404,body=MessageBody, description = "Unknown token, payment, invoice or workspace.")),security(("bearer"=[])))]
pub async fn detail(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<Value>, AppError> {
    let op = scoped(&state, &access, &id).await?;
    let mut view = service::view(&op);
    let transactions:Vec<Value>=sqlx::query_scalar("SELECT evidence || jsonb_build_object('id',transaction_id,'status',status,'settled',settled,'settlementId',settlement_id,'settlementDate',settlement_date) FROM payment_transactions WHERE workspace_id=$1 AND payment_id=$2 ORDER BY first_seen_at,transaction_id LIMIT 500")
        .bind(access.workspace_id).bind(op.id).fetch_all(&state.db).await?;
    let chargebacks:Vec<Value>=sqlx::query_scalar("SELECT evidence FROM payment_chargebacks WHERE workspace_id=$1 AND payment_id=$2 ORDER BY updated_at,chargeback_id LIMIT 500")
        .bind(access.workspace_id).bind(op.id).fetch_all(&state.db).await?;
    view["transactions"] = decimal_evidence(json!(transactions));
    view["chargebacks"] = decimal_evidence(json!(chargebacks));
    let observations:Vec<Value>=sqlx::query_scalar("SELECT evidence FROM payment_observations WHERE workspace_id=$1 AND payment_id=$2 ORDER BY observed_at DESC,id DESC LIMIT 500")
        .bind(access.workspace_id).bind(op.id).fetch_all(&state.db).await?;
    view["observations"] = decimal_evidence(json!(observations));
    Ok(Data(view))
}

#[utoipa::path(post,path="/workspace/{workspace}/payments/{payment}/refresh",tag="payments",operation_id="payments_gateway_refresh",summary="Refresh authoritative payment evidence",params(("workspace"=Uuid,Path),("payment"=Uuid,Path)),responses((status=200,body=Envelope<Value>, description = "Payment operation data."),(status=403,body=MessageBody, description = "Required owner permission is missing."),(status=404,body=MessageBody, description = "Unknown token, payment, invoice or workspace.")),security(("bearer"=[])))]
pub async fn refresh(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<Value>, AppError> {
    let op = scoped(&state, &access, &id).await?;
    service::refresh(&state, access.workspace_id, op.id).await?;
    service::chargebacks_account(&state, &op).await?;
    Ok(Data(service::view(
        &repo::find(&state.db, access.workspace_id, op.id)
            .await?
            .ok_or(AppError::NotFound)?,
    )))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct Resolution {
    outcome: Option<String>,
    reason: String,
    evidence: String,
}
impl Validate for Resolution {
    type Valid = Self;
    fn validate(self, r: &mut Rules) -> Option<Self> {
        Some(Self {
            outcome: r.opt_text("outcome", self.outcome, 32),
            reason: r.text("reason", Some(self.reason), 1000),
            evidence: r.text("evidence", Some(self.evidence), 2000),
        })
    }
}

#[utoipa::path(post,path="/workspace/{workspace}/payments/{payment}/resolve",tag="payments",operation_id="payments_gateway_resolve",summary="Record audited provider-confirmed resolution",params(("workspace"=Uuid,Path),("payment"=Uuid,Path)),request_body=Resolution,responses((status=200,body=Envelope<Value>, description = "Payment operation data."),(status=403,body=MessageBody, description = "Required owner permission is missing."),(status=404,body=MessageBody, description = "Unknown token, payment, invoice or workspace."),(status=409,body=MessageBody, description = "Operation conflicts with current payment state."),(status=422,body=MessageBody, description = "Invalid request fields.")),security(("bearer"=[])))]
pub async fn resolve(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<Resolution>,
) -> Result<Data<Value>, AppError> {
    if !access.is_admin() {
        return Err(AppError::Forbidden("Workspace administrator required."));
    }
    let op = scoped(&state, &access, &id).await?;
    if op.provider != "globadyme"
        || !matches!(op.status.as_str(), "unknown" | "open")
        || (op.kind == "payment" && op.captured_transaction_id.is_some())
    {
        return Err(AppError::Conflict(
            "Only an unresolved gateway submission can be resolved with provider evidence.",
        ));
    }
    let action = input.outcome.as_deref().unwrap_or("");
    if !matches!(action, "not_accepted" | "refund_confirmed")
        || (action == "refund_confirmed" && op.kind != "refund")
    {
        return Err(AppError::field(
            "outcome",
            "Select affirmative provider nonacceptance or matching refund confirmation.",
        ));
    }
    let mut tx = state.db.begin().await?;
    let inactive: bool =
        sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("globadyme:{}:{}", access.workspace_id, op.id))
            .fetch_one(&mut *tx)
            .await?;
    if !inactive {
        return Err(AppError::Conflict(
            "Submission is still being dispatched; wait for the provider call to finish.",
        ));
    }
    service::lock_owner(&mut tx, access.workspace_id, &op).await?;
    let current = repo::find(&mut *tx, access.workspace_id, op.id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !matches!(current.status.as_str(), "unknown" | "open")
        || (current.kind == "payment" && current.captured_transaction_id.is_some())
        || (action == "refund_confirmed" && current.status != "unknown")
    {
        return Err(AppError::Conflict(
            "Submission changed; refresh before resolving.",
        ));
    }
    sqlx::query("INSERT INTO payment_audit(workspace_id,payment_id,environment,user_id,action,reason,evidence) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(access.workspace_id).bind(op.id).bind(&op.environment).bind(access.user_id).bind(action).bind(&input.reason).bind(&input.evidence).execute(&mut *tx).await?;
    if action == "not_accepted" {
        sqlx::query("UPDATE order_payments SET status='failed',last_error='Provider nonacceptance confirmed by administrator',attention=CASE WHEN attention_reason='Submission outcome is unknown; do not retry.' THEN false ELSE attention END,attention_reason=CASE WHEN attention_reason='Submission outcome is unknown; do not retry.' THEN NULL ELSE attention_reason END,updated_at=now() WHERE workspace_id=$1 AND id=$2 AND status IN ('unknown','open') AND (kind='refund' OR captured_transaction_id IS NULL)")
            .bind(access.workspace_id).bind(op.id).execute(&mut *tx).await?;
    }
    if action == "refund_confirmed" {
        service::confirm_refund_in(&mut tx, access.workspace_id, op.id, true).await?;
    }
    tx.commit().await?;
    Ok(Data(service::view(
        &repo::find(&state.db, access.workspace_id, op.id)
            .await?
            .ok_or(AppError::NotFound)?,
    )))
}

#[utoipa::path(post,path="/workspace/{workspace}/payments/globadyme/{environment}/callback-secret/retire",tag="payments",operation_id="payments_globadyme_callback_retire",summary="Retire previous callback secret with audit evidence",params(("workspace"=Uuid,Path),("environment"=String,Path)),request_body=Resolution,responses((status=200,body=Envelope<Value>, description = "Payment operation data."),(status=403,body=MessageBody, description = "Required owner permission is missing."),(status=404,body=MessageBody, description = "Unknown token, payment, invoice or workspace."),(status=422,body=MessageBody, description = "Invalid request fields.")),security(("bearer"=[])))]
pub async fn retire(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, env)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<Resolution>,
) -> Result<Data<Value>, AppError> {
    if !access.is_admin() {
        return Err(AppError::Forbidden("Workspace administrator required."));
    }
    if !matches!(env.as_str(), "sandbox" | "production") {
        return Err(AppError::NotFound);
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("SELECT workspace_id FROM settings WHERE workspace_id=$1 FOR UPDATE")
        .bind(access.workspace_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("SELECT workspace_id FROM payment_accounts WHERE workspace_id=$1 AND environment=$2 FOR UPDATE").bind(access.workspace_id).bind(&env).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM workspace_credentials WHERE workspace_id=$1 AND path=$2")
        .bind(access.workspace_id)
        .bind(accounts::path(&env, "previous_callback_secret"))
        .execute(&mut *tx)
        .await?;
    accounts::audit(
        &mut tx,
        access.workspace_id,
        access.user_id,
        &env,
        "callback_retired",
        &input.reason,
        &input.evidence,
    )
    .await?;
    tx.commit().await?;
    Ok(Data(json!({"retired":true})))
}

#[utoipa::path(post,path="/payments/webhooks/globadyme/{workspace}/{environment}",tag="payments",operation_id="payments_globadyme_webhook",summary="Signed Globadyme status callback",params(("workspace"=Uuid,Path),("environment"=String,Path)),request_body(content=String,content_type="application/json"),responses((status=200,description="Received")),security())]
pub async fn callback(
    State(state): State<AppState>,
    Path((ws, env)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    if let Ok(ws) = Uuid::parse_str(&ws) {
        service::callback(&state, ws, &env, &headers, &body).await?;
    }
    Ok(StatusCode::OK)
}

/// Preserve exact provider numeric lexemes through browser JSON parsing.
fn decimal_evidence(value: Value) -> Value {
    match value {
        Value::Number(number) => Value::String(number.to_string()),
        Value::Array(values) => Value::Array(values.into_iter().map(decimal_evidence).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, decimal_evidence(value)))
                .collect(),
        ),
        value => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_provider_money_remains_exact_text() {
        let value: Value =
            serde_json::from_str(r#"{"amount":90071992547409.93,"calculation":{"fee":0.01}}"#)
                .unwrap();
        let value = decimal_evidence(value);
        assert_eq!(value["amount"], "90071992547409.93");
        assert_eq!(value["calculation"]["fee"], "0.01");
    }
}
