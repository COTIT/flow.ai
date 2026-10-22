//! HTTP edge for /v1/workspace/{workspace}/email-templates (`emailtemplate.*`).

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::Value;

use crate::{
    AppState,
    error::AppError,
    http::{
        json::{Rules, ValidatedJson},
        list::{self, Page, Paginated},
        response::{Data, Message},
        workspace::{Can, WorkspaceAccess, permissions},
    },
    modules::{
        email_templates::{
            dto::{
                CreateTemplateRequest, DOMAINS, ListQuery, TemplateCreated, UpdateTemplateRequest,
            },
            repo::ListFilter,
            service,
        },
        pdf_templates::fields,
    },
};

// `get`, as the catalog lists it (Laravel's policy checked `view`, which no team could grant).
permissions! {
    TemplateList => "emailtemplate.list",
    TemplateView => "emailtemplate.get",
    TemplateCreate => "emailtemplate.create",
    TemplateUpdate => "emailtemplate.update",
    TemplateDelete => "emailtemplate.delete",
}

/// GET /email-templates.
pub async fn list(
    State(state): State<AppState>,
    access: Can<TemplateList>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<Value>>, AppError> {
    let blank = |v: &Option<String>| {
        v.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let domain = blank(&q.template_domain).map(|d| d.to_lowercase());
    let mut r = Rules::default();
    if let Some(d) = &domain
        && !DOMAINS.contains(&d.as_str())
    {
        r.add(
            "template_domain",
            format!("The selected template domain: {d} is invalid."),
        );
    }
    r.into_result()?;
    let search = list::search(q.q.as_deref())?;
    let (relation_type, language, custom_type) = (
        blank(&q.relation_type),
        blank(&q.language),
        blank(&q.relation_custom_type_id),
    );
    let filter = ListFilter {
        domain: domain.as_deref(),
        search: search.as_deref(),
        relation_type: relation_type.as_deref(),
        language: language.as_deref(),
        relation_custom_type_id: custom_type.as_deref(),
    };
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        &filter,
        page.size,
        page.offset(),
    )
    .await?;
    Ok(Json(Paginated::new(rows, total, page)))
}

/// GET /email-templates/variables/{domain}: Laravel's variable lists (the PDF templates'
/// generated ones, served as written), `[]` for a domain without any. Laravel checked no
/// permission here, only membership.
pub async fn variables(
    _access: WorkspaceAccess,
    Path((_, domain)): Path<(String, String)>,
) -> Response {
    let body = match fields::variables_raw(&domain) {
        Some(raw) => format!("{{\"data\":{{\"variables\":{raw}}}}}"),
        None => r#"{"data":{"variables":[]}}"#.to_owned(),
    };
    ([(header::CONTENT_TYPE, "application/json")], body).into_response()
}

/// GET /email-templates/{id}.
pub async fn get(
    State(state): State<AppState>,
    access: Can<TemplateView>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(service::get(&state, access.workspace_id, &id).await?))
}

/// POST /email-templates -> 201 `{data:{email_template_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<TemplateCreate>,
    ValidatedJson(input): ValidatedJson<CreateTemplateRequest>,
) -> Result<(StatusCode, Data<TemplateCreated>), AppError> {
    let email_template_id = service::create(&state, access.workspace_id, input).await?;
    Ok((
        StatusCode::CREATED,
        Data(TemplateCreated { email_template_id }),
    ))
}

/// POST /email-templates/{id}.
pub async fn update(
    State(state): State<AppState>,
    access: Can<TemplateUpdate>,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<UpdateTemplateRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, input).await?;
    Ok(Message("ok"))
}

/// DELETE /email-templates/{id}.
pub async fn delete(
    State(state): State<AppState>,
    access: Can<TemplateDelete>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
