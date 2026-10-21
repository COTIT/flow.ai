//! Special offer rules (Laravel Store/UpdateSpecialOfferRequest + listeners), checked on the
//! merged row: a legacy offer (no rule type) or a rule offer, dates unless always active, no
//! overlapping offers per product, refs live in the workspace. Plus the active-offer pick.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::json::{Rules, human},
    modules::{
        products::pricing::Offer,
        special_offers::{
            dto::{OfferPatch, TYPES},
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

/// Live offers one product may hold; every product detail loads them all. Laravel had no cap.
pub const MAX_PER_PRODUCT: i64 = 100;

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
    room(&mut tx, workspace_id, f.product_id).await?;
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
    let before = row.product_id;
    let mut f = OfferFields::from(row);
    apply(&mut f, &patch, route);
    if let Some(p) = f.product_id {
        repo::lock_product(&mut tx, workspace_id, p).await?;
    }
    check(&mut tx, workspace_id, &mut f, &patch, Some(id)).await?;
    if f.product_id != before {
        room(&mut tx, workspace_id, f.product_id).await?;
    }
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

/// 422 when `product` already holds `MAX_PER_PRODUCT` offers (caller holds its lock).
async fn room(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    product: Option<Uuid>,
) -> Result<(), AppError> {
    let Some(product) = product else {
        return Ok(());
    };
    if repo::count_for_product(&mut *conn, workspace_id, product).await? >= MAX_PER_PRODUCT {
        return Err(AppError::field(
            "special_offers",
            format!("The product can't hold more than {MAX_PER_PRODUCT} special offers."),
        ));
    }
    Ok(())
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

/// Laravel's rules on the merged offer, then its `validated()` defaults. `except` = the offer
/// being updated.
async fn check(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    f: &mut OfferFields,
    p: &OfferPatch,
    except: Option<Uuid>,
) -> Result<(), AppError> {
    let creating = except.is_none();
    let mut r = Rules::default();
    let price = match f.product_id {
        Some(id) => repo::product_price(&mut *conn, workspace_id, id).await?,
        None => None,
    };
    if f.product_id.is_some() && price.is_none() {
        r.add("product_id", "The product id does not exist");
    }
    ref_errors(conn, workspace_id, p, &mut r).await?;
    // Laravel inferred a percentage discount type when a 0..100 discount value came in.
    if f.discount_rule_type.is_some()
        && f.discount_type.is_none()
        && (creating || p.discount_value.is_some() || p.discount_rule_type.is_some())
        && f.discount_value.is_some_and(|v| v <= Decimal::ONE_HUNDRED)
    {
        f.discount_type = Some("percentage".into());
    }
    match f.discount_rule_type.clone() {
        None if p.has_advanced() => r.add("discount_rule_type", RULE_REQUIRED),
        None => legacy_errors(f, p, price, creating, &mut r),
        Some(rule) => rule_errors(f, &rule, &mut r),
    }
    let dates_sent = creating
        || p.from_date.is_some()
        || p.to_date.is_some()
        || p.always_active.is_some()
        || p.product_id.is_some();
    if !f.always_active {
        window_errors(conn, workspace_id, f, except, dates_sent, &mut r).await?;
    }
    r.into_result()?;
    defaults(f, p, price, creating);
    Ok(())
}

/// Sent supplier, category and target product ids must be live rows of the workspace.
async fn ref_errors(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    p: &OfferPatch,
    r: &mut Rules,
) -> Result<(), AppError> {
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
            r.add(field, invalid(field));
        }
    }
    for (field, sent) in [
        ("condition_category_id", &p.condition_category_id),
        ("target_category_id", &p.target_category_id),
    ] {
        if let Some(Some(id)) = sent
            && !repo::category_live(&mut *conn, workspace_id, *id).await?
        {
            r.add(field, invalid(field));
        }
    }
    if let Some(Some(id)) = p.target_product_id
        && repo::product_price(&mut *conn, workspace_id, id)
            .await?
            .is_none()
    {
        r.add("target_product_id", invalid("target_product_id"));
    }
    Ok(())
}

/// A legacy offer: product, a legacy type and a value; the value checked against the price
/// when the value or type is written.
fn legacy_errors(
    f: &OfferFields,
    p: &OfferPatch,
    price: Option<Decimal>,
    creating: bool,
    r: &mut Rules,
) {
    if f.product_id.is_none() {
        r.add("product_id", "The product id field is required.");
    }
    match f.kind.as_deref() {
        None => r.add("type", "The type field is required."),
        // A rule offer's type can come from its discount type; legacy needs a real one.
        Some(k) if !TYPES.contains(&k) => {
            r.add("type", format!("The selected type: {k} is invalid."))
        }
        Some(_) => {}
    }
    let priced = creating || p.value.is_some() || p.kind.is_some();
    match (f.value, f.kind.as_deref(), price) {
        (None, ..) => r.add("value", "The value field is required."),
        (Some(v), Some("fixed"), Some(price)) if priced && v > price => r.add("value", OVER_PRICE),
        (Some(v), Some("percentage"), _)
            if priced && (v <= Decimal::ZERO || v >= Decimal::ONE_HUNDRED) =>
        {
            r.add("value", PERCENT)
        }
        _ => {}
    }
}

/// A rule offer: a condition unless global, a value, the rule's own fields, and a type (from
/// the discount type when left out, as Laravel copied it).
fn rule_errors(f: &mut OfferFields, rule: &str, r: &mut Rules) {
    let condition = f.product_id.is_some()
        || f.condition_brand.is_some()
        || f.condition_supplier_relation_id.is_some()
        || f.condition_category_id.is_some();
    if rule != "global_discount" && !condition {
        r.add("product_id", CONDITION);
    }
    // Laravel's `! $value`: zero counts as missing.
    let missing = |v: Option<Decimal>| v.is_none_or(|v| v.is_zero());
    if missing(f.value) && missing(f.discount_value) {
        r.add("discount_value", VALUE_OR_DISCOUNT);
    }
    let target = f.target_brand.is_some()
        || f.target_supplier_relation_id.is_some()
        || f.target_category_id.is_some()
        || f.target_product_id.is_some();
    if rule == "cross_product_discount" && !target {
        r.add("target_brand", TARGET);
    }
    if matches!(rule, "quantity_based" | "bogo" | "bundle_price") && f.condition_quantity.is_none()
    {
        r.missing("condition_quantity");
    }
    if rule == "bogo" && f.discount_quantity.is_none() {
        r.missing("discount_quantity");
    }
    if f.kind.is_none() {
        f.kind = f.discount_type.clone();
    }
    if f.kind.is_none() {
        r.missing("type");
    }
}

/// Both dates, start before end, and (when a date, the flag or the product changes) no
/// overlap with another offer of the product.
async fn window_errors(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    f: &OfferFields,
    except: Option<Uuid>,
    changed: bool,
    r: &mut Rules,
) -> Result<(), AppError> {
    match (f.from_date, f.to_date) {
        (Some(from), Some(to)) if from >= to => {
            r.add("from_date", "The from date must be a date before to date.");
            r.add("to_date", "The to date must be a date after from date.");
        }
        (Some(from), Some(to)) => {
            if let Some(product) = f.product_id.filter(|_| changed)
                && repo::overlaps(&mut *conn, workspace_id, product, from, to, except).await?
            {
                r.add("to_date", OVERLAP);
            }
        }
        (from, to) => {
            if from.is_none() {
                r.missing("from_date");
            }
            if to.is_none() {
                r.missing("to_date");
            }
        }
    }
    Ok(())
}

/// Laravel `validated()` and its listeners: value from discount_value (or 0), no dates when
/// always active, and the frozen product price under a legacy percentage/fixed offer (on
/// create and on every edit of a price field).
fn defaults(f: &mut OfferFields, p: &OfferPatch, price: Option<Decimal>, creating: bool) {
    f.value = f.value.or(f.discount_value).or(Some(Decimal::ZERO));
    if f.always_active {
        f.from_date = None;
        f.to_date = None;
    }
    if creating || p.touches_price() {
        let freezes = f.discount_rule_type.is_none()
            && matches!(f.kind.as_deref(), Some("percentage" | "fixed"));
        f.base_price = price.filter(|_| freezes);
    }
}

/// Laravel `exists` text.
fn invalid(field: &str) -> String {
    format!("The selected {} is invalid.", human(field))
}

/// Malformed id = unknown id = 404.
fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}
