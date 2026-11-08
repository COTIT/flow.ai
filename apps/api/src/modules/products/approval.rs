//! Native product and variant money quotes, captured and compared under the same row locks.

use super::{approval_repo, dto, repo, service};
use crate::{
    AppError,
    modules::{
        assistant::{dto::ProposalInput, preconditions, repo::Owner},
        permissions, product_variants,
    },
};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

/// Catalog writes whose resolved monetary defaults need an immutable review.
pub fn requires_facts(operation: &str) -> bool {
    matches!(
        operation,
        "products_create"
            | "products_update"
            | "products_duplicate"
            | "product_variants_create"
            | "product_variants_update"
            | "product_variants_generate"
    )
}

/// Permission precedes every catalog read; public visibility is checked separately.
#[tracing::instrument(name = "products_capture", level = "debug", skip_all)]
pub async fn capture(
    conn: &mut PgConnection,
    owner: Owner,
    input: &ProposalInput,
) -> Result<Option<Value>, AppError> {
    let operation = input.operation_id.as_str();
    if !requires_facts(operation) {
        return Ok(None);
    }
    let permission = match operation {
        "products_update" | "product_variants_update" => "product.update",
        "products_duplicate" => "product.duplicate",
        _ => "product.create",
    };
    let access = permissions::repo::access(&mut *conn, owner.workspace_id, owner.user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let grants = permissions::model::Granted::new(access.owner, access.permissions);
    if !access.active || !grants.allows(permission) {
        return Err(AppError::Forbidden(
            "You do not have permission for this action.",
        ));
    }
    let workspace = owner.workspace_id;
    let body = input.body.clone().unwrap_or_else(|| json!({}));
    let mut current = None;
    let (kind, priced) = match operation {
        "products_create" => {
            let fields = preconditions::validated::<dto::CreateProductRequest>(body)?;
            let row = service::prepare_create(conn, workspace, &fields).await?;
            ("product", product_quote(conn, workspace, &row).await?)
        }
        "products_update" | "products_duplicate" => {
            let id = target(input, 5)?;
            let before = repo::find_locked(&mut *conn, workspace, id)
                .await?
                .ok_or(AppError::NotFound)?;
            let row = if operation == "products_update" {
                let fields = preconditions::validated::<dto::UpdateProductRequest>(body)?;
                current = Some(product_quote(conn, workspace, &before).await?);
                service::prepare_update(conn, workspace, &before, &fields).await?
            } else {
                service::prepare_create(conn, workspace, &service::duplicate_fields(&before))
                    .await?
            };
            ("product", product_quote(conn, workspace, &row).await?)
        }
        _ => {
            let id = target(input, 5)?;
            let product = repo::find_locked(&mut *conn, workspace, id)
                .await?
                .ok_or(AppError::NotFound)?;
            let priced = match operation {
                "product_variants_generate" => {
                    let fields =
                        preconditions::validated::<product_variants::dto::GenerateBody>(body)?;
                    generation_quote(conn, workspace, &product, &fields).await?
                }
                "product_variants_update" => {
                    let fields =
                        preconditions::validated::<product_variants::dto::VariantBody>(body)?;
                    let before = product_variants::repo::find_locked(
                        &mut *conn,
                        workspace,
                        id,
                        target(input, 7)?,
                    )
                    .await?
                    .ok_or(AppError::NotFound)?;
                    current = Some(variant_quote(conn, workspace, &product, &before).await?);
                    let row =
                        product_variants::service::prepare_update(&before, &fields, product.price)?;
                    variant_quote(conn, workspace, &product, &row).await?
                }
                _ => {
                    let fields =
                        preconditions::validated::<product_variants::dto::VariantBody>(body)?;
                    let row =
                        product_variants::service::prepare_create(id, &fields, product.price)?;
                    variant_quote(conn, workspace, &product, &row).await?
                }
            };
            ("product_variant", priced)
        }
    };
    let mut facts = json!({"kind":kind,"priced":priced});
    if let Some(before) = current {
        // Variant quotes nest the row under `variant`; currency always comes from the parent product.
        let (old, new, currency) = match kind {
            "product" => (&before, &facts["priced"], &before["currency"]),
            _ => (
                &before["variant"],
                &facts["priced"]["variant"],
                &before["product"]["currency"],
            ),
        };
        facts["changes"] = Value::from(changes(old, new, currency));
        facts["before"] = before;
    }
    let review = json!({"version":1,"operation_id":operation,"facts":facts});
    if review.to_string().len() > 32768 {
        return Err(AppError::field(
            "body",
            "The material review exceeds 32768 bytes. Narrow this action.",
        ));
    }
    Ok(Some(review))
}

fn target(input: &ProposalInput, index: usize) -> Result<Uuid, AppError> {
    input
        .path
        .split('/')
        .nth(index)
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or(AppError::NotFound)
}
/// Money fields shown as exact currency amounts on the review.
const MONEY: [&str; 8] = [
    "price",
    "gross_price",
    "original_price",
    "wholesale_price",
    "supplier_wholesale_price",
    "original_wholesale_price",
    "impact_on_price",
    "impact_on_wholesale_price",
];
/// Money fields whose large relative change gets a warning.
const WATCHED: [&str; 3] = ["price", "gross_price", "wholesale_price"];
/// Warn at ten times up (+900%) or a 90% cut, in basis points.
const UP_BP: i64 = 90_000;
const DOWN_BP: i64 = -9_000;

/// Fields that differ between current and proposed quote; money exact, ratio in signed basis points.
fn changes(before: &Value, after: &Value, currency: &Value) -> Vec<Value> {
    let (Some(old), Some(new)) = (before.as_object(), after.as_object()) else {
        return Vec::new();
    };
    let decimals = currency["decimals"]
        .as_u64()
        .and_then(|d| u32::try_from(d).ok());
    new.iter()
        .filter(|(field, value)| !field.ends_with("_id") && old.get(*field) != Some(value))
        .map(|(field, value)| {
            let previous = old.get(field).unwrap_or(&Value::Null);
            if !MONEY.contains(&field.as_str()) {
                return json!({"field":field,"before":previous,"after":value});
            }
            let mut entry = json!({"field":field,"currency":{"code":currency["code"],"decimals":currency["decimals"]}});
            match decimals.and_then(|d| Some((minor(previous, d)?, minor(value, d)?))) {
                Some((b, a)) => {
                    entry["before_minor"] = b;
                    entry["after_minor"] = a;
                }
                None => {
                    entry["before"] = previous.clone();
                    entry["after"] = value.clone();
                }
            }
            if let (Some(b), Some(a)) = (amount(previous), amount(value))
                && b > Decimal::ZERO
                && let Some(bp) = ((a - b) * Decimal::from(10_000) / b).trunc().to_i64()
            {
                entry["change_bp"] = bp.into();
                if WATCHED.contains(&field.as_str()) && (bp >= UP_BP || bp <= DOWN_BP) {
                    entry["warning"] = "large_change".into();
                }
            }
            entry
        })
        .collect()
}
fn amount(value: &Value) -> Option<Decimal> {
    value.as_str()?.parse().ok()
}
/// Integer minor units only when exact at currency decimals; null stays null.
fn minor(value: &Value, decimals: u32) -> Option<Value> {
    if value.is_null() {
        return Some(Value::Null);
    }
    let scaled = amount(value)?.checked_mul(Decimal::from(10_i64.checked_pow(decimals)?))?;
    scaled
        .fract()
        .is_zero()
        .then(|| scaled.to_i64())
        .flatten()
        .map(Value::from)
}
fn decimal(value: Decimal) -> String {
    value.normalize().to_string()
}

/// Exact native prices and unit meaning; create/copy use null rather than a borrowed source id.
pub(super) async fn product_quote(
    conn: &mut PgConnection,
    workspace: Uuid,
    row: &repo::ProductRow,
) -> Result<Value, AppError> {
    let currency = approval_repo::currency(&mut *conn, workspace, row.currency_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let vat = match row.vat_group_id {
        Some(id) => approval_repo::vat(conn, workspace, id)
            .await?
            .ok_or(AppError::NotFound)?,
        None => Value::Null,
    };
    Ok(
        json!({"product_id":(!row.id.is_nil()).then_some(row.id),"name":row.name,"currency":currency,"vat_group":vat,"price":decimal(row.price),"gross_price":decimal(row.gross_price),"wholesale_price":row.wholesale_price.map(decimal),"supplier_wholesale_price":row.supplier_wholesale_price.map(decimal),"impact_on_price":row.impact_on_price.map(decimal),"impact_on_wholesale_price":row.impact_on_wholesale_price.map(decimal),"order_unit":decimal(row.order_unit),"sale_unit":decimal(row.sale_unit),"unit":row.unit}),
    )
}
fn variant_values(row: &product_variants::repo::VariantRow) -> Value {
    json!({"variant_id":(!row.id.is_nil()).then_some(row.id),"sku":row.sku,"is_default":row.is_default,"price":decimal(row.price),"gross_price":decimal(row.gross_price),"original_price":row.original_price.map(decimal),"wholesale_price":row.wholesale_price.map(decimal),"supplier_wholesale_price":row.supplier_wholesale_price.map(decimal),"original_wholesale_price":row.original_wholesale_price.map(decimal),"impact_on_price":row.impact_on_price.map(decimal),"impact_on_wholesale_price":row.impact_on_wholesale_price.map(decimal)})
}

/// A variant and the parent currency, VAT and price it inherits.
pub(crate) async fn variant_quote(
    conn: &mut PgConnection,
    workspace: Uuid,
    product: &repo::ProductRow,
    row: &product_variants::repo::VariantRow,
) -> Result<Value, AppError> {
    Ok(
        json!({"product":product_quote(conn,workspace,product).await?,"variant":variant_values(row)}),
    )
}

/// Whole generated set remains bound while the visible preview stays bounded.
pub(crate) async fn generation_quote(
    conn: &mut PgConnection,
    workspace: Uuid,
    product: &repo::ProductRow,
    fields: &product_variants::dto::Generate,
) -> Result<Value, AppError> {
    let variants: Vec<Value> = product_variants::service::generation_rows(product, fields)?
        .iter()
        .map(variant_values)
        .collect();
    let parent = product_quote(conn, workspace, product).await?;
    let full = json!({"product":parent,"variants":variants});
    let digest = hex::encode(Sha256::digest(full.to_string().as_bytes()));
    Ok(
        json!({"product":full["product"],"variant_count":variants.len(),"variants_preview":variants.iter().take(5).collect::<Vec<_>>(),"variants_omitted":variants.len().saturating_sub(5),"material_sha256":digest}),
    )
}

/// Native writes skip quote work unless an assistant review expects this resource kind.
pub(super) async fn check_product(
    conn: &mut PgConnection,
    workspace: Uuid,
    row: &repo::ProductRow,
) -> Result<(), AppError> {
    if preconditions::expects("product") {
        let actual = product_quote(conn, workspace, row).await?;
        preconditions::check_priced(workspace, "product", &actual)?;
    }
    Ok(())
}

/// Compare the resolved native variant before any row or default-switch write.
pub(crate) async fn check_variant(
    conn: &mut PgConnection,
    workspace: Uuid,
    product: &repo::ProductRow,
    row: &product_variants::repo::VariantRow,
) -> Result<(), AppError> {
    if preconditions::expects("product_variant") {
        let actual = variant_quote(conn, workspace, product, row).await?;
        preconditions::check_priced(workspace, "product_variant", &actual)?;
    }
    Ok(())
}

/// Check all generated monetary defaults before option rows or variants are created.
pub(crate) async fn check_generation(
    conn: &mut PgConnection,
    workspace: Uuid,
    product: &repo::ProductRow,
    fields: &product_variants::dto::Generate,
) -> Result<(), AppError> {
    if preconditions::expects("product_variant") {
        let actual = generation_quote(conn, workspace, product, fields).await?;
        preconditions::check_priced(workspace, "product_variant", &actual)?;
    }
    Ok(())
}
