//! AFM sales: 40 customers with contacts, office orders in every status, web orders paid,
//! failed, refunded and waiting, invoices from draft to paid, overdue and credited, and the
//! purchase discounts and special offers the price engine reads.

use std::str::FromStr;

use axum::http::Method;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Shop, catalog::Catalog, days_ago, names, supply::Supplied};
use crate::{
    error::AppError,
    farm_demo::text,
    modules::{invoices, messages::dto::SendInput, relations::address::Address},
    payments::Status,
};

/// What the date pass and the printout need.
pub struct Orders {
    /// Orders to move back: (id, days ago).
    pub dated: Vec<(Uuid, i64)>,
    /// Invoices to move back with their order: (id, days ago, numbered).
    pub invoices: Vec<(Uuid, i64, bool)>,
    /// Web orders left waiting for payment: their hold is stretched so the pay page stays live.
    pub waiting: Vec<Uuid>,
    pub samples: Vec<(&'static str, String)>,
}

struct Customer {
    id: String,
    contact: Uuid,
    address: Value,
}

fn uuid(v: &str) -> Result<Uuid, AppError> {
    v.parse()
        .map_err(|_| AppError::Internal(format!("demo seed: not a uuid: {v}")))
}

/// A decimal amount from the API (number or text) in cents.
fn cents(v: &Value) -> i64 {
    let s = v.to_string();
    Decimal::from_str(s.trim_matches('"'))
        .ok()
        .and_then(|d| (d * Decimal::from(100)).round().try_into().ok())
        .unwrap_or(0)
}

pub async fn seed(
    shop: &Shop<'_>,
    catalog: &Catalog,
    storefront_key: &str,
    coffee: &Supplied,
) -> Result<Orders, AppError> {
    let customers = customers(shop).await?;
    let shelf = catalog.shelf();
    let mut out = Orders {
        dated: Vec::new(),
        invoices: Vec::new(),
        waiting: Vec::new(),
        samples: Vec::new(),
    };
    let line = |i: usize, j: usize| {
        let p = shelf[(i * 5 + j * 11) % shelf.len()];
        let mut l = json!({ "product_id": p.id, "quantity": (1 + (i + j) % 3).to_string() });
        if !p.variants.is_empty() {
            l["product_variant_id"] = json!(p.variants[(i + j) % p.variants.len()].0);
        }
        l
    };

    // ---- office orders, from the warehouse ----
    let mut completed: Vec<(String, i64, usize)> = Vec::new();
    for i in 0..78usize {
        let (status, mut days) = match i {
            0..45 => ("completed", 3 + (i as i64 * 87) / 52),
            45..49 => ("partially_returned", 3 + (i as i64 * 87) / 52),
            49..52 => ("returned", 3 + (i as i64 * 87) / 52),
            52..62 => ("open", 1 + (i as i64 - 52) % 7),
            62..70 => ("draft", (i as i64 - 62) % 4),
            _ => ("cancelled", 5 + (i as i64 - 70) * 9),
        };
        let n_lines = if status.contains("returned") {
            2
        } else {
            1 + i % 3
        };
        let mut lines: Vec<Value> = (0..n_lines).map(|j| line(i, j)).collect();
        // Some completed orders carry farm coffee: they credit farmers today, so they stay recent.
        if status == "completed" && i % 9 == 0 {
            lines.push(json!({ "product_id": coffee.coffee_id, "quantity": "2" }));
            days = 1 + (i as i64) % 6;
        }
        let customer = &customers[i % customers.len()];
        let mut body = json!({ "warehouse_id": catalog.depot, "reference": format!("PO-{}", 1000 + i),
                               "lines": lines });
        if !(status == "draft" && i % 3 == 0) {
            body["relation_id"] = json!(customer.id);
        }
        let order = shop
            .api
            .post(&shop.at("/orders"), body, &shop.owner)
            .await?;
        let id = text(&order, "order_id")?;
        let action = |a: &str| shop.at(&format!("/orders/{id}/{a}"));
        let cancel_from_draft = status == "cancelled" && i % 2 == 0;
        if status != "draft" && !cancel_from_draft {
            shop.api
                .post(&action("confirm"), json!({}), &shop.owner)
                .await?;
        }
        if status == "completed" || status.contains("returned") {
            let lines = pick(shop, &id).await?;
            shop.api
                .post(&action("complete"), json!({}), &shop.owner)
                .await?;
            if status != "completed" {
                let back: Vec<Value> = lines
                    .iter()
                    .take(if status == "returned" { 2 } else { 1 })
                    .cloned()
                    .collect();
                shop.api
                    .post(
                        &action("returns"),
                        json!({ "lines": back, "restock": true }),
                        &shop.owner,
                    )
                    .await?;
            }
        }
        if status == "cancelled" {
            shop.api
                .post(&action("cancel"), json!({}), &shop.owner)
                .await?;
        }
        if status == "completed" && i % 9 != 0 {
            completed.push((id.clone(), days, i));
        }
        out.dated.push((uuid(&id)?, days));
    }

    invoices(shop, &customers, &mut completed, &mut out).await?;
    web(shop, catalog, storefront_key, coffee, &mut out).await?;
    pricing(shop, catalog, &customers).await?;
    Ok(out)
}

/// Pick every line in full; returns the lines as `{line_id, quantity}`.
async fn pick(shop: &Shop<'_>, id: &str) -> Result<Vec<Value>, AppError> {
    let order = shop
        .api
        .call(
            Method::GET,
            &shop.at(&format!("/orders/{id}")),
            None,
            Some(&shop.owner),
        )
        .await?;
    let lines: Vec<Value> = order["data"]["lines"]
        .as_array()
        .map(|l| {
            l.iter()
                .map(|l| json!({ "line_id": l["id"], "quantity": l["quantity"] }))
                .collect()
        })
        .unwrap_or_default();
    shop.api
        .post(
            &shop.at(&format!("/orders/{id}/pick")),
            json!({ "lines": lines }),
            &shop.owner,
        )
        .await?;
    Ok(lines)
}

/// 15 businesses (two contacts each) and 25 households (one), all with a US address.
async fn customers(shop: &Shop<'_>) -> Result<Vec<Customer>, AppError> {
    let mut out = Vec::new();
    for i in 0..40usize {
        let first = names::US_FIRST[i % names::US_FIRST.len()];
        let last = names::US_LAST[(i * 7) % names::US_LAST.len()];
        let (city, _) = names::US_CITIES[i % names::US_CITIES.len()];
        let address = json!({ "address": names::US_STREETS[i % names::US_STREETS.len()],
            "house_number": (100 + i * 37).to_string(), "zipcode": format!("28{:03}", 800 + i),
            "city": city, "country": "US", "main_address": true, "type": "invoice" });
        let mut body = json!({ "addresses": [address] });
        let mut contacts = vec![(first, last)];
        if let Some((company, bcity, bstate)) = names::BUSINESSES.get(i) {
            let slug: String = company
                .chars()
                .filter(char::is_ascii_alphanumeric)
                .collect::<String>()
                .to_lowercase();
            body["type"] = json!("business");
            body["company_name"] = json!(company);
            body["email"] = json!(format!("orders@{slug}.test"));
            body["addresses"][0]["city"] = json!(bcity);
            body["addresses"][0]["address2"] = json!(bstate);
            contacts.push((
                names::US_FIRST[(i + 11) % names::US_FIRST.len()],
                names::US_LAST[(i * 3 + 5) % names::US_LAST.len()],
            ));
        } else {
            body["type"] = json!("personal");
            body["alias"] = json!(format!("{first} {last}"));
            body["email"] = json!(format!(
                "{}.{}@mail.test",
                first.to_lowercase(),
                last.to_lowercase()
            ));
            body["phone_number"] = json!(format!("+1 828 555 {:04}", 100 + i));
        }
        let r = shop
            .api
            .post(&shop.at("/relations"), body, &shop.owner)
            .await?;
        let id = text(&r, "relation_id")?;
        let mut contact = Uuid::nil();
        for (n, (f, l)) in contacts.iter().enumerate() {
            let c = shop
                .api
                .post(
                    &shop.at(&format!("/relations/{id}/contacts")),
                    json!({ "first_name": f, "last_name": l,
                            "email": format!("{}.{}{}@mail.test", f.to_lowercase(), l.to_lowercase(), n) }),
                    &shop.owner,
                )
                .await?;
            if n == 0 {
                contact = uuid(&text(&c, "contact_id")?)?;
            }
        }
        out.push(Customer {
            id,
            contact,
            address: json!({ "address": names::US_STREETS[i % names::US_STREETS.len()],
                "house_number": (100 + i * 37).to_string(), "zipcode": format!("28{:03}", 800 + i),
                "city": city, "country": "US" }),
        });
    }
    Ok(out)
}

/// Number a draft the way the send route does, without the PDF and mail (staging has no
/// renderer): number, dates, status. Then it is a sent invoice for every other route.
async fn number(shop: &Shop<'_>, invoice: Uuid, c: &Customer) -> Result<(), AppError> {
    let owner: Uuid =
        sqlx::query_scalar!("SELECT id FROM users WHERE email = $1", super::SHOP_OWNER)
            .fetch_one(&shop.state.db)
            .await?;
    let address: Address = serde_json::from_value(c.address.clone())
        .map_err(|e| AppError::Internal(format!("demo seed: address: {e}")))?;
    let input = SendInput {
        addressed_contact_id: c.contact,
        address: Some(address),
        manual: true,
        to: Vec::new(),
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: String::new(),
        body: String::new(),
        attachments: Vec::new(),
        email_profile_id: None,
        pdf_template_id: None,
    };
    invoices::send::number(shop.state, shop.ws_id, owner, invoice, &input).await
}

/// Invoice completed orders, newest first: 4 drafts, 7 sent, 5 part paid, 4 overdue, 14 paid
/// (3 of them credited). Plus three direct invoices: one draft, two cancelled.
async fn invoices(
    shop: &Shop<'_>,
    customers: &[Customer],
    completed: &mut [(String, i64, usize)],
    out: &mut Orders,
) -> Result<(), AppError> {
    let api = shop.api;
    completed.sort_by_key(|(_, days, _)| *days);
    let plan = std::iter::repeat_n("draft", 4)
        .chain(std::iter::repeat_n("pending", 7))
        .chain(std::iter::repeat_n("partial", 5))
        .chain(std::iter::repeat_n("overdue", 4))
        .chain(std::iter::repeat_n("paid", 11))
        .chain(std::iter::repeat_n("credited", 3));
    for ((order, days, i), kind) in completed.iter().zip(plan) {
        let customer = &customers[i % customers.len()];
        let res = api
            .post(
                &shop.at(&format!("/orders/{order}/invoice")),
                json!({}),
                &shop.owner,
            )
            .await?;
        let id = uuid(&text(&res, "invoice_id")?)?;
        out.invoices.push((id, *days, kind != "draft"));
        if kind == "draft" {
            continue;
        }
        number(shop, id, customer).await?;
        let path = shop.at(&format!("/invoices/{id}"));
        let open = cents(
            &api.call(Method::GET, &path, None, Some(&shop.owner))
                .await?["data"]["balance"]["open"],
        );
        let paid = match kind {
            "partial" => open / 2,
            "paid" | "credited" => open,
            _ => 0,
        };
        if paid > 0 {
            let method = ["bank", "pin", "cash"][i % 3];
            api.post(
                &format!("{path}/payments"),
                json!({ "date": days_ago((days - 12).max(0)), "amount": super::dollars(paid),
                        "payment_method": method }),
                &shop.owner,
            )
            .await?;
        }
        if kind == "credited" {
            let note = api
                .post(
                    &format!("{path}/credit"),
                    json!({ "credit_all": true }),
                    &shop.owner,
                )
                .await?;
            let note = uuid(&text(&note, "invoice_id")?)?;
            number(shop, note, customer).await?;
            api.post(
                &shop.at(&format!("/invoices/{note}/mark-as-credited")),
                json!({ "date": days_ago((days - 20).max(0)), "payment_method": "bank" }),
                &shop.owner,
            )
            .await?;
            out.invoices.push((note, (days - 20).max(0), true));
        }
    }
    for (n, (title, qty, price)) in [
        ("Wholesale clover honey, 60 lb pail", 2, "189.00"),
        ("Beeswax candle workshop, 12 seats", 1, "420.00"),
        ("Wholesale wildflower honey, 5 gal", 3, "165.00"),
    ]
    .into_iter()
    .enumerate()
    {
        let customer = &customers[n * 4];
        let inv = api
            .post(
                &shop.at("/invoices"),
                json!({ "relation_id": customer.id, "reference": format!("WS-{}", 300 + n),
                        "items": [{ "title": title, "quantity": qty, "price": price, "vat": 7 }] }),
                &shop.owner,
            )
            .await?;
        let id = text(&inv, "invoice_id")?;
        if n > 0 {
            api.call(
                Method::DELETE,
                &shop.at(&format!("/invoices/{id}")),
                None,
                Some(&shop.owner),
            )
            .await?;
        }
        out.invoices.push((uuid(&id)?, 2 + 6 * n as i64, false));
    }
    Ok(())
}

/// Web orders through the storefront key: paid and shipped (honey back in time, coffee this
/// week), waiting, failed, refunded and cancelled, part refunded.
async fn web(
    shop: &Shop<'_>,
    catalog: &Catalog,
    key: &str,
    coffee: &Supplied,
    out: &mut Orders,
) -> Result<(), AppError> {
    let api = shop.api;
    let goods: Vec<_> = catalog.web();
    // (kind, count)
    let plan = [
        ("shipped", 18),
        ("coffee", 4),
        ("waiting", 4),
        ("failed", 2),
        ("refunded", 2),
        ("part_refund", 1),
    ];
    let mut i = 0usize;
    for (kind, count) in plan {
        for _ in 0..count {
            i += 1;
            let first = names::US_FIRST[(i * 3) % names::US_FIRST.len()];
            let last = names::US_LAST[(i * 5 + 2) % names::US_LAST.len()];
            let p = goods[(i * 7) % goods.len()];
            let mut line = json!({ "product_id": p.id, "quantity": 1 + i % 2 });
            if !p.variants.is_empty() {
                line["product_variant_id"] = json!(p.variants[i % p.variants.len()].0);
            }
            let mut lines = vec![line];
            if kind == "coffee" {
                lines = vec![json!({ "product_id": coffee.coffee_id, "quantity": 1 + i % 3 })];
            }
            let order = api
                .call(
                    Method::POST,
                    "/storefront/orders",
                    Some(json!({ "lines": lines,
                        "customer": { "email": format!("{}.{}@mail.test", first.to_lowercase(), last.to_lowercase()),
                                      "name": format!("{first} {last}") } })),
                    Some(key),
                )
                .await?;
            let id = text(&order["data"], "id")?;
            let days = match kind {
                "shipped" => 2 + (i as i64 * 83) / 18,
                "coffee" => (i as i64) % 5,
                "refunded" | "part_refund" => 10 + i as i64,
                "failed" => 1 + i as i64 % 3,
                _ => 0,
            };
            if kind == "waiting" {
                out.waiting.push(uuid(&id)?);
                if out.waiting.len() == 1 {
                    out.samples
                        .push(("shop order (awaiting payment)", format!("/orders/{id}")));
                }
                continue;
            }
            let payment = api
                .call(
                    Method::POST,
                    &format!("/storefront/orders/{id}/payment"),
                    Some(json!({ "redirect_url": "https://shop.afm-demo.test/thanks" })),
                    Some(key),
                )
                .await?;
            let checkout = text(&payment["data"], "checkout_url")?;
            let provider = checkout.rsplit('/').next().unwrap_or_default().to_owned();
            match kind {
                "failed" => {
                    shop.settle(&provider, Status::Failed).await?;
                    out.waiting.push(uuid(&id)?);
                    out.dated.push((uuid(&id)?, days));
                    continue;
                }
                _ => shop.settle(&provider, Status::Paid).await?,
            }
            if kind != "refunded" {
                pick(shop, &id).await?;
                api.post(
                    &shop.at(&format!("/orders/{id}/complete")),
                    json!({}),
                    &shop.owner,
                )
                .await?;
            }
            if matches!(kind, "refunded" | "part_refund") {
                let order = api
                    .call(
                        Method::GET,
                        &shop.at(&format!("/orders/{id}")),
                        None,
                        Some(&shop.owner),
                    )
                    .await?;
                let total = order["data"]["total"]
                    .as_i64()
                    .unwrap_or_else(|| cents(&order["data"]["total"]));
                let amount = if kind == "refunded" { total } else { total / 2 };
                let refund = api
                    .post(
                        &shop.at(&format!("/orders/{id}/refunds")),
                        json!({ "amount": amount }),
                        &shop.owner,
                    )
                    .await?;
                let refund_id = uuid(&text(&refund, "refund_id")?)?;
                let fr: String = sqlx::query_scalar!(
                    r#"SELECT provider_id AS "provider_id!" FROM order_payments WHERE workspace_id = $1 AND id = $2"#,
                    shop.ws_id,
                    refund_id
                )
                .fetch_one(&shop.state.db)
                .await?;
                shop.settle(&fr, Status::Refunded).await?;
                if kind == "refunded" {
                    api.post(
                        &shop.at(&format!("/orders/{id}/cancel")),
                        json!({}),
                        &shop.owner,
                    )
                    .await?;
                    if !out
                        .samples
                        .iter()
                        .any(|(l, _)| l.starts_with("shop order (refunded"))
                    {
                        out.samples
                            .push(("shop order (refunded, cancelled)", format!("/orders/{id}")));
                    }
                }
            }
            if kind == "shipped" && i == 1 {
                out.samples
                    .push(("shop order (paid)", format!("/orders/{id}")));
            }
            if kind != "coffee" {
                out.dated.push((uuid(&id)?, days));
            }
        }
    }
    Ok(())
}

/// A purchase discount group from a supplier, and three special offers (running, coming, always).
async fn pricing(
    shop: &Shop<'_>,
    catalog: &Catalog,
    customers: &[Customer],
) -> Result<(), AppError> {
    let api = shop.api;
    let group = api
        .post(
            &shop.at("/products/discount-groups"),
            json!({ "name": "Brightwater Co-op purchase terms", "active": true,
                    "supplier_relation_id": customers[5].id }),
            &shop.owner,
        )
        .await?;
    let group = text(&group, "discount_group_id")?;
    for (name, code, pct) in [("Honey jars", "HONEY", "12.5"), ("Candles", "CANDLE", "8")] {
        api.post(
            &shop.at("/products/discounts"),
            json!({ "discount_group_id": group, "name": name, "active": true,
                    "discount_product_code": code, "percentage": pct, "valid_from": days_ago(0) }),
            &shop.owner,
        )
        .await?;
    }
    let pick = |name: &str| {
        catalog
            .products
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.id.clone())
            .unwrap_or_default()
    };
    for body in [
        json!({ "product_id": pick("Hot honey 12 oz"), "type": "percentage", "value": 15,
                "from_date": days_ago(0), "to_date": days_ago(-14) }),
        json!({ "product_id": pick("Gift box: breakfast"), "type": "percentage", "value": 20,
                "from_date": days_ago(-21), "to_date": days_ago(-35) }),
        json!({ "product_id": pick("Honey stix, 20 pack"), "type": "percentage", "value": 10,
                "always_active": true }),
    ] {
        api.post(&shop.at("/erp/special-offers"), body, &shop.owner)
            .await?;
    }
    Ok(())
}
