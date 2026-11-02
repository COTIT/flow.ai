//! AFM catalog: sales tax, a stall and a warehouse with bins, honey, candles and the rest with
//! variants, stock levels (some low, some sold out), and honey lots with passports in every state.

use axum::http::Method;
use serde_json::{Value, json};

use super::{Shop, days_ago, dollars, ean13, names};
use crate::{error::AppError, farm_demo::text};

/// One sellable product, with its variants as (id, price in cents).
pub struct Product {
    pub id: String,
    pub name: String,
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
}

/// A product to create: (name, brand, cents, store only, description, variant option and
/// its (value, cents)).
type Row = (
    String,
    &'static str,
    i64,
    bool,
    String,
    Option<(&'static str, Vec<(String, i64)>)>,
);

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
    for (code, name) in [("A", "Honey and food"), ("B", "Candles and care")] {
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

    let mut rows: Vec<Row> = Vec::new();
    for (v, brand, cents) in names::VARIETALS {
        let sizes = names::JAR_SIZES
            .iter()
            .map(|(s, pct)| ((*s).to_owned(), cents * pct / 100))
            .collect();
        rows.push((
            format!("{v} honey"),
            brand,
            *cents,
            false,
            format!(
                "Raw {} honey, extracted and strained, never heated.",
                v.to_lowercase()
            ),
            Some(("Size", sizes)),
        ));
        rows.push((
            format!("{v} creamed honey 12 oz"),
            brand,
            cents * 130 / 100,
            false,
            "Whipped smooth and spreadable.".into(),
            None,
        ));
        rows.push((
            format!("{v} comb honey 12 oz"),
            brand,
            cents * 190 / 100,
            false,
            "Cut comb straight from the frame.".into(),
            None,
        ));
    }
    for (name, brand, cents) in names::CANDLES {
        let scents = names::SCENTS
            .iter()
            .map(|s| ((*s).to_owned(), *cents))
            .collect();
        rows.push((
            (*name).into(),
            brand,
            *cents,
            false,
            "Poured from our own filtered beeswax.".into(),
            Some(("Scent", scents)),
        ));
    }
    for (name, brand, cents, store_only, desc) in names::OTHERS {
        rows.push((
            (*name).into(),
            brand,
            *cents,
            *store_only,
            (*desc).into(),
            None,
        ));
    }

    let mut products = Vec::new();
    let mut oversell = Vec::new();
    for (i, (name, brand, cents, store_only, desc, options)) in rows.into_iter().enumerate() {
        let created = api
            .post(
                &shop.at("/products"),
                json!({ "name": name, "price": dollars(cents), "brand": brand,
                        "description": desc, "short_description": desc,
                        "ean13": ean13(i as u64 + 1), "store_only": store_only,
                        "reference": format!("AFM-{:03}", i + 1) }),
                &shop.owner,
            )
            .await?;
        let id = text(&created, "product_id")?;
        let mut variants = Vec::new();
        if let Some((option, values)) = options {
            let combos: Vec<Value> = values
                .iter()
                .enumerate()
                .map(|(n, (value, c))| {
                    json!({ "option_value_ids": [value], "price": dollars(*c),
                            "sku": format!("AFM-{:03}-{}", i + 1, n + 1) })
                })
                .collect();
            let made = api
                .post(
                    &shop.at(&format!("/products/{id}/variants/generate")),
                    json!({ "option_types": [{ "name": option,
                                "values": values.iter().map(|(v, _)| json!({ "value": v })).collect::<Vec<_>>() }],
                            "combinations": combos }),
                    &shop.owner,
                )
                .await?;
            for (v, (_, c)) in made.as_array().into_iter().flatten().zip(&values) {
                variants.push((text(v, "id")?, *c));
            }
        }

        // Stock: warehouse bins, the store and sometimes the stall. Low, zero and oversold on purpose.
        let low = i % 12 == 5;
        let tiny = name.starts_with("Clover honey") || name.starts_with("Honey stix");
        let stocked = !sold_out(i) && name != "Mead tasting glass";
        let store_qty = match (stocked, low, tiny) {
            (false, ..) => 0,
            (_, true, _) => 3,
            (_, _, true) => 1,
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
            name,
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

/// Honey lots: released and published, drafts, quarantine and recalled (both published, so the
/// public page shows the banner), and one past its expiry.
async fn lots(
    shop: &Shop<'_>,
    products: &[Product],
    depot: &str,
) -> Result<Vec<(&'static str, String)>, AppError> {
    let api = shop.api;
    let lot_of = |name: &str| {
        products
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.id.clone())
            .ok_or_else(|| AppError::Internal(format!("demo seed: no product {name}")))
    };
    // (product, code, produced days ago, passport: none / draft / published, status after)
    let plan: &[(&str, &str, i64, &str, &str)] = &[
        (
            "Wildflower honey",
            "WF-2026-07",
            110,
            "published",
            "released",
        ),
        ("Sourwood honey", "SW-2026-08", 80, "published", "released"),
        ("Tupelo honey", "TU-2026-05", 150, "published", "released"),
        (
            "Orange Blossom honey",
            "OB-2026-04",
            170,
            "published",
            "released",
        ),
        ("Buckwheat honey", "BW-2026-09", 40, "draft", "released"),
        ("Sage honey", "SG-2026-09", 35, "draft", "released"),
        (
            "Blueberry honey",
            "BB-2026-06",
            120,
            "published",
            "quarantine",
        ),
        ("Basswood honey", "BS-2026-06", 125, "published", "recalled"),
        ("Alfalfa honey", "AL-2024-07", 480, "none", "released"),
    ];
    let mut out = Vec::new();
    for (product, code, produced, passport, status) in plan {
        let expires = if *produced > 400 { 30 } else { -700 };
        let lot = api
            .post(
                &shop.at("/lots"),
                json!({ "product_id": lot_of(product)?, "code": code,
                        "produced_on": days_ago(*produced), "best_before": days_ago(*produced - 540),
                        "expires_on": days_ago(expires), "origin": "Buncombe County, NC" }),
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
            let varietal = product.trim_end_matches(" honey");
            api.call(
                Method::PUT,
                &shop.at(&format!("/lots/{id}/passport")),
                Some(json!({
                    "title": format!("{varietal} honey, lot {code}"),
                    "intro": format!("DEMO: {} honey from hives in the Blue Ridge foothills.", varietal.to_lowercase()),
                    "origin": { "country": "US", "region": "North Carolina", "place": "Buncombe County" },
                    "materials": [{ "name": "Raw honey", "origin": "Buncombe County, NC", "share_bp": 10000 }],
                    "story": [
                        { "kind": "text", "title": "From the hive",
                          "body": "Supers pulled at the end of the flow, extracted cold and strained once." },
                        { "kind": "facts", "items": [
                            { "label": "Hives", "value": "40" },
                            { "label": "Extracted", "value": days_ago(*produced) }] },
                        { "kind": "quote", "text": "The bees did the hard part.", "by": "DEMO beekeeper" }
                    ] })),
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
