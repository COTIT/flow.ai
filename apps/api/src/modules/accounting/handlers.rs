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
    error::{AppError, UnprocessableBody, ValidationErrorBody},
    http::{
        json::{Rules, Validate, ValidatedJson},
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
        workspace::{Can, permissions},
    },
    idempotency::{Answer, Command},
    modules::accounting::{
        banking::{self, ReconcilePayload, UnmatchPayload},
        close,
        dto::{
            AccountListQuery, AccountPatch, AccountPayload, AccountResource, Action, AgingQuery,
            AgingResource, AsOfQuery, BalanceSheetResource, BankAccountPayload,
            BankAccountResource, CloseChecksQuery, CloseChecksResource, ClosePayload, Create,
            GeneralLedgerResource, InventoryValuationResource, JournalPayload, JournalResource,
            LedgerQuery, ListQuery, PartyLedgerQuery, PartyLedgerResource, PeriodPayload,
            PeriodResource, PostPayload, ProfitAndLossResource, RangeQuery, RevaluationResource,
            RevaluePayload, RevalueReviewBody, ReversePayload, ReverseReviewBody, ReviewResource,
            SetupPayload, SetupResource, TrialBalanceResource, ValuationQuery, YearCloseResource,
            YearPayload, YearResource,
        },
        journals,
        opening::{self, CorrectionPayload, ReviewBody},
        reports, revaluation, service,
    },
};
use serde_json::Value;

permissions! {
    Read => "accounting.read",
    Configure => "accounting.configure",
    JournalCreate => "accounting.journal_create",
    Post => "accounting.post",
    Reverse => "accounting.reverse",
    Revalue => "accounting.revalue",
    Reconcile => "accounting.reconcile",
    Close => "accounting.close",
    Activate => "accounting.activate",
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
    params(("workspace" = Uuid, Path, description = "Workspace id"), AccountListQuery),
    responses(
        (status = 200, body = Paginated<AccountResource>, description = "Success"),
        (status = 422, body = ValidationErrorBody, description = "`q` longer than 100 characters"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn accounts_list(
    State(state): State<AppState>,
    access: Can<Read>,
    Query(q): Query<AccountListQuery>,
) -> Result<Json<Paginated<AccountResource>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let search = crate::http::list::search(q.q.as_deref())?;
    let (rows, total) = service::accounts(&state, access.workspace_id, search, page).await?;
    Ok(Json(Paginated::new(rows, total, page)))
}

/// accounting_accounts_get: Get one ledger account.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/accounts/{account}", tag = "accounting",
    operation_id = "accounting_accounts_get",
    summary = "Get one ledger account",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("account" = Uuid, Path, description = "Account id")),
    responses(
        (status = 200, body = Envelope<AccountResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or account, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn accounts_get(
    State(state): State<AppState>,
    access: Can<Read>,
    Path((_, raw)): Path<(String, String)>,
) -> Result<Data<AccountResource>, AppError> {
    let mut conn = state.db.acquire().await?;
    Ok(Data(
        service::account(&mut conn, access.workspace_id, id(&raw)?).await?,
    ))
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

/// accounting_periods_revalue_review: Preview a period's FX revaluation (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/periods/{period}/revalue/review", tag = "accounting",
    operation_id = "accounting_periods_revalue_review",
    summary = "Preview a period's FX revaluation (no writes)",
    request_body = RevalueReviewBody,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("period" = Uuid, Path, description = "Period id")),
    responses(
        (status = 200, body = Envelope<RevaluationResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 422, body = UnprocessableBody, description = "Invalid input or missing rate, or `accounting_not_configured` / `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn periods_revalue_review(
    State(state): State<AppState>,
    access: Can<Revalue>,
    Path((_, raw)): Path<(String, String)>,
    ValidatedJson(rate_date): ValidatedJson<RevalueReviewBody>,
) -> Result<Data<RevaluationResource>, AppError> {
    Ok(Data(
        revaluation::review(&state, access.workspace_id, id(&raw)?, rate_date).await?,
    ))
}

/// accounting_periods_revalue: Post a reviewed FX revaluation of a period.
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/periods/{period}/revalue", tag = "accounting",
    operation_id = "accounting_periods_revalue",
    summary = "Post a reviewed FX revaluation of a period",
    request_body = Action<RevaluePayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("period" = Uuid, Path, description = "Period id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<RevaluationResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` or reused client_ref"),
        (status = 422, body = UnprocessableBody, description = "Invalid input or missing rate, or `accounting_not_configured` / `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn periods_revalue(
    State(state): State<AppState>,
    access: Can<Revalue>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<RevaluePayload>>,
) -> Result<Answer, AppError> {
    revaluation::post(&state, &access, id(&raw)?, cmd).await
}

/// accounting_close_checks: The §13 close checks of a period (no writes).
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/reports/close-checks", tag = "accounting",
    operation_id = "accounting_close_checks",
    summary = "Close checks of a period",
    params(("workspace" = Uuid, Path, description = "Workspace id"), CloseChecksQuery),
    responses(
        (status = 200, body = Envelope<CloseChecksResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or period, or not a member"),
        (status = 422, body = UnprocessableBody, description = "Invalid input, or `accounting_not_configured` / `accounting_period_closed`"),
    ),
    security(("bearer" = []))
)]
pub async fn close_checks(
    State(state): State<AppState>,
    access: Can<Read>,
    Query(q): Query<CloseChecksQuery>,
) -> Result<Data<CloseChecksResource>, AppError> {
    let period = valid(q)?;
    Ok(Data(
        close::close_checks(&state, access.workspace_id, period).await?,
    ))
}

/// accounting_periods_close: Close an ordinary period whose close checks pass.
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/periods/{period}/close", tag = "accounting",
    operation_id = "accounting_periods_close",
    summary = "Close an ordinary period whose close checks pass",
    request_body = Action<ClosePayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("period" = Uuid, Path, description = "Period id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<PeriodResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or period, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` or reused client_ref"),
        (status = 422, body = UnprocessableBody, description = "A failing check (`receipt_booking_required` / `external_financial_fact_pending` / `unsupported_active_erp_operation`), the final period, or `accounting_not_configured` / `accounting_period_closed`"),
    ),
    security(("bearer" = []))
)]
pub async fn periods_close(
    State(state): State<AppState>,
    access: Can<Close>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<ClosePayload>>,
) -> Result<Answer, AppError> {
    close::close_period(&state, &access, id(&raw)?, cmd).await
}

/// accounting_year_close_review: Preview a fiscal year's year-end close (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/fiscal-years/{year}/close/review", tag = "accounting",
    operation_id = "accounting_year_close_review",
    summary = "Preview a fiscal year's year-end close (no writes)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("year" = Uuid, Path, description = "Fiscal year id")),
    responses(
        (status = 200, body = Envelope<YearCloseResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or fiscal year, or not a member"),
        (status = 422, body = UnprocessableBody, description = "Calendar gap or earlier period open (`unsupported_active_erp_operation`), or `accounting_not_configured` / `accounting_period_closed`"),
    ),
    security(("bearer" = []))
)]
pub async fn years_close_review(
    State(state): State<AppState>,
    access: Can<Close>,
    Path((_, raw)): Path<(String, String)>,
) -> Result<Data<YearCloseResource>, AppError> {
    Ok(Data(
        close::year_review(&state, access.workspace_id, id(&raw)?).await?,
    ))
}

/// accounting_year_close: Post the year-end close, then close the final period and the year.
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/fiscal-years/{year}/close", tag = "accounting",
    operation_id = "accounting_year_close",
    summary = "Post the year-end close, then close the final period and the year",
    request_body = Action<PostPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("year" = Uuid, Path, description = "Fiscal year id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<YearCloseResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or fiscal year, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` or reused client_ref"),
        (status = 422, body = UnprocessableBody, description = "A failing check, calendar gap or earlier period open, or `accounting_not_configured` / `accounting_period_closed`"),
    ),
    security(("bearer" = []))
)]
pub async fn years_close(
    State(state): State<AppState>,
    access: Can<Close>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<PostPayload>>,
) -> Result<Answer, AppError> {
    close::close_year(&state, &access, id(&raw)?, cmd).await
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

/// Validate a report query; field errors are 422.
fn valid<Q: Validate>(q: Q) -> Result<Q::Valid, AppError> {
    let mut r = Rules::default();
    let v = q.validate(&mut r);
    r.into_result()?;
    v.ok_or(AppError::NotFound)
}

/// accounting_trial_balance: Opening, activity and closing per account, posted only.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/reports/trial-balance", tag = "accounting",
    operation_id = "accounting_trial_balance",
    summary = "Trial balance for [from, to)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), RangeQuery),
    responses(
        (status = 200, body = Envelope<TrialBalanceResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid dates, or `accounting_not_configured`"),
    ),
    security(("bearer" = []))
)]
pub async fn trial_balance(
    State(state): State<AppState>,
    access: Can<Read>,
    Query(q): Query<RangeQuery>,
) -> Result<Data<TrialBalanceResource>, AppError> {
    let range = valid(q)?;
    let mut conn = state.db.acquire().await?;
    Ok(Data(
        reports::trial_balance(&mut conn, access.workspace_id, range).await?,
    ))
}

/// accounting_general_ledger: One account's posted lines with a running balance.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/reports/general-ledger", tag = "accounting",
    operation_id = "accounting_general_ledger",
    summary = "General ledger of one account for [from, to)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), LedgerQuery),
    responses(
        (status = 200, body = Envelope<GeneralLedgerResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or account, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `accounting_not_configured`"),
    ),
    security(("bearer" = []))
)]
pub async fn general_ledger(
    State(state): State<AppState>,
    access: Can<Read>,
    Query(q): Query<LedgerQuery>,
) -> Result<Data<GeneralLedgerResource>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (account, range) = valid(q)?;
    let mut conn = state.db.acquire().await?;
    Ok(Data(
        reports::general_ledger(&mut conn, access.workspace_id, account, range, page).await?,
    ))
}

/// accounting_profit_and_loss: Income and expense, excluding year-end close transfers.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/reports/profit-and-loss", tag = "accounting",
    operation_id = "accounting_profit_and_loss",
    summary = "Profit and loss for [from, to)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), RangeQuery),
    responses(
        (status = 200, body = Envelope<ProfitAndLossResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid dates, or `accounting_not_configured`"),
    ),
    security(("bearer" = []))
)]
pub async fn profit_and_loss(
    State(state): State<AppState>,
    access: Can<Read>,
    Query(q): Query<RangeQuery>,
) -> Result<Data<ProfitAndLossResource>, AppError> {
    let range = valid(q)?;
    let mut conn = state.db.acquire().await?;
    Ok(Data(
        reports::profit_and_loss(&mut conn, access.workspace_id, range).await?,
    ))
}

/// accounting_balance_sheet: Balances on as_of (inclusive) with unclosed earnings.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/reports/balance-sheet", tag = "accounting",
    operation_id = "accounting_balance_sheet",
    summary = "Balance sheet on as_of",
    params(("workspace" = Uuid, Path, description = "Workspace id"), AsOfQuery),
    responses(
        (status = 200, body = Envelope<BalanceSheetResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid date, or `accounting_not_configured`"),
    ),
    security(("bearer" = []))
)]
pub async fn balance_sheet(
    State(state): State<AppState>,
    access: Can<Read>,
    Query(q): Query<AsOfQuery>,
) -> Result<Data<BalanceSheetResource>, AppError> {
    let as_of = valid(q)?;
    let mut conn = state.db.acquire().await?;
    Ok(Data(
        reports::balance_sheet(&mut conn, access.workspace_id, as_of).await?,
    ))
}

/// accounting_receivables_aging: Receivables aging on as_of.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/reports/receivables-aging", tag = "accounting",
    operation_id = "accounting_receivables_aging",
    summary = "Receivables aging on as_of",
    params(("workspace" = Uuid, Path, description = "Workspace id"), AgingQuery),
    responses(
        (status = 200, body = Envelope<AgingResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or relation, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `accounting_not_configured`"),
    ),
    security(("bearer" = []))
)]
pub async fn receivables_aging(
    State(state): State<AppState>,
    access: Can<Read>,
    Query(q): Query<AgingQuery>,
) -> Result<Data<AgingResource>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (as_of, relation) = valid(q)?;
    let mut conn = state.db.acquire().await?;
    Ok(Data(
        reports::aging(&mut conn, access.workspace_id, as_of, relation, false, page).await?,
    ))
}

/// accounting_payables_aging: Payables aging on as_of.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/reports/payables-aging", tag = "accounting",
    operation_id = "accounting_payables_aging",
    summary = "Payables aging on as_of",
    params(("workspace" = Uuid, Path, description = "Workspace id"), AgingQuery),
    responses(
        (status = 200, body = Envelope<AgingResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or relation, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `accounting_not_configured`"),
    ),
    security(("bearer" = []))
)]
pub async fn payables_aging(
    State(state): State<AppState>,
    access: Can<Read>,
    Query(q): Query<AgingQuery>,
) -> Result<Data<AgingResource>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (as_of, relation) = valid(q)?;
    let mut conn = state.db.acquire().await?;
    Ok(Data(
        reports::aging(&mut conn, access.workspace_id, as_of, relation, true, page).await?,
    ))
}

/// accounting_party_ledger: Party ledger of one relation for [from, to).
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/reports/party-ledger", tag = "accounting",
    operation_id = "accounting_party_ledger",
    summary = "Party ledger of one relation for [from, to)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), PartyLedgerQuery),
    responses(
        (status = 200, body = Envelope<PartyLedgerResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or relation, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `accounting_not_configured`"),
    ),
    security(("bearer" = []))
)]
pub async fn party_ledger(
    State(state): State<AppState>,
    access: Can<Read>,
    Query(q): Query<PartyLedgerQuery>,
) -> Result<Data<PartyLedgerResource>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (relation, range) = valid(q)?;
    let mut conn = state.db.acquire().await?;
    Ok(Data(
        reports::party_ledger(&mut conn, access.workspace_id, relation, range, page).await?,
    ))
}

/// accounting_inventory_valuation: Inventory valuation on as_of.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/reports/inventory-valuation", tag = "accounting",
    operation_id = "accounting_inventory_valuation",
    summary = "Inventory valuation on as_of",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ValuationQuery),
    responses(
        (status = 200, body = Envelope<InventoryValuationResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or warehouse, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `accounting_not_configured`"),
    ),
    security(("bearer" = []))
)]
pub async fn inventory_valuation(
    State(state): State<AppState>,
    access: Can<Read>,
    Query(q): Query<ValuationQuery>,
) -> Result<Data<InventoryValuationResource>, AppError> {
    let (as_of, warehouse) = valid(q)?;
    let mut conn = state.db.acquire().await?;
    Ok(Data(
        reports::inventory_valuation(&mut conn, access.workspace_id, as_of, warehouse).await?,
    ))
}

/// accounting_bank_accounts_list: List bank accounts.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/bank-accounts", tag = "accounting",
    operation_id = "accounting_bank_accounts_list",
    summary = "List bank accounts",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<Vec<BankAccountResource>>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn bank_accounts_list(
    State(state): State<AppState>,
    access: Can<Read>,
) -> Result<Data<Vec<BankAccountResource>>, AppError> {
    Ok(Data(
        service::bank_accounts(&state, access.workspace_id).await?,
    ))
}

/// accounting_bank_accounts_create: Add a bank account money can be attributed to.
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/bank-accounts", tag = "accounting",
    operation_id = "accounting_bank_accounts_create",
    summary = "Add a bank account money can be attributed to",
    request_body = Create<BankAccountPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<BankAccountResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "Reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn bank_accounts_create(
    State(state): State<AppState>,
    access: Can<Configure>,
    cmd: Command<Create<BankAccountPayload>>,
) -> Result<Answer, AppError> {
    service::create_bank_account(&state, &access, cmd).await
}

/// bank_statements_list: List bank statements, newest first.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/bank-statements", tag = "bank_statements",
    operation_id = "bank_statements_list",
    summary = "List bank statements, newest first",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<Value>, description = "Statements with reconciliation totals"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn bank_statements_list(
    State(state): State<AppState>,
    access: Can<Read>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<Value>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    Ok(Json(
        banking::statements(&state.db, access.workspace_id, page).await?,
    ))
}

/// bank_statements_get: One bank statement.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/bank-statements/{statement}", tag = "bank_statements",
    operation_id = "bank_statements_get",
    summary = "One bank statement",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("statement" = Uuid, Path, description = "Statement id")),
    responses(
        (status = 200, body = Envelope<Value>, description = "Statement with reconciliation totals"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn bank_statements_get(
    State(state): State<AppState>,
    access: Can<Read>,
    Path((_, raw)): Path<(String, String)>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(
        banking::statement(&state.db, access.workspace_id, id(&raw)?).await?,
    ))
}

/// bank_statement_lines: Lines of a bank statement with their live match and residual.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/bank-statements/{statement}/lines", tag = "bank_statements",
    operation_id = "bank_statements_lines",
    summary = "Lines of a bank statement with their live match and residual",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("statement" = Uuid, Path, description = "Statement id"), ListQuery),
    responses(
        (status = 200, body = Paginated<Value>, description = "Lines in file order"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn bank_statement_lines(
    State(state): State<AppState>,
    access: Can<Read>,
    Path((_, raw)): Path<(String, String)>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<Value>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    Ok(Json(
        banking::lines(&state.db, access.workspace_id, id(&raw)?, page).await?,
    ))
}

/// bank_statements_line_candidates: Journal lines a statement line can be matched to.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/bank-statements/{statement}/lines/{line}/candidates", tag = "bank_statements",
    operation_id = "bank_statements_line_candidates",
    summary = "Journal lines a statement line can be matched to, with capacity left",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("statement" = Uuid, Path, description = "Statement id"), ("line" = Uuid, Path, description = "Statement line id"), ListQuery),
    responses(
        (status = 200, body = Paginated<Value>, description = "Candidates, newest first"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, statement or line, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn bank_statements_line_candidates(
    State(state): State<AppState>,
    access: Can<Reconcile>,
    Path((_, statement, line)): Path<(String, String, String)>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<Value>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    Ok(Json(
        banking::candidates(
            &state.db,
            access.workspace_id,
            id(&statement)?,
            id(&line)?,
            page,
        )
        .await?,
    ))
}

/// bank_reconciliations_create: Match a statement line to posted bank journal lines.
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/bank-reconciliations", tag = "bank_reconciliations",
    operation_id = "bank_reconciliations_create",
    summary = "Match a statement line to posted bank journal lines",
    request_body = Create<ReconcilePayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<Value>, description = "Reconciliation"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, statement line or journal line"),
        (status = 409, body = MessageBody, description = "`already_reconciled` or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `amount_mismatch` / `capacity_exceeded` / `journal_line_reversed` / `not_this_bank_account` / `currency_mismatch` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn bank_reconciliations_create(
    State(state): State<AppState>,
    access: Can<Reconcile>,
    cmd: Command<Create<ReconcilePayload>>,
) -> Result<Answer, AppError> {
    banking::reconcile(&state.db, &access, cmd).await
}

/// bank_reconciliations_reverse: Reverse a match; the statement line and capacity are free again.
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/bank-reconciliations/{reconciliation}/reverse", tag = "bank_reconciliations",
    operation_id = "bank_reconciliations_reverse",
    summary = "Reverse a match; the statement line and capacity are free again",
    request_body = Action<UnmatchPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("reconciliation" = Uuid, Path, description = "Reconciliation id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<Value>, description = "Reversed reconciliation"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, `already_reversed` or `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn bank_reconciliations_reverse(
    State(state): State<AppState>,
    access: Can<Reconcile>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<UnmatchPayload>>,
) -> Result<Answer, AppError> {
    banking::unmatch(&state.db, &access, id(&raw)?, cmd).await
}

/// accounting_opening_validate: Review staged opening batches (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/opening/validate", tag = "accounting",
    operation_id = "accounting_opening_validate",
    summary = "Review staged opening batches (no writes)",
    request_body = ReviewBody,
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<Value>, description = "Schedules, issues, ready flag and opening_fingerprint"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document`: setup version changed"),
        (status = 422, body = UnprocessableBody, description = "Invalid batch_ids, or `accounting_not_configured` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn opening_validate(
    State(state): State<AppState>,
    access: Can<Configure>,
    ValidatedJson(input): ValidatedJson<ReviewBody>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(
        opening::review(&state, access.workspace_id, input).await?,
    ))
}

/// accounting_activate: Activate the books from a reviewed opening.
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/activate", tag = "accounting",
    operation_id = "accounting_activate",
    summary = "Activate the books from a reviewed opening",
    request_body = Action<opening::ActivatePayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<SetupResource>, description = "The book, now active"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` (setup version or opening fingerprint changed), `already_active` or reused client_ref"),
        (status = 422, body = UnprocessableBody, description = "Invalid input, an opening that is not ready (its review issues), or `accounting_not_configured` / `accounting_period_closed`"),
    ),
    security(("bearer" = []))
)]
pub async fn activate(
    State(state): State<AppState>,
    access: Can<Activate>,
    cmd: Command<Action<opening::ActivatePayload>>,
) -> Result<Answer, AppError> {
    opening::activate(&state, &access, cmd).await
}

/// accounting_opening_baselines_list: The opening's receivables and payables, as corrected.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/opening-baselines", tag = "accounting",
    operation_id = "accounting_opening_baselines_list",
    summary = "The opening's receivables and payables, as corrected",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<Value>, description = "One page of baselines: source, party, currency, face, settled, outstanding, corrected_outstanding, corrections"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn opening_baselines_list(
    State(state): State<AppState>,
    access: Can<Read>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<Value>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    Ok(Json(
        opening::baselines(&state, access.workspace_id, page).await?,
    ))
}

/// accounting_opening_corrections_review: Preview a typed opening correction (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/opening-corrections/review", tag = "accounting",
    operation_id = "accounting_opening_corrections_review",
    summary = "Preview a typed opening correction (no writes)",
    request_body = CorrectionPayload,
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<Value>, description = "Current and corrected outstanding, journal plan and review_fingerprint"),
        (status = 403, body = MessageBody, description = "Missing accounting.configure or accounting.post"),
        (status = 404, body = MessageBody, description = "Unknown workspace or baseline, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, nothing to correct, over confirmed cash, or `accounting_not_configured` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn opening_corrections_review(
    State(state): State<AppState>,
    access: Can<Configure>,
    ValidatedJson(input): ValidatedJson<CorrectionPayload>,
) -> Result<Data<Value>, AppError> {
    if !access.allows("accounting.post") {
        return Err(AppError::Forbidden(crate::http::workspace::NOT_ALLOWED));
    }
    Ok(Data(
        opening::correction_review(&state, access.workspace_id, input).await?,
    ))
}

/// accounting_opening_corrections_create: Append a reviewed opening correction in the current period.
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/opening-corrections", tag = "accounting",
    operation_id = "accounting_opening_corrections_create",
    summary = "Append a reviewed opening correction in the current period",
    request_body = Create<CorrectionPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<Value>, description = "The correction with its journal plan"),
        (status = 403, body = MessageBody, description = "Missing accounting.configure or accounting.post"),
        (status = 404, body = MessageBody, description = "Unknown workspace or baseline, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` (review changed) or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, nothing to correct, over confirmed cash, or `accounting_period_closed` / `accounting_not_configured` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn opening_corrections_create(
    State(state): State<AppState>,
    access: Can<Configure>,
    cmd: Command<Create<CorrectionPayload>>,
) -> Result<Answer, AppError> {
    if !access.allows("accounting.post") {
        return Err(AppError::Forbidden(crate::http::workspace::NOT_ALLOWED));
    }
    opening::correction_create(&state, &access, cmd).await
}
