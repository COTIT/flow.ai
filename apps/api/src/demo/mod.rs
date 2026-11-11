//! Staging demo seed (CLI `seed-demo`, never a route): every part end to end, through the
//! API's own routes in-process, so every rule, provenance stamp and audit row applies.
//! Refuses unless `GENESY_ALLOW_DEMO_SEED` is set (only the staging compose sets it). Runs once:
//! both demo workspaces present = nothing to do; one of them alone = refused (wipe the staging
//! volume and seed again; there is no in-CLI reset).
//!
//! - AFM shop `afm-demo` (USD): catalog, stock, customers, orders in every status, invoices,
//!   lots and passports (`catalog`, `sales`), two tills with a week of shifts (`till`).
//! - RUAHA farm `ruaha-demo`: `farm_demo::run`, then more villages, people, seasons, surveys,
//!   energy and market (`farm`).
//! - Supply chain both ways (`supply`): links, verified harvests bought into lots, packed,
//!   transferred and received; sales credit the farmers, who are paid out (`farm::payouts`).
//! - Dates spread over 90 days by one pass on mutable timestamps (`dates`); append-only ledgers
//!   keep their real times, so the rows they log stay in the last 7 days.
//!
//! Sizes are fixed. Every password, PIN and key is random per run and is returned once, for the
//! CLI to print.

mod catalog;
mod dates;
mod farm;
mod names;
pub mod photos;
mod sales;
mod supply;
mod till;

use std::time::{Duration, Instant};

use axum::{
    body::{Body, to_bytes},
    extract::ConnectInfo,
    http::{Method, Request},
};
use serde_json::json;
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
    /// The published coffee passport's path (what its QR code encodes).
    pub passport_path: Option<String>,
    /// More paths worth opening: (label, path).
    pub samples: Vec<(&'static str, String)>,
    /// With photos: (missing, total) AFM photos; missing ones left products without them.
    pub photos: Option<(usize, usize)>,
    /// How long the run took.
    pub elapsed: Duration,
}

/// One demo workspace as the seed drives it.
pub(crate) struct Shop<'a> {
    pub api: &'a Api,
    pub state: &'a AppState,
    pub fake: &'a Fake,
    pub owner: String,
    pub ws: String,
    pub ws_id: Uuid,
}

impl Shop<'_> {
    /// `/workspace/{ws}{tail}`.
    pub fn at(&self, tail: &str) -> String {
        format!("/workspace/{}{tail}", self.ws)
    }

    /// Mark a provider payment or refund, post its webhook and run the inbox, as the provider would.
    pub async fn settle(&self, provider_id: &str, status: Status) -> Result<(), AppError> {
        self.fake.set_status(provider_id, status);
        webhook(self.api, &self.ws, provider_id).await?;
        drain_payments(self.state, self.fake).await
    }
}

fn refuse(message: &str) -> AppError {
    AppError::Unprocessable(message.to_owned())
}

/// Cents as a decimal string, `1234` → `"12.34"`.
pub(crate) fn dollars(cents: i64) -> String {
    format!("{}.{:02}", cents / 100, cents % 100)
}

/// The date `days` before today (`YYYY-MM-DD`; negative is ahead).
pub(crate) fn days_ago(days: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

/// `days_ago` for people to read, e.g. "2 July 2026".
pub(crate) fn days_ago_read(days: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::days(days))
        .format("%-d %B %Y")
        .to_string()
}

/// An EAN-13 in the in-store range (prefix 2), so it can't collide with a real product.
pub(crate) fn ean13(n: u64) -> String {
    let body = format!("2{:011}", n % 100_000_000_000);
    let sum: u32 = body
        .bytes()
        .enumerate()
        .map(|(i, b)| u32::from(b - b'0') * if i % 2 == 0 { 1 } else { 3 })
        .sum();
    format!("{body}{}", (10 - sum % 10) % 10)
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
        invoice_state: None,
        assistant_api_key: None,
        db: state.db.clone(),
        files: None,
        http: Outbound::new(&[]).map_err(|e| AppError::Internal(e.to_string()))?,
        credentials: None,
        payments: Provider::Fake(fake.clone()).into(),
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
    run_with_photos(state, allowed, None).await
}

/// `run`, plus AFM's product photos when `photos` is set (`seed-demo --images`).
pub async fn run_with_photos(
    state: AppState,
    allowed: bool,
    photos: Option<photos::Photos>,
) -> Result<Seeded, AppError> {
    let started = Instant::now();
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
                samples: Vec::new(),
                photos: None,
                elapsed: started.elapsed(),
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

    let base = farm_demo::run(state.clone()).await?;
    let (Some(staff_password), Some(farmer_password)) = (base.staff_password, base.farmer_password)
    else {
        return Err(refuse(
            "half-seeded: wipe the staging volume and run seed-demo again",
        ));
    };

    // The seed's own Fake provider settles web payments (Q8): those rows say `fake`.
    let fake = Fake::default();
    let mut seed_state = state.clone();
    seed_state.payments = Provider::Fake(fake.clone()).into();
    seed_state.signup_enabled = true;
    for limit in [
        &mut seed_state.rate_limit,
        &mut seed_state.private_rate_limit,
        &mut seed_state.private_ip_rate_limit,
    ] {
        limit.burst = 100_000;
    }
    let api = Api {
        app: build_app(seed_state.clone()),
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
        "UPDATE workspaces SET meta = coalesce(meta, '{}'::jsonb) || '{\"demo\": true}'::jsonb,
                profile = coalesce(profile, '{}'::jsonb) || '{\"company_name\": \"AFM Shop\"}'::jsonb
         WHERE id = $1",
        a_id
    )
    .execute(&state.db)
    .await?;
    let shop = Shop {
        api: &api,
        state: &seed_state,
        fake: &fake,
        owner,
        ws: a,
        ws_id: a_id,
    };

    let currencies = api
        .call(
            Method::GET,
            &shop.at("/currencies"),
            None,
            Some(&shop.owner),
        )
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
        &shop.at(&format!("/currencies/{usd_id}")),
        json!({ "default_currency": true }),
        &shop.owner,
    )
    .await?;
    // US notes and coins for the drawer counts (the default list is the euro's).
    api.call(
        Method::POST,
        &shop.at("/settings"),
        Some(json!({ "cash_register": {
            "accepted_cash": ["0.01", "0.05", "0.10", "0.25", "1.00", "5.00", "10.00", "20.00", "50.00", "100.00"],
            "receipt_header": "AFM Honey & Coffee, Asheville NC (DEMO)" } })),
        Some(&shop.owner),
    )
    .await?;
    let store = api
        .post(
            &shop.at("/warehouses"),
            json!({ "name": "AFM store", "default_warehouse": true, "is_store": true }),
            &shop.owner,
        )
        .await?;
    let store_id = text(&store, "warehouse_id")?;
    let key = api
        .post(
            &shop.at("/app-keys"),
            json!({ "name": "Web shop", "kind": "shop", "scopes": ["catalog:read", "checkout:write"] }),
            &shop.owner,
        )
        .await?;
    let storefront_key = text(&key, "key")?;

    // The shop catalog and the farm records don't touch each other: build them side by side.
    let (catalog, ruaha) = tokio::try_join!(
        catalog::seed(&shop, &store_id),
        farm::seed(&api, base.workspace_id, &state.db)
    )?;
    let supplied = supply::seed(&shop, &ruaha, &store_id, &catalog).await?;
    let photos = match &photos {
        Some(p) => {
            let owner = sqlx::query_scalar!("SELECT id FROM users WHERE email = $1", SHOP_OWNER)
                .fetch_one(&state.db)
                .await?;
            Some(photos::attach(&shop, &catalog, owner, p).await?)
        }
        None => None,
    };
    let mut samples: Vec<(&'static str, String)> = catalog
        .passports
        .iter()
        .map(|(label, code)| (*label, format!("/b/{code}")))
        .collect();
    let orders = sales::seed(&shop, &catalog, &storefront_key, &supplied).await?;
    samples.extend(orders.samples.iter().cloned());
    let tills = till::seed(&shop, &catalog, &store_id, &supplied).await?;
    farm::payouts(&api, &ruaha).await?;

    // Settings: a revoked key and two webhook endpoints, made last so the seed fires no delivery.
    let old = api
        .post(
            &shop.at("/app-keys"),
            json!({ "name": "Old web shop (revoked)", "kind": "shop", "scopes": ["catalog:read"] }),
            &shop.owner,
        )
        .await?;
    api.call(
        Method::DELETE,
        &shop.at(&format!("/app-keys/{}", text(&old, "id")?)),
        None,
        Some(&shop.owner),
    )
    .await?;
    for (url, events, active) in [
        (
            "https://webhooks.example.com/afm/stock",
            json!(["stock.changed", "product.changed"]),
            true,
        ),
        (
            "https://webhooks.example.com/afm/orders",
            json!(["order.created"]),
            false,
        ),
    ] {
        let hook = api
            .post(
                &shop.at("/webhook-endpoints"),
                json!({ "url": url, "events": events, "description": "DEMO endpoint" }),
                &shop.owner,
            )
            .await?;
        if !active {
            api.call(
                Method::PATCH,
                &shop.at(&format!("/webhook-endpoints/{}", text(&hook, "id")?)),
                Some(json!({ "active": false })),
                Some(&shop.owner),
            )
            .await?;
        }
    }

    dates::backdate(&state.db, a_id, base.workspace_id, &orders, &tills).await?;

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
    for (email, role) in farm_demo::STAFF
        .iter()
        .map(|(email, _, _, role, _)| (*email, *role))
        .chain(ruaha.staff.iter().map(|(e, r)| (*e, *r)))
    {
        credentials.push(Credential {
            app: "field (RUAHA)",
            role: role.into(),
            login: email.into(),
            secret: staff_password.clone(),
            path: "/login",
        });
    }
    for phone in &ruaha.farmer_phones {
        credentials.push(Credential {
            app: "field (RUAHA)",
            role: "farmer".into(),
            login: phone.clone(),
            secret: farmer_password.clone(),
            path: "/farm/login",
        });
    }
    for (register, key) in tills.devices {
        credentials.push(Credential {
            app: "till (AFM)",
            role: "device key".into(),
            login: register.into(),
            secret: key,
            path: "/till/login",
        });
    }
    for (name, p) in tills.cashiers {
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
        passport_path: Some(format!("/b/{}", supplied.public_code)),
        samples,
        photos,
        elapsed: started.elapsed(),
    })
}
