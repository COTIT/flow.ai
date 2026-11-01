//! Till catalog bodies: what the till needs to sell offline. Never purchase prices, supplier
//! data or other warehouses' stock.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::list::Meta,
    modules::{
        storefront::dto::VariantOption,
        till_catalog::repo::{ProductRow, VariantRow},
    },
};

/// A product the register sells.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "A product as the till sells it. `unit_price` is what `POST /till/sales` \
    books (minor units excl VAT, the register's currency)."
)]
pub struct TillProduct {
    id: Uuid,
    name: String,
    /// physical, digital, service, giftcard, …
    r#type: String,
    #[schema(required = true, example = "8712345678906")]
    ean13: Option<String>,
    #[schema(required = true)]
    barcode: Option<String>,
    /// Sold at the till only; the web shop never lists it.
    store_only: bool,
    /// VAT percentage as a decimal string; `0` when the product has no VAT group.
    #[schema(example = "9")]
    vat_rate: String,
    /// Minor units excl VAT in the register's currency.
    #[schema(example = 1250)]
    unit_price: i64,
    has_stock: bool,
    /// Units on hand minus reserved in the register's warehouse (usable lots only), as a
    /// decimal string; may be negative; null when not stock-tracked.
    #[schema(required = true, example = "7.00")]
    stock: Option<String>,
    /// False once archived, disabled or deleted: only a delta read (`updated_since`) returns
    /// such a row, so the till drops it.
    sellable: bool,
    /// Default variant first.
    variants: Vec<TillVariant>,
    updated_at: DateTime<Utc>,
}

/// A live variant of a till product.
#[derive(Serialize, ToSchema)]
pub struct TillVariant {
    id: Uuid,
    #[schema(required = true)]
    sku: Option<String>,
    #[schema(required = true)]
    ean13: Option<String>,
    #[schema(required = true)]
    barcode: Option<String>,
    is_default: bool,
    /// Minor units excl VAT in the register's currency.
    unit_price: i64,
    options: Vec<VariantOption>,
}

impl From<VariantRow> for TillVariant {
    fn from(v: VariantRow) -> Self {
        Self {
            id: v.id,
            sku: v.sku,
            ean13: v.ean13,
            barcode: v.barcode,
            is_default: v.is_default,
            unit_price: v.unit_price,
            options: v.options.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<ProductRow> for TillProduct {
    fn from(p: ProductRow) -> Self {
        Self {
            id: p.id,
            name: p.name,
            r#type: p.r#type,
            ean13: p.ean13,
            barcode: p.barcode,
            store_only: p.store_only,
            vat_rate: p.vat_rate,
            unit_price: p.unit_price,
            has_stock: p.has_stock,
            stock: p.stock,
            sellable: p.sellable,
            variants: p.variants.0.into_iter().map(Into::into).collect(),
            updated_at: p.updated_at,
        }
    }
}

/// A catalog page plus where the next delta read starts.
#[derive(Serialize, ToSchema)]
pub struct TillCatalogPage {
    pub data: Vec<TillProduct>,
    pub meta: Meta,
    /// Send as `updated_since` on the next sync. A minute before this read, so repeats are
    /// possible; upsert by id.
    pub next_since: DateTime<Utc>,
}

/// GET catalog query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CatalogQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
    /// RFC 3339. Only products changed after it (price, variants, VAT group, this register's
    /// stock or lots), including ones no longer sellable.
    #[param(value_type = Option<DateTime<Utc>>)]
    pub updated_since: Option<String>,
    /// Exact product or variant EAN-13 or barcode, any case, at most 100 characters.
    #[param(max_length = 100)]
    pub code: Option<String>,
}
