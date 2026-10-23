//! HTTP edge for invoice payments (`invoicepayment.*`, `invoice.credit`). Every write takes
//! an optional Idempotency-Key.

use axum::extract::{Path, State};

use crate::{
    AppState,
    error::AppError,
    http::workspace::{Can, permissions},
    idempotency::{Answer, KeyOnly, Keyed},
    modules::invoice_payments::{
        dto::{CreatePaymentRequest, MarkCreditedRequest},
        service,
    },
};

permissions! {
    PaymentCreate => "invoicepayment.create",
    PaymentDelete => "invoicepayment.delete",
    MarkCredited => "invoice.credit",
}

/// POST /invoices/{id}/payments: 201 `{data: {invoice_payment_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<PaymentCreate>,
    Path((_, id)): Path<(String, String)>,
    Keyed { input, key }: Keyed<CreatePaymentRequest>,
) -> Result<Answer, AppError> {
    service::create(&state, access.workspace_id, access.user_id, &id, input, key).await
}

/// DELETE /invoices/{id}/payments/{pid}: voids it.
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
pub async fn mark_as_credited(
    State(state): State<AppState>,
    access: Can<MarkCredited>,
    Path((_, id)): Path<(String, String)>,
    Keyed { input, key }: Keyed<MarkCreditedRequest>,
) -> Result<Answer, AppError> {
    service::mark_as_credited(&state, access.workspace_id, access.user_id, &id, input, key).await
}
