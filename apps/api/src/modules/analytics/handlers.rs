//! HTTP edge for /v1/workspace/{workspace}/sales/* (`order.list`) and /invoices-export
//! (`invoice.list`).

use axum::extract::{Query, State};

use crate::{
    AppState,
    error::AppError,
    error::ValidationErrorBody,
    http::{
        response::{Data, Envelope, MessageBody},
        workspace::{Can, Permission, permissions},
    },
    modules::analytics::{
        dto::{
            CompareQuery, ExportQuery, InvoiceExport, SalesCompare, TopCustomers, TopProducts,
            TopQuery,
        },
        service,
    },
};

permissions! {
    SalesList => "order.list",
    InvoiceList => "invoice.list",
}

/// `order.list` plus `relation.list`: the ranking names customers.
pub struct CustomerSales;

impl Permission for CustomerSales {
    const NAME: &'static str = "order.list";
    const ALSO: Option<&'static str> = Some("relation.list");
}

/// GET /sales/top-customers.
#[utoipa::path(
    get, path = "/workspace/{workspace}/sales/top-customers", tag = "orders",
    operation_id = "orders_top_customers",
    summary = "Top customers by sales in a period (best customers, who bought the most)",
    description = "Customers ranked per currency by the totals incl VAT of their orders sold in `[from, to)` (same sold rule as `orders_sales_summary`). `from`/`to` take RFC 3339 or `YYYY-MM-DD` (local midnight in the workspace time zone, UTC when unset), at most 366 days apart. Orders without a customer (till walk-ins) are never ranked; they are `without_customer`. Money in minor units.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), TopQuery),
    responses(
        (status = 200, body = Envelope<TopCustomers>, description = "Ranked customers per currency"),
        (status = 403, body = MessageBody, description = "Inactive membership, or missing `order.list` or `relation.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "`from`/`to` missing or malformed, `to` not after `from`, over 366 days, unknown `source`, `limit` outside 1..20"),
    ),
    security(("bearer" = []))
)]
pub async fn top_customers(
    State(state): State<AppState>,
    access: Can<CustomerSales>,
    Query(query): Query<TopQuery>,
) -> Result<Data<TopCustomers>, AppError> {
    let q = query.top()?;
    Ok(Data(
        service::top_customers(&state, access.workspace_id, q).await?,
    ))
}

/// GET /sales/top-products.
#[utoipa::path(
    get, path = "/workspace/{workspace}/sales/top-products", tag = "products",
    operation_id = "products_top_sellers",
    summary = "Best-selling products in a period (best sellers, top products)",
    description = "Products ranked per currency by the line totals incl VAT of orders sold in `[from, to)` (same sold rule as `orders_sales_summary`), with units, lines and orders. `by=variant` ranks each size apart. `from`/`to` take RFC 3339 or `YYYY-MM-DD` (local midnight in the workspace time zone), at most 366 days apart. Money in minor units.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), TopQuery),
    responses(
        (status = 200, body = Envelope<TopProducts>, description = "Ranked products per currency"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `order.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "`from`/`to` missing or malformed, `to` not after `from`, over 366 days, unknown `source` or `by`, `limit` outside 1..20"),
    ),
    security(("bearer" = []))
)]
pub async fn top_products(
    State(state): State<AppState>,
    access: Can<SalesList>,
    Query(query): Query<TopQuery>,
) -> Result<Data<TopProducts>, AppError> {
    let q = query.top()?;
    Ok(Data(
        service::top_products(&state, access.workspace_id, q).await?,
    ))
}

/// GET /sales/compare.
#[utoipa::path(
    get, path = "/workspace/{workspace}/sales/compare", tag = "orders",
    operation_id = "orders_sales_compare",
    summary = "Compare sales of two periods (compare periods, this vs last, change %)",
    description = "Sales of `[from, to)` against `[compare_from, compare_to)` per currency sold in either: both totals, `gross_difference`, `orders_difference` and `gross_change_bp` (signed basis points, 10000 = +100 %, absent when the base gross is 0). Same sold rule as `orders_sales_summary`. Bounds take RFC 3339 or `YYYY-MM-DD` (local midnight in the workspace time zone); each period at most 366 days. Money in minor units.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), CompareQuery),
    responses(
        (status = 200, body = Envelope<SalesCompare>, description = "Both periods per currency"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `order.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "A bound missing or malformed, an end not after its start, a period over 366 days, unknown `source`"),
    ),
    security(("bearer" = []))
)]
pub async fn compare(
    State(state): State<AppState>,
    access: Can<SalesList>,
    Query(query): Query<CompareQuery>,
) -> Result<Data<SalesCompare>, AppError> {
    let q = query.compare()?;
    Ok(Data(
        service::compare(&state, access.workspace_id, q).await?,
    ))
}

/// GET /invoices-export.
#[utoipa::path(
    get, path = "/workspace/{workspace}/invoices-export", tag = "invoices",
    operation_id = "invoices_export",
    summary = "Export invoices for my accountant as a CSV download (export invoices for accountant, bookkeeping)",
    description = "CSV of the numbered invoices (credit notes and cancelled ones too, for a gapless number sequence) dated in `[from, to)`: one row per line with customer, VAT number, VAT rate, line and invoice totals. Stored as a workspace file for 24 hours; `url` downloads it for 10 minutes. Nothing in range stores nothing (`url` null). At most 10000 lines per export.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ExportQuery),
    responses(
        (status = 200, body = Envelope<InvoiceExport>, description = "The stored export and a download link"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `invoice.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "`from`/`to` missing or malformed, `to` not after `from`, over 366 days, or over 10000 lines"),
    ),
    security(("bearer" = []))
)]
pub async fn invoices_export(
    State(state): State<AppState>,
    access: Can<InvoiceList>,
    Query(query): Query<ExportQuery>,
) -> Result<Data<InvoiceExport>, AppError> {
    let bounds = query.bounds()?;
    Ok(Data(
        service::export_invoices(&state, access.workspace_id, access.user_id, bounds).await?,
    ))
}
