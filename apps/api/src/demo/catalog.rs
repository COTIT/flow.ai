//! AFM catalog: AFM's real store (`afm_catalogue.json`, made by `scripts/afm-catalogue.mjs`, used
//! with AFM's consent), its ten categories, sales tax, a stall and a warehouse with bins, stock
//! levels (some low, some sold out, two one-offs oversold) and lots with passports in every state.

use std::collections::HashMap;

use axum::http::Method;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Shop, days_ago, days_ago_read, dollars, ean13};
use crate::{error::AppError, farm_demo::text};

/// One sellable product, with its variants as (id, price in cents).
pub struct Product {
    pub id: String,
    /// AFM's store slug: the seed finds products by it (two AFM teas share a title).
    pub slug: String,
    pub name: String,
    /// AFM's photo URLs, in order (`seed-demo --images` stores them).
    pub images: Vec<String>,
    pub cents: i64,
    pub variants: Vec<(String, i64)>,
    pub store_only: bool,
    /// Units in the AFM store at the start.
    pub store_qty: i64,
    /// Six units at the market stall.
    pub at_stall: bool,
}

pub struct Catalog {
    pub products: Vec<Product>,
    pub depot: String,
    pub stall: String,
    /// Products the store holds one of: till sales take them below zero.
    pub oversell: Vec<String>,
    /// Lot passports printed after the table: (label, public code).
    pub passports: Vec<(&'static str, String)>,
}

impl Catalog {
    /// Products the office and the web shop sell from the warehouse (stock there, not store-only).
    pub fn shelf(&self) -> Vec<&Product> {
        self.products
            .iter()
            .filter(|p| !p.store_only && p.store_qty > 0)
            .collect()
    }

    /// Products the web shop and the till can sell several of from the store.
    pub fn web(&self) -> Vec<&Product> {
        self.products
            .iter()
            .filter(|p| !p.store_only && p.store_qty >= 8)
            .collect()
    }

    /// The product with AFM slug `slug`.
    pub fn by_slug(&self, slug: &str) -> Result<&Product, AppError> {
        self.products
            .iter()
            .find(|p| p.slug == slug)
            .ok_or_else(|| AppError::Internal(format!("demo seed: no product {slug}")))
    }
}

/// `afm_catalogue.json` as the seed reads it.
#[derive(Deserialize)]
struct AfmFile {
    products: Vec<AfmProduct>,
}

/// One AFM product.
#[derive(Deserialize)]
pub struct AfmProduct {
    pub slug: String,
    pub title: String,
    pub category: String,
    pub short_description: String,
    pub description: String,
    pub option: Option<String>,
    pub variants: Vec<AfmVariant>,
    pub images: Vec<String>,
}

/// One AFM variant: its option value (None without options) and price in cents.
#[derive(Deserialize)]
pub struct AfmVariant {
    pub value: Option<String>,
    pub cents: i64,
}

/// AFM's catalogue, compiled in.
pub fn afm() -> Result<Vec<AfmProduct>, AppError> {
    serde_json::from_str::<AfmFile>(include_str!("afm_catalogue.json"))
        .map(|f| f.products)
        .map_err(|e| AppError::Internal(format!("demo seed: afm_catalogue.json: {e}")))
}

/// The shop's categories, grouped as AFM's category pages group them: (name, description).
const CATEGORIES: &[(&str, &str)] = &[
    (
        "Honey",
        "Raw wildflower honey from the beekeepers of Illundo, Tanzania.",
    ),
    ("Coffee", "Tanzanian Arabica, whole bean and ground."),
    ("Tea", "Kenyan black tea and Tanzanian roselle."),
    ("Spices", "Whole and ground spices for everyday cooking."),
    (
        "Baskets",
        "Handwoven and beaded baskets and bags from Kenya and Tanzania.",
    ),
    ("Toys & games", "Hand-carved animals and traditional games."),
    (
        "Art",
        "Original paintings and prints by East African artists.",
    ),
    ("Books", "Stories of the farmers behind the market."),
    ("Apparel", "T-shirts from African Farmers Market."),
    ("Stickers", "Stickers from African Farmers Market."),
];

/// Sold at the market stall and the store only (the till's store-only goods).
const STORE_ONLY: &[&str] = &[
    "nairobi-night-kenyan-woven-makeup-bag",
    "ruaha-grasslands-natural-basket",
    "ground-pumpkin-seed",
    "whole-red-pepper",
];

/// Original paintings: one in the store, so till sales take them below zero.
const ONE_OFFS: &[&str] = &["chaotic-mind-pascal-masegesse", "daisies-benard-omondi"];

/// Every 14th product has no stock anywhere (the shop's sold-out badge).
fn sold_out(i: usize) -> bool {
    i % 14 == 13
}

pub async fn seed(shop: &Shop<'_>, store: &str) -> Result<Catalog, AppError> {
    let api = shop.api;
    api.post(
        &shop.at("/settings/vat-groups"),
        json!({ "name": "NC sales tax", "vat_amount": 7, "default_group": true, "country": "US" }),
        &shop.owner,
    )
    .await?;
    let stall = api
        .post(
            &shop.at("/warehouses"),
            json!({ "name": "Market stall", "is_store": true }),
            &shop.owner,
        )
        .await?;
    let stall = text(&stall, "warehouse_id")?;
    let depot = api
        .post(
            &shop.at("/warehouses"),
            json!({ "name": "Asheville warehouse" }),
            &shop.owner,
        )
        .await?;
    let depot = text(&depot, "warehouse_id")?;
    for (code, name) in [("A", "Honey and food"), ("B", "Crafts and art")] {
        api.post(
            &shop.at(&format!("/warehouses/{depot}/sections")),
            json!({ "section_name": name, "section_code": code, "rows": 2,
                    "shelves_per_row": 3, "bins_per_shelf": 4 }),
            &shop.owner,
        )
        .await?;
    }
    let bins = api
        .call(
            Method::GET,
            &shop.at(&format!("/warehouses/{depot}/bins?page_size=100")),
            None,
            Some(&shop.owner),
        )
        .await?;
    let bins: Vec<Value> = bins["data"]
        .as_array()
        .map(|b| b.iter().map(|b| b["id"].clone()).collect())
        .unwrap_or_default();

    let afm = afm()?;
    let mut categories = HashMap::new();
    for (name, description) in CATEGORIES {
        let made = api
            .post(
                &shop.at("/erp/product-categories"),
                json!({ "name": name, "description": description }),
                &shop.owner,
            )
            .await?;
        categories.insert(*name, text(&made, "product_category_id")?);
    }

    let mut products = Vec::new();
    let mut oversell = Vec::new();
    for (i, p) in afm.into_iter().enumerate() {
        let category = categories
            .get(p.category.as_str())
            .ok_or_else(|| AppError::Internal(format!("demo seed: no category {}", p.category)))?;
        let cents = p.variants.iter().map(|v| v.cents).min().unwrap_or(0);
        let store_only = STORE_ONLY.contains(&p.slug.as_str());
        let created = api
            .post(
                &shop.at("/products"),
                json!({ "name": p.title, "price": dollars(cents), "category_id": category,
                        "description": p.description, "short_description": p.short_description,
                        "ean13": ean13(i as u64 + 1), "store_only": store_only,
                        "reference": format!("AFM-{:03}", i + 1) }),
                &shop.owner,
            )
            .await?;
        let id = text(&created, "product_id")?;
        let mut variants = Vec::new();
        if let Some(option) = &p.option {
            let combos: Vec<Value> = p
                .variants
                .iter()
                .enumerate()
                .map(|(n, v)| {
                    json!({ "option_value_ids": [v.value], "price": dollars(v.cents),
                            "sku": format!("AFM-{:03}-{}", i + 1, n + 1) })
                })
                .collect();
            let made = api
                .post(
                    &shop.at(&format!("/products/{id}/variants/generate")),
                    json!({ "option_types": [{ "name": option,
                                "values": p.variants.iter().map(|v| json!({ "value": v.value })).collect::<Vec<_>>() }],
                            "combinations": combos }),
                    &shop.owner,
                )
                .await?;
            for (v, afm) in made.as_array().into_iter().flatten().zip(&p.variants) {
                variants.push((text(v, "id")?, afm.cents));
            }
        }

        // Stock: warehouse bins, the store and sometimes the stall. Low, zero and oversold on purpose.
        let low = i % 12 == 5;
        let tiny = ONE_OFFS.contains(&p.slug.as_str());
        // Its stock comes only from the farm's lots (`supply`), so its sales credit farmers.
        let farm = p.slug == super::supply::COFFEE;
        let stocked = !farm && (tiny || !sold_out(i));
        let store_qty = match (stocked, low, tiny) {
            (false, ..) => 0,
            (_, _, true) => 1,
            (_, true, _) => 3,
            _ => 8 + (i as i64 * 3) % 25,
        };
        if stocked {
            let mut entries = vec![json!({ "warehouse_id": store, "stock_lvl": store_qty,
                "minimal_stock_level": 10, "maximum_stock_level": 60,
                "reason": "DEMO opening count" })];
            if !store_only {
                // Orders hold from the warehouse-level row; bins carry overflow on some products.
                entries.push(json!({ "warehouse_id": depot,
                    "stock_lvl": 40 + (i * 7) % 90, "minimal_stock_level": 20,
                    "maximum_stock_level": 200, "reason": "DEMO opening count" }));
                if i % 3 == 1 {
                    entries.push(
                        json!({ "warehouse_id": depot, "bin_id": bins.get(i % bins.len().max(1)),
                        "stock_lvl": 12, "reason": "DEMO opening count" }),
                    );
                }
            }
            if i % 3 == 0 {
                entries.push(json!({ "warehouse_id": stall, "stock_lvl": 6,
                    "reason": "DEMO opening count" }));
            }
            api.post(
                &shop.at(&format!("/products/{id}/set-stock-levels")),
                json!({ "warehouses": entries }),
                &shop.owner,
            )
            .await?;
        }
        if tiny {
            oversell.push(id.clone());
        }
        products.push(Product {
            id,
            slug: p.slug,
            name: p.title,
            images: p.images,
            cents,
            variants,
            store_only,
            store_qty,
            at_stall: stocked && i % 3 == 0,
        });
    }

    let passports = lots(shop, &products, &depot).await?;
    Ok(Catalog {
        products,
        depot,
        stall,
        oversell,
        passports,
    })
}

/// (country, region, place, as read).
type Origin<'a> = (&'a str, &'a str, &'a str, &'a str);
/// (product slug, code, produced days ago, passport: none / draft / published, status after, origin).
type LotPlan<'a> = (&'a str, &'a str, i64, &'a str, &'a str, Origin<'a>);

/// Lots of AFM's food: released and published, drafts, quarantine and recalled (both
/// published, so the public page shows the banner), and one past its expiry.
async fn lots(
    shop: &Shop<'_>,
    products: &[Product],
    depot: &str,
) -> Result<Vec<(&'static str, String)>, AppError> {
    let api = shop.api;
    let lot_of = |slug: &str| {
        products
            .iter()
            .find(|p| p.slug == slug)
            .ok_or_else(|| AppError::Internal(format!("demo seed: no product {slug}")))
    };
    // (product slug, code, produced days ago, passport: none / draft / published, status after,
    // origin: (country, region, place, as read))
    let ilundo = ("TZ", "Iringa", "Illundo", "Illundo, Iringa, Tanzania");
    let tanzania = ("TZ", "Tanzania", "Tanzania", "Tanzania");
    let kenya = ("KE", "Kenya", "Kenya", "Kenya");
    let plan: &[LotPlan] = &[
        (
            "tanzanian-wildflower-honey-13oz",
            "MH13-2026-07",
            110,
            "published",
            "released",
            ilundo,
        ),
        (
            "tanzanian-wildflower-honey-7oz",
            "MH7-2026-08",
            80,
            "published",
            "released",
            ilundo,
        ),
        (
            "roselle-tea-tanzania",
            "RT-2026-05",
            150,
            "published",
            "released",
            tanzania,
        ),
        (
            "kizazi-tea",
            "KZ-2026-04",
            170,
            "published",
            "released",
            kenya,
        ),
        (
            "kizazi-kenyan-black-tea-small",
            "KZS-2026-09",
            40,
            "draft",
            "released",
            kenya,
        ),
        (
            "rift-valley-tanzanian-ground-coffee",
            "RV-2026-09",
            35,
            "draft",
            "released",
            tanzania,
        ),
        (
            "tanzanian-wildflower-honey-7oz",
            "MH7-2026-06",
            120,
            "published",
            "quarantine",
            ilundo,
        ),
        (
            "tanzanian-wildflower-honey-13oz",
            "MH13-2026-06",
            125,
            "published",
            "recalled",
            ilundo,
        ),
        (
            "cardamom",
            "CD-2024-07",
            480,
            "none",
            "released",
            ("", "", "", "East Africa"),
        ),
    ];
    let mut out = Vec::new();
    for (slug, code, produced, passport, status, (country, region, place, origin)) in plan {
        let product = lot_of(slug)?;
        let expires = if *produced > 400 { 30 } else { -700 };
        let lot = api
            .post(
                &shop.at("/lots"),
                json!({ "product_id": product.id, "code": code,
                        "produced_on": days_ago(*produced), "best_before": days_ago(*produced - 540),
                        "expires_on": days_ago(expires), "origin": origin }),
                &shop.owner,
            )
            .await?;
        let id = text(&lot, "id")?;
        api.post(
            &shop.at(&format!("/lots/{id}/stock-mutations")),
            json!({ "warehouse_id": depot, "quantity": "24", "reason": "received" }),
            &shop.owner,
        )
        .await?;
        if *passport != "none" {
            let honey = slug.contains("honey");
            let story = if honey {
                "Mary's Tanzanian Wildflower Honey is raw, unfiltered, and traced directly to the \
                 beekeepers of Illundo, Tanzania. African Farmers Market creates direct \
                 partnerships with rural farming communities across Africa and champions \
                 opportunity for women in agriculture."
            } else {
                "Packed for African Farmers Market, which buys directly from farming communities \
                 across East Africa."
            };
            let mut blocks = vec![
                json!({ "kind": "text", "title": "Where it comes from", "body": story }),
                json!({ "kind": "facts", "items": [
                    { "label": "Origin", "value": origin },
                    { "label": "Packed", "value": days_ago_read(*produced) }] }),
            ];
            if honey {
                blocks.push(json!({ "kind": "quote",
                    "text": "Buy a jar. Know your source. Taste the difference.",
                    "by": "African Farmers Market" }));
            }
            api.call(
                Method::PUT,
                &shop.at(&format!("/lots/{id}/passport")),
                Some(json!({
                    "title": format!("{}, lot {code}", product.name),
                    "intro": format!("{} from {origin}.", product.name),
                    "origin": { "country": country, "region": region, "place": place },
                    "materials": [{ "name": product.name, "origin": origin, "share_bp": 10000 }],
                    "story": blocks })),
                Some(&shop.owner),
            )
            .await?;
        }
        if *passport == "published" {
            api.post(
                &shop.at(&format!("/lots/{id}/passport/publish")),
                json!({}),
                &shop.owner,
            )
            .await?;
        }
        if *status != "released" {
            api.call(
                Method::PATCH,
                &shop.at(&format!("/lots/{id}")),
                Some(json!({ "status": status })),
                Some(&shop.owner),
            )
            .await?;
        }
        if *passport == "published" && matches!(*status, "quarantine" | "recalled") {
            let lot = api
                .call(
                    Method::GET,
                    &shop.at(&format!("/lots/{id}")),
                    None,
                    Some(&shop.owner),
                )
                .await?;
            let label = if *status == "recalled" {
                "passport (recalled)"
            } else {
                "passport (quarantine)"
            };
            out.push((label, text(&lot["data"], "public_code")?));
        }
    }
    Ok(out)
}
