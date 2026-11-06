//! Staging demo seed (`seed-demo`): refuses without its flag, seeds every part once, and every
//! credential it prints actually logs in. No secret reaches the logs.

use crate::common;

use std::sync::{Arc, Mutex};

use axum::http::{Method, StatusCode};
use common::send;
use genesy_360_api::{demo, mail::Mailer};
use serde_json::json;
use sqlx::PgPool;

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

async fn count(pool: &PgPool, q: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(q.to_owned()))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test]
async fn the_seed_refuses_without_its_flag(pool: PgPool) {
    let state = common::state(pool.clone(), Mailer::capture().0);
    let err = demo::run(state, false).await.err().expect("refused");
    assert!(err.to_string().contains("GENESY_ALLOW_DEMO_SEED"), "{err}");
    assert_eq!(count(&pool, "SELECT count(*) FROM workspaces").await, 0);
}

#[sqlx::test]
async fn the_seed_runs_once_and_every_login_works(pool: PgPool) {
    let logs = Captured::default();
    let writer = logs.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);
    let seeded = demo::run(common::state(pool.clone(), Mailer::capture().0), true)
        .await
        .unwrap();
    drop(guard);
    assert!(seeded.created);
    let text = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    for c in &seeded.credentials {
        // A 4-digit PIN occurs by chance in timestamps and ids: look for it as a value.
        let needle = if c.secret.len() < 8 {
            format!("\"{}\"", c.secret)
        } else {
            c.secret.clone()
        };
        assert!(
            !text.contains(&needle),
            "a {} secret reached the logs",
            c.role
        );
    }

    let app = common::app(pool.clone());
    let roles: Vec<&str> = seeded.credentials.iter().map(|c| c.role.as_str()).collect();
    for role in [
        "owner",
        "admin",
        "ops",
        "field_officer",
        "farmer",
        "device key",
        "cashier PIN",
    ] {
        assert!(roles.contains(&role), "{role} in {roles:?}");
    }
    let device = seeded
        .credentials
        .iter()
        .find(|c| c.role == "device key")
        .unwrap()
        .secret
        .clone();
    let shop: String =
        sqlx::query_scalar("SELECT id::text FROM workspaces WHERE name = 'afm-demo'")
            .fetch_one(&pool)
            .await
            .unwrap();
    for c in &seeded.credentials {
        let (status, res) = match c.path {
            "/login" => {
                let body = json!({ "email": c.login, "password": c.secret });
                send(&app, Method::POST, "/v1/login", Some(body), None).await
            }
            "/farm/login" => {
                let body = json!({ "phone": c.login, "password": c.secret });
                send(&app, Method::POST, "/v1/farm/login", Some(body), None).await
            }
            _ if c.role == "cashier PIN" => {
                let id: String =
                    sqlx::query_scalar("SELECT id::text FROM till_users WHERE name = $1")
                        .bind(&c.login)
                        .fetch_one(&pool)
                        .await
                        .unwrap();
                let body = json!({ "till_user_id": id, "pin": c.secret });
                let uri = format!("/v1/workspace/{shop}/till/login");
                send(&app, Method::POST, &uri, Some(body), Some(&device)).await
            }
            _ => {
                let uri = format!("/v1/workspace/{shop}/till/login/users");
                send(&app, Method::GET, &uri, None, Some(&device)).await
            }
        };
        assert_eq!(status, StatusCode::OK, "{} {}: {res}", c.app, c.role);
    }
    let key = seeded.storefront_key.clone().unwrap();
    let (status, res) = send(
        &app,
        Method::GET,
        "/v1/storefront/products",
        None,
        Some(&key),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let passport = seeded.passport_path.clone().unwrap();
    let code = passport.trim_start_matches("/b/");
    let (status, res) = send(
        &app,
        Method::GET,
        &format!("/v1/public/lots/{code}"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");

    // The supply chain ran end to end: one live link, farmer credits from web, office and till
    // sales, payouts, invoices, paid web orders (counts per module: `the_seed_fills_every_module`).
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM supply_links WHERE accepted_at IS NOT NULL AND revoked_at IS NULL"
        )
        .await,
        1
    );
    assert!(count(&pool, "SELECT count(*) FROM farm_sale_attributions").await >= 2);
    assert!(count(&pool, "SELECT count(*) FROM farm_payouts").await >= 1);
    assert!(count(&pool, "SELECT count(*) FROM invoices").await >= 1);
    assert!(
        count(
            &pool,
            "SELECT count(*) FROM order_payments WHERE status = 'paid'"
        )
        .await
            >= 1
    );

    // A second run changes nothing.
    let tables = [
        "workspaces",
        "users",
        "orders",
        "farm_pue_requests",
        "farm_payouts",
        "till_users",
    ];
    let mut before = Vec::new();
    for t in tables {
        before.push(count(&pool, &format!("SELECT count(*) FROM {t}")).await);
    }
    let again = demo::run(common::state(pool.clone(), Mailer::capture().0), true)
        .await
        .unwrap();
    assert!(!again.created && again.credentials.is_empty());
    for (t, n) in tables.iter().zip(before) {
        assert_eq!(
            count(&pool, &format!("SELECT count(*) FROM {t}")).await,
            n,
            "{t}"
        );
    }
}

#[sqlx::test]
async fn a_half_seeded_staging_is_refused(pool: PgPool) {
    genesy_360_api::farm_demo::run(common::state(pool.clone(), Mailer::capture().0))
        .await
        .unwrap();
    let err = demo::run(common::state(pool, Mailer::capture().0), true)
        .await
        .err()
        .expect("refused");
    assert!(err.to_string().contains("wipe the staging volume"), "{err}");
}

/// `count(*)` of `q` with `$1` bound to the workspace named `ws`.
async fn count_in(pool: &PgPool, ws: &str, q: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(q.to_owned()))
        .bind(workspace_id(pool, ws).await)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("{q}: {e}"))
}

async fn workspace_id(pool: &PgPool, name: &str) -> uuid::Uuid {
    sqlx::query_scalar("SELECT id FROM workspaces WHERE name = $1")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test]
async fn the_seed_fills_every_module(pool: PgPool) {
    let seeded = demo::run(common::state(pool.clone(), Mailer::capture().0), true)
        .await
        .unwrap();
    assert!(seeded.created);

    // (workspace, minimum, label, query with $1 = workspace id)
    let afm = "afm-demo";
    let ruaha = "ruaha-demo";
    let checks: &[(&str, i64, &str)] = &[
        // catalog and stock
        (
            afm,
            70,
            "SELECT count(*) FROM products WHERE workspace_id = $1 AND deleted_at IS NULL",
        ),
        (
            afm,
            30,
            "SELECT count(*) FROM product_variants WHERE workspace_id = $1",
        ),
        (
            afm,
            4,
            "SELECT count(*) FROM products WHERE workspace_id = $1 AND store_only",
        ),
        (
            afm,
            3,
            "SELECT count(*) FROM warehouses WHERE workspace_id = $1",
        ),
        (
            afm,
            24,
            "SELECT count(*) FROM warehouse_section_row_bins WHERE workspace_id = $1",
        ),
        (
            afm,
            5,
            "SELECT count(*) FROM stocks WHERE workspace_id = $1 AND stock_lvl > 0 AND stock_lvl < minimal_stock_level",
        ),
        (
            afm,
            1,
            "SELECT count(*) FROM stocks WHERE workspace_id = $1 AND stock_lvl < 0",
        ),
        (
            afm,
            100,
            "SELECT count(*) FROM stock_movements WHERE workspace_id = $1",
        ),
        // customers
        (
            afm,
            40,
            "SELECT count(*) FROM relations WHERE workspace_id = $1",
        ),
        (
            afm,
            50,
            "SELECT count(*) FROM contacts WHERE workspace_id = $1",
        ),
        // orders in every status, spread over time
        (
            afm,
            5,
            "SELECT count(*) FROM orders WHERE workspace_id = $1 AND status = 'draft'",
        ),
        (
            afm,
            5,
            "SELECT count(*) FROM orders WHERE workspace_id = $1 AND status = 'open'",
        ),
        (
            afm,
            150,
            "SELECT count(*) FROM orders WHERE workspace_id = $1 AND status = 'completed'",
        ),
        (
            afm,
            5,
            "SELECT count(*) FROM orders WHERE workspace_id = $1 AND status = 'cancelled'",
        ),
        (
            afm,
            2,
            "SELECT count(*) FROM orders WHERE workspace_id = $1 AND status = 'partially_returned'",
        ),
        (
            afm,
            2,
            "SELECT count(*) FROM orders WHERE workspace_id = $1 AND status = 'returned'",
        ),
        (
            afm,
            2,
            "SELECT count(*) FROM orders WHERE workspace_id = $1 AND status = 'awaiting_payment'",
        ),
        (
            afm,
            60,
            "SELECT (extract(epoch FROM max(created_at) - min(created_at)) / 86400)::bigint FROM orders WHERE workspace_id = $1",
        ),
        (
            afm,
            15,
            "SELECT count(*) FROM order_payments WHERE workspace_id = $1 AND kind = 'payment' AND status = 'paid'",
        ),
        (
            afm,
            1,
            "SELECT count(*) FROM order_payments WHERE workspace_id = $1 AND status = 'failed'",
        ),
        (
            afm,
            2,
            "SELECT count(*) FROM order_payments WHERE workspace_id = $1 AND status = 'refunded'",
        ),
        // invoices, payments, credit notes, overdue
        (
            afm,
            30,
            "SELECT count(*) FROM invoices WHERE workspace_id = $1",
        ),
        (
            afm,
            3,
            "SELECT count(*) FROM invoices WHERE workspace_id = $1 AND status = 'draft'",
        ),
        (
            afm,
            3,
            "SELECT count(*) FROM invoices WHERE workspace_id = $1 AND status = 'partially_paid'",
        ),
        (
            afm,
            10,
            "SELECT count(*) FROM invoices WHERE workspace_id = $1 AND status = 'paid'",
        ),
        (
            afm,
            2,
            "SELECT count(*) FROM invoices WHERE workspace_id = $1 AND is_credit",
        ),
        (
            afm,
            2,
            "SELECT count(*) FROM invoices WHERE workspace_id = $1 AND status IN ('pending', 'partially_paid') AND due_date < now()",
        ),
        (
            afm,
            20,
            "SELECT count(*) FROM invoice_payments WHERE workspace_id = $1",
        ),
        // discounts and offers
        (
            afm,
            2,
            "SELECT count(*) FROM discounts WHERE workspace_id = $1",
        ),
        (
            afm,
            3,
            "SELECT count(*) FROM special_offers WHERE workspace_id = $1",
        ),
        // lots and passports
        (afm, 8, "SELECT count(*) FROM lots WHERE workspace_id = $1"),
        (
            afm,
            5,
            "SELECT count(*) FROM lots WHERE workspace_id = $1 AND published_at IS NOT NULL",
        ),
        (
            afm,
            1,
            "SELECT count(*) FROM lots WHERE workspace_id = $1 AND status = 'quarantine' AND published_at IS NOT NULL",
        ),
        (
            afm,
            1,
            "SELECT count(*) FROM lots WHERE workspace_id = $1 AND status = 'recalled' AND published_at IS NOT NULL",
        ),
        (
            afm,
            1,
            "SELECT count(*) FROM lots WHERE workspace_id = $1 AND published_at IS NULL AND passport_draft IS NOT NULL",
        ),
        // supply: live, revoked and pending links; received and dispatched transfers
        (
            afm,
            1,
            "SELECT count(*) FROM supply_links WHERE to_workspace_id = $1 AND revoked_at IS NOT NULL",
        ),
        (
            afm,
            1,
            "SELECT count(*) FROM supply_links WHERE from_workspace_id = $1 AND accepted_at IS NULL AND revoked_at IS NULL",
        ),
        (
            afm,
            2,
            "SELECT count(*) FROM supply_transfers WHERE to_workspace_id = $1 AND received_at IS NOT NULL",
        ),
        (
            afm,
            1,
            "SELECT count(*) FROM supply_transfers WHERE to_workspace_id = $1 AND received_at IS NULL",
        ),
        // till: registers, cashiers, a week of shifts and sales
        (
            afm,
            2,
            "SELECT count(*) FROM till_registers WHERE workspace_id = $1",
        ),
        (
            afm,
            4,
            "SELECT count(*) FROM till_users WHERE workspace_id = $1",
        ),
        (
            afm,
            1,
            "SELECT count(*) FROM till_users WHERE workspace_id = $1 AND NOT active",
        ),
        (
            afm,
            100,
            "SELECT count(*) FROM till_sales WHERE workspace_id = $1",
        ),
        (
            afm,
            5,
            "SELECT count(DISTINCT sold_at::date) FROM till_sales WHERE workspace_id = $1",
        ),
        (
            afm,
            2,
            "SELECT count(*) FROM till_sales WHERE workspace_id = $1 AND price_mismatch <> 0",
        ),
        (
            afm,
            5,
            "SELECT count(*) FROM drawer_shifts WHERE workspace_id = $1 AND closed_at IS NOT NULL",
        ),
        (
            afm,
            2,
            "SELECT count(*) FROM drawer_shifts WHERE workspace_id = $1 AND difference <> 0",
        ),
        (
            afm,
            1,
            "SELECT count(*) FROM drawer_shifts WHERE workspace_id = $1 AND closed_at IS NULL",
        ),
        (
            afm,
            1,
            "SELECT count(*) FROM cash_vaults WHERE workspace_id = $1",
        ),
        // settings
        (
            afm,
            1,
            "SELECT count(*) FROM app_keys WHERE workspace_id = $1 AND revoked_at IS NOT NULL",
        ),
        (
            afm,
            1,
            "SELECT count(*) FROM webhook_endpoints WHERE workspace_id = $1 AND active",
        ),
        (
            afm,
            1,
            "SELECT count(*) FROM webhook_endpoints WHERE workspace_id = $1 AND NOT active",
        ),
        // farm: people, records, seasons
        (
            ruaha,
            5,
            "SELECT count(*) FROM farm_villages WHERE workspace_id = $1",
        ),
        (
            ruaha,
            9,
            "SELECT count(DISTINCT user_id) FROM farm_memberships WHERE workspace_id = $1 AND role <> 'farmer'",
        ),
        (
            ruaha,
            1,
            "SELECT count(*) FROM (SELECT user_id FROM farm_memberships WHERE workspace_id = $1 AND role <> 'farmer' GROUP BY user_id HAVING count(DISTINCT role) > 1) t",
        ),
        (
            ruaha,
            60,
            "SELECT count(*) FROM farm_persons WHERE workspace_id = $1",
        ),
        (
            ruaha,
            40,
            "SELECT count(*) FROM farm_households WHERE workspace_id = $1",
        ),
        (
            ruaha,
            40,
            "SELECT count(*) FROM farms WHERE workspace_id = $1",
        ),
        (
            ruaha,
            60,
            "SELECT count(*) FROM farm_plots WHERE workspace_id = $1",
        ),
        (
            ruaha,
            90,
            "SELECT count(*) FROM farm_crop_cycles WHERE workspace_id = $1",
        ),
        (
            ruaha,
            3,
            "SELECT count(DISTINCT season_label) FROM farm_crop_cycles WHERE workspace_id = $1",
        ),
        (
            ruaha,
            4,
            "SELECT count(DISTINCT status) FROM farm_crop_cycles WHERE workspace_id = $1",
        ),
        (
            ruaha,
            140,
            "SELECT count(*) FROM farm_harvest_reports WHERE workspace_id = $1",
        ),
        (
            ruaha,
            40,
            "SELECT count(*) FROM farm_persons WHERE workspace_id = $1 AND verification = 'verified'",
        ),
        (
            ruaha,
            60,
            "SELECT (extract(epoch FROM max(captured_at) - min(captured_at)) / 86400)::bigint FROM farm_persons WHERE workspace_id = $1",
        ),
        (
            ruaha,
            6,
            "SELECT count(*) FROM farm_persons WHERE workspace_id = $1 AND user_id IS NOT NULL",
        ),
        // surveys and vouchers
        (
            ruaha,
            8,
            "SELECT count(*) FROM farm_surveys WHERE workspace_id = $1",
        ),
        (
            ruaha,
            3,
            "SELECT count(DISTINCT status) FROM farm_surveys WHERE workspace_id = $1",
        ),
        (
            ruaha,
            12,
            "SELECT count(*) FROM farm_vouchers WHERE workspace_id = $1",
        ),
        (
            ruaha,
            3,
            "SELECT count(*) FROM farm_vouchers WHERE workspace_id = $1 AND status = 'redeemed'",
        ),
        (
            ruaha,
            1,
            "SELECT count(*) FROM farm_vouchers WHERE workspace_id = $1 AND status = 'void'",
        ),
        (
            ruaha,
            1,
            "SELECT count(*) FROM farm_vouchers WHERE workspace_id = $1 AND status = 'issued' AND expires_at < now()",
        ),
        // money to farmers
        (
            ruaha,
            3,
            "SELECT count(*) FROM farm_supply_batches WHERE workspace_id = $1",
        ),
        (
            ruaha,
            5,
            "SELECT count(DISTINCT person_id) FROM farm_passbook_entries WHERE workspace_id = $1",
        ),
        (
            ruaha,
            4,
            "SELECT count(*) FROM farm_payouts WHERE workspace_id = $1",
        ),
        (
            ruaha,
            1,
            "SELECT count(*) FROM farm_payout_voids WHERE workspace_id = $1",
        ),
        (
            ruaha,
            2,
            "SELECT count(*) FROM farm_cash_ledger WHERE workspace_id = $1",
        ),
        // energy and the market
        (
            ruaha,
            6,
            "SELECT count(*) FROM farm_equipment WHERE workspace_id = $1",
        ),
        (
            ruaha,
            3,
            "SELECT count(*) FROM farm_loan_products WHERE workspace_id = $1",
        ),
        (
            ruaha,
            12,
            "SELECT count(*) FROM farm_pue_requests WHERE workspace_id = $1",
        ),
        (
            ruaha,
            6,
            "SELECT count(DISTINCT status) FROM farm_pue_requests WHERE workspace_id = $1",
        ),
        (
            ruaha,
            4,
            "SELECT count(*) FROM farm_buyers WHERE workspace_id = $1",
        ),
        (
            ruaha,
            6,
            "SELECT count(*) FROM farm_buyer_demands WHERE workspace_id = $1",
        ),
        (
            ruaha,
            5,
            "SELECT count(*) FROM farm_opportunities WHERE workspace_id = $1",
        ),
        (
            ruaha,
            4,
            "SELECT count(DISTINCT status) FROM farm_opportunities WHERE workspace_id = $1",
        ),
        (
            ruaha,
            3,
            "SELECT count(*) FROM lots WHERE workspace_id = $1",
        ),
    ];
    let mut short = Vec::new();
    for (ws, min, q) in checks {
        let n = count_in(&pool, ws, q).await;
        if n < *min {
            short.push(format!("{ws}: {n} < {min}: {q}"));
        }
    }
    assert!(short.is_empty(), "short:\n{}", short.join("\n"));

    // AFM's real catalogue: its 75 products, each in one of the ten shop categories, its
    // prices, and the farm's coffee lots on AFM's own whole-bean coffee.
    let exact: &[(i64, &str)] = &[
        (
            75,
            "SELECT count(*) FROM products WHERE workspace_id = $1 AND deleted_at IS NULL",
        ),
        (
            75,
            "SELECT count(*) FROM products WHERE workspace_id = $1 AND category_id IS NOT NULL",
        ),
        (
            10,
            "SELECT count(*) FROM product_categories WHERE workspace_id = $1",
        ),
        (
            1,
            "SELECT count(*) FROM products WHERE workspace_id = $1 \
             AND name = 'Mary''s Tanzanian Wildflower Honey 13oz' AND price = 27.99",
        ),
        (
            0,
            "SELECT count(*) FROM products WHERE workspace_id = $1 AND name LIKE 'Ruaha coffee%'",
        ),
        (
            16,
            "SELECT count(*) FROM products p JOIN product_categories c ON c.id = p.category_id \
              WHERE p.workspace_id = $1 AND c.name = 'Baskets'",
        ),
    ];
    for (n, q) in exact {
        assert_eq!(count_in(&pool, afm, q).await, *n, "{q}");
    }
    let coffee_lots: i64 = count_in(
        &pool,
        afm,
        "SELECT count(*) FROM lots l JOIN products p ON p.id = l.product_id \
         WHERE l.workspace_id = $1 AND l.code LIKE 'AFM-CF-%' \
           AND p.name = 'Africa’s Wildest Whole Bean Coffee'",
    )
    .await;
    assert!(
        coffee_lots >= 1,
        "farm coffee lots on AFM's coffee: {coffee_lots}"
    );

    // Every row of the big tables belongs to one of the two demo workspaces, and no order or
    // invoice of one points at the other's relation, product or warehouse.
    let (a, r) = (
        workspace_id(&pool, afm).await,
        workspace_id(&pool, ruaha).await,
    );
    for t in [
        "products",
        "orders",
        "invoices",
        "lots",
        "relations",
        "farm_persons",
    ] {
        let stray: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM {t} WHERE workspace_id NOT IN ($1, $2)"
        )))
        .bind(a)
        .bind(r)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stray, 0, "{t}");
    }
    for q in [
        "SELECT count(*) FROM orders o JOIN relations x ON x.id = o.relation_id WHERE x.workspace_id <> o.workspace_id",
        "SELECT count(*) FROM order_lines l JOIN orders o ON o.id = l.order_id JOIN products p ON p.id = l.product_id WHERE p.workspace_id <> o.workspace_id",
        "SELECT count(*) FROM invoices i JOIN relations x ON x.id = i.relation_id WHERE x.workspace_id <> i.workspace_id",
        "SELECT count(*) FROM stocks s JOIN warehouses w ON w.id = s.warehouse_id WHERE w.workspace_id <> s.workspace_id",
    ] {
        assert_eq!(count(&pool, q).await, 0, "{q}");
    }

    // Each owner is refused on the other's workspace; a farmer is refused on the shop.
    let app = common::app(pool.clone());
    let cred = |role: &str, app_name: &str| {
        seeded
            .credentials
            .iter()
            .find(|c| c.role == role && c.app.contains(app_name))
            .unwrap_or_else(|| panic!("{role} {app_name}"))
    };
    let mut tokens = Vec::new();
    for c in [cred("owner", "AFM"), cred("admin", "RUAHA")] {
        let body = json!({ "email": c.login, "password": c.secret });
        let (_, res) = send(&app, Method::POST, "/v1/login", Some(body), None).await;
        tokens.push(res["data"]["access_token"].as_str().unwrap().to_owned());
    }
    let farmer = cred("farmer", "RUAHA");
    let body = json!({ "phone": farmer.login, "password": farmer.secret });
    let (_, res) = send(&app, Method::POST, "/v1/farm/login", Some(body), None).await;
    let farmer_token = res["data"]["access_token"].as_str().unwrap().to_owned();
    for (token, ws) in [(&tokens[0], r), (&tokens[1], a), (&farmer_token, a)] {
        for tail in ["/products", "/orders", "/lots", "/farm/persons"] {
            let uri = format!("/v1/workspace/{ws}{tail}");
            let (status, _) = send(&app, Method::GET, &uri, None, Some(token)).await;
            assert!(
                matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND),
                "{uri}: {status}"
            );
        }
    }

    // Buy harvest has something to buy on staging: some current, verified, actual harvests
    // stay unbought, verified by neither ops user (four eyes), so both see them.
    let village = |code: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, String>(
                "SELECT v.id::text FROM farm_villages v JOIN workspaces w ON w.id = v.workspace_id
                 WHERE w.name = 'ruaha-demo' AND v.code = $1",
            )
            .bind(code)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    let crop = |code: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, String>(
                "SELECT c.id::text FROM farm_crops c JOIN workspaces w ON w.id = c.workspace_id
                 WHERE w.name = 'ruaha-demo' AND c.code = $1",
            )
            .bind(code)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    for email in ["hamisi.ops@ruaha-demo.test", "asha.ops@ruaha-demo.test"] {
        let c = seeded
            .credentials
            .iter()
            .find(|c| c.login == email)
            .unwrap_or_else(|| panic!("{email} in the logins"));
        let body = json!({ "email": c.login, "password": c.secret });
        let (_, res) = send(&app, Method::POST, "/v1/login", Some(body), None).await;
        let token = res["data"]["access_token"].as_str().unwrap().to_owned();
        for (v, k) in [("ILUNDO", "COFFEE"), ("MGAMA", "MAIZE")] {
            let uri = format!(
                "/v1/workspace/{r}/farm/supply-batches/options?village_id={}&crop_id={}",
                village(v).await,
                crop(k).await
            );
            let (status, res) = send(&app, Method::GET, &uri, None, Some(&token)).await;
            assert_eq!(status, StatusCode::OK, "{email} {v} {k}: {res}");
            let n = res["data"]["harvests"].as_array().map_or(0, Vec::len);
            assert!(n >= 3, "{email} can buy {n} {k} harvests in {v}: {res}");
        }
    }

    // Raw values read as a person would: raw products in kg, passport dates in words,
    // the shop's company name beside its slug.
    let unitless: Vec<String> = sqlx::query_scalar(
        "SELECT p.name FROM products p JOIN workspaces w ON w.id = p.workspace_id
         WHERE w.name = 'ruaha-demo' AND p.name IN
               ('Coffee cherries (raw)', 'Maize grain, bulk', 'Beans, bulk')
           AND p.unit IS DISTINCT FROM 'kg'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(unitless.is_empty(), "raw products not in kg: {unitless:?}");
    let extracted: Vec<String> = sqlx::query_scalar(
        "SELECT f->>'value' FROM lots l
           CROSS JOIN LATERAL jsonb_array_elements(l.passport_draft->'story') s
           CROSS JOIN LATERAL jsonb_array_elements(s->'items') f
         WHERE s->>'kind' = 'facts' AND f->>'label' = 'Packed'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(!extracted.is_empty());
    for v in &extracted {
        let (day, rest) = v.split_once(' ').unwrap_or_else(|| panic!("{v}"));
        assert!(
            day.parse::<u8>().is_ok() && rest.len() > 5,
            "readable date: {v}"
        );
    }
    let (_, res) = send(
        &app,
        Method::GET,
        &format!("/v1/workspace/{r}/farm/dispatch/options"),
        None,
        Some(&tokens[1]),
    )
    .await;
    assert_eq!(
        res["data"]["links"][0]["partner"]["name"], "afm-demo",
        "{res}"
    );
    assert_eq!(
        res["data"]["links"][0]["partner"]["display_name"], "AFM Shop",
        "{res}"
    );
    // Both workspaces have a company name, so switchers show it instead of the slug.
    for (token, name) in [(&tokens[0], "AFM Shop"), (&tokens[1], "Ruaha Coffee Co-op")] {
        let (_, res) = send(&app, Method::GET, "/v1/me/workspaces", None, Some(token)).await;
        assert_eq!(res["data"][0]["display_name"], name, "{res}");
    }
}

/// A 1×1 PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

/// A fake image CDN on loopback answering every path with `status` (a PNG when 200); the
/// number of requests it got.
async fn cdn(status: StatusCode) -> (demo::photos::Photos, Arc<Mutex<usize>>) {
    let hits = Arc::new(Mutex::new(0usize));
    let seen = hits.clone();
    let app = axum::Router::new().fallback(move || {
        let seen = seen.clone();
        async move {
            *seen.lock().unwrap() += 1;
            (status, [("content-type", "image/png")], PNG)
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let photos = demo::photos::Photos {
        http: genesy_360_api::outbound::Outbound::new(&["127.0.0.1".into()]).unwrap(),
        origin: format!("http://{addr}"),
        pause: std::time::Duration::ZERO,
    };
    (photos, hits)
}

#[sqlx::test]
async fn with_images_every_product_gets_its_photos_fetched_once(pool: PgPool) {
    let (photos, hits) = cdn(StatusCode::OK).await;
    let state = common::state(pool.clone(), Mailer::capture().0);
    let store = state.files.clone().unwrap();

    let seeded = demo::run_with_photos(state, true, Some(photos))
        .await
        .unwrap();

    let (missing, total) = seeded.photos.unwrap();
    assert_eq!((missing, total), (0, 187));
    assert_eq!(*hits.lock().unwrap(), 187);
    let afm = "afm-demo";
    let bare = count_in(
        &pool,
        afm,
        "SELECT count(*) FROM products p WHERE p.workspace_id = $1 AND NOT EXISTS
           (SELECT 1 FROM product_images i WHERE i.workspace_id = $1 AND i.product_id = p.id)",
    )
    .await;
    assert_eq!(bare, 0, "every AFM product has a photo");
    assert_eq!(
        count_in(
            &pool,
            afm,
            "SELECT count(*) FROM files WHERE workspace_id = $1"
        )
        .await,
        187
    );
    // RUAHA's workspace sees none of AFM's photos.
    assert_eq!(
        count_in(
            &pool,
            "ruaha-demo",
            "SELECT count(*) FROM files WHERE workspace_id = $1"
        )
        .await,
        0
    );
    // A reseed reads the cached copy: AFM is never asked twice. The copy sits under `genesy/`,
    // the only prefix staging's and production's uploads keys may write.
    let url = demo::photos::CDN.to_owned() + "/content/v1/x/y.PNG?format=1000w";
    let root = format!("{}/", genesy_360_api::storage::UPLOADS_ROOT);
    assert!(demo::photos::cache_key(&url).starts_with(&root));
    assert!(demo::photos::cache_key(&url).starts_with("genesy/demo-cache/afm/"));
    let (again, _) = cdn(StatusCode::SERVICE_UNAVAILABLE).await;
    store
        .put(
            &demo::photos::cache_key(&url),
            PNG.to_vec(),
            "image/png",
            "p",
            false,
        )
        .await
        .unwrap();
    let cached = demo::photos::fetch(&store, &again, &url).await.unwrap();
    assert_eq!(cached.as_deref(), Some(PNG));
}

#[sqlx::test]
async fn with_images_and_afm_down_the_seed_still_finishes(pool: PgPool) {
    let (photos, _) = cdn(StatusCode::SERVICE_UNAVAILABLE).await;
    let state = common::state(pool.clone(), Mailer::capture().0);

    let seeded = demo::run_with_photos(state, true, Some(photos))
        .await
        .unwrap();

    assert!(seeded.created);
    assert_eq!(seeded.photos, Some((187, 187)));
    assert_eq!(
        count_in(
            &pool,
            "afm-demo",
            "SELECT count(*) FROM product_images WHERE workspace_id = $1"
        )
        .await,
        0
    );
}
