//! Storefront response bodies. Only what a shop may show: never purchase or cost prices,
//! supplier data, internal notes or stock locations.

use std::collections::HashMap;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::modules::{
    lots::public::Link,
    orders::service::unit_gross,
    storefront::repo::{CategoryRow, OptionRow, ProductRow, VariantRow},
};

/// A sellable product.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "A product the shop may sell. Prices are integer minor units \
    (cents) of `currency`; `currency_decimals` says where the decimal point goes."
)]
pub struct StorefrontProduct {
    id: Uuid,
    name: String,
    #[schema(required = true)]
    brand: Option<String>,
    /// physical, digital, service, giftcard, …
    r#type: String,
    #[schema(required = true)]
    short_description: Option<String>,
    #[schema(required = true)]
    description: Option<String>,
    #[schema(required = true)]
    ean13: Option<String>,
    #[schema(required = true)]
    barcode: Option<String>,
    /// One unit incl the product's VAT, minor units: what checkout charges for quantity 1.
    #[schema(example = 1250)]
    price_gross: i64,
    /// Price excluding VAT, in minor units.
    #[schema(example = 1033)]
    price_net: i64,
    /// ISO 4217 code.
    #[schema(example = "EUR")]
    currency: String,
    #[schema(example = 2)]
    currency_decimals: i16,
    /// Units on hand minus reserved, over every location, as a decimal string; null when the
    /// product isn't stock-tracked.
    #[schema(required = true, example = "7.50")]
    available_stock: Option<String>,
    /// `in_stock`, `low` (at most the workspace's `product.low_stock_threshold` units left,
    /// default 5) or `sold_out` (none available); null when the product isn't stock-tracked.
    #[schema(required = true, example = "low")]
    stock_status: Option<&'static str>,
    /// Short public labels to show as chips, in order.
    #[schema(example = json!(["Raw", "Unheated"]))]
    labels: Vec<String>,
    /// The product's main category; null when it has none.
    #[schema(required = true)]
    category: Option<CategoryRef>,
    /// The lot passport of the jar the shop sells next: the published, released, unexpired lot
    /// with free stock and the earliest expiry or best-before (undated last). Read it at
    /// `GET /storefront/lots/{public_code}`. Null when no such lot.
    #[schema(required = true)]
    passport: Option<PassportRef>,
    /// The product's own images in order, as signed links (fetch before `expires_at`).
    images: Vec<Link>,
    /// Default variant first.
    variants: Vec<StorefrontVariant>,
}

/// A product's category, as the shop links it (`?category=<id>`).
#[derive(Serialize, ToSchema)]
pub struct CategoryRef {
    id: Uuid,
    name: String,
}

/// A category the shop can browse: only ones holding sellable products.
#[derive(Serialize, ToSchema)]
pub struct StorefrontCategory {
    id: Uuid,
    name: String,
    #[schema(required = true)]
    description: Option<String>,
    /// Sellable web products in it (never 0).
    #[schema(example = 12)]
    product_count: i64,
}

impl From<CategoryRow> for StorefrontCategory {
    fn from(c: CategoryRow) -> Self {
        Self {
            id: c.id,
            name: c.name,
            description: c.description,
            product_count: c.product_count,
        }
    }
}

/// Where a product's lot passport lives.
#[derive(Serialize, ToSchema)]
pub struct PassportRef {
    #[schema(example = "7K3M9Q2XAB")]
    public_code: String,
}

/// Fallback for `product.low_stock_threshold`.
pub const LOW_STOCK_DEFAULT: i64 = 5;

/// `in_stock`, `low` (0 < available ≤ `threshold`) or `sold_out` (≤ 0); None when untracked.
pub fn stock_status(available: Option<&str>, threshold: i64) -> Option<&'static str> {
    let units = available?.parse::<rust_decimal::Decimal>().ok()?;
    Some(if units <= rust_decimal::Decimal::ZERO {
        "sold_out"
    } else if units <= rust_decimal::Decimal::from(threshold) {
        "low"
    } else {
        "in_stock"
    })
}

/// A sellable variant of a product.
#[derive(Serialize, ToSchema)]
pub struct StorefrontVariant {
    id: Uuid,
    #[schema(required = true)]
    sku: Option<String>,
    #[schema(required = true)]
    barcode: Option<String>,
    #[schema(required = true)]
    ean13: Option<String>,
    is_default: bool,
    /// One unit of this variant incl the product's VAT, minor units: what checkout charges.
    price_gross: i64,
    /// This variant's catalog price excl VAT, minor units.
    price_net: i64,
    options: Vec<VariantOption>,
    /// This variant's own images in order, as signed links.
    images: Vec<Link>,
}

/// One option value of a variant.
#[derive(Serialize, ToSchema)]
pub struct VariantOption {
    #[schema(example = "Size")]
    r#type: String,
    #[schema(example = "500 g")]
    value: String,
}

impl From<OptionRow> for VariantOption {
    fn from(o: OptionRow) -> Self {
        Self {
            r#type: o.r#type,
            value: o.value,
        }
    }
}

/// Signed image links per (product, variant); variant None = the product's own.
pub type Images = HashMap<(Uuid, Option<Uuid>), Vec<Link>>;

/// One unit at `net` minor units incl `vat` %, priced like a one-line checkout. Past the
/// engine's range (net above i64/2) it saturates; such a price can't be charged anyway.
fn gross(net: i64, vat: Decimal, decimals: i16) -> i64 {
    let dp = u32::try_from(decimals).unwrap_or(2);
    unit_gross(net, vat, dp).unwrap_or(i64::MAX)
}

impl StorefrontVariant {
    fn new(v: VariantRow, product_id: Uuid, images: &Images, vat: Decimal, dp: i16) -> Self {
        Self {
            images: images
                .get(&(product_id, Some(v.id)))
                .cloned()
                .unwrap_or_default(),
            id: v.id,
            sku: v.sku,
            barcode: v.barcode,
            ean13: v.ean13,
            is_default: v.is_default,
            price_gross: gross(v.price_net, vat, dp),
            price_net: v.price_net,
            options: v.options.into_iter().map(Into::into).collect(),
        }
    }
}

impl StorefrontProduct {
    /// `p` with its images from `images`; `low_stock` is the workspace's threshold.
    pub fn new(p: ProductRow, images: &Images, low_stock: i64) -> Self {
        let (id, vat, dp) = (p.id, p.vat, p.currency_decimals);
        Self {
            stock_status: stock_status(p.available_stock.as_deref(), low_stock),
            labels: p.labels,
            category: p
                .category_id
                .zip(p.category_name)
                .map(|(id, name)| CategoryRef { id, name }),
            passport: p
                .passport_code
                .map(|public_code| PassportRef { public_code }),
            images: images.get(&(id, None)).cloned().unwrap_or_default(),
            id: p.id,
            name: p.name,
            brand: p.brand,
            r#type: p.r#type,
            short_description: p.short_description,
            description: p.description,
            ean13: p.ean13,
            barcode: p.barcode,
            price_gross: gross(p.price_net, p.vat, p.currency_decimals),
            price_net: p.price_net,
            currency: p.currency,
            currency_decimals: p.currency_decimals,
            available_stock: p.available_stock,
            variants: p
                .variants
                .0
                .into_iter()
                .map(|v| StorefrontVariant::new(v, id, images, vat, dp))
                .collect(),
        }
    }
}

/// GET list query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
    /// Only products of this category (`GET /storefront/categories`); an unknown or foreign id
    /// gives an empty list.
    #[param(value_type = Option<Uuid>)]
    pub category: Option<String>,
}
