//! Staging demo seed (CLI `seed-demo`, never a route): every part end to end, through the
//! API's own routes in-process, so every rule, provenance stamp and audit row applies.
//! Refuses unless `GENESY_ALLOW_DEMO_SEED` is set (only the staging compose sets it). Runs once:
//! both demo workspaces present = nothing to do; one of them alone = refused (wipe the staging
//! volume and seed again; there is no in-CLI reset).
//!
//! - AFM shop `afm-demo`: USD, a store warehouse, a storefront app key, a till register with two
//!   cashiers, an invoice.
//! - RUAHA farm `ruaha-demo`: `farm_demo::run` (the farm story plus the 4c parts).
//! - Supply chain: a live link RUAHA → AFM; Asha's verified coffee harvest is bought into a raw
//!   lot, packed into 250 g bags, transferred to AFM and received into a lot with a published
//!   passport; a paid web order and a till sale of those bags credit the farmer; Juma pays her
//!   out at the desk.
//!
//! Every password, PIN and key is random per run and is returned once, for the CLI to print.

use std::time::Duration;

use axum::{
    body::{Body, to_bytes},
    extract::ConnectInfo,
    http::{Method, Request},
};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

use crate::{
    AppState, build_app,
    error::AppError,
    farm_demo::{self, Api, OWNER as RUAHA_OWNER, random_password, text},
    jobs::{
        self,
        kinds::{self, Ctx},
        runner::Step,
    },
    outbound::Outbound,
    payments::{Provider, Status, fake::Fake},
};

/// The AFM shop demo workspace's name.
pub const SHOP: &str = "afm-demo";
const SHOP_OWNER: &str = "afm.owner@afm-demo.test";

/// One login to print: which app, which role, the login name, its secret, where to use it.
pub struct Credential {
    pub app: &'static str,
    pub role: String,
    pub login: String,
    pub secret: String,
    pub path: &'static str,
}

/// What a run did. Secrets exist only here: print them once.
pub struct Seeded {
    /// False: both demo workspaces were already there and nothing changed.
    pub created: bool,
    pub credentials: Vec<Credential>,
    /// The key the shop container needs (`catalog:read`, `checkout:write`).
    pub storefront_key: Option<String>,
    /// The published passport's path (what its QR code encodes).
    pub passport_path: Option<String>,
}

fn refuse(message: &str) -> AppError {
    AppError::Unprocessable(message.to_owned())
}

/// A 4-digit till PIN.
fn pin() -> Result<String, AppError> {
    let (plain, _) = crate::modules::auth::token::generate()?;
    let n = plain.expose().bytes().fold(0u32, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(u32::from(b))
    });
    Ok(format!("{:04}", n % 10_000))
}

/// (exists, flagged demo) for a workspace name.
async fn workspace(state: &AppState, name: &str) -> Result<Option<bool>, AppError> {
    let row = sqlx::query_scalar!(
        r#"SELECT coalesce(meta->>'demo', '') = 'true' AS "demo!" FROM workspaces
           WHERE name = $1 AND deleted_at IS NULL"#,
        name
    )
    .fetch_optional(&state.db)
    .await?;
    Ok(row)
}

/// POST the Fake provider's webhook for `provider_id` (form-encoded, no token).
async fn webhook(api: &Api, ws: &str, provider_id: &str) -> Result<(), AppError> {
    let mut req = Request::post(format!("/v1/payments/webhooks/fake/{ws}"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(format!("id={provider_id}")))
        .map_err(|e| AppError::Internal(e.to_string()))?;
    req.extensions_mut()
        .insert(ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 0))));
    let res = api
        .app
        .clone()
        .oneshot(req)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let status = res.status();
    let _ = to_bytes(res.into_body(), 1 << 16).await;
    if !status.is_success() {
        return Err(AppError::Internal(format!(
            "demo seed: webhook -> {status}"
        )));
    }
    Ok(())
}

/// Run the payment inbox jobs in-process (the job runner is not up during a seed).
async fn drain_payments(state: &AppState, fake: &Fake) -> Result<(), AppError> {
    let ctx = Ctx {
        db: state.db.clone(),
        files: None,
        http: Outbound::new(&[]).map_err(|e| AppError::Internal(e.to_string()))?,
        credentials: None,
        payments: Some(Provider::Fake(fake.clone())),
    };
    while let Some(job) = jobs::repo::claim(
        &state.db,
        &[kinds::PAYMENT_INBOX],
        100,
        Duration::from_secs(60),
    )
    .await?
    {
        let (id, token) = (job.id, job.token);
        match kinds::dispatch(&ctx, job).await? {
            Step::Done => jobs::repo::finish(&state.db, id, token).await?,
            Step::More => jobs::repo::requeue(&state.db, id, token).await?,
        };
    }
    Ok(())
}

/// Seed everything. `allowed` is `GENESY_ALLOW_DEMO_SEED`.
pub async fn run(state: AppState, allowed: bool) -> Result<Seeded, AppError> {
    if !allowed {
        return Err(refuse(
            "seed-demo refuses to run: set GENESY_ALLOW_DEMO_SEED=1 (staging only, never production)",
        ));
    }
    match (
        workspace(&state, SHOP).await?,
        workspace(&state, farm_demo::WORKSPACE).await?,
    ) {
        (Some(true), Some(true)) => {
            return Ok(Seeded {
                created: false,
                credentials: Vec::new(),
                storefront_key: None,
                passport_path: None,
            });
        }
        (None, None) => {}
        (Some(false), _) | (_, Some(false)) => {
            return Err(refuse(
                "a workspace with a demo name exists and is not a demo workspace: refusing to seed",
            ));
        }
        _ => {
            return Err(refuse(
                "half-seeded: wipe the staging volume and run seed-demo again",
            ));
        }
    }

    let farm = farm_demo::run(state.clone()).await?;
    let (Some(staff_password), Some(farmer_password)) = (farm.staff_password, farm.farmer_password)
    else {
        return Err(refuse(
            "half-seeded: wipe the staging volume and run seed-demo again",
        ));
    };
    let r = farm.workspace_id.to_string();

    // The seed's own Fake provider marks the web order paid (Q8): the payment row says `fake`.
    let fake = Fake::default();
    let mut seed_state = state.clone();
    seed_state.payments = Some(Provider::Fake(fake.clone()));
    seed_state.signup_enabled = true;
    for limit in [
        &mut seed_state.rate_limit,
        &mut seed_state.private_rate_limit,
        &mut seed_state.private_ip_rate_limit,
    ] {
        limit.burst = 100_000;
    }
    let api = Api {
        app: build_app(seed_state),
        staff_password: staff_password.clone(),
        farmer_password: farmer_password.clone(),
    };

    // ---- AFM shop ----
    let shop_password = random_password()?;
    let signup = json!({ "first_name": "Faraji", "last_name": "Owner", "email": SHOP_OWNER,
                         "password": shop_password, "workspace_name": SHOP });
    let session = api
        .call(Method::POST, "/create-workspace", Some(signup), None)
        .await?;
    let owner = text(&session["data"], "access_token")?;
    let a = session["data"]["workspace_ids"][0]
        .as_str()
        .ok_or_else(|| AppError::Internal("demo seed: no shop workspace".into()))?
        .to_owned();
    let a_id: Uuid = a
        .parse()
        .map_err(|_| AppError::Internal("demo seed: shop workspace id".into()))?;
    sqlx::query!(
        "UPDATE workspaces SET meta = coalesce(meta, '{}'::jsonb) || '{\"demo\": true}'::jsonb WHERE id = $1",
        a_id
    )
    .execute(&state.db)
    .await?;
    let shop = |tail: &str| format!("/workspace/{a}{tail}");

    let currencies = api
        .call(Method::GET, &shop("/currencies"), None, Some(&owner))
        .await?;
    let usd = currencies["data"]
        .as_array()
        .and_then(|c| {
            c.iter().find(|c| {
                c["asset_type"]
                    .as_str()
                    .is_some_and(|t| t.eq_ignore_ascii_case("usd"))
            })
        })
        .ok_or_else(|| AppError::Internal("demo seed: no USD".into()))?;
    let usd_id = text(usd, "id").or_else(|_| text(usd, "currency_id"))?;
    api.post(
        &shop(&format!("/currencies/{usd_id}")),
        json!({ "default_currency": true }),
        &owner,
    )
    .await?;
    let store = api
        .post(
            &shop("/warehouses"),
            json!({ "name": "AFM store", "default_warehouse": true, "is_store": true }),
            &owner,
        )
        .await?;
    let store_id = text(&store, "warehouse_id")?;
    let coffee = api
        .post(
            &shop("/products"),
            json!({ "name": "Ruaha coffee 250 g", "price": "8.50" }),
            &owner,
        )
        .await?;
    let coffee_id = text(&coffee, "product_id")?;
    let key = api
        .post(
            &shop("/app-keys"),
            json!({ "name": "Web shop", "kind": "shop", "scopes": ["catalog:read", "checkout:write"] }),
            &owner,
        )
        .await?;
    let storefront_key = text(&key, "key")?;

    // ---- supply chain: RUAHA side ----
    let ruaha_owner = api.login(RUAHA_OWNER).await?;
    let ops = api.login("asha.ops@ruaha-demo.test").await?;
    let salima = api.login("salima@ruaha-demo.test").await?;
    let juma = api.login("juma@ruaha-demo.test").await?;
    let farm = |tail: &str| format!("/workspace/{r}/farm{tail}");
    let ws_r = |tail: &str| format!("/workspace/{r}{tail}");

    let link = api
        .post(
            &ws_r("/supply-links"),
            json!({ "to_workspace_id": a, "fields": ["village", "farmer_count", "harvest_window"] }),
            &ruaha_owner,
        )
        .await?;
    let link_id = text(&link, "id")?;
    api.post(
        &shop(&format!("/supply-links/{link_id}/accept")),
        json!({}),
        &owner,
    )
    .await?;

    // Asha's coffee cycle, from the farm demo's registration.
    let cycle: Uuid = sqlx::query_scalar!(
        "SELECT c.id FROM farm_crop_cycles c JOIN farm_crops cr ON cr.id = c.crop_id
         JOIN farm_plots p ON p.id = c.plot_id JOIN farm_managers m ON m.farm_id = p.farm_id
         JOIN farm_persons fp ON fp.id = m.person_id
         WHERE c.workspace_id = $1 AND cr.code = 'COFFEE' AND fp.given_name = 'Asha'",
        farm_demo_id(&r)?
    )
    .fetch_one(&state.db)
    .await?;
    let harvest = api
        .post(
            &farm(&format!("/crop-cycles/{cycle}/harvest-reports")),
            json!({ "kind": "actual", "quantity_kg": "300", "source": "field_verified" }),
            &salima,
        )
        .await?;
    let harvest_id = text(&harvest, "id")?;
    api.post(
        &farm(&format!("/harvest-reports/{harvest_id}/verification")),
        json!({}),
        &ruaha_owner,
    )
    .await?;
    let r_currencies = api
        .call(Method::GET, &ws_r("/currencies"), None, Some(&ruaha_owner))
        .await?;
    let tzs = r_currencies["data"]
        .as_array()
        .and_then(|c| {
            c.iter().find(|c| {
                c["asset_type"]
                    .as_str()
                    .is_some_and(|t| t.eq_ignore_ascii_case("tzs"))
            })
        })
        .ok_or_else(|| AppError::Internal("demo seed: no TZS".into()))?;
    let tzs_id = text(tzs, "id").or_else(|_| text(tzs, "currency_id"))?;
    let villages = api
        .call(Method::GET, &farm("/villages"), None, Some(&ops))
        .await?;
    let ilundo = villages["data"]
        .as_array()
        .and_then(|v| v.iter().find(|v| v["code"] == "ILUNDO"))
        .map(|v| v["id"].clone())
        .ok_or_else(|| AppError::Internal("demo seed: no Ilundo".into()))?;
    let crops = api
        .call(Method::GET, &farm("/crops"), None, Some(&ops))
        .await?;
    let coffee_crop = crops["data"]
        .as_array()
        .and_then(|v| v.iter().find(|v| v["code"] == "COFFEE"))
        .map(|v| v["id"].clone())
        .ok_or_else(|| AppError::Internal("demo seed: no coffee crop".into()))?;
    let raw = api
        .post(
            &ws_r("/products"),
            json!({ "name": "Coffee cherries (raw)", "price": "1" }),
            &ruaha_owner,
        )
        .await?;
    let bags = api
        .post(
            &ws_r("/products"),
            json!({ "name": "Ruaha coffee 250 g", "price": "8" }),
            &ruaha_owner,
        )
        .await?;
    let batch = api
        .post(
            &farm("/supply-batches"),
            json!({ "client_ref": Uuid::now_v7(), "village_id": ilundo, "crop_id": coffee_crop,
                    "code": "CF-2026-01", "raw_product_id": raw["product_id"],
                    "price_per_kg": 800_000, "currency_id": tzs_id,
                    "lines": [{ "harvest_report_id": harvest_id, "kg": "300" }] }),
            &ops,
        )
        .await?;
    let raw_lot = batch["lot"]["id"]
        .as_str()
        .ok_or_else(|| AppError::Internal("demo seed: no raw lot".into()))?
        .to_owned();
    let r_store = api
        .post(
            &ws_r("/warehouses"),
            json!({ "name": "Ilundo store", "default_warehouse": true, "is_store": true }),
            &ruaha_owner,
        )
        .await?;
    let r_store_id = text(&r_store, "warehouse_id")?;
    api.post(
        &ws_r(&format!("/lots/{raw_lot}/stock-mutations")),
        json!({ "warehouse_id": r_store_id, "quantity": "300", "reason": "received" }),
        &ruaha_owner,
    )
    .await?;
    let packed = api
        .post(
            &ws_r(&format!("/lots/{raw_lot}/packings")),
            json!({ "product_id": bags["product_id"], "code": "CF-BAG-01", "units": "40",
                    "raw_kg": "10.00", "warehouse_id": r_store_id }),
            &ruaha_owner,
        )
        .await?;
    let bag_lot = text(&packed, "id")?;
    let transfer = api
        .post(
            &ws_r(&format!("/lots/{bag_lot}/transfers")),
            json!({ "link_id": link_id, "warehouse_id": r_store_id, "quantity": "40" }),
            &ruaha_owner,
        )
        .await?;
    let transfer_id = text(&transfer, "id")?;

    // ---- supply chain: AFM side ----
    let received = api
        .post(
            &shop(&format!("/supply-transfers/{transfer_id}/receive")),
            json!({ "product_id": coffee_id, "code": "AFM-CF-01", "warehouse_id": store_id,
                    "quantity": "40" }),
            &owner,
        )
        .await?;
    let shop_lot = received["to_lot"]["id"]
        .as_str()
        .ok_or_else(|| AppError::Internal("demo seed: no received lot".into()))?
        .to_owned();
    api.call(
        Method::PUT,
        &shop(&format!("/lots/{shop_lot}/passport")),
        Some(json!({ "title": "Ruaha coffee, Ilundo village",
                     "intro": "DEMO: grown by smallholders in Ilundo, packed at the village store." })),
        Some(&owner),
    )
    .await?;
    let published = api
        .post(
            &shop(&format!("/lots/{shop_lot}/passport/publish")),
            json!({}),
            &owner,
        )
        .await?;
    let lot = api
        .call(
            Method::GET,
            &shop(&format!("/lots/{shop_lot}")),
            None,
            Some(&owner),
        )
        .await?;
    let public_code = lot["data"]["public_code"]
        .as_str()
        .or_else(|| published["public_code"].as_str())
        .unwrap_or_default()
        .to_owned();

    // A paid web order of the farmer's coffee (the seed's own Fake provider).
    let order = api
        .call(
            Method::POST,
            "/storefront/orders",
            Some(
                json!({ "lines": [{ "product_id": coffee_id, "quantity": 2 }],
                         "customer": { "email": "buyer@afm-demo.test", "name": "Demo Buyer" } }),
            ),
            Some(&storefront_key),
        )
        .await?;
    let order_id = text(&order["data"], "id")?;
    let payment = api
        .call(
            Method::POST,
            &format!("/storefront/orders/{order_id}/payment"),
            Some(json!({ "redirect_url": "https://shop.afm-demo.test/thanks" })),
            Some(&storefront_key),
        )
        .await?;
    let checkout = text(&payment["data"], "checkout_url")?;
    let provider_id = checkout.rsplit('/').next().unwrap_or_default().to_owned();
    fake.set_status(&provider_id, Status::Paid);
    webhook(&api, &a, &provider_id).await?;
    drain_payments(&state, &fake).await?;
    let open = api
        .call(
            Method::GET,
            &shop(&format!("/orders/{order_id}")),
            None,
            Some(&owner),
        )
        .await?;
    let lines: Vec<Value> = open["data"]["lines"]
        .as_array()
        .map(|l| {
            l.iter()
                .map(|l| json!({ "line_id": l["id"], "quantity": l["quantity"] }))
                .collect()
        })
        .unwrap_or_default();
    api.post(
        &shop(&format!("/orders/{order_id}/pick")),
        json!({ "lines": lines }),
        &owner,
    )
    .await?;
    api.post(
        &shop(&format!("/orders/{order_id}/complete")),
        json!({}),
        &owner,
    )
    .await?;
    let relation = api
        .post(
            &shop("/relations"),
            json!({ "company_name": "Demo Buyer Ltd", "type": "business" }),
            &owner,
        )
        .await?;
    api.call(
        Method::PATCH,
        &shop(&format!("/orders/{order_id}")),
        Some(json!({ "relation_id": relation["relation_id"] })),
        Some(&owner),
    )
    .await?;
    api.post(
        &shop(&format!("/orders/{order_id}/invoice")),
        json!({}),
        &owner,
    )
    .await?;

    // ---- till ----
    let register = api
        .post(
            &shop("/till/registers"),
            json!({ "name": "AFM counter", "warehouse_id": store_id }),
            &owner,
        )
        .await?;
    let device_key = text(&register, "key")?;
    let mut cashiers = Vec::new();
    for name in ["Zawadi", "Baraka"] {
        let p = pin()?;
        let user = api
            .post(
                &shop("/till/users"),
                json!({ "name": name, "pin": p }),
                &owner,
            )
            .await?;
        cashiers.push((name, text(&user, "id")?, p));
    }
    let (_, first_id, first_pin) = &cashiers[0];
    let session = api
        .call(
            Method::POST,
            &shop("/till/login"),
            Some(json!({ "till_user_id": first_id, "pin": first_pin })),
            Some(&device_key),
        )
        .await?;
    let till = text(&session["data"], "token")?;
    api.call(
        Method::POST,
        &shop("/till/shift/open"),
        Some(json!({ "count": [{ "denomination": 1000, "count": 5 }] })),
        Some(&till),
    )
    .await?;
    for payment in [
        json!({ "method": "cash", "tendered": 1000 }),
        json!({ "method": "card" }),
    ] {
        api.call(
            Method::POST,
            &shop("/till/sales"),
            Some(json!({ "client_ref": Uuid::now_v7(), "payment": payment,
                         "lines": [{ "product_id": coffee_id, "quantity": "1", "unit_price": 850 }] })),
            Some(&till),
        )
        .await?;
    }

    // ---- payout: Juma pays Asha what the sales credited (four eyes: he neither registered
    // nor verified her household) ----
    let asha: Uuid = sqlx::query_scalar!(
        "SELECT id FROM farm_persons WHERE workspace_id = $1 AND given_name = 'Asha'",
        farm_demo_id(&r)?
    )
    .fetch_one(&state.db)
    .await?;
    let passbook = api
        .call(
            Method::GET,
            &farm(&format!("/persons/{asha}/passbook")),
            None,
            Some(&ops),
        )
        .await?;
    let balance = passbook["data"][0]["balance_after"]["minor"]
        .as_i64()
        .unwrap_or(0);
    if balance > 0 {
        api.post(
            &farm("/payouts"),
            json!({ "client_ref": Uuid::now_v7(), "person_id": asha,
                    "amount": balance.min(10_000_000), "currency_id": tzs_id,
                    "id_type": "nida", "name_confirmed": true }),
            &juma,
        )
        .await?;
    }

    let mut credentials = vec![Credential {
        app: "office (AFM)",
        role: "owner".into(),
        login: SHOP_OWNER.into(),
        secret: shop_password,
        path: "/login",
    }];
    credentials.push(Credential {
        app: "office (RUAHA)",
        role: "admin".into(),
        login: RUAHA_OWNER.into(),
        secret: staff_password.clone(),
        path: "/login",
    });
    for (email, _, _, role, _) in farm_demo::STAFF {
        credentials.push(Credential {
            app: "field (RUAHA)",
            role: (*role).into(),
            login: (*email).into(),
            secret: staff_password.clone(),
            path: "/login",
        });
    }
    credentials.push(Credential {
        app: "field (RUAHA)",
        role: "farmer".into(),
        login: "0712000001".into(),
        secret: farmer_password,
        path: "/farm/login",
    });
    credentials.push(Credential {
        app: "till (AFM)",
        role: "device key".into(),
        login: "AFM counter".into(),
        secret: device_key,
        path: "/till/login",
    });
    for (name, _, p) in cashiers {
        credentials.push(Credential {
            app: "till (AFM)",
            role: "cashier PIN".into(),
            login: name.into(),
            secret: p,
            path: "/till/login",
        });
    }
    Ok(Seeded {
        created: true,
        credentials,
        storefront_key: Some(storefront_key),
        passport_path: Some(format!("/b/{public_code}")),
    })
}

fn farm_demo_id(r: &str) -> Result<Uuid, AppError> {
    r.parse()
        .map_err(|_| AppError::Internal("demo seed: farm workspace id".into()))
}
