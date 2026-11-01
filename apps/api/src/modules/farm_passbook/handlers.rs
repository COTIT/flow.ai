//! HTTP edge for /v1/workspace/{workspace}/farm/passbook and a person's passbook and consent.

use axum::{
    Json,
    extract::{Path, Query, State},
};

use crate::{
    AppState,
    error::AppError,
    http::{
        farm::FarmAccess,
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
    },
    modules::farm_passbook::{
        dto::{ConsentResource, EntryResource, ListQuery},
        service,
    },
};

fn page(
    rows: (Vec<crate::modules::farm_passbook::repo::EntryRow>, i64),
    page: Page,
) -> Json<Paginated<EntryResource>> {
    let (rows, total) = rows;
    Json(Paginated::new(
        rows.into_iter().map(Into::into).collect(),
        total,
        page,
    ))
}

/// GET /farm/passbook/mine.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/passbook/mine", tag = "farm_passbook",
    operation_id = "farm_passbook_mine", summary = "My passbook",
    description = "The caller's own passbook lines, newest first, each with the balance in its currency after it.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<EntryResource>, description = "One page"),
        (status = 404, body = MessageBody, description = "Account is no registered person here, or no farm role"),
    ),
    security(("bearer" = []))
)]
pub async fn mine(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<EntryResource>>, AppError> {
    let p = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    Ok(page(
        service::mine(&state, &access, p.size, p.offset()).await?,
        p,
    ))
}

/// GET /farm/persons/{person}/passbook.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/persons/{person}/passbook", tag = "farm_passbook",
    operation_id = "farm_passbook_person", summary = "A person's passbook",
    description = "Staff of the person's village, or the person. Newest first, each line with the balance after it.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("person" = Uuid, Path, description = "Person id"),
        ListQuery,
    ),
    responses(
        (status = 200, body = Paginated<EntryResource>, description = "One page"),
        (status = 403, body = MessageBody, description = "Another farmer"),
        (status = 404, body = MessageBody, description = "Unknown person, or outside the caller's villages or workspace"),
    ),
    security(("bearer" = []))
)]
pub async fn of_person(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, person)): Path<(String, String)>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<EntryResource>>, AppError> {
    let p = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let rows = service::of_person(&state, &access, &person, p.size, p.offset()).await?;
    Ok(page(rows, p))
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
