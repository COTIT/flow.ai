//! Attention rules: which sections the caller may see, and the shared status sets they use.

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    error::AppError,
    http::workspace::{NOT_ALLOWED, WorkspaceAccess},
    modules::{
        attention::{
            dto::{
                Attention, AttentionInvoice, AttentionInvoices, AttentionLots, AttentionMoney,
                AttentionOrders, AttentionShifts, AttentionStock, AttentionWebhooks, BelowMinimum,
                BelowMinimumItem, KINDS, Mark, NewMark, Priorities, PriorityGroup, PriorityImpact,
                PriorityItem,
            },
            repo,
        },
        invoices::actions::PAYABLE,
        settings,
        storefront::dto::LOW_STOCK_DEFAULT,
    },
    numbering::Appendix,
};

/// Items per section.
const ITEMS: i64 = 5;

/// The summary for `access`: each section only when its list route would let the caller in.
pub async fn summary(db: &PgPool, access: &WorkspaceAccess) -> Result<Attention, AppError> {
    sections(db, access.workspace_id, |perm| access.allows(perm)).await
}

/// The summary of `ws` for a reader holding the permissions `may` allows (scheduled reports
/// pass the owner's grants, rechecked per run).
pub async fn sections(
    db: &PgPool,
    ws: uuid::Uuid,
    may: impl Fn(&str) -> bool,
) -> Result<Attention, AppError> {
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
        let (items, out_of_stock) = repo::sold_out(db, ws, 0, ITEMS).await?;
        let rows = repo::below_minimum(db, ws, Uuid::nil(), threshold(db, ws).await?, false, ITEMS)
            .await?;
        Some(AttentionStock {
            negative: repo::negative_stock(db, ws).await?,
            out_of_stock,
            items,
            below_minimum: BelowMinimum {
                count: rows.first().map_or(0, |r| r.total),
                items: rows
                    .into_iter()
                    .map(|r| BelowMinimumItem {
                        product_id: r.ref_id,
                        product_name: r.label,
                        available: r.available,
                        minimum: r.minimum,
                        shortfall: r.shortfall,
                    })
                    .collect(),
            },
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
pub async fn invoices(db: &PgPool, ws: uuid::Uuid) -> Result<AttentionInvoices, AppError> {
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

/// Workspace `product.low_stock_threshold` (default 5): the minimum of a product with no row
/// minimum.
async fn threshold(db: &PgPool, ws: Uuid) -> Result<i64, AppError> {
    Ok(settings::repo::section(db, ws, "product")
        .await?
        .and_then(|s| s.get("low_stock_threshold")?.as_i64())
        .unwrap_or(LOW_STOCK_DEFAULT))
}

/// The permission that lets a caller see `kind` (None: any member). Same gates as `sections`.
fn gate(kind: &str) -> Option<&'static str> {
    match kind {
        "stock.below_minimum" => Some("stock.list"),
        "orders.open" => Some("order.list"),
        "invoices.overdue" => Some("invoice.list"),
        "till.open_shifts" => Some("cashregister.list"),
        "webhooks.failed" => Some("workspace_admin"),
        _ => None,
    }
}

/// Group totals shared by every kind.
struct Totals {
    total: i64,
    acked: i64,
    snoozed: i64,
    impact: Option<i64>,
}

fn group(
    kind: &'static str,
    t: Totals,
    items: Vec<PriorityItem>,
    actions: Vec<&'static str>,
) -> PriorityGroup {
    PriorityGroup {
        kind,
        count: t.total,
        open_count: t.total - t.snoozed,
        acknowledged_count: t.acked,
        snoozed_count: t.snoozed,
        snoozed: t.total == t.snoozed,
        headline_ref: items.first().map(|i| i.ref_id),
        impact: PriorityImpact {
            open_orders: t.impact,
        },
        items,
        actions,
    }
}

fn item_group(kind: &'static str, rows: Vec<repo::ItemRow>) -> PriorityGroup {
    let t = rows.first().map_or(
        Totals {
            total: 0,
            acked: 0,
            snoozed: 0,
            impact: None,
        },
        |r| Totals {
            total: r.total,
            acked: r.acked_n,
            snoozed: r.snoozed_n,
            impact: None,
        },
    );
    let items = rows
        .into_iter()
        .filter(|r| !r.snoozed)
        .map(|r| PriorityItem {
            ref_id: r.ref_id,
            label: match r.invoice_number.zip(r.invoice_appendix) {
                Some((n, a)) => Appendix::from_section(&a).format(n),
                None => r.label,
            },
            available: None,
            minimum: None,
            shortfall: None,
            open_orders: None,
            money: r.amount.zip(r.currency).zip(r.decimals).map(
                |((amount, currency), decimals)| AttentionMoney {
                    currency,
                    decimals,
                    amount,
                },
            ),
            since: r.since,
            acknowledged: r.acked,
        })
        .collect();
    group(kind, t, items, vec!["review", "acknowledge", "snooze"])
}

/// Flow noticed for the caller: one statement per visible kind, the caller's marks applied.
/// Groups with nothing in them are left out; kinds the caller can't see are `restricted`.
pub async fn priorities(db: &PgPool, access: &WorkspaceAccess) -> Result<Priorities, AppError> {
    let (ws, user) = (access.workspace_id, access.user_id);
    let may = |kind: &str| gate(kind).is_none_or(|p| access.allows(p));
    let mut groups = Vec::new();
    if may("stock.below_minimum") {
        let rows = repo::below_minimum(
            db,
            ws,
            user,
            threshold(db, ws).await?,
            access.allows("order.list"),
            ITEMS,
        )
        .await?;
        let t = rows.first().map_or(
            Totals {
                total: 0,
                acked: 0,
                snoozed: 0,
                impact: None,
            },
            |r| Totals {
                total: r.total,
                acked: r.acked_n,
                snoozed: r.snoozed_n,
                impact: r.impact,
            },
        );
        let items = rows
            .into_iter()
            .filter(|r| !r.snoozed)
            .map(|r| PriorityItem {
                ref_id: r.ref_id,
                label: r.label,
                available: Some(r.available),
                minimum: Some(r.minimum),
                shortfall: Some(r.shortfall),
                open_orders: r.open_orders,
                money: None,
                since: None,
                acknowledged: r.acked,
            })
            .collect();
        let mut actions = vec!["review", "acknowledge", "snooze"];
        if access.allows("stock.mutate") || access.allows("stock.set_stock_levels") {
            actions.push("adjust");
        }
        groups.push(group("stock.below_minimum", t, items, actions));
    }
    if may("orders.open") {
        groups.push(item_group(
            "orders.open",
            repo::open_order_items(db, ws, user, ITEMS).await?,
        ));
    }
    if may("invoices.overdue") {
        let payable = PAYABLE.map(String::from);
        let today = repo::today_start(db, ws).await?;
        let rows = repo::overdue_invoice_items(db, ws, user, &payable, today, ITEMS).await?;
        groups.push(item_group("invoices.overdue", rows));
    }
    if may("till.open_shifts") {
        groups.push(item_group(
            "till.open_shifts",
            repo::open_shift_items(db, ws, user, ITEMS).await?,
        ));
    }
    groups.push(item_group(
        "lots.quarantined",
        repo::quarantined_lot_items(db, ws, user, ITEMS).await?,
    ));
    if may("webhooks.failed") {
        groups.push(item_group(
            "webhooks.failed",
            repo::failed_delivery_items(db, ws, user, ITEMS).await?,
        ));
    }
    groups.retain(|g| g.count > 0);
    // Stable sort: ties keep the fixed kind order they were pushed in.
    groups.sort_by_key(|g| {
        (
            g.snoozed,
            std::cmp::Reverse(g.impact.open_orders.unwrap_or(0)),
        )
    });
    Ok(Priorities {
        generated_at: Utc::now(),
        groups,
        restricted: KINDS.into_iter().filter(|k| !may(k)).collect(),
    })
}

/// Acknowledge or snooze one item for the caller. 403 without the kind's list permission, 404
/// when `ref_id` is no live record of that kind in this workspace.
pub async fn set_mark(db: &PgPool, access: &WorkspaceAccess, m: NewMark) -> Result<Mark, AppError> {
    if gate(m.kind).is_some_and(|p| !access.allows(p)) {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    if !repo::ref_exists(db, access.workspace_id, m.kind, m.ref_id).await? {
        return Err(AppError::NotFound);
    }
    Ok(repo::set_mark(db, access.workspace_id, access.user_id, &m).await?)
}

/// Remove one of the caller's marks; 404 when it isn't theirs.
pub async fn delete_mark(db: &PgPool, access: &WorkspaceAccess, id: Uuid) -> Result<(), AppError> {
    match repo::delete_mark(db, access.workspace_id, access.user_id, id).await? {
        0 => Err(AppError::NotFound),
        _ => Ok(()),
    }
}
