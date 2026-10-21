//! Resolve currency, relation and products for a calculate request, then run the engine.
//! Laravel Values\Item + CalculateItemsRequest::getCurrency.

use std::collections::{BTreeMap, HashMap};

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::json::human,
    modules::{
        currencies,
        pricing::{dto::Calculate, engine, repo},
        products,
    },
};

/// Laravel CurrencyExist text.
const NO_CURRENCY: &str = "This currency id does not exist";
/// Laravel `currency.response.no_default_currency`.
const NO_MAIN_CURRENCY: &str = "No default currency has been found. Please create one";

/// A calculate request with products applied and every line priced.
pub struct Priced {
    pub input: Calculate,
    /// The lines of `input.items`, same order.
    pub lines: Vec<engine::Line>,
    pub summary: engine::Summary,
    /// Currency decimals the amounts are rounded to.
    pub dp: u32,
}

/// Price `input` in `workspace_id`. Every id must be a live row of this workspace (422 else).
/// Product name and purchase price are catalog data: applied only when `view_products`
/// (`product.view`); otherwise product lines keep what was sent and nothing is looked up.
pub async fn price(
    state: &AppState,
    workspace_id: Uuid,
    view_products: bool,
    mut input: Calculate,
) -> Result<Priced, AppError> {
    let dp = decimals(state, workspace_id, input.currency_id).await?;
    if let Some(id) = input.relation_id {
        let found = match id {
            Some(id) => repo::relation_exists(&state.db, workspace_id, id).await?,
            None => false,
        };
        if !found {
            return Err(AppError::field(
                "relation_id",
                "The relation id is an invalid id for the relation domain.",
            ));
        }
    }
    if view_products {
        apply_products(state, workspace_id, &mut input).await?;
    }
    let lines: Vec<engine::Line> = input.items.iter().map(|i| i.line).collect();
    let summary = engine::summarise(&lines, dp).map_err(out_of_range)?;
    Ok(Priced {
        input,
        lines,
        summary,
        dp,
    })
}

/// Margin of a priced request; 422 on the line whose cost overflowed.
pub fn margin(priced: &Priced) -> Result<engine::Margin, AppError> {
    engine::margin(&priced.lines, &priced.summary.lines, priced.dp).map_err(out_of_range)
}

fn out_of_range(n: usize) -> AppError {
    let key = format!("items.{n}");
    AppError::field(&key, format!("The {key} is out of range."))
}

/// Decimals of the sent currency, else of the workspace's main currency.
async fn decimals(
    state: &AppState,
    workspace_id: Uuid,
    sent: crate::modules::pricing::dto::RefId,
) -> Result<u32, AppError> {
    let id = match sent {
        Some(Some(id)) => id,
        Some(None) => return Err(AppError::field("currency_id", NO_CURRENCY)),
        None => products::repo::main_currency(&state.db, workspace_id)
            .await?
            .ok_or_else(|| AppError::field("currency_id", NO_MAIN_CURRENCY))?,
    };
    let row = currencies::repo::find(&state.db, workspace_id, id)
        .await?
        .ok_or_else(|| AppError::field("currency_id", NO_CURRENCY))?;
    // CHECK (decimals BETWEEN 0 AND 8) on the column.
    u32::try_from(row.decimals).map_err(|_| AppError::Internal("negative currency decimals".into()))
}

/// Laravel Values\Item constructor: a product line takes the product's name as description and
/// its purchase price when none (or 0) was sent. One query for all lines.
async fn apply_products(
    state: &AppState,
    workspace_id: Uuid,
    input: &mut Calculate,
) -> Result<(), AppError> {
    let wanted: Vec<Uuid> = input
        .items
        .iter()
        .filter_map(|i| {
            i.product_id
                .as_deref()
                .and_then(|s| Uuid::parse_str(s).ok())
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let found: HashMap<Uuid, repo::PricingProduct> = if wanted.is_empty() {
        HashMap::new()
    } else {
        repo::products(&state.db, workspace_id, &wanted)
            .await?
            .into_iter()
            .map(|p| (p.id, p))
            .collect()
    };
    let mut errors = BTreeMap::new();
    for (n, item) in input.items.iter_mut().enumerate() {
        let Some(raw) = item.product_id.as_deref() else {
            continue;
        };
        let Some(p) = Uuid::parse_str(raw).ok().and_then(|id| found.get(&id)) else {
            let key = format!("items.{n}.product_id");
            let msg = format!("The selected {} is invalid.", human(&key));
            errors.insert(key, vec![msg]);
            continue;
        };
        item.description = Some(p.name.clone());
        if item.line.purchase_price.is_none_or(|v| v.is_zero()) {
            item.line.purchase_price = products::pricing::purchase_price(
                p.impact_on_wholesale_price,
                p.wholesale_price,
                p.supplier_wholesale_price,
            );
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::Validation(errors))
    }
}
