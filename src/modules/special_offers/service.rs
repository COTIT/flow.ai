//! Special offer rules (Laravel Store/UpdateSpecialOfferRequest + listeners), checked on the
//! merged row: a legacy offer (no rule type) or a rule offer, dates unless always active, no
//! overlapping offers per product, refs live in the workspace. Plus the active-offer pick.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        products::pricing::Offer,
        special_offers::{
            dto::OfferPatch,
            repo::{self, ListFilter, OfferFields, OfferRow},
        },
    },
};

const RULE_REQUIRED: &str = "The discount rule type field is required when discount type / discount value / condition quantity / condition brand / condition supplier relation id / condition category id / target brand / target supplier relation id / target category id / target product id / priority / apply to cheapest is present.";
const CONDITION: &str = "At least one condition (product_id, condition_brand, condition_supplier_relation_id, or condition_category_id) must be specified.";
const VALUE_OR_DISCOUNT: &str = "Either discount_value or value must be provided.";
const TARGET: &str = "At least one target field must be specified for cross-product discounts.";
const OVERLAP: &str = "There is already an special offer active between the given dates.";
const OVER_PRICE: &str = "The selected value is higher then the current product price.";
const PERCENT: &str = "Only values between 1 and 99 are available";

/// The offer that sets a product's price today: the first active one of `rows` (best first:
/// priority, then newest; user decision 2026-10-21).
pub fn active(rows: &[OfferRow], today: NaiveDate) -> Option<&OfferRow> {
    rows.iter().find(|o| {
        o.always_active
            || matches!((o.from_date, o.to_date), (Some(f), Some(t)) if f <= today && today <= t)
    })
}

/// The pricing view of an offer.
pub fn pricing(o: &OfferRow) -> Offer<'_> {
    Offer {
        kind: &o.kind,
        value: o.value,
        base_price: o.base_price,
        rule: o.discount_rule_type.as_deref(),
        discount_value: o.discount_value,
        discount_type: o.discount_type.as_deref(),
        condition_quantity: o.condition_quantity,
    }
}

/// Page of rows and the total; on the product route the product must be live.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    product: Option<&str>,
    rule: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<OfferRow>, i64), AppError> {
    let filter = ListFilter {
        product: route_product(state, workspace_id, product).await?,
        rule: rule.map(str::trim).filter(|r| !r.is_empty()),
    };
    let rows = repo::list(&state.db, workspace_id, &filter, limit, offset).await?;
    let total = match rows.first() {
        Some(r) => r.total.unwrap_or(0),
        None if offset > 0 => repo::count(&state.db, workspace_id, &filter).await?,
        None => 0,
    };
    Ok((rows, total))
}

/// One live offer (of the route's product), else 404.
pub async fn get(
    state: &AppState,
    workspace_id: Uuid,
    product: Option<&str>,
    id: &str,
) -> Result<OfferRow, AppError> {
    let product = route_product(state, workspace_id, product).await?;
    repo::find(&state.db, workspace_id, parse(id)?, product)
        .await?
        .ok_or(AppError::NotFound)
}

/// Create; on the product route the offer belongs to that product.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    product: Option<&str>,
    patch: OfferPatch,
) -> Result<Uuid, AppError> {
    let route = route_product(state, workspace_id, product).await?;
    let mut f = OfferFields::default();
    apply(&mut f, &patch, route);
    let mut tx = state.db.begin().await?;
    if let Some(p) = f.product_id {
        repo::lock_product(&mut tx, workspace_id, p).await?;
    }
    check(&mut tx, workspace_id, &mut f, &patch, None).await?;
    let id = repo::insert(&mut *tx, workspace_id, &f).await?;
    tx.commit().await?;
    Ok(id)
}

/// Patch a live offer (of the route's product): merged onto the locked row, then checked as a
/// whole, else 404.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    product: Option<&str>,
    id: &str,
    patch: OfferPatch,
) -> Result<(), AppError> {
    let route = route_product(state, workspace_id, product).await?;
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    let row = repo::find_locked(&mut tx, workspace_id, id, route)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut f = OfferFields::from(row);
    apply(&mut f, &patch, route);
    if let Some(p) = f.product_id {
        repo::lock_product(&mut tx, workspace_id, p).await?;
    }
    check(&mut tx, workspace_id, &mut f, &patch, Some(id)).await?;
    repo::update(&mut *tx, workspace_id, id, &f).await?;
    tx.commit().await?;
    Ok(())
}

/// Soft delete a live offer (of the route's product), else 404.
pub async fn delete(
    state: &AppState,
    workspace_id: Uuid,
    product: Option<&str>,
    id: &str,
) -> Result<(), AppError> {
    let product = route_product(state, workspace_id, product).await?;
    repo::soft_delete(&state.db, workspace_id, parse(id)?, product)
        .await?
        .then_some(())
        .ok_or(AppError::NotFound)
}

/// The product of a product route: a live product of the workspace, else 404.
async fn route_product(
    state: &AppState,
    workspace_id: Uuid,
    product: Option<&str>,
) -> Result<Option<Uuid>, AppError> {
    let Some(raw) = product else {
        return Ok(None);
    };
    let id = parse(raw)?;
    match repo::product_price(&state.db, workspace_id, id).await? {
        Some(_) => Ok(Some(id)),
        None => Err(AppError::NotFound),
    }
}

/// Sent keys over the current values. The product route fixes the product; a null flag or
/// priority goes back to its default.
fn apply(f: &mut OfferFields, p: &OfferPatch, route: Option<Uuid>) {
    fn set<T: Clone>(field: &mut T, sent: &Option<T>) {
        if let Some(v) = sent {
            *field = v.clone();
        }
    }
    match route {
        Some(id) => f.product_id = Some(id),
        None => set(&mut f.product_id, &p.product_id),
    }
    set(&mut f.from_date, &p.from_date);
    set(&mut f.to_date, &p.to_date);
    if let Some(v) = p.always_active {
        f.always_active = v.unwrap_or(false);
    }
    set(&mut f.value, &p.value);
    set(&mut f.kind, &p.kind);
    set(&mut f.discount_rule_type, &p.discount_rule_type);
    set(&mut f.condition_quantity, &p.condition_quantity);
    set(&mut f.discount_quantity, &p.discount_quantity);
    set(&mut f.condition_brand, &p.condition_brand);
    set(
        &mut f.condition_supplier_relation_id,
        &p.condition_supplier_relation_id,
    );
    set(&mut f.condition_category_id, &p.condition_category_id);
    set(&mut f.target_brand, &p.target_brand);
    set(
        &mut f.target_supplier_relation_id,
        &p.target_supplier_relation_id,
    );
    set(&mut f.target_category_id, &p.target_category_id);
    set(&mut f.target_product_id, &p.target_product_id);
    set(&mut f.discount_value, &p.discount_value);
    set(&mut f.discount_type, &p.discount_type);
    if let Some(v) = p.priority {
        f.priority = v.unwrap_or(0);
    }
    if let Some(v) = p.apply_to_cheapest {
        f.apply_to_cheapest = v.unwrap_or(false);
    }
    set(&mut f.meta, &p.meta);
}

/// Laravel's rules on the merged offer, then its `validated()` defaults: value from
/// discount_value, an inferred percentage discount type, type from discount type, no dates
/// when always active, and the frozen base price. `except` = the offer being updated.
async fn check(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    f: &mut OfferFields,
    p: &OfferPatch,
    except: Option<Uuid>,
) -> Result<(), AppError> {
    let creating = except.is_none();
    let mut e = Errors::default();
    // Refs sent in this request must be live rows of the workspace.
    let price = match f.product_id {
        Some(id) => repo::product_price(&mut *conn, workspace_id, id).await?,
        None => None,
    };
    if f.product_id.is_some() && price.is_none() {
        e.add("product_id", "The product id does not exist");
    }
    for (field, sent) in [
        (
            "condition_supplier_relation_id",
            &p.condition_supplier_relation_id,
        ),
        (
            "target_supplier_relation_id",
            &p.target_supplier_relation_id,
        ),
    ] {
        if let Some(Some(id)) = sent
            && !repo::relation_live(&mut *conn, workspace_id, *id).await?
        {
            e.add(field, invalid(field));
        }
    }
    for (field, sent) in [
        ("condition_category_id", &p.condition_category_id),
        ("target_category_id", &p.target_category_id),
    ] {
        if let Some(Some(id)) = sent
            && !repo::category_live(&mut *conn, workspace_id, *id).await?
        {
            e.add(field, invalid(field));
        }
    }
    if let Some(Some(id)) = p.target_product_id
        && repo::product_price(&mut *conn, workspace_id, id)
            .await?
            .is_none()
    {
        e.add("target_product_id", invalid("target_product_id"));
    }

    // Laravel inferred a percentage discount type before its rules read it.
    if f.discount_rule_type.is_some()
        && f.discount_type.is_none()
        && f.discount_value.is_some_and(|v| v <= Decimal::ONE_HUNDRED)
    {
        f.discount_type = Some("percentage".into());
    }

    match f.discount_rule_type.as_deref() {
        None if p.has_advanced() => e.add("discount_rule_type", RULE_REQUIRED),
        None => {
            if f.product_id.is_none() {
                e.add("product_id", "The product id field is required.");
            }
            if f.kind.is_none() {
                e.add("type", "The type field is required.");
            }
            // Checked against the price only when the value or type changes.
            let priced = creating || p.value.is_some() || p.kind.is_some();
            match (f.value, f.kind.as_deref(), price) {
                (None, ..) => e.add("value", "The value field is required."),
                (Some(v), Some("fixed"), Some(price)) if priced && v > price => {
                    e.add("value", OVER_PRICE)
                }
                (Some(v), Some("percentage"), _)
                    if priced && (v <= Decimal::ZERO || v >= Decimal::ONE_HUNDRED) =>
                {
                    e.add("value", PERCENT)
                }
                _ => {}
            }
        }
        Some(rule) => {
            let condition = f.product_id.is_some()
                || f.condition_brand.is_some()
                || f.condition_supplier_relation_id.is_some()
                || f.condition_category_id.is_some();
            if rule != "global_discount" && !condition {
                e.add("product_id", CONDITION);
            }
            if f.value.is_none() && f.discount_value.is_none() {
                e.add("discount_value", VALUE_OR_DISCOUNT);
            }
            let target = f.target_brand.is_some()
                || f.target_supplier_relation_id.is_some()
                || f.target_category_id.is_some()
                || f.target_product_id.is_some();
            if rule == "cross_product_discount" && !target {
                e.add("target_brand", TARGET);
            }
            if matches!(rule, "quantity_based" | "bogo" | "bundle_price")
                && f.condition_quantity.is_none()
            {
                e.add(
                    "condition_quantity",
                    "The condition quantity field is required.",
                );
            }
            if rule == "bogo" && f.discount_quantity.is_none() {
                e.add(
                    "discount_quantity",
                    "The discount quantity field is required.",
                );
            }
            // Laravel copied the discount type into `type` when it was left out.
            if f.kind.is_none() {
                f.kind = f.discount_type.clone();
            }
            if f.kind.is_none() {
                e.add("type", "The type field is required.");
            }
        }
    }

    if f.always_active {
        f.from_date = None;
        f.to_date = None;
    } else {
        match (f.from_date, f.to_date) {
            (Some(from), Some(to)) if from >= to => {
                e.add("from_date", "The from date must be a date before to date.");
                e.add("to_date", "The to date must be a date after from date.");
            }
            (Some(from), Some(to)) => {
                if let Some(product) = f.product_id
                    && repo::overlaps(&mut *conn, workspace_id, product, from, to, except).await?
                {
                    e.add("to_date", OVERLAP);
                }
            }
            (from, to) => {
                if from.is_none() {
                    e.add("from_date", "The from date field is required.");
                }
                if to.is_none() {
                    e.add("to_date", "The to date field is required.");
                }
            }
        }
    }
    e.into_result()?;
    f.value = f.value.or(f.discount_value).or(Some(Decimal::ZERO));

    // Laravel froze the product price under a legacy percentage/fixed offer, on create and on
    // every edit of a price field.
    if creating || p.touches_price() {
        let freezes = f.discount_rule_type.is_none()
            && matches!(f.kind.as_deref(), Some("percentage" | "fixed"));
        f.base_price = price.filter(|_| freezes);
    }
    Ok(())
}

/// Laravel `exists` text.
fn invalid(field: &str) -> String {
    format!("The selected {} is invalid.", field.replace('_', " "))
}

#[derive(Default)]
struct Errors(BTreeMap<String, Vec<String>>);

impl Errors {
    fn add(&mut self, field: &str, msg: impl Into<String>) {
        self.0.entry(field.to_owned()).or_default().push(msg.into());
    }

    fn into_result(self) -> Result<(), AppError> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(AppError::Validation(self.0))
        }
    }
}

/// Malformed id = unknown id = 404.
fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}
