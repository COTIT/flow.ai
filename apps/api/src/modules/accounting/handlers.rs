//! HTTP edge for /v1/workspace/{workspace}/accounting. Every route needs an `accounting.*`
//! permission (checked before the body or any replay is read). Writes take the C/A envelope
//! plus a required Idempotency-Key and replay permanently by `client_ref`.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
        workspace::{Can, permissions},
    },
    idempotency::{Answer, Command},
    modules::accounting::{
        dto::{
            AccountPatch, AccountPayload, AccountResource, Action, Create, JournalPayload,
            JournalResource, ListQuery, PeriodPayload, PeriodResource, PostPayload, ReversePayload,
            ReverseReviewBody, ReviewResource, SetupPayload, SetupResource, YearPayload,
            YearResource,
        },
        journals, service,
    },
};

permissions! {
    Read => "accounting.read",
    Configure => "accounting.configure",
    JournalCreate => "accounting.journal_create",
    Post => "accounting.post",
    Reverse => "accounting.reverse",
}

/// A path id; malformed reads as missing.
fn id(raw: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(raw).map_err(|_| AppError::NotFound)
}

/// accounting_setup_get: Read the workspace book's setup.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/setup", tag = "accounting",
    operation_id = "accounting_setup_get",
    summary = "Read the workspace book's setup",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<SetupResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn setup_get(
    State(state): State<AppState>,
    access: Can<Read>,
) -> Result<Data<SetupResource>, AppError> {
    let mut conn = state.db.acquire().await?;
    Ok(Data(service::setup(&mut conn, access.workspace_id).await?))
}

/// accounting_setup_update: Change setup (currency, timezone, role mappings) before activation.
#[utoipa::path(
    put, path = "/workspace/{workspace}/accounting/setup", tag = "accounting",
    operation_id = "accounting_setup_update",
    summary = "Change setup (currency, timezone, role mappings) before activation",
    request_body = Action<SetupPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<SetupResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document`, reused client_ref, or already reversed"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `accounting_not_configured` / `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn setup_update(
    State(state): State<AppState>,
    access: Can<Configure>,
    cmd: Command<Action<SetupPayload>>,
) -> Result<Answer, AppError> {
    service::update_setup(&state, &access, cmd).await
}

/// accounting_accounts_list: List the chart of accounts.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/accounts", tag = "accounting",
    operation_id = "accounting_accounts_list",
    summary = "List the chart of accounts",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<AccountResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn accounts_list(
    State(state): State<AppState>,
    access: Can<Read>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<AccountResource>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::accounts(&state, access.workspace_id, page).await?;
    Ok(Json(Paginated::new(rows, total, page)))
}

/// accounting_accounts_create: Create a ledger account.
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/accounts", tag = "accounting",
    operation_id = "accounting_accounts_create",
    summary = "Create a ledger account",
    request_body = Create<AccountPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<AccountResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document`, reused client_ref, or already reversed"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `accounting_not_configured` / `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn accounts_create(
    State(state): State<AppState>,
    access: Can<Configure>,
    cmd: Command<Create<AccountPayload>>,
) -> Result<Answer, AppError> {
    service::create_account(&state, &access, cmd).await
}

/// accounting_accounts_update: Rename or (de)activate a ledger account.
#[utoipa::path(
    patch, path = "/workspace/{workspace}/accounting/accounts/{account}", tag = "accounting",
    operation_id = "accounting_accounts_update",
    summary = "Rename or (de)activate a ledger account",
    request_body = Action<AccountPatch>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("account" = Uuid, Path, description = "Account id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<AccountResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document`, reused client_ref, or already reversed"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `accounting_not_configured` / `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn accounts_update(
    State(state): State<AppState>,
    access: Can<Configure>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<AccountPatch>>,
) -> Result<Answer, AppError> {
    service::update_account(&state, &access, id(&raw)?, cmd).await
}

/// accounting_years_list: List fiscal years.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/fiscal-years", tag = "accounting",
    operation_id = "accounting_years_list",
    summary = "List fiscal years",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<YearResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn years_list(
    State(state): State<AppState>,
    access: Can<Read>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<YearResource>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::years(&state, access.workspace_id, page).await?;
    Ok(Json(Paginated::new(rows, total, page)))
}

/// accounting_years_create: Create a fiscal year.
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/fiscal-years", tag = "accounting",
    operation_id = "accounting_years_create",
    summary = "Create a fiscal year",
    request_body = Create<YearPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<YearResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document`, reused client_ref, or already reversed"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `accounting_not_configured` / `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn years_create(
    State(state): State<AppState>,
    access: Can<Configure>,
    cmd: Command<Create<YearPayload>>,
) -> Result<Answer, AppError> {
    service::create_year(&state, &access, cmd).await
}

/// accounting_periods_list: List posting periods.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/periods", tag = "accounting",
    operation_id = "accounting_periods_list",
    summary = "List posting periods",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<PeriodResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn periods_list(
    State(state): State<AppState>,
    access: Can<Read>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<PeriodResource>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::periods(&state, access.workspace_id, page).await?;
    Ok(Json(Paginated::new(rows, total, page)))
}

/// accounting_periods_create: Create a posting period inside a fiscal year.
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/periods", tag = "accounting",
    operation_id = "accounting_periods_create",
    summary = "Create a posting period inside a fiscal year",
    request_body = Create<PeriodPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<PeriodResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document`, reused client_ref, or already reversed"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `accounting_not_configured` / `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn periods_create(
    State(state): State<AppState>,
    access: Can<Configure>,
    cmd: Command<Create<PeriodPayload>>,
) -> Result<Answer, AppError> {
    service::create_period(&state, &access, cmd).await
}

/// accounting_journals_list: List journals, newest date first.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/journals", tag = "accounting",
    operation_id = "accounting_journals_list",
    summary = "List journals, newest date first",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<JournalResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn journals_list(
    State(state): State<AppState>,
    access: Can<Read>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<JournalResource>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = journals::list(&state, access.workspace_id, page).await?;
    Ok(Json(Paginated::new(rows, total, page)))
}

/// accounting_journals_create: Create a manual draft journal.
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/journals", tag = "accounting",
    operation_id = "accounting_journals_create",
    summary = "Create a manual draft journal",
    request_body = Create<JournalPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<JournalResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document`, reused client_ref, or already reversed"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `accounting_not_configured` / `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn journals_create(
    State(state): State<AppState>,
    access: Can<JournalCreate>,
    cmd: Command<Create<JournalPayload>>,
) -> Result<Answer, AppError> {
    journals::create(&state, &access, cmd).await
}

/// accounting_journals_get: Read one journal with its lines.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/journals/{journal}", tag = "accounting",
    operation_id = "accounting_journals_get",
    summary = "Read one journal with its lines",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("journal" = Uuid, Path, description = "Journal id")),
    responses(
        (status = 200, body = Envelope<JournalResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn journals_get(
    State(state): State<AppState>,
    access: Can<Read>,
    Path((_, raw)): Path<(String, String)>,
) -> Result<Data<JournalResource>, AppError> {
    let mut conn = state.db.acquire().await?;
    Ok(Data(
        journals::get(&mut conn, access.workspace_id, id(&raw)?).await?,
    ))
}

/// accounting_journals_update: Replace a manual draft journal.
#[utoipa::path(
    patch, path = "/workspace/{workspace}/accounting/journals/{journal}", tag = "accounting",
    operation_id = "accounting_journals_update",
    summary = "Replace a manual draft journal",
    request_body = Action<JournalPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("journal" = Uuid, Path, description = "Journal id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<JournalResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document`, reused client_ref, or already reversed"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `accounting_not_configured` / `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn journals_update(
    State(state): State<AppState>,
    access: Can<JournalCreate>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<JournalPayload>>,
) -> Result<Answer, AppError> {
    journals::update(&state, &access, id(&raw)?, cmd).await
}

/// accounting_journals_post_review: Preview posting a draft (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/journals/{journal}/post/review", tag = "accounting",
    operation_id = "accounting_journals_post_review",
    summary = "Preview posting a draft (no writes)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("journal" = Uuid, Path, description = "Journal id")),
    responses(
        (status = 200, body = Envelope<ReviewResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document`, reused client_ref, or already reversed"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `accounting_not_configured` / `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn journals_post_review(
    State(state): State<AppState>,
    access: Can<Post>,
    Path((_, raw)): Path<(String, String)>,
) -> Result<Data<ReviewResource>, AppError> {
    Ok(Data(
        journals::post_review(&state, access.workspace_id, id(&raw)?).await?,
    ))
}

/// accounting_journals_post: Post a reviewed manual draft.
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/journals/{journal}/post", tag = "accounting",
    operation_id = "accounting_journals_post",
    summary = "Post a reviewed manual draft",
    request_body = Action<PostPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("journal" = Uuid, Path, description = "Journal id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<JournalResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document`, reused client_ref, or already reversed"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `accounting_not_configured` / `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn journals_post(
    State(state): State<AppState>,
    access: Can<Post>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<PostPayload>>,
) -> Result<Answer, AppError> {
    journals::post(&state, &access, id(&raw)?, cmd).await
}

/// accounting_journals_reverse_review: Preview reversing a posted manual journal (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/journals/{journal}/reverse/review", tag = "accounting",
    operation_id = "accounting_journals_reverse_review",
    summary = "Preview reversing a posted manual journal (no writes)",
    request_body = ReverseReviewBody,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("journal" = Uuid, Path, description = "Journal id")),
    responses(
        (status = 200, body = Envelope<ReviewResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document`, reused client_ref, or already reversed"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `accounting_not_configured` / `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn journals_reverse_review(
    State(state): State<AppState>,
    access: Can<Reverse>,
    Path((_, raw)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<ReverseReviewBody>,
) -> Result<Data<ReviewResource>, AppError> {
    Ok(Data(
        journals::reverse_review(&state, access.workspace_id, id(&raw)?, &input).await?,
    ))
}

/// accounting_journals_reverse: Reverse a posted manual journal into an open period.
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/journals/{journal}/reverse", tag = "accounting",
    operation_id = "accounting_journals_reverse",
    summary = "Reverse a posted manual journal into an open period",
    request_body = Action<ReversePayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("journal" = Uuid, Path, description = "Journal id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<JournalResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document`, reused client_ref, or already reversed"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `accounting_not_configured` / `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn journals_reverse(
    State(state): State<AppState>,
    access: Can<Reverse>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<ReversePayload>>,
) -> Result<Answer, AppError> {
    journals::reverse(&state, &access, id(&raw)?, cmd).await
}
