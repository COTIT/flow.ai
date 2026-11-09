//! Analytics rules: resolve periods in the workspace time zone, group ranked rows per currency,
//! build and store the invoice export.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        analytics::{
            dto::{
                Bounds, Compare, CompareCurrency, CustomerCurrency, CustomerRow, ExportTotals,
                InvoiceExport, Named, OrderTotals, Period, ProductCurrency, ProductRow,
                SalesCompare, Sums, Top, TopCustomers, TopProducts, VariantRef, When,
            },
            export, repo,
        },
        files,
    },
};

/// Most CSV lines one export may hold (~2 MB, under the 10 MB file limit).
pub const EXPORT_LINES_MAX: i64 = 10_000;
/// How long a stored export lives before the file sweep removes it.
const EXPORT_TTL: chrono::TimeDelta = chrono::TimeDelta::hours(24);

/// `bounds` as instants plus the zone the days were read in; each period checked (422 on its
/// `to` field: `to`, `compare_to`).
async fn resolve(
    state: &AppState,
    workspace_id: Uuid,
    bounds: &[(Bounds, &str)],
) -> Result<(String, Vec<Period>), AppError> {
    let days: Vec<_> = bounds
        .iter()
        .flat_map(|(b, _)| [b.from, b.to])
        .filter_map(|w| match w {
            When::Day(d) => Some(d),
            When::At(_) => None,
        })
        .collect();
    let (tz, midnights) = repo::midnights(&state.db, workspace_id, &days).await?;
    let mut midnights = midnights.into_iter();
    let mut at = |w: When| match w {
        When::At(t) => Ok(t),
        When::Day(_) => midnights
            .next()
            .ok_or_else(|| AppError::Internal("midnight missing".into())),
    };
    let mut periods = Vec::with_capacity(bounds.len());
    for (b, to_field) in bounds {
        let p = Period {
            from: at(b.from)?,
            to: at(b.to)?,
        };
        periods.push(p.check(to_field)?);
    }
    Ok((tz, periods))
}

/// Best customers per currency in the period.
pub async fn top_customers(
    state: &AppState,
    workspace_id: Uuid,
    q: Top,
) -> Result<TopCustomers, AppError> {
    let (time_zone, p) = resolve(state, workspace_id, &[(q.bounds, "to")]).await?;
    let p = p[0];
    let rows = repo::top_customers(
        &state.db,
        workspace_id,
        p.from,
        p.to,
        q.source.as_deref(),
        q.limit,
        q.product_id,
        q.variant_id,
    )
    .await?;
    let mut currencies: Vec<CustomerCurrency> = Vec::new();
    for r in rows {
        if currencies.last().is_none_or(|c| c.currency != r.currency) {
            currencies.push(CustomerCurrency {
                currency: r.currency.clone(),
                decimals: r.decimals,
                rows: Vec::new(),
                without_customer: OrderTotals::default(),
            });
        }
        let Some(c) = currencies.last_mut() else {
            continue;
        };
        match r.relation_id {
            Some(id) => c.rows.push(CustomerRow {
                relation: Named { id, name: r.name },
                orders: r.orders,
                gross_total: r.gross_total,
                net_total: r.net_total,
                vat_total: r.vat_total,
            }),
            None => {
                c.without_customer = OrderTotals {
                    orders: r.orders,
                    gross_total: r.gross_total,
                    net_total: r.net_total,
                    vat_total: r.vat_total,
                }
            }
        }
    }
    Ok(TopCustomers {
        from: p.from,
        to: p.to,
        time_zone,
        source: q.source,
        currencies,
    })
}

/// Best-selling products (or variants) per currency in the period.
pub async fn top_products(
    state: &AppState,
    workspace_id: Uuid,
    q: Top,
) -> Result<TopProducts, AppError> {
    let (time_zone, p) = resolve(state, workspace_id, &[(q.bounds, "to")]).await?;
    let p = p[0];
    let rows = repo::top_products(
        &state.db,
        workspace_id,
        p.from,
        p.to,
        q.source.as_deref(),
        q.limit,
        q.by_variant,
        q.product_id,
        q.variant_id,
    )
    .await?;
    let mut currencies: Vec<ProductCurrency> = Vec::new();
    for r in rows {
        if currencies.last().is_none_or(|c| c.currency != r.currency) {
            currencies.push(ProductCurrency {
                currency: r.currency.clone(),
                decimals: r.decimals,
                rows: Vec::new(),
            });
        }
        if let Some(c) = currencies.last_mut() {
            c.rows.push(ProductRow {
                product: Named {
                    id: r.product_id,
                    name: r.name,
                },
                variant: r.variant_id.map(|id| VariantRef { id, sku: r.sku }),
                title: r.title,
                units: r.units,
                lines: r.lines,
                orders: r.orders,
                gross_total: r.gross_total,
                net_total: r.net_total,
                vat_total: r.vat_total,
            });
        }
    }
    Ok(TopProducts {
        from: p.from,
        to: p.to,
        time_zone,
        source: q.source,
        by: if q.by_variant { "variant" } else { "product" }.to_owned(),
        currencies,
    })
}

/// Sales of the period against the base period, per currency.
pub async fn compare(
    state: &AppState,
    workspace_id: Uuid,
    q: Compare,
) -> Result<SalesCompare, AppError> {
    let (time_zone, p) = resolve(
        state,
        workspace_id,
        &[(q.current, "to"), (q.base, "compare_to")],
    )
    .await?;
    let (cur, base) = (p[0], p[1]);
    let rows = repo::compare(
        &state.db,
        workspace_id,
        cur.from,
        cur.to,
        base.from,
        base.to,
        q.source.as_deref(),
    )
    .await?;
    Ok(SalesCompare {
        period: cur,
        compare_to_period: base,
        time_zone,
        source: q.source,
        currencies: rows
            .into_iter()
            .map(|r| CompareCurrency {
                currency: r.currency,
                decimals: r.decimals,
                current: Sums {
                    orders: r.current_orders,
                    gross_total: r.current_gross,
                    net_total: r.current_net,
                    vat_total: r.current_gross - r.current_net,
                    refunded_total: r.current_refunded,
                },
                base: Sums {
                    orders: r.base_orders,
                    gross_total: r.base_gross,
                    net_total: r.base_net,
                    vat_total: r.base_gross - r.base_net,
                    refunded_total: r.base_refunded,
                },
                gross_difference: r.gross_difference,
                orders_difference: r.orders_difference,
                gross_change_bp: r.gross_change_bp,
                net_difference: r.net_difference,
                net_change_bp: r.net_change_bp,
            })
            .collect(),
    })
}

/// CSV of the numbered invoices dated in the period, stored for 24 h; a 10-minute link to it.
/// Nothing in range stores nothing.
pub async fn export_invoices(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    bounds: Bounds,
) -> Result<InvoiceExport, AppError> {
    let bucket = files::service::store(state)?;
    let (tz, p) = resolve(state, workspace_id, &[(bounds, "to")]).await?;
    let p = p[0];
    let name = export::file_name(bounds);
    let lines = repo::export_lines(
        &state.db,
        workspace_id,
        p.from,
        p.to,
        &tz,
        EXPORT_LINES_MAX + 1,
    )
    .await?;
    if lines.len() as i64 > EXPORT_LINES_MAX {
        return Err(AppError::field(
            "to",
            format!(
                "The range holds over {EXPORT_LINES_MAX} invoice lines; export at most \
                 {EXPORT_LINES_MAX} at a time. Pick a shorter range."
            ),
        ));
    }
    let csv = export::csv(&lines);
    let mut out = InvoiceExport {
        file_id: None,
        name,
        url: None,
        expires_at: None,
        invoices: csv.invoices,
        lines: csv.lines,
        currencies: csv
            .totals
            .into_iter()
            .map(|((currency, decimals), t)| ExportTotals {
                currency,
                decimals,
                excl: t.0,
                vat: t.1,
                total: t.2,
            })
            .collect(),
    };
    if out.invoices == 0 {
        return Ok(out);
    }
    let meta = serde_json::json!({ "kind": "invoice_export", "from": p.from, "to": p.to });
    let description = format!("Invoice export {}", out.name);
    let expires: DateTime<Utc> = Utc::now() + EXPORT_TTL;
    let id = files::service::create(
        state,
        workspace_id,
        user_id,
        files::service::NewFile {
            document_name: &out.name,
            description: Some(&description),
            meta: Some(&meta),
            bytes: csv.bytes,
            expires_at: Some(expires),
        },
    )
    .await?;
    let row = files::repo::find(&state.db, workspace_id, id)
        .await?
        .ok_or_else(|| AppError::Internal("export file vanished".into()))?;
    let (url, link_expires) = files::service::signed_link(bucket, &row)?;
    out.file_id = Some(id);
    out.url = Some(url);
    out.expires_at = Some(link_expires);
    Ok(out)
}
