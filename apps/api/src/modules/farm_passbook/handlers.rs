//! HTTP edge for /v1/workspace/{workspace}/farm/passbook and a person's passbook and consent.

use axum::{
    Json,
    extract::{Path, Query, State},
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        farm::FarmAccess,
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
    },
    modules::farm_passbook::{
        dto::{BalancesPage, BalancesQuery, ConsentResource, ListQuery, PassbookPage},
        service,
    },
};

fn page(book: service::Passbook, page: Page) -> Json<PassbookPage> {
    let (rows, total, balances) = book;
    let (data, meta) =
        Paginated::new(rows.into_iter().map(Into::into).collect(), total, page).into_parts();
    Json(PassbookPage {
        data,
        meta,
        balances,
    })
}

/// GET /farm/passbook/mine.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/passbook/mine", tag = "farm_passbook",
    operation_id = "farm_passbook_mine", summary = "My passbook",
    description = "The caller's own passbook lines, newest first, each with the balance in its currency after it. `from` / `to` keep Tanzania days (inclusive); the balances still cover every line.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = PassbookPage, description = "One page, with the balance per currency"),
        (status = 404, body = MessageBody, description = "Account is no registered person here, or no farm role"),
        (status = 422, body = ValidationErrorBody, description = "Bad date, or `from` after `to`"),
    ),
    security(("bearer" = []))
)]
pub async fn mine(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(query): Query<ListQuery>,
) -> Result<Json<PassbookPage>, AppError> {
    let p = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let days = service::days(query.from.as_deref(), query.to.as_deref())?;
    Ok(page(
        service::mine(&state, &access, days, p.size, p.offset()).await?,
        p,
    ))
}

/// GET /farm/persons/{person}/passbook.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/persons/{person}/passbook", tag = "farm_passbook",
    operation_id = "farm_passbook_person", summary = "A person's passbook",
    description = "Staff of the person's village, or the person. Newest first, each line with the balance after it. `from` / `to` keep Tanzania days (inclusive), e.g. one month; the running balance and `balances` still count every line.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("person" = Uuid, Path, description = "Person id"),
        ListQuery,
    ),
    responses(
        (status = 200, body = PassbookPage, description = "One page, with the balance per currency"),
        (status = 403, body = MessageBody, description = "Another farmer"),
        (status = 404, body = MessageBody, description = "Unknown person, or outside the caller's villages or workspace"),
        (status = 422, body = ValidationErrorBody, description = "Bad date, or `from` after `to`"),
    ),
    security(("bearer" = []))
)]
pub async fn of_person(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, person)): Path<(String, String)>,
    Query(query): Query<ListQuery>,
) -> Result<Json<PassbookPage>, AppError> {
    let p = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let days = service::days(query.from.as_deref(), query.to.as_deref())?;
    let rows = service::of_person(&state, &access, &person, days, p.size, p.offset()).await?;
    Ok(page(rows, p))
}

/// GET /farm/passbook/balances.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/passbook/balances", tag = "farm_passbook",
    operation_id = "farm_passbook_balances", summary = "What we owe farmers",
    description = "Staff. Every farmer in the caller's villages (ops and admin: all) with a positive passbook balance, largest first, one row per farmer and currency, and `totals`: the sum owed per currency over every page. Summed in SQL in minor units. `village_id` narrows to one village.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), BalancesQuery),
    responses(
        (status = 200, body = BalancesPage, description = "One page of farmers owed, with totals"),
        (status = 403, body = MessageBody, description = "A farmer"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role"),
        (status = 422, body = ValidationErrorBody, description = "Malformed village id"),
    ),
    security(("bearer" = []))
)]
pub async fn balances(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(query): Query<BalancesQuery>,
) -> Result<Json<BalancesPage>, AppError> {
    let p = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let village = query.village_id.as_deref();
    let (rows, total, totals) = service::owed(&state, &access, village, p.size, p.offset()).await?;
    let (data, meta) =
        Paginated::new(rows.into_iter().map(Into::into).collect(), total, p).into_parts();
    Ok(Json(BalancesPage { data, meta, totals }))
}

/// GET /farm/persons/{person}/passport-consent.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/persons/{person}/passport-consent", tag = "farm_passbook",
    operation_id = "farm_passbook_consent", summary = "Read passport consent",
    description = "The person or staff of their village. Whether the person agreed that the public lot page may name or picture them, when and by whom. Supply links never share names or photos; this flag is the only per-farmer consent.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("person" = Uuid, Path, description = "Person id"),
    ),
    responses(
        (status = 200, body = Envelope<ConsentResource>, description = "Current consent"),
        (status = 403, body = MessageBody, description = "Another farmer"),
        (status = 404, body = MessageBody, description = "Unknown person, or outside the caller's villages or workspace"),
    ),
    security(("bearer" = []))
)]
pub async fn get_consent(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, person)): Path<(String, String)>,
) -> Result<Data<ConsentResource>, AppError> {
    Ok(Data(
        service::consent_of(&state, &access, &person).await?.into(),
    ))
}

/// POST /farm/persons/{person}/passport-consent.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/persons/{person}/passport-consent", tag = "farm_passbook",
    operation_id = "farm_passbook_give_consent", summary = "Record passport consent",
    description = "The person or staff of their village. Records that the public lot page may name or picture this person once the data-protection gate opens; nothing is shown before that.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("person" = Uuid, Path, description = "Person id"),
    ),
    responses(
        (status = 200, body = Envelope<ConsentResource>, description = "Consent recorded"),
        (status = 403, body = MessageBody, description = "Another farmer"),
        (status = 404, body = MessageBody, description = "Unknown person, or outside the caller's villages or workspace"),
    ),
    security(("bearer" = []))
)]
pub async fn give_consent(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, person)): Path<(String, String)>,
) -> Result<Data<ConsentResource>, AppError> {
    Ok(Data(
        service::consent(&state, &access, &person, true)
            .await?
            .into(),
    ))
}

/// DELETE /farm/persons/{person}/passport-consent.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/farm/persons/{person}/passport-consent", tag = "farm_passbook",
    operation_id = "farm_passbook_withdraw_consent", summary = "Withdraw passport consent",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("person" = Uuid, Path, description = "Person id"),
    ),
    responses(
        (status = 200, body = Envelope<ConsentResource>, description = "Consent withdrawn"),
        (status = 403, body = MessageBody, description = "Another farmer"),
        (status = 404, body = MessageBody, description = "Unknown person, or outside the caller's villages or workspace"),
    ),
    security(("bearer" = []))
)]
pub async fn withdraw_consent(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, person)): Path<(String, String)>,
) -> Result<Data<ConsentResource>, AppError> {
    Ok(Data(
        service::consent(&state, &access, &person, false)
            .await?
            .into(),
    ))
}
