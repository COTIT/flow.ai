//! Storefront response bodies. Only what a shop may show: never purchase or cost prices,
//! supplier data, internal notes or stock locations.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::modules::{
    lots::public::Link,
    storefront::repo::{OptionRow, ProductRow, VariantRow},
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
    /// Consumer price including VAT, in minor units.
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
    /// The product's own images in order, as signed links (fetch before `expires_at`).
    images: Vec<Link>,
    /// Default variant first.
    variants: Vec<StorefrontVariant>,
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
    /// Minor units of the product's currency.
    price_gross: i64,
    /// Minor units of the product's currency.
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

impl StorefrontVariant {
    fn new(v: VariantRow, product_id: Uuid, images: &Images) -> Self {
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
            price_gross: v.price_gross,
            price_net: v.price_net,
            options: v.options.into_iter().map(Into::into).collect(),
        }
    }
}

impl StorefrontProduct {
    /// `p` with its images from `images`.
    pub fn new(p: ProductRow, images: &Images) -> Self {
        let id = p.id;
        Self {
            images: images.get(&(id, None)).cloned().unwrap_or_default(),
            id: p.id,
            name: p.name,
            brand: p.brand,
            r#type: p.r#type,
            short_description: p.short_description,
            description: p.description,
            ean13: p.ean13,
            barcode: p.barcode,
            price_gross: p.price_gross,
            price_net: p.price_net,
            currency: p.currency,
            currency_decimals: p.currency_decimals,
            available_stock: p.available_stock,
            variants: p
                .variants
                .0
                .into_iter()
                .map(|v| StorefrontVariant::new(v, id, images))
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
}
