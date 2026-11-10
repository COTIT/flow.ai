//! Token-scoped invoice collections and credit-note refunds. Provider calls run after commit;
//! confirmed receipts are immutable ledger entries, including payments arriving late.
use axum::{
    extract::{Path, State},
    http::header,
    response::IntoResponse,
};
use chrono::Utc;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgConnection;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, UnprocessableBody},
    http::{
        json::{Rules, Validate, ValidatedJson},
        response::{Data, Envelope, MessageBody},
        workspace::{Can, permissions},
    },
    idempotency::{self, Actor, Answer, Key, Keyed},
    modules::{
        auth::token,
        currencies,
        gateway::{accounts, repo as operations, repo::Operation, service as engine},
        invoice_payments::{repo as ledger, service as payments},
        invoices::{self, repo, send},
        orders, workspaces,
    },
    money,
};

permissions! { Refund => "invoice.credit", Inspect => "invoice.view" }

#[derive(Deserialize, ToSchema)]
#[schema(as = InvoiceGatewayCollectionRequest)]
pub struct CollectionRequest {
    #[schema(required = true)]
    pub country: Option<String>,
    #[schema(required = true)]
    pub payer_name: Option<String>,
    pub payer_email: Option<String>,
}

pub struct CollectionInput {
    pub country: String,
    pub payer_name: String,
    pub payer_email: Option<String>,
}

impl Validate for CollectionRequest {
    type Valid = CollectionInput;
    fn validate(self, r: &mut Rules) -> Option<Self::Valid> {
        let sent_country = r.text("country", self.country, 2);
        let country = r.country("country", &sent_country).unwrap_or_default();
        let payer_name = r.text("payer_name", self.payer_name, 255);
        let payer_email = r.opt_text("payer_email", self.payer_email, 255);
        if let Some(email) = &payer_email {
            r.email("payer_email", email);
        }
        Some(CollectionInput {
            country,
            payer_name,
            payer_email,
        })
    }
}

#[derive(Deserialize, ToSchema)]
#[schema(as = InvoiceGatewayRefundRequest)]
pub struct RefundRequest {
    #[schema(value_type = Uuid, required = true)]
    pub payment_id: Option<String>,
    #[schema(value_type = String, example = "12.50", required = true)]
    pub amount: Option<Value>,
}

pub struct RefundInput {
    pub payment_id: Uuid,
    pub amount: Decimal,
}

impl Validate for RefundRequest {
    type Valid = RefundInput;
    fn validate(self, r: &mut Rules) -> Option<Self::Valid> {
        let id = r.text("payment_id", self.payment_id, 36);
        let payment_id = Uuid::parse_str(&id).ok();
        if payment_id.is_none() {
            r.add("payment_id", "The selected payment id is invalid.");
        }
        let amount = match self.amount {
            Some(value) => r.decimal("amount", &value, -money::MAX, money::MAX),
            None => {
                r.missing("amount");
                None
            }
        };
        if amount.is_some_and(|a| a <= Decimal::ZERO) {
            r.add("amount", "The amount must be greater than 0.");
        }
        Some(RefundInput {
            payment_id: payment_id.unwrap_or_default(),
            amount: amount.unwrap_or_default(),
        })
    }
}

/// Mounted under the same public IP limiter as the invoice token lookup.
pub fn public() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create_payment, payment_status))
        .routes(routes!(payment_qr))
}

pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(refund))
        .routes(routes!(inspect))
}

#[utoipa::path(
    post, path = "/invoices/fetch-from-hashed/{token}/payment", tag = "invoices",
    operation_id = "invoices_gateway_create", summary = "Start or resume an invoice payment",
    params(("token" = String, Path)), request_body = CollectionRequest,
    responses((status = 200, body = Envelope<Value>, description = "Payment operation data."), (status = 404, body = MessageBody, description = "Unknown token, payment, invoice or workspace."),
        (status = 409, body = MessageBody, description = "Operation conflicts with current payment state."), (status = 422, body = UnprocessableBody, description = "Invalid request fields.")), security()
)]
pub async fn create_payment(
    State(state): State<AppState>,
    Path(token): Path<String>,
    ValidatedJson(input): ValidatedJson<CollectionRequest>,
) -> Result<Data<Value>, AppError> {
    let (ws, invoice) = token_target(&state, &token).await?;
    let account = accounts::select(&state, ws)
        .await?
        .ok_or(AppError::Unavailable("no payment provider configured"))?;
    let mut tx = state.db.begin().await?;
    let target = ledger::lock(&mut tx, ws, invoice)
        .await?
        .ok_or(AppError::NotFound)?;
    if target.is_credit
        || target.invoice_number.is_none()
        || !invoices::actions::PAYABLE.contains(&target.status.as_str())
        || target.total <= target.paid_total
    {
        return Err(AppError::Conflict("This invoice is not payable."));
    }
    let live: Option<Uuid> = sqlx::query_scalar("SELECT id FROM order_payments WHERE workspace_id=$1 AND invoice_id=$2 AND provider='globadyme' AND kind='payment' AND status IN ('creating','open','unknown') ORDER BY created_at DESC LIMIT 1")
        .bind(ws).bind(invoice).fetch_optional(&mut *tx).await?;
    let id = if let Some(id) = live {
        id
    } else {
        let currency = currencies::repo::find(&mut *tx, ws, target.currency_id)
            .await?
            .ok_or(AppError::NotFound)?;
        let decimals = target.dp.unwrap_or(i32::from(currency.decimals));
        accounts::bind(&mut tx, ws, &account).await?;
        operations::insert_collection(
            &mut tx,
            ws,
            None,
            Some(invoice),
            target
                .total
                .checked_sub(target.paid_total)
                .ok_or(AppError::Conflict("Invoice amount is out of range."))?,
            &currency.asset_type.to_ascii_uppercase(),
            decimals,
            &account,
            &input.country,
            &input.payer_name,
            input.payer_email.as_deref(),
        )
        .await?
    };
    tx.commit().await?;
    engine::submit(&state, ws, id, &redirect(&state, ws, invoice).await?).await?;
    let op = operations::find(&state.db, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Data(engine::view(&op)))
}

#[utoipa::path(
    get, path = "/invoices/fetch-from-hashed/{token}/payment", tag = "invoices",
    operation_id = "invoices_gateway_status", summary = "Read an invoice payment status",
    params(("token" = String, Path)), responses((status = 200, body = Envelope<Value>, description = "Payment operation data."), (status = 404, body = MessageBody, description = "Unknown token, payment, invoice or workspace.")), security()
)]
pub async fn payment_status(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Data<Value>, AppError> {
    let (ws, invoice) = token_target(&state, &token).await?;
    let Some(id) = latest(&state, ws, invoice).await? else {
        return Ok(Data(Value::Null));
    };
    engine::refresh(&state, ws, id).await?;
    let op = operations::find(&state.db, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Data(engine::view(&op)))
}

#[utoipa::path(
    get, path = "/invoices/fetch-from-hashed/{token}/payment/qr", tag = "invoices",
    operation_id = "invoices_gateway_qr", summary = "Read an invoice payment QR image",
    params(("token" = String, Path)), responses((status = 200, content_type = "image/png", body = Vec<u8>, description = "Provider payment QR image."), (status = 404, body = MessageBody, description = "Unknown token, payment, invoice or workspace.")), security()
)]
pub async fn payment_qr(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let (ws, invoice) = token_target(&state, &token).await?;
    let id = latest(&state, ws, invoice)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        engine::qr(&state, ws, id).await?,
    ))
}

#[utoipa::path(
    get, path = "/workspace/{workspace}/invoices/{invoice}/gateway", tag = "invoices",
    operation_id = "invoices_gateway_inspect", summary = "Inspect invoice payments and refunds",
    params(("workspace" = Uuid, Path), ("invoice" = Uuid, Path)),
    responses((status = 200, body = Envelope<Value>, description = "Payment operation data."), (status = 403, body = MessageBody, description = "Required owner permission is missing."), (status = 404, body = MessageBody, description = "Unknown token, payment, invoice or workspace.")), security(("bearer" = []))
)]
pub async fn inspect(
    State(state): State<AppState>,
    access: Can<Inspect>,
    Path((_, invoice)): Path<(String, String)>,
) -> Result<Data<Value>, AppError> {
    let invoice = parse(&invoice)?;
    repo::find(&state.db, access.workspace_id, invoice)
        .await?
        .ok_or(AppError::NotFound)?;
    let ops: Vec<Operation> = sqlx::query_as("SELECT p.* FROM order_payments p LEFT JOIN orders o ON o.workspace_id=p.workspace_id AND o.id=p.order_id WHERE p.workspace_id=$1 AND p.provider='globadyme' AND (p.invoice_id=$2 OR p.credit_note_id=$2 OR o.invoice_id=$2) ORDER BY p.created_at DESC LIMIT 500")
        .bind(access.workspace_id).bind(invoice).fetch_all(&state.db).await?;
    Ok(Data(Value::Array(ops.iter().map(engine::view).collect())))
}

#[utoipa::path(
    post, path = "/workspace/{workspace}/invoices/{invoice}/refunds", tag = "invoices",
    operation_id = "invoices_gateway_refund", summary = "Refund a credit note through its source payment",
    params(("workspace" = Uuid, Path), ("invoice" = Uuid, Path), ("Idempotency-Key" = String, Header)), request_body = RefundRequest,
    responses((status = 201, body = Envelope<Value>, description = "Refund reservation created."), (status = 403, body = MessageBody, description = "Required owner permission is missing."), (status = 404, body = MessageBody, description = "Unknown token, payment, invoice or workspace."), (status = 409, body = MessageBody, description = "Operation conflicts with current payment state."), (status = 422, body = UnprocessableBody, description = "Invalid request fields.")), security(("bearer" = []))
)]
pub async fn refund(
    State(state): State<AppState>,
    access: Can<Refund>,
    Path((_, invoice)): Path<(String, String)>,
    Keyed { input, key }: Keyed<RefundRequest>,
) -> Result<Answer, AppError> {
    create_refund(
        &state,
        access.workspace_id,
        access.user_id,
        parse(&invoice)?,
        input,
        key,
    )
    .await
}

async fn create_refund(
    state: &AppState,
    ws: Uuid,
    user: Uuid,
    note_id: Uuid,
    input: RefundInput,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    if key.is_none() {
        return Err(AppError::field(
            "Idempotency-Key",
            "The Idempotency-Key header is required.",
        ));
    }
    let mut tx = state.db.begin().await?;
    if let Some(answer) = idempotency::replay(&mut tx, ws, Actor::User(user), key.as_ref()).await? {
        return Ok(answer);
    }
    let note = repo::find(&mut *tx, ws, note_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let source_id = note.previous_invoice_id.ok_or(AppError::Conflict(
        "A refund requires a source-linked credit note.",
    ))?;
    let candidate = operations::find(&mut *tx, ws, input.payment_id)
        .await?
        .filter(|op| op.kind == "payment" && op.provider == "globadyme")
        .ok_or(AppError::NotFound)?;
    if let Some(order) = candidate.order_id {
        let order = orders::repo::lock(&mut tx, ws, order)
            .await?
            .ok_or(AppError::NotFound)?;
        if order.invoice_id != Some(source_id) {
            return Err(AppError::NotFound);
        }
    } else if candidate.invoice_id != Some(source_id) {
        return Err(AppError::NotFound);
    }
    let mut ids = [source_id, note_id];
    ids.sort_unstable();
    for id in ids {
        ledger::lock(&mut tx, ws, id)
            .await?
            .ok_or(AppError::NotFound)?;
        invoices::approval::check(&mut tx, ws, id, None).await?;
    }
    let note = ledger::lock(&mut tx, ws, note_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let source = ledger::lock(&mut tx, ws, source_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !note.is_credit
        || note.invoice_number.is_none()
        || note.status != "pending"
        || note.relation_id != source.relation_id
        || note.currency_id != source.currency_id
    {
        return Err(AppError::Conflict(
            "Only a sent pending credit note of the source invoice can be refunded.",
        ));
    }
    let parent: Operation = sqlx::query_as("SELECT p.* FROM order_payments p LEFT JOIN orders o ON o.workspace_id=p.workspace_id AND o.id=p.order_id WHERE p.workspace_id=$1 AND p.id=$2 AND (p.invoice_id=$3 OR o.invoice_id=$3) AND p.kind='payment' AND p.provider='globadyme' AND p.status='paid' FOR UPDATE OF p")
        .bind(ws).bind(input.payment_id).bind(source_id).fetch_optional(&mut *tx).await?.ok_or(AppError::NotFound)?;
    if parent.attention || parent.captured_transaction_id.is_none() {
        return Err(AppError::Conflict(
            "Payment requires reconciliation before refunding.",
        ));
    }
    let decimals = u32::try_from(parent.decimals)
        .map_err(|_| AppError::Conflict("Payment currency is invalid."))?;
    if note.dp != Some(parent.decimals) || source.dp != Some(parent.decimals) {
        return Err(AppError::Conflict(
            "The credit note currency decimals differ from the source payment.",
        ));
    }
    let amount = money::minor(input.amount, decimals)?;
    let source_reserved: i64 = sqlx::query_scalar("SELECT COALESCE(sum(amount),0)::bigint FROM order_payments WHERE workspace_id=$1 AND parent_id=$2 AND kind='refund' AND status NOT IN ('failed','canceled')")
        .bind(ws).bind(parent.id).fetch_one(&mut *tx).await?;
    let note_reserved: i64 = sqlx::query_scalar("SELECT COALESCE(sum(amount),0)::bigint FROM order_payments WHERE workspace_id=$1 AND credit_note_id=$2 AND kind='refund' AND status IN ('creating','open','unknown')")
        .bind(ws).bind(note_id).fetch_one(&mut *tx).await?;
    let available = parent
        .amount
        .checked_sub(source_reserved)
        .and_then(|remaining| {
            note.paid_total
                .checked_sub(note.total)
                .and_then(|open| open.checked_sub(note_reserved))
                .map(|open| open.min(remaining))
        })
        .ok_or(AppError::Conflict("Refund balance is out of range."))?;
    if amount <= 0 || amount > available {
        return Err(AppError::field(
            "amount",
            format!(
                "At most {} can be refunded.",
                Decimal::from_i128_with_scale(i128::from(available.max(0)), decimals)
            ),
        ));
    }
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO order_payments (id,workspace_id,order_id,invoice_id,credit_note_id,parent_id,kind,provider,environment,merchant_id,account_revision,request_id,status,amount,currency,decimals,customer_id,country,payer_name,payer_email,captured_transaction_id,created_by) SELECT $1,workspace_id,order_id,invoice_id,$3,id,'refund',provider,environment,merchant_id,account_revision,$1,'creating',$4,currency,decimals,customer_id,country,payer_name,payer_email,captured_transaction_id,$5 FROM order_payments WHERE workspace_id=$2 AND id=$6")
        .bind(id).bind(ws).bind(note_id).bind(amount).bind(user).bind(parent.id).execute(&mut *tx).await?;
    let answer = Answer::created(
        &json!({"data":{"refund_id":id,"amount":amount,"currency":parent.currency,"decimals":parent.decimals,"status":"creating"}}),
    );
    idempotency::finish(&mut tx, ws, key.as_ref(), &answer).await?;
    tx.commit().await?;
    // A failed network call remains a durable creating request for the shared sweep.
    if let Err(error) = engine::submit(state, ws, id, "").await {
        tracing::warn!(operation=%id, error=%error, "invoice refund submit deferred");
    }
    Ok(answer)
}

async fn token_target(state: &AppState, plain: &str) -> Result<(Uuid, Uuid), AppError> {
    if !token::looks_valid(plain) {
        return Err(AppError::NotFound);
    }
    let (ws, invoice) = repo::by_token(&state.db, &token::hash(plain))
        .await?
        .ok_or(AppError::NotFound)?;
    workspaces::repo::record(&state.db, ws)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok((ws, invoice))
}

async fn latest(state: &AppState, ws: Uuid, invoice: Uuid) -> Result<Option<Uuid>, AppError> {
    Ok(sqlx::query_scalar("SELECT id FROM order_payments WHERE workspace_id=$1 AND invoice_id=$2 AND provider='globadyme' AND kind='payment' ORDER BY created_at DESC,id DESC LIMIT 1")
        .bind(ws).bind(invoice).fetch_optional(&state.db).await?)
}

/// Recovered creates rebuild this URL from the sealed invoice token, never persisted URLs.
pub async fn redirect(state: &AppState, ws: Uuid, invoice: Uuid) -> Result<String, AppError> {
    let row = repo::find(&state.db, ws, invoice)
        .await?
        .ok_or(AppError::NotFound)?;
    let token = send::payment_token(state, ws, &row).ok_or(AppError::Unavailable(
        "Invoice payment link is unavailable.",
    ))?;
    Ok(send::payment_link(state, &token))
}

/// Called by the gateway engine after invoice locks, before committing its paid status.
pub async fn apply_collection(
    conn: &mut PgConnection,
    ws: Uuid,
    op: &Operation,
    transaction_id: &str,
) -> Result<(), AppError> {
    let invoice = op
        .invoice_id
        .ok_or(AppError::Conflict("Payment has no invoice."))?;
    let target = ledger::lock(conn, ws, invoice)
        .await?
        .ok_or(AppError::NotFound)?;
    if receipt_exists(conn, ws, op.id).await? {
        return Ok(());
    }
    let open = target.total.checked_sub(target.paid_total);
    let attention = target.is_credit
        || !invoices::actions::PAYABLE.contains(&target.status.as_str())
        || open.is_none_or(|open| op.amount > open);
    receipt(conn, ws, invoice, op, transaction_id, op.amount).await?;
    payments::rebalance(conn, ws, &target, None).await?;
    if attention {
        mark_attention(
            conn,
            ws,
            op.id,
            "Invoice changed or collection exceeds its outstanding balance.",
        )
        .await?;
    }
    Ok(())
}

/// A confirmed refund books only on its credit note; the source retains its original receipt.
pub async fn apply_refund(
    conn: &mut PgConnection,
    ws: Uuid,
    op: &Operation,
    transaction_id: &str,
) -> Result<(), AppError> {
    let note = op
        .credit_note_id
        .ok_or(AppError::Conflict("Refund has no credit note."))?;
    let target = ledger::lock(conn, ws, note)
        .await?
        .ok_or(AppError::NotFound)?;
    if receipt_exists(conn, ws, op.id).await? {
        return Ok(());
    }
    let amount = op
        .amount
        .checked_neg()
        .ok_or(AppError::Conflict("Refund amount is out of range."))?;
    receipt(conn, ws, note, op, transaction_id, amount).await?;
    payments::rebalance(conn, ws, &target, None).await?;
    if !target.is_credit
        || target.status != "pending"
        || target
            .paid_total
            .checked_sub(target.total)
            .is_none_or(|open| op.amount > open)
    {
        mark_attention(
            conn,
            ws,
            op.id,
            "Credit note changed before its refund completed.",
        )
        .await?;
    }
    Ok(())
}

async fn receipt_exists(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM invoice_payments WHERE workspace_id=$1 AND id=$2)",
    )
    .bind(ws)
    .bind(id)
    .fetch_one(conn)
    .await?)
}

async fn receipt(
    conn: &mut PgConnection,
    ws: Uuid,
    invoice: Uuid,
    op: &Operation,
    transaction_id: &str,
    amount: i64,
) -> Result<(), AppError> {
    ledger::insert(
        conn,
        ws,
        &[ledger::NewPayment {
            id: op.id,
            invoice_id: invoice,
            user_id: None,
            date: Utc::now().date_naive(),
            amount,
            description: Some(if op.kind == "refund" {
                "Gateway refund"
            } else {
                "Gateway collection"
            }),
            payment_method: "provider",
            provider: Some("globadyme"),
            provider_id: Some(transaction_id),
            meta: None,
            settle_twin_id: None,
        }],
    )
    .await?;
    Ok(())
}

async fn mark_attention(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    reason: &str,
) -> Result<(), AppError> {
    sqlx::query("UPDATE order_payments SET attention=true,attention_reason=$3,updated_at=now() WHERE workspace_id=$1 AND id=$2").bind(ws).bind(id).bind(reason).execute(conn).await?;
    Ok(())
}

fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

#[cfg(test)]
mod tests {
    #[test]
    fn invoice_and_order_refunds_have_distinct_contracts() {
        let spec = crate::openapi::spec().unwrap();
        let request = |path: &str| {
            spec["paths"][path]["post"]["requestBody"]["content"]["application/json"]["schema"]["$ref"].as_str().unwrap().to_owned()
        };
        let invoice = request("/workspace/{workspace}/invoices/{invoice}/refunds");
        let order = request("/workspace/{workspace}/orders/{order}/refunds");
        assert_ne!(invoice, order);
        assert!(
            spec["components"]["schemas"]["InvoiceGatewayRefundRequest"]["properties"]
                .get("payment_id")
                .is_some()
        );
        assert!(
            spec["components"]["schemas"]["RefundRequest"]["properties"]
                .get("payment_id")
                .is_none()
        );
    }

    use super::*;
    #[test]
    fn requests_reject_invalid_identity_and_nonpositive_money() {
        let mut rules = Rules::default();
        CollectionRequest {
            country: Some("XX".into()),
            payer_name: Some(" ".into()),
            payer_email: Some("invalid".into()),
        }
        .validate(&mut rules);
        assert!(rules.into_result().is_err());
        for amount in [json!("0"), json!("-1"), json!("1e2")] {
            let mut rules = Rules::default();
            RefundRequest {
                payment_id: Some(Uuid::now_v7().to_string()),
                amount: Some(amount),
            }
            .validate(&mut rules);
            assert!(rules.into_result().is_err());
        }
    }
}
