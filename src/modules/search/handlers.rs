//! HTTP edge for /v1/workspace/{workspace}/searches. Dispatches `domain` to the module's list
//! service and answers in that module's list shape. Postgres ILIKE on the name column instead
//! of Laravel's Meilisearch; results stay in insertion order.
// shortcut: no relevance ranking; fine for reference lists, revisit for products/relations.

use axum::{
    Json,
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    error::AppError,
    http::{
        list::{Page, Paginated, flag, search as pattern_of},
        workspace::{Can, permissions},
    },
    modules::{
        currencies::{dto::CurrencyResource, service as currencies},
        custom_statuses::{
            dto::CustomStatusResource, repo::ListFilter as StatusFilter, service as custom_statuses,
        },
        custom_types::{
            dto::CustomTypeResource, repo::ListFilter as TypeFilter, service as custom_types,
        },
        hourly_rates::{dto::HourlyRateResource, service as hourly_rates},
        relations,
        sales_groups::{dto::SalesGroupResource, service as sales_groups},
        shipment_methods::{
            dto::ShipmentMethodResource, repo::ListFilter as ShipmentFilter,
            service as shipment_methods,
        },
        vat_groups::{dto::VatGroupResource, repo::ListFilter as VatFilter, service as vat_groups},
    },
};

permissions! {
    GlobalSearch => "workspace.global-search",
}

/// Query. Unknown params ignored (dropdowns spread their own filters).
#[derive(Deserialize)]
pub struct SearchQuery {
    domain: Option<String>,
    q: Option<String>,
    page: Option<String>,
    page_size: Option<String>,
    /// Laravel ScoutFilterHelper: shipment methods only.
    can_be_selected: Option<String>,
    /// Relation pickers: suppliers only.
    is_supplier: Option<String>,
}

fn page_of<T: Serialize, R: Into<T>>(rows: Vec<R>, total: i64, page: Page) -> Response {
    let data: Vec<T> = rows.into_iter().map(Into::into).collect();
    Json(Paginated::new(data, total, page)).into_response()
}

/// GET /searches?domain=X&q=... -> the domain's `{data:[...], meta}`. A domain that isn't
/// ported yet, or none (Laravel's global search), is 404.
pub async fn search(
    State(state): State<AppState>,
    access: Can<GlobalSearch>,
    Query(query): Query<SearchQuery>,
) -> Result<Response, AppError> {
    let pattern = pattern_of(query.q.as_deref())?;
    let pattern = pattern.as_deref();
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (limit, offset) = (page.size, page.offset());
    let ws = access.workspace_id;
    let s = &state;
    Ok(match query.domain.as_deref().map(str::trim) {
        Some("currency") => {
            let (rows, total) = currencies::list(s, ws, pattern, limit, offset).await?;
            page_of::<CurrencyResource, _>(rows, total, page)
        }
        Some("vat_group") => {
            let f = VatFilter {
                country: None,
                pattern,
                limit,
                offset,
            };
            let (rows, total) = vat_groups::list(s, ws, &f).await?;
            page_of::<VatGroupResource, _>(rows, total, page)
        }
        Some("sales_group") => {
            let (rows, total) = sales_groups::list(s, ws, pattern, limit, offset).await?;
            page_of::<SalesGroupResource, _>(rows, total, page)
        }
        Some("hourly_rate") => {
            let (rows, total) = hourly_rates::list(s, ws, pattern, limit, offset).await?;
            page_of::<HourlyRateResource, _>(rows, total, page)
        }
        Some("shipment_method") => {
            let f = ShipmentFilter {
                pattern,
                can_be_selected: query.can_be_selected.as_deref().map(flag),
                limit,
                offset,
            };
            let (rows, total) = shipment_methods::list(s, ws, &f).await?;
            page_of::<ShipmentMethodResource, _>(rows, total, page)
        }
        Some("custom_status") => {
            let f = StatusFilter {
                domain: None,
                pattern,
                limit,
                offset,
            };
            let (rows, total) = custom_statuses::list(s, ws, &f).await?;
            page_of::<CustomStatusResource, _>(rows, total, page)
        }
        Some("custom_type") => {
            let f = TypeFilter {
                domain: None,
                pattern,
                default_for_work_order: None,
                is_leave_hours_type: None,
                is_supplier: None,
                limit,
                offset,
            };
            let (rows, total) = custom_types::list(s, ws, &f).await?;
            page_of::<CustomTypeResource, _>(rows, total, page)
        }
        Some("relation") => {
            // Relations parse `q` themselves (phrases, id-like and phone matches).
            let iq = relations::service::IndexQuery {
                q: query.q.as_deref(),
                suppliers: query.is_supplier.as_deref().is_some_and(flag),
                include_advice: false,
                last_name: None,
                zipcode: None,
                company_name: None,
                page,
            };
            Json(relations::service::index(s, ws, iq).await?).into_response()
        }
        _ => return Err(AppError::NotFound),
    })
}
