//! HTTP edge for /v1/workspace/{workspace}/till/catalog: the till reads what its register sells.

use axum::{
    Json,
    extract::{Query, State},
};
use chrono::{DateTime, Utc};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::Rules,
        list::{Page, Paginated},
        response::MessageBody,
    },
    modules::{
        till::session::TillSession,
        till_catalog::{
            dto::{CatalogQuery, TillCatalogPage},
            repo::{self, Filter},
        },
    },
};

const CODE_MAX: usize = 100;

/// GET /till/catalog -> `{data:[...], meta, next_since}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/till/catalog", tag = "till",
    operation_id = "till_catalog", summary = "What this register sells (paged, delta sync, barcode lookup)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), CatalogQuery),
    responses(
        (status = 200, body = TillCatalogPage, description = "One page of the register's catalog"),
        (status = 401, body = MessageBody, description = "No live till session (ended, expired, key rotated or revoked, user inactive)"),
        (status = 404, body = MessageBody, description = "Workspace is not the session's"),
        (status = 422, body = ValidationErrorBody, description = "`updated_since` not RFC 3339, or `code` too long"),
    ),
    security(("till_session" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    session: TillSession,
    Query(q): Query<CatalogQuery>,
) -> Result<Json<TillCatalogPage>, AppError> {
    let mut r = Rules::default();
    let since = q.updated_since.as_deref().and_then(|s| {
        let at = DateTime::parse_from_rfc3339(s.trim()).ok();
        r.check(
            "updated_since",
            at.is_some(),
            "The updated since must be an RFC 3339 time.",
        );
        at.map(|t| t.with_timezone(&Utc))
    });
    let code = q.code.as_deref().map(str::trim).filter(|c| !c.is_empty());
    if let Some(c) = code {
        r.max_chars("code", c, CODE_MAX);
    }
    r.into_result()?;
    let s = &session.row;
    let filter = Filter {
        workspace_id: s.workspace_id,
        currency_id: s.currency_id,
        warehouse_id: s.warehouse_id,
        since,
        code,
    };
    let next_since = repo::next_since(&state.db).await?;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let rows = repo::products(&state.db, &filter, page.size, page.offset()).await?;
    let total = match rows.first() {
        Some(r) => r.total,
        // Past the end: the first row of page one carries the count.
        None if page.offset() > 0 => repo::products(&state.db, &filter, 1, 0)
            .await?
            .first()
            .map_or(0, |r| r.total),
        None => 0,
    };
    let data = rows.into_iter().map(Into::into).collect();
    let (data, meta) = Paginated::new(data, total, page).into_parts();
    Ok(Json(TillCatalogPage {
        data,
        meta,
        next_since,
    }))
}
