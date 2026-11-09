//! Attention rules: which sections the caller may see, and the shared status sets they use.

use chrono::Utc;
use sqlx::PgPool;

use crate::{
    error::AppError,
    http::workspace::WorkspaceAccess,
    modules::{
        attention::{
            dto::{
                Attention, AttentionInvoice, AttentionInvoices, AttentionLots, AttentionOrders,
                AttentionShifts, AttentionStock, AttentionWebhooks,
            },
            repo,
        },
        invoices::actions::PAYABLE,
    },
    numbering::Appendix,
};

/// Items per section.
const ITEMS: i64 = 5;

/// The summary for `access`: each section only when its list route would let the caller in.
pub async fn summary(db: &PgPool, access: &WorkspaceAccess) -> Result<Attention, AppError> {
    let ws = access.workspace_id;
    let may = |perm: &str| access.allows(perm);
    let orders = if may("order.list") {
        let c = repo::order_counts(db, ws).await?;
        Some(AttentionOrders {
            draft: c.draft,
            open: c.open,
            awaiting_payment: c.awaiting_payment,
            items: repo::order_items(db, ws, ITEMS).await?,
        })
    } else {
        None
    };
    let invoices = if may("invoice.list") {
        Some(invoices(db, ws).await?)
    } else {
        None
    };
    let stock = if may("stock.list") {
        let (items, out_of_stock) = repo::sold_out(db, ws, ITEMS).await?;
        Some(AttentionStock {
            negative: repo::negative_stock(db, ws).await?,
            out_of_stock,
            items,
        })
    } else {
        None
    };
    let till_shifts = if may("cashregister.list") {
        let (items, open) = repo::open_shifts(db, ws, ITEMS).await?;
        Some(AttentionShifts { open, items })
    } else {
        None
    };
    let l = repo::lot_counts(db, ws).await?;
    let lots = AttentionLots {
        quarantined: l.quarantined,
        held_orders: l.held_orders,
        items: repo::lot_items(db, ws, ITEMS).await?,
    };
    // Webhook routes are admin-only (`Can<WorkspaceAdmin>`).
    let webhooks = if may("workspace_admin") {
        let (items, failed) = repo::failed_deliveries(db, ws, ITEMS).await?;
        Some(AttentionWebhooks { failed, items })
    } else {
        None
    };
    Ok(Attention {
        generated_at: Utc::now(),
        orders,
        invoices,
        stock,
        till_shifts,
        lots,
        webhooks,
    })
}

/// Draft, unpaid (the `Pay` rule's statuses) and overdue invoices of `ws`.
async fn invoices(db: &PgPool, ws: uuid::Uuid) -> Result<AttentionInvoices, AppError> {
    let payable = PAYABLE.map(String::from);
    let today = repo::today_start(db, ws).await?;
    let c = repo::invoice_counts(db, ws, &payable, today).await?;
    let items = repo::invoice_items(db, ws, &payable, today, ITEMS)
        .await?
        .into_iter()
        .map(|r| AttentionInvoice {
            id: r.id,
            number: r
                .invoice_number
                .zip(r.invoice_appendix)
                .map(|(n, a)| Appendix::from_section(&a).format(n)),
            status: r.status,
            customer_name: r.customer_name,
            due_date: r.due_date,
            overdue: r.overdue,
            open: r.open,
            currency: r.currency,
            decimals: r.decimals,
        })
        .collect();
    Ok(AttentionInvoices {
        draft: c.draft,
        unpaid: c.unpaid,
        overdue: c.overdue,
        open: repo::invoice_open(db, ws, &payable).await?,
        items,
    })
}
