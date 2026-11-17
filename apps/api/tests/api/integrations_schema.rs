//! Integration glue tables (contracts §2.1): CHECKs, uniqueness and composite workspace FKs,
//! exercised in raw SQL.

use sqlx::PgPool;
use uuid::Uuid;

struct Ws {
    id: Uuid,
    user: Uuid,
}

async fn ws(pool: &PgPool, name: &str) -> Ws {
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, first_name, last_name, password_hash)
         VALUES ($1, 'O', 'W', 'h') RETURNING id",
    )
    .bind(format!("{name}@x.io"))
    .fetch_one(pool)
    .await
    .unwrap();
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO workspaces (name, created_by_user_id) VALUES ($2, $1) RETURNING id",
    )
    .bind(user)
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap();
    Ws { id, user }
}

/// SQLSTATE of a failed statement.
fn code(e: sqlx::Error) -> String {
    e.as_database_error()
        .and_then(|d| d.code())
        .map(|c| c.into_owned())
        .unwrap_or_else(|| panic!("not a database error: {e}"))
}

async fn connection(pool: &PgPool, w: &Ws, provider: &str, label: &str) -> Result<Uuid, String> {
    sqlx::query_scalar(
        "INSERT INTO integration_connections (workspace_id, provider, label, config, created_by)
         VALUES ($1, $2, $3, '{}', $4) RETURNING id",
    )
    .bind(w.id)
    .bind(provider)
    .bind(label)
    .bind(w.user)
    .fetch_one(pool)
    .await
    .map_err(code)
}

async fn pair(pool: &PgPool, w: &Ws, source: Uuid, target: Uuid) -> Result<Uuid, String> {
    sqlx::query_scalar(
        "INSERT INTO integration_pairs
           (workspace_id, workflow_key, source_connection_id, target_connection_id, config, created_by)
         VALUES ($1, 'wf01_purchase', $2, $3, '{}', $4) RETURNING id",
    )
    .bind(w.id)
    .bind(source)
    .bind(target)
    .bind(w.user)
    .fetch_one(pool)
    .await
    .map_err(code)
}

async fn mapping(
    pool: &PgPool,
    w: &Ws,
    pair: Uuid,
    entity: &str,
    source: &str,
) -> Result<Uuid, String> {
    sqlx::query_scalar(
        "INSERT INTO integration_mappings (workspace_id, pair_id, entity, source_id, target_id, created_by)
         VALUES ($1, $2, $3, $4, '7', $5) RETURNING id",
    )
    .bind(w.id)
    .bind(pair)
    .bind(entity)
    .bind(source)
    .bind(w.user)
    .fetch_one(pool)
    .await
    .map_err(code)
}

async fn exec(pool: &PgPool, sql: &'static str, a: Uuid, b: Uuid) -> Result<u64, String> {
    sqlx::query(sql)
        .bind(a)
        .bind(b)
        .execute(pool)
        .await
        .map(|r| r.rows_affected())
        .map_err(code)
}

#[sqlx::test]
async fn connections_check_provider_status_config_and_label(pool: PgPool) {
    let a = ws(&pool, "a").await;
    let odoo = connection(&pool, &a, "odoo", "Main").await.unwrap();
    let status: String =
        sqlx::query_scalar("SELECT status FROM integration_connections WHERE id = $1")
            .bind(odoo)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "draft");

    assert_eq!(
        connection(&pool, &a, "sap", "X").await.unwrap_err(),
        "23514"
    );
    assert_eq!(
        connection(&pool, &a, "odoo", "").await.unwrap_err(),
        "23514"
    );
    assert_eq!(
        connection(&pool, &a, "odoo", &"x".repeat(101))
            .await
            .unwrap_err(),
        "23514"
    );
    // Label unique per workspace + provider, any case; the other provider may reuse it.
    assert_eq!(
        connection(&pool, &a, "odoo", "MAIN").await.unwrap_err(),
        "23505"
    );
    connection(&pool, &a, "netsuite", "Main").await.unwrap();
    let b = ws(&pool, "b").await;
    connection(&pool, &b, "odoo", "Main").await.unwrap();

    let bad_status = exec(
        &pool,
        "UPDATE integration_connections SET status = 'broken' WHERE id = $1 AND workspace_id = $2",
        odoo,
        a.id,
    )
    .await;
    assert_eq!(bad_status.unwrap_err(), "23514");
    let array_config = exec(
        &pool,
        "UPDATE integration_connections SET config = '[]' WHERE id = $1 AND workspace_id = $2",
        odoo,
        a.id,
    )
    .await;
    assert_eq!(array_config.unwrap_err(), "23514");
    let big_config = sqlx::query(
        "UPDATE integration_connections SET config = jsonb_build_object('k', repeat('x', 17000))
         WHERE id = $1",
    )
    .bind(odoo)
    .execute(&pool)
    .await
    .map_err(code);
    assert_eq!(big_config.unwrap_err(), "23514");
}

#[sqlx::test]
async fn secrets_check_name_and_size_and_cascade(pool: PgPool) {
    let a = ws(&pool, "a").await;
    let odoo = connection(&pool, &a, "odoo", "Main").await.unwrap();
    let insert = |name: &'static str, len: i32| {
        let pool = pool.clone();
        let ws = a.id;
        async move {
            sqlx::query(
                "INSERT INTO integration_secrets (workspace_id, connection_id, name, key_id, sealed)
                 VALUES ($1, $2, $3, '0123456789abcdef', decode(repeat('00', $4), 'hex'))",
            )
            .bind(ws)
            .bind(odoo)
            .bind(name)
            .bind(len)
            .execute(&pool)
            .await
            .map_err(code)
        }
    };
    // A secret never points at another workspace's connection.
    let b = ws(&pool, "b").await;
    let foreign = sqlx::query(
        "INSERT INTO integration_secrets (workspace_id, connection_id, name, key_id, sealed)
         VALUES ($1, $2, 'api_key', '0123456789abcdef', decode(repeat('00', 40), 'hex'))",
    )
    .bind(b.id)
    .bind(odoo)
    .execute(&pool)
    .await
    .map_err(code);
    assert_eq!(foreign.unwrap_err(), "23503");

    insert("api_key", 40).await.unwrap();
    assert_eq!(insert("api_key", 40).await.unwrap_err(), "23505");
    assert_eq!(insert("password", 40).await.unwrap_err(), "23514");
    assert_eq!(insert("private_key", 39).await.unwrap_err(), "23514");
    assert_eq!(insert("private_key", 16041).await.unwrap_err(), "23514");
    insert("private_key", 16040).await.unwrap();

    exec(
        &pool,
        "DELETE FROM integration_connections WHERE id = $1 AND workspace_id = $2",
        odoo,
        a.id,
    )
    .await
    .unwrap();
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM integration_secrets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[sqlx::test]
async fn pairs_refuse_foreign_or_same_connections(pool: PgPool) {
    let a = ws(&pool, "a").await;
    let b = ws(&pool, "b").await;
    let odoo = connection(&pool, &a, "odoo", "O").await.unwrap();
    let ns = connection(&pool, &a, "netsuite", "N").await.unwrap();
    let foreign_ns = connection(&pool, &b, "netsuite", "N").await.unwrap();

    let p = pair(&pool, &a, odoo, ns).await.unwrap();
    assert_eq!(pair(&pool, &a, odoo, ns).await.unwrap_err(), "23505");
    assert_eq!(pair(&pool, &a, odoo, odoo).await.unwrap_err(), "23514");
    assert_eq!(
        pair(&pool, &a, odoo, foreign_ns).await.unwrap_err(),
        "23503"
    );

    // paused <=> paused_reason
    let paused_no_reason = exec(
        &pool,
        "UPDATE integration_pairs SET status = 'paused' WHERE id = $1 AND workspace_id = $2",
        p,
        a.id,
    )
    .await;
    assert_eq!(paused_no_reason.unwrap_err(), "23514");
    exec(
        &pool,
        "UPDATE integration_pairs SET status = 'paused', paused_reason = 'manual'
         WHERE id = $1 AND workspace_id = $2",
        p,
        a.id,
    )
    .await
    .unwrap();
    let unknown = exec(
        &pool,
        "UPDATE integration_pairs SET workflow_key = 'wf99' WHERE id = $1 AND workspace_id = $2",
        p,
        a.id,
    )
    .await;
    assert_eq!(unknown.unwrap_err(), "23514");
}

#[sqlx::test]
async fn mappings_unique_per_pair_entity_source(pool: PgPool) {
    let a = ws(&pool, "a").await;
    let odoo = connection(&pool, &a, "odoo", "O").await.unwrap();
    let ns = connection(&pool, &a, "netsuite", "N").await.unwrap();
    let p = pair(&pool, &a, odoo, ns).await.unwrap();

    mapping(&pool, &a, p, "vendor", "42").await.unwrap();
    assert_eq!(
        mapping(&pool, &a, p, "vendor", "42").await.unwrap_err(),
        "23505"
    );
    mapping(&pool, &a, p, "product", "42").await.unwrap();
    assert_eq!(
        mapping(&pool, &a, p, "tax", "1").await.unwrap_err(),
        "23514"
    );
    assert_eq!(
        mapping(&pool, &a, p, "vendor", "").await.unwrap_err(),
        "23514"
    );

    let b = ws(&pool, "b").await;
    assert_eq!(
        mapping(&pool, &b, p, "vendor", "43").await.unwrap_err(),
        "23503"
    );

    // Deleting the pair takes its mappings.
    exec(
        &pool,
        "DELETE FROM integration_pairs WHERE id = $1 AND workspace_id = $2",
        p,
        a.id,
    )
    .await
    .unwrap();
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM integration_mappings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[sqlx::test]
async fn cursors_and_links(pool: PgPool) {
    let a = ws(&pool, "a").await;
    let odoo = connection(&pool, &a, "odoo", "O").await.unwrap();
    let ns = connection(&pool, &a, "netsuite", "N").await.unwrap();

    let cursor = |stream: &'static str| {
        let pool = pool.clone();
        let ws = a.id;
        async move {
            sqlx::query(
                "INSERT INTO integration_poll_cursors (workspace_id, connection_id, stream, cursor_ts)
                 VALUES ($1, $2, $3, now())",
            )
            .bind(ws)
            .bind(odoo)
            .bind(stream)
            .execute(&pool)
            .await
            .map_err(code)
        }
    };
    cursor("wf01_purchase:x").await.unwrap();
    assert_eq!(cursor("wf01_purchase:x").await.unwrap_err(), "23505");
    assert_eq!(cursor("").await.unwrap_err(), "23514");

    let link = |corr: &'static str, state: &'static str, target: Option<&'static str>| {
        let pool = pool.clone();
        let ws = a.id;
        async move {
            sqlx::query(
                "INSERT INTO external_links (workspace_id, correlation_id, source_connection_id,
                   source_model, source_id, target_connection_id, target_type, target_id, state)
                 VALUES ($1, $2, $3, 'purchase.order', '17', $4, 'purchaseOrder', $5, $6)",
            )
            .bind(ws)
            .bind(corr)
            .bind(odoo)
            .bind(ns)
            .bind(target)
            .bind(state)
            .execute(&pool)
            .await
            .map_err(code)
        }
    };
    let corr = "vz-0f1e2d3c4b5a69788796a5b4c3d2e1f0";
    link(corr, "pending", None).await.unwrap();
    assert_eq!(link(corr, "pending", None).await.unwrap_err(), "23505");
    assert_eq!(
        link("vz-0F1E2D3C4B5A69788796A5B4C3D2E1F0", "pending", None)
            .await
            .unwrap_err(),
        "23514"
    );
    assert_eq!(
        link("vz-00000000000000000000000000000001", "linked", None)
            .await
            .unwrap_err(),
        "23514"
    );
    link(
        "vz-00000000000000000000000000000002",
        "linked",
        Some("8812"),
    )
    .await
    .unwrap();
    assert_eq!(
        link("vz-00000000000000000000000000000003", "done", None)
            .await
            .unwrap_err(),
        "23514"
    );
}
