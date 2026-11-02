//! AFM tills: the counter and the market stall, four cashiers (one inactive), a back-office safe,
//! and a week of shifts with cash and card sales. Some shifts close short or over, three sales
//! were rung at an old price, two items sell below zero, and the counter's shift is open now.

use axum::http::Method;
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Shop, catalog::Catalog, pin, supply::Supplied};
use crate::{error::AppError, farm_demo::text};

/// What the printout and the date pass need.
pub struct Tills {
    /// (register name, device key).
    pub devices: Vec<(&'static str, String)>,
    /// Active cashiers: (name, PIN).
    pub cashiers: Vec<(&'static str, String)>,
    /// Closed shifts to move to their day: (shift id, days ago).
    pub shifts: Vec<(Uuid, i64)>,
}

/// US notes and coins, largest first, in cents.
const CASH: &[i64] = &[10_000, 5_000, 2_000, 1_000, 500, 100, 25, 10, 5, 1];

/// A drawer count that adds up to `amount`.
fn count(mut amount: i64) -> Vec<Value> {
    let mut out = Vec::new();
    for d in CASH {
        let n = amount / d;
        if n > 0 {
            out.push(json!({ "denomination": d, "count": n }));
            amount -= n * d;
        }
    }
    if out.is_empty() {
        out.push(json!({ "denomination": 100, "count": 0 }));
    }
    out
}

pub async fn seed(
    shop: &Shop<'_>,
    catalog: &Catalog,
    store: &str,
    coffee: &Supplied,
) -> Result<Tills, AppError> {
    let api = shop.api;
    let mut devices = Vec::new();
    for (name, warehouse) in [
        ("AFM counter", store),
        ("Market stall", catalog.stall.as_str()),
    ] {
        let r = api
            .post(
                &shop.at("/till/registers"),
                json!({ "name": name, "warehouse_id": warehouse }),
                &shop.owner,
            )
            .await?;
        devices.push((name, text(&r, "key")?));
    }
    let mut cashiers = Vec::new();
    let mut ids = Vec::new();
    for name in ["Zawadi", "Baraka", "Jordan", "Riley"] {
        let p = pin()?;
        let user = api
            .post(
                &shop.at("/till/users"),
                json!({ "name": name, "pin": p }),
                &shop.owner,
            )
            .await?;
        let id = text(&user, "id")?;
        if name == "Riley" {
            // Left the shop: kept for the sales history, can't sign in.
            api.call(
                Method::PATCH,
                &shop.at(&format!("/till/users/{id}")),
                Some(json!({ "active": false })),
                Some(&shop.owner),
            )
            .await?;
        } else {
            cashiers.push((name, p.clone()));
            ids.push((id, p));
        }
    }
    let vault = api
        .post(
            &shop.at("/till/vaults"),
            json!({ "name": "Back office safe" }),
            &shop.owner,
        )
        .await?;
    let vault = text(&vault, "id")?;

    let counter: Vec<_> = catalog.web();
    let stall: Vec<_> = catalog
        .products
        .iter()
        .filter(|p| p.at_stall && !p.store_only)
        .collect();
    let store_only: Vec<_> = catalog
        .products
        .iter()
        .filter(|p| p.store_only && p.store_qty > 0)
        .collect();
    let mut shifts = Vec::new();
    let mut sale_no = 0usize;
    // (register, days ago, sales, close difference in cents; None = leave open)
    let plan: &[(usize, i64, usize, Option<i64>)] = &[
        (0, 6, 16, Some(0)),
        (1, 5, 8, Some(0)),
        (0, 5, 18, Some(-150)),
        (0, 4, 15, Some(0)),
        (0, 3, 17, Some(275)),
        (1, 3, 9, Some(-500)),
        (0, 2, 16, Some(0)),
        (0, 1, 14, Some(0)),
        (0, 0, 6, None),
    ];
    for (n, (register, days, sales, diff)) in plan.iter().enumerate() {
        let (cashier, p) = &ids[n % ids.len()];
        let session = api
            .call(
                Method::POST,
                &shop.at("/till/login"),
                Some(json!({ "till_user_id": cashier, "pin": p })),
                Some(&devices[*register].1),
            )
            .await?;
        let till = text(&session["data"], "token")?;
        api.call(
            Method::POST,
            &shop.at("/till/shift/open"),
            Some(json!({ "count": count(20_000) })),
            Some(&till),
        )
        .await?;
        let day = Utc::now() - Duration::days(*days);
        let goods = if *register == 0 { &counter } else { &stall };
        for s in 0..*sales {
            sale_no += 1;
            let p = goods[(sale_no * 7) % goods.len()];
            let (variant, price) = match p.variants.get(sale_no % p.variants.len().max(1)) {
                Some((v, c)) => (Some(v.clone()), *c),
                None => (None, p.cents),
            };
            let mut line = json!({ "product_id": p.id, "quantity": "1",
                // An old shelf price on three sales: the office's mismatch filter.
                "unit_price": if sale_no % 41 == 7 { price - price / 10 } else { price } });
            if let Some(v) = variant {
                line["product_variant_id"] = json!(v);
            }
            let mut lines = vec![line];
            if *register == 0 && s % 4 == 0 {
                lines.push(json!({ "product_id": coffee.coffee_id, "quantity": "1", "unit_price": coffee.coffee_cents }));
            }
            if *register == 0
                && s % 9 == 3
                && let Some(extra) = store_only.get(sale_no % store_only.len().max(1))
            {
                lines.push(
                    json!({ "product_id": extra.id, "quantity": "1", "unit_price": extra.cents }),
                );
            }
            // Two lines the store holds one of sell three times this week: stock goes below zero.
            if *register == 0 && s == 5 && *days <= 3 {
                for id in &catalog.oversell {
                    let cents = catalog
                        .products
                        .iter()
                        .find(|p| &p.id == id)
                        .map_or(100, |p| p.cents);
                    lines.push(json!({ "product_id": id, "quantity": "1", "unit_price": cents }));
                }
            }
            let payment = if sale_no.is_multiple_of(3) {
                json!({ "method": "card", "reference": format!("VISA {:04}", 1000 + sale_no) })
            } else {
                let quote = api
                    .call(
                        Method::POST,
                        &shop.at("/till/sales/quote"),
                        Some(json!({ "lines": lines, "payment": { "method": "cash" } })),
                        Some(&till),
                    )
                    .await?;
                let due = quote["data"]["due"].as_i64().unwrap_or(0);
                json!({ "method": "cash", "tendered": (due + 499) / 500 * 500 })
            };
            let hour = 9 + (s as i64 * 9) / (*sales as i64).max(1);
            let sold_at = if *days == 0 {
                Utc::now() - Duration::minutes(5 * (*sales - s) as i64)
            } else {
                day.date_naive()
                    .and_hms_opt(14, 0, 0)
                    .map_or(day, |t| t.and_utc())
                    + Duration::minutes((hour - 14) * 60 + (s as i64 * 7) % 50)
            };
            api.call(
                Method::POST,
                &shop.at("/till/sales"),
                Some(
                    json!({ "client_ref": Uuid::now_v7(), "sold_at": sold_at.to_rfc3339(),
                             "lines": lines, "payment": payment }),
                ),
                Some(&till),
            )
            .await?;
        }
        let Some(diff) = diff else { continue };
        if *register == 0 {
            // The day's drop to the safe.
            api.call(
                Method::POST,
                &shop.at("/till/transfers"),
                Some(json!({ "vault_id": vault, "direction": "to_vault", "count": count(10_000) })),
                Some(&till),
            )
            .await?;
        }
        let shift = api
            .call(Method::GET, &shop.at("/till/shift"), None, Some(&till))
            .await?;
        let expected = shift["data"]["expected"].as_i64().unwrap_or(0);
        let closed = api
            .call(
                Method::POST,
                &shop.at("/till/shift/close"),
                Some(json!({ "count": count((expected + diff).max(0)) })),
                Some(&till),
            )
            .await?;
        let id = text(&closed["data"], "id").or_else(|_| text(&shift["data"], "id"))?;
        shifts.push((
            id.parse()
                .map_err(|_| AppError::Internal("demo seed: shift id".into()))?,
            *days,
        ));
    }
    Ok(Tills {
        devices,
        cashiers,
        shifts,
    })
}
