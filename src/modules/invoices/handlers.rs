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
    error::AppError,
    http::{
        json::{Rules, ValidatedJson},
        list::{self, Page, Paginated},
        response::{Data, Message},
        workspace::{Can, permissions},
    },
    modules::invoices::{
        dto::{CreateInvoiceRequest, InvoiceCreated, ListQuery, UpdateInvoiceRequest},
        repo::ListFilter,
        service::{self, ListArgs},
    },
};

permissions! {
    InvoiceList => "invoice.list",
    InvoiceView => "invoice.view",
    InvoiceCreate => "invoice.create",
    InvoiceUpdate => "invoice.update",
    InvoiceDelete => "invoice.delete",
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
pub async fn get(
    State(state): State<AppState>,
    access: Can<InvoiceView>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(service::get(&state, access.workspace_id, &id).await?))
}

/// POST /invoices -> 201 `{data:{invoice_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<InvoiceCreate>,
    ValidatedJson(input): ValidatedJson<CreateInvoiceRequest>,
) -> Result<(StatusCode, Data<InvoiceCreated>), AppError> {
    let invoice_id = service::create(&state, access.workspace_id, access.user_id, input).await?;
    Ok((StatusCode::CREATED, Data(InvoiceCreated { invoice_id })))
}

/// POST /invoices/{id}.
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
pub async fn delete(
    State(state): State<AppState>,
    access: Can<InvoiceDelete>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Message, AppError> {
    service::cancel(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}

/// POST /invoices/{id}/duplicate -> 201 `{data:{invoice_id}}` (needs `invoice.create`).
pub async fn duplicate(
    State(state): State<AppState>,
    access: Can<InvoiceCreate>,
    Path((_, id)): Path<(String, String)>,
) -> Result<(StatusCode, Data<InvoiceCreated>), AppError> {
    let invoice_id = service::duplicate(&state, access.workspace_id, access.user_id, &id).await?;
    Ok((StatusCode::CREATED, Data(InvoiceCreated { invoice_id })))
}
