//! HTTP edge for invoice payments (`invoicepayment.*`, `invoice.credit`). Every write takes
//! an optional Idempotency-Key.

use axum::extract::{Path, State};

use crate::{
    AppState,
    error::{AppError, UnprocessableBody},
    http::{
        response::{Envelope, MessageBody},
        workspace::{Can, permissions},
    },
    idempotency::{Answer, KeyOnly, Keyed},
    modules::invoice_payments::{
        dto::{CreatePaymentRequest, MarkCreditedRequest, PaymentCreated},
        service,
    },
};

permissions! {
    PaymentCreate => "invoicepayment.create",
    PaymentDelete => "invoicepayment.delete",
    MarkCredited => "invoice.credit",
}

/// POST /invoices/{id}/payments: 201 `{data: {invoice_payment_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/invoices/{invoice}/payments", tag = "invoice_payments",
    operation_id = "invoice_payments_create", summary = "Record a payment on an invoice",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("invoice" = Uuid, Path, description = "Invoice id"), ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters. A retry with the same key and request replays the first answer; another request under the key is a 422. Kept for a day.")),
    request_body = CreatePaymentRequest,
    responses(
        (status = 201, body = Envelope<PaymentCreated>, description = "Recorded; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `invoicepayment.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or invoice, or caller not a member"),
        (status = 422, body = UnprocessableBody, description = "Invalid input or Idempotency-Key, key reused for another request, invoice not open, credit note, amount invalid or too many payments"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<PaymentCreate>,
    Path((_, id)): Path<(String, String)>,
    Keyed { input, key }: Keyed<CreatePaymentRequest>,
) -> Result<Answer, AppError> {
    service::create(&state, access.workspace_id, access.user_id, &id, input, key).await
}

/// DELETE /invoices/{id}/payments/{pid}: voids it.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/invoices/{invoice}/payments/{payment}", tag = "invoice_payments",
    operation_id = "invoice_payments_void", summary = "Void a payment",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("invoice" = Uuid, Path, description = "Invoice id"), ("payment" = Uuid, Path, description = "Payment id"), ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters. A retry with the same key and request replays the first answer; another request under the key is a 422. Kept for a day.")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `invoicepayment.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, invoice or payment, or caller not a member"),
        (status = 422, body = UnprocessableBody, description = "Invalid Idempotency-Key, or key reused for another request"),
    ),
    security(("bearer" = []))
)]
pub async fn void(
    State(state): State<AppState>,
    access: Can<PaymentDelete>,
    Path((_, id, payment)): Path<(String, String, String)>,
    KeyOnly(key): KeyOnly,
) -> Result<Answer, AppError> {
    service::void(
        &state,
        access.workspace_id,
        access.user_id,
        &id,
        &payment,
        key,
    )
    .await
}

/// POST /invoices/{id}/mark-as-credited.
#[utoipa::path(
    post, path = "/workspace/{workspace}/invoices/{invoice}/mark-as-credited", tag = "invoice_payments",
    operation_id = "invoice_payments_mark_as_credited", summary = "Mark a pending credit note as paid out",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("invoice" = Uuid, Path, description = "Invoice id"), ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters. A retry with the same key and request replays the first answer; another request under the key is a 422. Kept for a day.")),
    request_body = MarkCreditedRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 400, body = MessageBody, description = "Only a pending credit note can be marked as credited"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `invoice.credit`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or invoice, or caller not a member"),
        (status = 422, body = UnprocessableBody, description = "Invalid input or Idempotency-Key, or key reused for another request"),
    ),
    security(("bearer" = []))
)]
pub async fn mark_as_credited(
    State(state): State<AppState>,
    access: Can<MarkCredited>,
    Path((_, id)): Path<(String, String)>,
    Keyed { input, key }: Keyed<MarkCreditedRequest>,
) -> Result<Answer, AppError> {
    service::mark_as_credited(&state, access.workspace_id, access.user_id, &id, input, key).await
}
