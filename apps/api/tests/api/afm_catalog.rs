//! AFM catalogue sync (`seed-afm-catalog`): only `afm` / `afm-demo`, fills the catalogue once,
//! repairs drift on rerun, touches no stock, VAT or other workspace, leaves no login behind.

use crate::common;

use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use genesy_360_api::{bootstrap, demo, mail::Mailer};
use sqlx::PgPool;
use uuid::Uuid;

const PNG: &[u8] = &[
    0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n', 0, 0, 0, 0,
];

async fn cdn() -> (demo::photos::Photos, Arc<Mutex<usize>>) {
    let hits = Arc::new(Mutex::new(0usize));
    let seen = hits.clone();
    let app = axum::Router::new().fallback(move || {
        let seen = seen.clone();
        async move {
            *seen.lock().unwrap() += 1;
            (StatusCode::OK, [("content-type", "image/png")], PNG)
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

async fn n(pool: &PgPool, ws: Uuid, q: &'static str) -> i64 {
    sqlx::query_scalar::<_, i64>(q)
        .bind(ws)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn owner(state: &genesy_360_api::AppState, ws: &str) -> Uuid {
    bootstrap::create_owner(state, &format!("owner@{ws}.test"), ws, "A", "B")
        .await
        .unwrap()
        .workspace_id
}

#[sqlx::test]
async fn fills_afm_once_with_the_catalogue_its_photos_and_one_shop_key(pool: PgPool) {
    let state = common::state(pool.clone(), Mailer::capture().0);
    let afm = owner(&state, "afm").await;
    let other = owner(&state, "acme").await;
    let tokens = "SELECT count(*) FROM access_tokens";
    let tokens_before: i64 = sqlx::query_scalar(tokens).fetch_one(&pool).await.unwrap();
    let (photos, hits) = cdn().await;

    let done = demo::afm::sync(state.clone(), "afm", Some(&photos), true)
        .await
        .unwrap();

    assert_eq!(done.products_created, 79);
    assert_eq!(done.photos_added, 192);
    let key = done.app_key.expect("a new shop key");
    assert!(key.starts_with("gsk_") && key.len() == 68);
    let p = "SELECT count(*) FROM products WHERE workspace_id = $1 AND deleted_at IS NULL";
    assert_eq!(n(&pool, afm, p).await, 79);
    assert_eq!(
        n(
            &pool,
            afm,
            "SELECT count(*) FROM product_categories WHERE workspace_id = $1"
        )
        .await,
        12
    );
    for (q, want) in [
        // Everything online and buyable without stock rows (parent decision: has_stock false).
        ("SELECT count(*) FROM products WHERE workspace_id = $1 AND (has_stock OR store_only)", 0),
        ("SELECT count(*) FROM products WHERE workspace_id = $1 AND category_id IS NULL", 0),
        ("SELECT count(*) FROM stocks WHERE workspace_id = $1", 0),
        ("SELECT count(*) FROM warehouses WHERE workspace_id = $1", 0),
        ("SELECT count(*) FROM vat_groups WHERE workspace_id = $1", 0),
        ("SELECT count(*) FROM files WHERE workspace_id = $1", 192),
        ("SELECT count(*) FROM products WHERE workspace_id = $1 AND reference = 'AFM-033' AND price = 16.99", 1),
        ("SELECT count(*) FROM products WHERE workspace_id = $1 AND reference = 'AFM-077' AND price = 74.99", 1),
        // Two options: 13 colours x 9 spices.
        ("SELECT count(*) FROM product_variants v JOIN products p ON p.id = v.product_id
          WHERE p.workspace_id = $1 AND p.reference = 'AFM-077' AND NOT v.is_default AND v.deleted_at IS NULL", 117),
        ("SELECT count(*) FROM currencies WHERE workspace_id = $1 AND default_currency AND asset_type = 'usd'", 1),
        ("SELECT count(*) FROM workspaces WHERE id = $1 AND profile->>'company_name' = 'AFM Shop'", 1),
        ("SELECT count(*) FROM app_keys WHERE workspace_id = $1 AND revoked_at IS NULL AND name = 'Web shop'
          AND kind = 'shop' AND scopes = ARRAY['catalog:read', 'checkout:write']", 1),
    ] {
        assert_eq!(n(&pool, afm, q).await, want, "{q}");
    }
    // The other workspace is untouched; the seed's login is gone.
    assert_eq!(n(&pool, other, p).await, 0);
    assert_eq!(
        n(
            &pool,
            other,
            "SELECT count(*) FROM files WHERE workspace_id = $1"
        )
        .await,
        0
    );
    let tokens_after: i64 = sqlx::query_scalar(tokens).fetch_one(&pool).await.unwrap();
    assert_eq!(tokens_after, tokens_before);

    // Rerun: nothing changes, no download, no second key.
    let again = demo::afm::sync(state, "afm", Some(&photos), true)
        .await
        .unwrap();
    assert_eq!(again.changes(), 0, "{again:?}");
    assert!(again.app_key.is_none());
    // 192 photos, 190 distinct URLs (the three gift baskets share one): each fetched once.
    assert_eq!(*hits.lock().unwrap(), 190);
    assert_eq!(n(&pool, afm, p).await, 79);
}

#[sqlx::test]
async fn a_rerun_repairs_catalogue_drift_and_keeps_stock_settings(pool: PgPool) {
    let state = common::state(pool.clone(), Mailer::capture().0);
    let afm = owner(&state, "afm-demo").await;
    demo::afm::sync(state.clone(), "afm-demo", None, false)
        .await
        .unwrap();
    // Drift: an old price, a renamed product, a lost gift-basket variant, an extra one, a
    // store-only flag set by the shop (not catalogue data: kept).
    for q in [
        "UPDATE products SET price = 19.99 WHERE workspace_id = $1 AND reference = 'AFM-050'",
        "UPDATE products SET name = 'Old name' WHERE workspace_id = $1 AND reference = 'AFM-001'",
        "UPDATE products SET store_only = true WHERE workspace_id = $1 AND reference = 'AFM-021'",
        "UPDATE product_variants SET deleted_at = now() WHERE id = (SELECT v.id FROM product_variants v
           JOIN products p ON p.id = v.product_id WHERE p.workspace_id = $1 AND p.reference = 'AFM-077'
           AND NOT v.is_default ORDER BY v.id LIMIT 1)",
        "UPDATE product_variants SET price = 1.00 WHERE id = (SELECT v.id FROM product_variants v
           JOIN products p ON p.id = v.product_id WHERE p.workspace_id = $1 AND p.reference = 'AFM-078'
           AND NOT v.is_default ORDER BY v.id LIMIT 1)",
    ] {
        sqlx::query(q).bind(afm).execute(&pool).await.unwrap();
    }

    let fixed = demo::afm::sync(state.clone(), "afm-demo", None, false)
        .await
        .unwrap();

    assert_eq!(fixed.products_created, 0);
    assert_eq!(fixed.products_updated, 2, "{fixed:?}");
    assert_eq!(fixed.variants_created, 1, "{fixed:?}");
    assert_eq!(fixed.variants_updated, 1, "{fixed:?}");
    for (q, want) in [
        ("SELECT count(*) FROM products WHERE workspace_id = $1 AND reference = 'AFM-050' AND price = 21.99", 1),
        ("SELECT count(*) FROM products WHERE workspace_id = $1 AND name = 'Old name'", 0),
        ("SELECT count(*) FROM products WHERE workspace_id = $1 AND reference = 'AFM-021' AND store_only", 1),
        ("SELECT count(*) FROM product_variants v JOIN products p ON p.id = v.product_id
          WHERE p.workspace_id = $1 AND p.reference = 'AFM-077' AND NOT v.is_default AND v.deleted_at IS NULL", 117),
        ("SELECT count(*) FROM product_variants v JOIN products p ON p.id = v.product_id
          WHERE p.workspace_id = $1 AND p.reference = 'AFM-078' AND v.deleted_at IS NULL AND v.price <> 74.99
          AND NOT v.is_default", 0),
    ] {
        assert_eq!(n(&pool, afm, q).await, want, "{q}");
    }
    let quiet = demo::afm::sync(state, "afm-demo", None, false)
        .await
        .unwrap();
    assert_eq!(quiet.changes(), 0, "{quiet:?}");
}

#[sqlx::test]
async fn only_afms_own_workspaces_and_only_when_they_exist(pool: PgPool) {
    let state = common::state(pool.clone(), Mailer::capture().0);
    owner(&state, "acme").await;
    for ws in ["acme", "afm", "afm-demo"] {
        let err = demo::afm::sync(state.clone(), ws, None, false).await.err();
        assert!(err.is_some(), "{ws} refused");
    }
    let products: i64 = sqlx::query_scalar("SELECT count(*) FROM products")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(products, 0);
}
