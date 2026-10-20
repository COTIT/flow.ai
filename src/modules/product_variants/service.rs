//! Variant rules: reached through a live product; prices normalised against the product's
//! price; one default per product; price history; generate from option types.

use std::collections::HashMap;

use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        product_variants::{
            dto::{Fields, Generate, render},
            repo::{self, VariantRow},
        },
        products::{
            pricing::{Priority, normalise, out_of_range},
            repo::{self as products, PriceChange, ProductRow},
            service::{Actor, render_index},
        },
    },
};

/// The path's live product (row + list-shape JSON), else 404.
async fn product(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
) -> Result<(ProductRow, Value), AppError> {
    render_index(state, workspace_id, parse(id)?).await
}

/// Lock the live product row: serialises default switches and reads its current price.
async fn lock_product(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<ProductRow, AppError> {
    products::find_locked(conn, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)
}

/// Render `rows` with their option values and the parent product.
async fn render_all(
    state: &AppState,
    workspace_id: Uuid,
    rows: &[VariantRow],
    product: &Value,
) -> Result<Vec<Value>, AppError> {
    let ids: Vec<Uuid> = rows.iter().map(|v| v.id).collect();
    let values = repo::option_values(&state.db, workspace_id, &ids).await?;
    let mut by_variant: HashMap<Uuid, Vec<&repo::OptionValueRow>> = HashMap::new();
    for o in &values {
        by_variant.entry(o.variant_id).or_default().push(o);
    }
    Ok(rows
        .iter()
        .map(|v| {
            render(
                v,
                product,
                by_variant.get(&v.id).map_or(&[][..], Vec::as_slice),
                workspace_id,
            )
        })
        .collect())
}

/// Every live variant of the product, oldest first.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    product_id: &str,
) -> Result<Vec<Value>, AppError> {
    let (p, pv) = product(state, workspace_id, product_id).await?;
    let rows = repo::list(&state.db, workspace_id, p.id).await?;
    render_all(state, workspace_id, &rows, &pv).await
}

/// One variant of the product, else 404.
pub async fn get(
    state: &AppState,
    workspace_id: Uuid,
    product_id: &str,
    id: &str,
) -> Result<Value, AppError> {
    let (p, pv) = product(state, workspace_id, product_id).await?;
    let v = repo::find(&state.db, workspace_id, p.id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)?;
    render_all(state, workspace_id, std::slice::from_ref(&v), &pv)
        .await?
        .pop()
        .ok_or(AppError::NotFound)
}

/// Create (Laravel CreateProductVariant); returns the new id.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    product_id: &str,
    f: Fields,
    actor: Actor,
) -> Result<Uuid, AppError> {
    let product_id = parse(product_id)?;
    let now = chrono::Utc::now();
    let mut v = VariantRow {
        id: Uuid::nil(),
        product_id,
        is_default: false,
        sku: None,
        barcode: None,
        ean13: None,
        price: Decimal::ZERO,
        gross_price: Decimal::ZERO,
        original_price: None,
        wholesale_price: None,
        original_wholesale_price: None,
        supplier_wholesale_price: None,
        impact_on_price: None,
        impact_on_wholesale_price: None,
        weight: None,
        reference_vendor: None,
        reference_vendor2: None,
        reference_vendor3: None,
        meta: None,
        created_at: now,
        updated_at: now,
    };
    apply(&mut v, &f);
    let priority = if f.price.flatten().is_some() {
        Priority::Price
    } else {
        Priority::Impact
    };
    let mut tx = state.db.begin().await?;
    let p = lock_product(&mut tx, workspace_id, product_id).await?;
    normalise_variant(&mut v, p.price, priority)?;
    if v.is_default {
        // Uuid::nil() is never a real id, so this clears every current default.
        repo::clear_default(&mut tx, workspace_id, p.id, Uuid::nil()).await?;
    }
    v.id = repo::insert(&mut tx, workspace_id, &v).await?;
    let changes = created_prices(&v);
    products::insert_history(
        &mut tx,
        workspace_id,
        p.id,
        Some(v.id),
        &changes,
        Some(actor.user_id),
        actor.flow,
    )
    .await?;
    tx.commit().await?;
    Ok(v.id)
}

/// Patch (Laravel UpdateProductVariant); returns the variant rendered.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    product_id: &str,
    id: &str,
    f: Fields,
    actor: Actor,
) -> Result<Value, AppError> {
    let (p, pv) = product(state, workspace_id, product_id).await?;
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    let p = lock_product(&mut tx, workspace_id, p.id).await?;
    let before = repo::find_locked(&mut tx, workspace_id, p.id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut v = before.clone();
    apply(&mut v, &f);
    if before.is_default && !v.is_default {
        return Err(AppError::field(
            "is_default",
            "The default variant stays the default until another variant takes over.",
        ));
    }
    if v.price != before.price || v.impact_on_price != before.impact_on_price {
        let priority = if v.price != before.price {
            Priority::Price
        } else {
            Priority::Impact
        };
        normalise_variant(&mut v, p.price, priority)?;
    }
    // Clear the old default first: the one-default index checks each statement.
    if v.is_default && !before.is_default {
        repo::clear_default(&mut tx, workspace_id, p.id, v.id).await?;
    }
    repo::update(&mut tx, workspace_id, &v).await?;
    let changes = changed_prices(&before, &v);
    products::insert_history(
        &mut tx,
        workspace_id,
        p.id,
        Some(v.id),
        &changes,
        Some(actor.user_id),
        actor.flow,
    )
    .await?;
    tx.commit().await?;
    // Re-read: the row's `updated_at` comes from the database.
    let v = repo::find(&state.db, workspace_id, p.id, v.id)
        .await?
        .ok_or(AppError::NotFound)?;
    render_all(state, workspace_id, std::slice::from_ref(&v), &pv)
        .await?
        .pop()
        .ok_or(AppError::NotFound)
}

/// Soft delete. The default variant stays: orders and the POS resolve it (Laravel 500'd).
pub async fn delete(
    state: &AppState,
    workspace_id: Uuid,
    product_id: &str,
    id: &str,
) -> Result<(), AppError> {
    let mut tx = state.db.begin().await?;
    let p = lock_product(&mut tx, workspace_id, parse(product_id)?).await?;
    let v = repo::find_locked(&mut tx, workspace_id, p.id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)?;
    if v.is_default {
        return Err(AppError::field(
            "variant",
            "The default variant can't be deleted.",
        ));
    }
    repo::soft_delete(&mut tx, workspace_id, v.id).await?;
    tx.commit().await?;
    Ok(())
}

/// Laravel GenerateProductVariantsController: option types/values firstOrCreate, one
/// variant per resolvable combination, all in one transaction. Returns the new variants.
pub async fn generate(
    state: &AppState,
    workspace_id: Uuid,
    product_id: &str,
    g: Generate,
    actor: Actor,
) -> Result<Vec<Value>, AppError> {
    let (p, pv) = product(state, workspace_id, product_id).await?;
    let mut tx = state.db.begin().await?;
    let p = lock_product(&mut tx, workspace_id, p.id).await?;
    repo::lock_options(&mut tx, workspace_id).await?;
    let mut by_index: Vec<Vec<Uuid>> = Vec::new();
    let mut by_key: HashMap<String, Uuid> = HashMap::new();
    for (ot, t) in g.option_types.iter().enumerate() {
        let type_id = repo::option_type(&mut tx, workspace_id, &t.name).await?;
        let sort = t
            .sort_order
            .unwrap_or(i32::try_from(ot).unwrap_or_default());
        repo::attach_type(&mut tx, workspace_id, p.id, type_id, sort).await?;
        let mut ids = Vec::new();
        for (vi, (value, sort)) in t.values.iter().enumerate() {
            let sort = sort.unwrap_or(i32::try_from(vi).unwrap_or_default());
            let id = repo::option_value(&mut tx, workspace_id, type_id, value, sort).await?;
            by_key.insert(format!("{value}-{ot}-{vi}"), id);
            by_key.insert(value.clone(), id);
            ids.push(id);
        }
        by_index.push(ids);
    }
    let default_price = g.default_price.unwrap_or(p.price);
    let default_stock = g.default_stock.unwrap_or_default();
    let mut made = Vec::new();
    for (i, c) in g.combinations.iter().enumerate() {
        let ids = resolve(&c.ids, &by_key, &by_index);
        if ids.is_empty() {
            continue;
        }
        let price = c.price.unwrap_or(default_price);
        let sku = c.sku.clone().or_else(|| {
            g.sku_pattern
                .as_ref()
                .map(|pat| pat.replace("{index}", &(i + 1).to_string()))
        });
        let now = chrono::Utc::now();
        let mut v = VariantRow {
            id: Uuid::nil(),
            product_id: p.id,
            is_default: false,
            sku,
            barcode: None,
            ean13: None,
            price,
            gross_price: price,
            original_price: Some(price),
            wholesale_price: p.wholesale_price,
            original_wholesale_price: p.wholesale_price,
            supplier_wholesale_price: p.supplier_wholesale_price,
            impact_on_price: p.impact_on_price,
            impact_on_wholesale_price: p.impact_on_wholesale_price,
            weight: p.weight,
            reference_vendor: p.reference_vendor.clone(),
            reference_vendor2: p.reference_vendor2.clone(),
            reference_vendor3: p.reference_vendor3.clone(),
            meta: Some(json!({ "stock": number(c.stock.unwrap_or(default_stock)) })),
            created_at: now,
            updated_at: now,
        };
        normalise_variant(&mut v, p.price, Priority::Price)?;
        v.id = repo::insert(&mut tx, workspace_id, &v).await?;
        repo::link_values(&mut tx, workspace_id, v.id, &ids).await?;
        let changes = created_prices(&v);
        products::insert_history(
            &mut tx,
            workspace_id,
            p.id,
            Some(v.id),
            &changes,
            Some(actor.user_id),
            actor.flow,
        )
        .await?;
        made.push(v);
    }
    tx.commit().await?;
    render_all(state, workspace_id, &made, &pv).await
}

/// Raw combination ids -> option value ids: a synthetic key or value first, then a trailing
/// `-{type}-{value}` index pair (Laravel resolveOptionValueIds).
fn resolve(raw: &[String], by_key: &HashMap<String, Uuid>, by_index: &[Vec<Uuid>]) -> Vec<Uuid> {
    raw.iter()
        .filter_map(|r| {
            by_key.get(r).copied().or_else(|| {
                let mut parts = r.rsplitn(3, '-');
                let vi: usize = parts.next()?.parse().ok()?;
                let ot: usize = parts.next()?.parse().ok()?;
                by_index.get(ot)?.get(vi).copied()
            })
        })
        .collect()
}

fn apply(v: &mut VariantRow, f: &Fields) {
    macro_rules! set {
        ($($field:ident),* $(,)?) => {
            $( if let Some(x) = &f.$field { v.$field = x.clone(); } )*
        };
    }
    set!(
        sku,
        barcode,
        ean13,
        original_price,
        wholesale_price,
        original_wholesale_price,
        supplier_wholesale_price,
        impact_on_price,
        impact_on_wholesale_price,
        weight,
        reference_vendor,
        reference_vendor2,
        reference_vendor3,
        meta,
    );
    if let Some(Some(x)) = f.price {
        v.price = x;
    }
    if let Some(Some(x)) = f.gross_price {
        v.gross_price = x;
    }
    if let Some(x) = f.is_default {
        v.is_default = x;
    }
}

/// Laravel ProductVariant saving hook: the product's price is the gross for the markup.
fn normalise_variant(
    v: &mut VariantRow,
    product_price: Decimal,
    priority: Priority,
) -> Result<(), AppError> {
    let (price, impact) = normalise(
        Some(product_price),
        Some(v.price),
        v.impact_on_price,
        priority,
    )
    .map_err(out_of_range)?;
    if let Some(price) = price {
        v.price = price;
    }
    v.impact_on_price = impact;
    Ok(())
}

fn created_prices(v: &VariantRow) -> Vec<PriceChange> {
    [
        ("gross_price", Some(v.gross_price)),
        ("price", Some(v.price)),
        ("supplier_wholesale_price", v.supplier_wholesale_price),
        ("wholesale_price", v.wholesale_price),
    ]
    .into_iter()
    .filter(|(_, x)| x.is_some())
    .map(|(f, x)| (f, None, x))
    .collect()
}

fn changed_prices(a: &VariantRow, b: &VariantRow) -> Vec<PriceChange> {
    [
        ("gross_price", Some(a.gross_price), Some(b.gross_price)),
        ("price", Some(a.price), Some(b.price)),
        (
            "supplier_wholesale_price",
            a.supplier_wholesale_price,
            b.supplier_wholesale_price,
        ),
        ("wholesale_price", a.wholesale_price, b.wholesale_price),
    ]
    .into_iter()
    .filter(|(_, x, y)| x != y)
    .collect()
}

/// A JSON number as the client sent it: whole -> integer, else float (Laravel kept the input).
fn number(d: Decimal) -> Value {
    if d.fract().is_zero() {
        d.to_i64().map_or(Value::Null, Value::from)
    } else {
        d.to_f64().map_or(Value::Null, Value::from)
    }
}

/// Malformed id = unknown id = 404.
fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}
