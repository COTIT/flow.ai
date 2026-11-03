//! HTTP edge for /v1/workspace/{workspace}/searches and /quick-search. `searches` dispatches
//! `domain` to the module's list service and answers in that module's list shape: Postgres
//! ILIKE on the name column instead of Laravel's Meilisearch; results stay in insertion order.
//! `quick-search` returns a few rows per entity kind (see `service::quick`).
// shortcut: no relevance ranking; fine for reference lists, revisit for products/relations.

use std::borrow::Cow;

use axum::{
    Json,
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::Rules,
        list::{Page, Paginated, flag, search as pattern_of},
        response::{Data, Envelope, MessageBody},
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
        product_brands::{dto::BrandResource, service as product_brands},
        product_categories::{
            dto::CategoryResource, repo::ListFilter as CategoryFilter,
            service as product_categories,
        },
        relations,
        sales_groups::{dto::SalesGroupResource, service as sales_groups},
        search::{
            dto::{QuickQuery, QuickResults},
            service,
        },
        shipment_methods::{
            dto::ShipmentMethodResource, repo::ListFilter as ShipmentFilter,
            service as shipment_methods,
        },
        vat_groups::{dto::VatGroupResource, repo::ListFilter as VatFilter, service as vat_groups},
        warehouse_locations::{dto::LocationResource, repo as location_repo},
        warehouses::dto::WarehouseCompacted,
    },
};

/// Shortest and longest quick-search `q` (trigram indexes need 3 characters).
const QUICK_MIN: usize = 3;
const QUICK_MAX: usize = 100;
/// Rows per quick-search group: default and most.
const QUICK_LIMIT: i64 = 5;
const QUICK_LIMIT_MAX: i64 = 10;

permissions! {
    GlobalSearch => "workspace.global-search",
}

/// Query. Unknown params ignored (dropdowns spread their own filters).
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchQuery {
    /// What to search. Anything else (or none) is 404.
    #[param(value_type = Option<SearchDomain>, inline)]
    domain: Option<String>,
    /// Search on the name column.
    q: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    page_size: Option<String>,
    // Laravel ScoutFilterHelper: shipment methods only.
    /// `shipment_method` only: selectable ones.
    #[param(value_type = Option<bool>)]
    can_be_selected: Option<String>,
    /// `relation` only: suppliers only.
    #[param(value_type = Option<bool>)]
    is_supplier: Option<String>,
    /// `warehouse_location` only: one warehouse's labels.
    #[param(value_type = Option<Uuid>)]
    warehouse_id: Option<String>,
}

/// Searchable domains.
// Doc-only: allowed `domain` values; the handler matches on the raw string.
#[derive(ToSchema)]
#[schema(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum SearchDomain {
    Currency,
    VatGroup,
    ProductBrand,
    ProductCategory,
    WarehouseLocation,
    SalesGroup,
    HourlyRate,
    ShipmentMethod,
    CustomStatus,
    CustomType,
    Relation,
}

/// One row in the chosen domain's list shape (as that domain's list route returns it).
// Doc-only: rows differ per domain; never constructed.
// shortcut: untyped rows; switch to a oneOf of the domain resources once every one has a
// schema.
#[derive(ToSchema)]
pub struct SearchRow {}

fn page_of<T: Serialize, R: Into<T>>(rows: Vec<R>, total: i64, page: Page) -> Response {
    let data: Vec<T> = rows.into_iter().map(Into::into).collect();
    Json(Paginated::new(data, total, page)).into_response()
}

/// GET /searches?domain=X&q=... -> the domain's `{data:[...], meta}`. A domain that isn't
/// ported yet, or none (Laravel's global search), is 404.
#[utoipa::path(
    get, path = "/workspace/{workspace}/searches", tag = "search",
    operation_id = "search_search", summary = "Search one domain",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        SearchQuery,
    ),
    responses(
        (status = 200, body = Paginated<SearchRow>, description = "One page of the domain's rows"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `workspace.global-search`"),
        (status = 404, body = MessageBody, description = "Unknown or unsupported `domain`, unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Search text too long"),
    ),
    security(("bearer" = []))
)]
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
        Some("product_brand") => {
            let (rows, total) = product_brands::list(s, ws, pattern, limit, offset).await?;
            page_of::<BrandResource, _>(rows, total, page)
        }
        Some("product_category") => {
            let f = CategoryFilter {
                pattern,
                parent_id: None,
                root_only: false,
                limit,
                offset,
            };
            let (rows, total) = product_categories::list(s, ws, &f).await?;
            page_of::<CategoryResource, _>(rows, total, page)
        }
        Some("warehouse_location") => {
            // A malformed id matches nothing, as Laravel's Meilisearch filter did.
            let warehouse = query
                .warehouse_id
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(|v| Uuid::parse_str(v).unwrap_or_default());
            let (rows, total) =
                location_repo::search(&s.db, ws, warehouse, pattern, limit, offset).await?;
            let data: Vec<LocationResource> = rows
                .into_iter()
                .map(|r| {
                    let compact = WarehouseCompacted::from(&r.warehouse);
                    LocationResource::new(r.location, Cow::Owned(compact))
                })
                .collect();
            Json(Paginated::new(data, total, page)).into_response()
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

/// GET /quick-search?q=: a few rows per kind for the office top bar.
#[utoipa::path(
    get, path = "/workspace/{workspace}/quick-search", tag = "search",
    operation_id = "search_quick", summary = "Search products, orders, invoices, lots and relations at once",
    description = "Contains-match on `q` (any case; `%` and `_` literal): products by name, number, EAN-13 or barcode; orders by number or reference; lots by code or public code; relations by company name, alias or a contact's first or last name. Invoices match when the digits of `q` are their number. Each group is null when the caller lacks that list permission (`product.list`, `order.list`, `invoice.list`, `relation.list`; lots: any member), else at most `limit` rows, newest first.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        QuickQuery,
    ),
    responses(
        (status = 200, body = Envelope<QuickResults>, description = "Hits per group"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `workspace.global-search`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "`q` missing, under 3 or over 100 characters"),
    ),
    security(("bearer" = []))
)]
pub async fn quick(
    State(state): State<AppState>,
    access: Can<GlobalSearch>,
    Query(query): Query<QuickQuery>,
) -> Result<Data<QuickResults>, AppError> {
    let q = query.q.as_deref().map(str::trim).unwrap_or_default();
    let mut r = Rules::default();
    r.check(
        "q",
        (QUICK_MIN..=QUICK_MAX).contains(&q.chars().count()),
        format!("The q must be {QUICK_MIN} to {QUICK_MAX} characters."),
    );
    r.into_result()?;
    let limit = query
        .limit
        .as_deref()
        .and_then(|l| l.trim().parse::<i64>().ok())
        .filter(|l| *l > 0)
        .map_or(QUICK_LIMIT, |l| l.min(QUICK_LIMIT_MAX));
    Ok(Data(service::quick(&state, &access, q, limit).await?))
}
