//! Quick search rules: which groups the caller may see, and what `q` matches in each.

use crate::{
    AppState,
    error::AppError,
    http::{list::contains_pattern, workspace::WorkspaceAccess},
    modules::search::{
        dto::{QuickInvoice, QuickRelation, QuickResults},
        repo,
    },
    numbering::Appendix,
};

/// Each group the caller may list (lots: any member, as the lot routes), searched for `q`
/// (already trimmed, 3 to 100 characters). Invoices match when the digits of `q` are a
/// number; the others contain `q`, any case.
pub async fn quick(
    state: &AppState,
    access: &WorkspaceAccess,
    q: &str,
    limit: i64,
) -> Result<QuickResults, AppError> {
    let (db, ws) = (&state.db, access.workspace_id);
    let pattern = contains_pattern(q);
    let may = |perm: &str| access.allows(perm);
    let products = if may("product.list") {
        Some(repo::products(db, ws, &pattern, limit).await?)
    } else {
        None
    };
    let orders = if may("order.list") {
        Some(repo::orders(db, ws, &pattern, limit).await?)
    } else {
        None
    };
    let invoices = if may("invoice.list") {
        Some(invoices(state, ws, q).await?)
    } else {
        None
    };
    let relations = if may("relation.list") {
        Some(
            repo::relations(db, ws, &pattern, limit)
                .await?
                .into_iter()
                .map(|r| QuickRelation {
                    id: r.id,
                    name: r.name,
                })
                .collect(),
        )
    } else {
        None
    };
    Ok(QuickResults {
        products,
        orders,
        invoices,
        lots: Some(repo::lots(db, ws, &pattern, limit).await?),
        relations,
    })
}

/// The invoice whose number is the digits of `q` (`INV-1000` and `1000` both find 1000).
async fn invoices(
    state: &AppState,
    workspace_id: uuid::Uuid,
    q: &str,
) -> Result<Vec<QuickInvoice>, AppError> {
    let digits: String = q.chars().filter(char::is_ascii_digit).collect();
    let Some(number) = digits.parse::<i64>().ok().filter(|n| *n > 0) else {
        return Ok(Vec::new());
    };
    Ok(repo::invoice(&state.db, workspace_id, number)
        .await?
        .into_iter()
        .map(|i| QuickInvoice {
            id: i.id,
            number: Appendix::from_section(&i.invoice_appendix).format(i.invoice_number),
            status: i.status,
            customer_name: i.customer_name,
        })
        .collect())
}
