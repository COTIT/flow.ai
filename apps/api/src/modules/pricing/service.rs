//! Resolve currency, relation and products for a calculate request, then run the engine.
//! Laravel Values\Item + CalculateItemsRequest::getCurrency.

use std::collections::{BTreeMap, HashMap};

use rust_decimal::Decimal;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::json::human,
    modules::{
        currencies, discounts,
        pricing::{dto::Calculate, engine, repo},
        products, special_offers,
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
/// (`product.view`); a relation's discount applies for everyone.
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
    apply_products(state, workspace_id, &mut input, view_products).await?;
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

/// Laravel Values\Item constructor. Every product line must name a live product. With a
/// `relation_id`, the customer's discount fills `productDiscount` when none was sent (Laravel
/// GetProductPrice; for every member, since it decides the total). Name and purchase price are
/// catalog data: only with `view_products`. Three queries at most, whatever the line count.
async fn apply_products(
    state: &AppState,
    workspace_id: Uuid,
    input: &mut Calculate,
    view_products: bool,
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
    if input.items.iter().all(|i| i.product_id.is_none()) {
        return Ok(());
    }
    let found: HashMap<Uuid, repo::PricingProduct> =
        repo::products(&state.db, workspace_id, &wanted)
            .await?
            .into_iter()
            .map(|p| (p.id, p))
            .collect();
    let purchase: HashMap<Uuid, Decimal> = if view_products {
        discounts::repo::purchase_discounts(&state.db, workspace_id, &wanted)
            .await?
            .into_iter()
            .collect()
    } else {
        HashMap::new()
    };
    let relation_discounts: Option<HashMap<Uuid, Decimal>> = match input.relation_id {
        Some(Some(relation)) => Some(
            discounts::repo::relation_discounts(&state.db, workspace_id, relation, &wanted)
                .await?
                .into_iter()
                .collect(),
        ),
        _ => None,
    };
    let today = chrono::Utc::now().date_naive();
    let mut offers: HashMap<Uuid, Vec<special_offers::repo::OfferRow>> = HashMap::new();
    if relation_discounts.is_some() {
        let active =
            special_offers::repo::for_products(&state.db, workspace_id, &wanted, Some(today));
        for o in active.await? {
            if let Some(product) = o.product_id {
                offers.entry(product).or_default().push(o);
            }
        }
    }
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
        if view_products {
            item.description = Some(p.name.clone());
            if item.line.purchase_price.is_none_or(|v| v.is_zero()) {
                item.line.purchase_price = products::pricing::purchase_price(
                    p.impact_on_wholesale_price,
                    p.wholesale_price,
                    p.supplier_wholesale_price,
                    purchase.get(&p.id).copied(),
                    p.gross_price,
                );
            }
        }
        if let Some(by_product) = &relation_discounts {
            let customer = by_product.get(&p.id).copied().unwrap_or_default();
            // Laravel GetProductPrice: a bigger percentage offer cancels the customer discount.
            let offer = offers
                .get(&p.id)
                .and_then(|o| special_offers::service::active(o, today));
            let customer = match offer {
                Some(o) if o.kind == "percentage" && o.value > customer => Decimal::ZERO,
                _ => customer,
            };
            if !item.discount_sent {
                item.line.discount = customer;
            }
            if item.client_default_discount.is_none_or(|d| d.is_zero()) {
                item.client_default_discount = Some(customer);
            }
            // Laravel: a line discount (typed or filled) replaces the default one.
            if item.line.discount > Decimal::ZERO {
                item.client_default_discount = Some(Decimal::ZERO);
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::Validation(errors))
    }
}
