//! HTTP edge for /v1/workspace/{workspace}/invoices (`invoice.*`).

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, UnprocessableBody, ValidationErrorBody},
    http::{
        json::{Rules, ValidatedJson},
        list::{self, Page, Paginated},
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, Permission, permissions},
    },
    modules::{
        files::dto::FileResource,
        invoices::{
            credit,
            dto::{
                CreateInvoiceRequest, CreditInvoiceRequest, InvoiceCreated, InvoiceListPage,
                InvoiceResource, ListQuery, PublicInvoice, UpdateInvoiceRequest,
            },
            repo::ListFilter,
            send,
            service::{self, ListArgs},
        },
        messages::dto::SendMessageRequest,
    },
};

permissions! {
    InvoiceList => "invoice.list",
    InvoiceView => "invoice.view",
    InvoiceCreate => "invoice.create",
    InvoiceUpdate => "invoice.update",
    InvoiceDelete => "invoice.delete",
}

/// `invoice.send` plus `workspace.send-message` (Laravel's policy + SendMessageRequest).
pub struct InvoiceSend;

impl Permission for InvoiceSend {
    const NAME: &'static str = "invoice.send";
    const ALSO: Option<&'static str> = Some("workspace.send-message");
}

/// `invoice.credit` plus `invoice.create` (Laravel's credit policy + the new invoice).
pub struct InvoiceCredit;

impl Permission for InvoiceCredit {
    const NAME: &'static str = "invoice.credit";
    const ALSO: Option<&'static str> = Some("invoice.create");
}

/// Laravel InvoiceStatus values (`in_status` entries must be one).
const STATUSES: &[&str] = &[
    "awaiting_payment",
    "awaiting_pos_payment",
    "cancelled",
    "credited",
    "draft",
    "paid",
    "partially_paid",
    "pending",
    "planned",
];

/// GET /invoices: a page (`page`, `page_size`), or `limit` rows without `meta`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/invoices", tag = "invoices",
    operation_id = "invoices_list", summary = "List invoices (a page, or `limit` rows without `meta`)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = InvoiceListPage, description = "Invoices, newest first"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `invoice.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Malformed relation id, limit or status filter, or search text too long"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<InvoiceList>,
    Query(q): Query<ListQuery>,
) -> Result<Response, AppError> {
    let mut r = Rules::default();
    let relation_id = q
        .relation_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let relation_id = relation_id.and_then(|s| {
        let id = Uuid::parse_str(s).ok();
        if id.is_none() {
            r.add("relation_id", "The relation id must be a valid UUID.");
        }
        id
    });
    let limit = q
        .limit
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|s| r.integer("limit", &Value::from(s), 0, 100))
        .filter(|n| *n > 0);
    let status = q.status.as_deref().filter(|s| !s.is_empty());
    if let Some(s) = status {
        r.max_chars("status", s, 100);
    }
    let in_status: Option<Vec<String>> = q.in_status.as_deref().map(|csv| {
        csv.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_lowercase)
            .collect()
    });
    for (n, s) in in_status.iter().flatten().enumerate() {
        if !STATUSES.contains(&s.as_str()) {
            r.add(
                &format!("in_status.{n}"),
                format!("The selected in_status.{n} is invalid."),
            );
        }
    }
    r.into_result()?;
    let search = list::search(q.q.as_deref())?;
    let args = ListArgs {
        filter: ListFilter {
            relation_id,
            status,
            in_status: in_status.as_deref().filter(|s| !s.is_empty()),
            is_credit: q
                .is_credit
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(list::flag),
            on_account: q.on_account.as_deref().is_some_and(list::flag),
            search: search.as_deref(),
        },
        limit,
        page: Page::parse(q.page.as_deref(), q.page_size.as_deref()),
    };
    let (rows, total) = service::list(&state, access.workspace_id, &args).await?;
    Ok(match limit {
        Some(_) => Json(json!({ "data": rows })).into_response(),
        None => Json(Paginated::new(rows, total, args.page)).into_response(),
    })
}

/// GET /invoices/{id}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/invoices/{invoice}", tag = "invoices",
    operation_id = "invoices_get", summary = "Get an invoice with lines, totals and payments",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("invoice" = Uuid, Path, description = "Invoice id")),
    responses(
        (status = 200, body = Envelope<InvoiceResource>, description = "The invoice"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `invoice.view`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or invoice, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<InvoiceView>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(service::get(&state, access.workspace_id, &id).await?))
}

/// POST /invoices -> 201 `{data:{invoice_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/invoices", tag = "invoices",
    operation_id = "invoices_create", summary = "Create a draft invoice",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateInvoiceRequest,
    responses(
        (status = 201, body = Envelope<InvoiceCreated>, description = "Created; returns the new invoice id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `invoice.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or unknown relation, currency, contact, product or warehouse"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<InvoiceCreate>,
    ValidatedJson(input): ValidatedJson<CreateInvoiceRequest>,
) -> Result<(StatusCode, Data<InvoiceCreated>), AppError> {
    let invoice_id = service::create(&state, access.workspace_id, access.user_id, input).await?;
    Ok((StatusCode::CREATED, Data(InvoiceCreated { invoice_id })))
}

/// POST /invoices/{id}.
#[utoipa::path(
    post, path = "/workspace/{workspace}/invoices/{invoice}", tag = "invoices",
    operation_id = "invoices_update", summary = "Update a draft invoice and the lines sent",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("invoice" = Uuid, Path, description = "Invoice id")),
    request_body = UpdateInvoiceRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `invoice.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or invoice, or caller not a member"),
        (status = 422, body = UnprocessableBody, description = "Invalid input, unknown references, or the invoice is no longer a draft"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<InvoiceUpdate>,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<UpdateInvoiceRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, input).await?;
    Ok(Message("ok"))
}

/// DELETE /invoices/{id}: cancels the draft.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/invoices/{invoice}", tag = "invoices",
    operation_id = "invoices_delete", summary = "Cancel a draft invoice",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("invoice" = Uuid, Path, description = "Invoice id")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `invoice.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or invoice, or caller not a member"),
        (status = 422, body = MessageBody, description = "The invoice is no longer a draft"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<InvoiceDelete>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Message, AppError> {
    service::cancel(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}

// `?pdf_template_id=` of the preview (`print_with_ftp` is not ported: no FTP servers).
/// Preview query.
#[derive(serde::Deserialize, Default, utoipa::IntoParams)]
#[serde(default)]
#[into_params(parameter_in = Query)]
pub struct PreviewQuery {
    /// An invoice PDF template of this workspace; the built-in layout when left out or unknown.
    #[param(value_type = Option<Uuid>)]
    pdf_template_id: Option<String>,
}

/// Bounded pending or uncertain delivery tasks.
#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
pub struct SendTasks {
    #[schema(value_type = Vec<Object>, max_items = 100)]
    data: Vec<Value>,
}

/// GET /invoices/{id}/preview -> `{data: FileResource}` of a new PDF.
#[utoipa::path(
    get, path = "/workspace/{workspace}/invoices/{invoice}/preview", tag = "invoices",
    operation_id = "invoices_preview", summary = "Render the invoice as a PDF",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("invoice" = Uuid, Path, description = "Invoice id"), PreviewQuery),
    responses(
        (status = 200, body = Envelope<FileResource>, description = "The rendered PDF, stored as a file"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `invoice.view`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or invoice, or caller not a member"),
        (status = 422, body = UnprocessableBody, description = "Malformed template id, or the renderer refused the document"),
    ),
    security(("bearer" = []))
)]
pub async fn preview(
    State(state): State<AppState>,
    access: Can<InvoiceView>,
    Path((_, id)): Path<(String, String)>,
    Query(q): Query<PreviewQuery>,
) -> Result<Data<FileResource>, AppError> {
    let template = match q
        .pdf_template_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        None => None,
        Some(s) => Some(Uuid::parse_str(s).map_err(|_| {
            AppError::field(
                "pdf_template_id",
                "The pdf template id must be a valid UUID.",
            )
        })?),
    };
    Ok(Data(
        service::preview(&state, access.workspace_id, access.user_id, &id, template).await?,
    ))
}

/// POST /invoices/{id}/duplicate -> 201 `{data:{invoice_id}}` (needs `invoice.create`).
#[utoipa::path(
    post, path = "/workspace/{workspace}/invoices/{invoice}/duplicate", tag = "invoices",
    operation_id = "invoices_duplicate", summary = "Copy an invoice as a new draft",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("invoice" = Uuid, Path, description = "Invoice id")),
    responses(
        (status = 201, body = Envelope<InvoiceCreated>, description = "Created; returns the new invoice id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `invoice.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or invoice, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn duplicate(
    State(state): State<AppState>,
    access: Can<InvoiceCreate>,
    Path((_, id)): Path<(String, String)>,
) -> Result<(StatusCode, Data<InvoiceCreated>), AppError> {
    let invoice_id = service::duplicate(&state, access.workspace_id, access.user_id, &id).await?;
    Ok((StatusCode::CREATED, Data(InvoiceCreated { invoice_id })))
}

/// POST /invoices/{id}/send (Laravel SendInvoiceController).
#[utoipa::path(
    post, path = "/workspace/{workspace}/invoices/{invoice}/send", tag = "invoices",
    operation_id = "invoices_send", summary = "Number, render and email an invoice",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("invoice" = Uuid, Path, description = "Invoice id")),
    request_body = SendMessageRequest,
    responses(
        (status = 200, body = super::delivery::Reply, description = "Numbered invoice and transport receipt"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `invoice.send` / `workspace.send-message`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or invoice, or caller not a member"),
        (status = 422, body = UnprocessableBody, description = "Invalid message, wrong status, unknown contact, template or attachment, no recipient, no stock warehouse, or no usable mail server"),
    ),
    security(("bearer" = []))
)]
pub async fn send(
    State(state): State<AppState>,
    access: Can<InvoiceSend>,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<SendMessageRequest>,
) -> Result<Response, AppError> {
    let receipt = send::send(&state, access.workspace_id, access.user_id, &id, input).await?;
    Ok(delivery_response(receipt))
}

/// POST /invoices/{id}/resend (Laravel ResendInvoiceController).
#[utoipa::path(
    post, path = "/workspace/{workspace}/invoices/{invoice}/resend", tag = "invoices",
    operation_id = "invoices_resend", summary = "Email a sent invoice again",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("invoice" = Uuid, Path, description = "Invoice id")),
    request_body = SendMessageRequest,
    responses(
        (status = 200, body = super::delivery::Reply, description = "Numbered invoice and transport receipt"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `invoice.send` / `workspace.send-message`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or invoice, or caller not a member"),
        (status = 422, body = UnprocessableBody, description = "Invalid message, wrong status, unknown contact, template or attachment, no recipient, no stock warehouse, or no usable mail server"),
    ),
    security(("bearer" = []))
)]
pub async fn resend(
    State(state): State<AppState>,
    access: Can<InvoiceSend>,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<SendMessageRequest>,
) -> Result<Response, AppError> {
    let receipt = send::resend(&state, access.workspace_id, access.user_id, &id, input).await?;
    Ok(delivery_response(receipt))
}

fn delivery_response(receipt: super::delivery::Receipt) -> Response {
    let mut response = Json(super::delivery::Reply {
        message: "ok",
        delivery: receipt.clone(),
    })
    .into_response();
    response.extensions_mut().insert(receipt);
    response
}

/// Pending or uncertain delivery receipts, bounded to this workspace.
#[utoipa::path(
    get, path = "/workspace/{workspace}/invoices/send-tasks/active", tag = "invoices",
    operation_id = "invoices_send_tasks", summary = "Pending or uncertain invoice deliveries",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = SendTasks, description = "At most 100 pending or uncertain deliveries"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `invoice.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn send_tasks(
    State(state): State<AppState>,
    access: Can<InvoiceList>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(
        json!({"data":super::delivery_repo::active(&state.db,access.workspace_id).await?}),
    ))
}

/// GET /v1/invoices/fetch-from-hashed/{token}: public, rate limited.
#[utoipa::path(
    get, path = "/invoices/fetch-from-hashed/{token}", tag = "invoices",
    operation_id = "invoices_public", summary = "Public payment page of a sent invoice (no login)",
    params(("token" = String, Path, description = "Token from the payment link")),
    responses(
        (status = 200, body = Envelope<PublicInvoice>, description = "The invoice as the customer sees it"),
        (status = 404, body = MessageBody, description = "Unknown or revoked token"),
    )
)]
pub async fn public(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(service::public(&state, &token).await?))
}

/// POST /invoices/{id}/credit (Laravel CreditInvoiceController): 201 with the credit note.
#[utoipa::path(
    post, path = "/workspace/{workspace}/invoices/{invoice}/credit", tag = "invoices",
    operation_id = "invoices_credit", summary = "Create a credit note for an invoice",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("invoice" = Uuid, Path, description = "Invoice id")),
    request_body = CreditInvoiceRequest,
    responses(
        (status = 201, body = Envelope<InvoiceCreated>, description = "Created; returns the credit note id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `invoice.credit` / `invoice.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or invoice, or caller not a member"),
        (status = 422, body = UnprocessableBody, description = "Invalid lines or amounts, a credit note, or nothing left to credit"),
    ),
    security(("bearer" = []))
)]
pub async fn credit(
    State(state): State<AppState>,
    access: Can<InvoiceCredit>,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<CreditInvoiceRequest>,
) -> Result<(StatusCode, Data<InvoiceCreated>), AppError> {
    let invoice_id =
        credit::credit(&state, access.workspace_id, access.user_id, &id, &input).await?;
    Ok((StatusCode::CREATED, Data(InvoiceCreated { invoice_id })))
}
