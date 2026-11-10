//! Home design revisions: tenant isolation, explicit publishing and safe retries.

use crate::common::{self, Api};
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;

fn document() -> Value {
    serde_json::from_str(include_str!("../fixtures/storefront-design-v1.json")).unwrap()
}

async fn write(api: &Api, path: &str, body: Value, key: &str) -> (StatusCode, Value) {
    write_as(api, path, body, key, &api.owner).await
}

async fn write_as(
    api: &Api,
    path: &str,
    body: Value,
    key: &str,
    token: &str,
) -> (StatusCode, Value) {
    let (status, bytes) = common::send_raw(
        &api.app,
        Method::POST,
        &format!("/v1/workspace/{}/storefront/pages/home{path}", api.ws),
        Some(body),
        Some(token),
        &[("idempotency-key", key)],
    )
    .await;
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    common::contract::check(
        &Method::POST,
        &format!("/v1/workspace/{}/storefront/pages/home{path}", api.ws),
        status,
        &body,
    );
    (status, body)
}

#[test]
fn every_storefront_design_operation_is_documented() {
    let spec = genesy_360_api::openapi::spec().unwrap();
    for (method, path, id) in [
        (
            "get",
            "/workspace/{workspace}/storefront/pages/home",
            "storefront_design_get",
        ),
        (
            "post",
            "/workspace/{workspace}/storefront/pages/home/revisions",
            "storefront_design_save",
        ),
        (
            "get",
            "/workspace/{workspace}/storefront/pages/home/revisions",
            "storefront_design_revisions",
        ),
        (
            "get",
            "/workspace/{workspace}/storefront/pages/home/preview",
            "storefront_design_preview",
        ),
        (
            "post",
            "/workspace/{workspace}/storefront/pages/home/restore",
            "storefront_design_restore",
        ),
        (
            "post",
            "/workspace/{workspace}/storefront/pages/home/publish",
            "storefront_design_publish",
        ),
        (
            "get",
            "/workspace/{workspace}/storefront/design-capabilities",
            "storefront_design_capabilities",
        ),
        (
            "get",
            "/storefront/pages/home",
            "storefront_design_published",
        ),
    ] {
        assert_eq!(spec["paths"][path][method]["operationId"], id);
    }
}

async fn catalog_key(api: &Api) -> String {
    let (status, body) = api
        .with_base("/app-keys")
        .call(
            Method::POST,
            "",
            Some(json!({"name":"Shop","kind":"shop","scopes":["catalog:read"]})),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["data"]["key"].as_str().unwrap().to_owned()
}

async fn public(api: &Api, key: &str) -> (StatusCode, Value) {
    common::send(
        &api.app,
        Method::GET,
        "/v1/storefront/pages/home",
        None,
        Some(key),
    )
    .await
}

async fn save(api: &Api) -> Value {
    let (status, body) = write(
        api,
        "/revisions",
        json!({"expected_version":0,"document":document()}),
        "initial-save",
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["data"].clone()
}

#[sqlx::test]
async fn draft_preview_publish_and_restore_keep_public_separate(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let (status, initial) = api.call(Method::GET, "", None, &api.owner).await;
    assert_eq!(status, StatusCode::OK, "{initial}");
    assert_eq!(initial["data"]["version"], 0);
    assert_eq!(initial["data"]["draft_revision_id"], Value::Null);
    let doc = document();
    let key = catalog_key(&api).await;
    assert_eq!(public(&api, &key).await.1, json!({"data":null}));
    let (status, saved) = write(
        &api,
        "/revisions",
        json!({"expected_version":0,"document":doc}),
        "save-one",
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    assert_eq!(saved["data"]["version"], 1);
    assert_eq!(saved["data"]["draft"]["document"], doc);
    let first = saved["data"]["draft_revision_id"].as_str().unwrap();
    assert_eq!(public(&api, &key).await.1, json!({"data":null}));
    let (status, preview) = api
        .call(
            Method::GET,
            &format!("/preview?revision_id={first}"),
            None,
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["data"]["version"], 1);
    assert_eq!(preview["data"]["document"], doc);
    assert_eq!(
        preview["data"]["collections"],
        json!([{"section_id":"featured-products","products":[]}])
    );
    let (status, published) = write(
        &api,
        "/publish",
        json!({"expected_version":1,"revision_id":first}),
        "publish-one",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{published}");
    assert_eq!(published["data"]["version"], 2);
    let visible = public(&api, &key).await;
    assert_eq!(visible.0, StatusCode::OK);
    assert_eq!(visible.1["data"]["revision_id"], first);
    assert_eq!(visible.1["data"]["document"], doc);
    let keys: Vec<_> = visible.1["data"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(keys, ["collections", "document", "revision_id"]);
    let mut changed = doc.clone();
    changed["sections"][0]["props"]["heading"] = json!("New harvest");
    let (status, edited) = write(
        &api,
        "/revisions",
        json!({"expected_version":2,"document":changed}),
        "save-two",
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{edited}");
    assert_ne!(edited["data"]["draft_revision_id"], first);
    assert_eq!(public(&api, &key).await.1["data"]["document"], doc);
    let (status, old_publish) = write(
        &api,
        "/publish",
        json!({"expected_version":3,"revision_id":first}),
        "old-publish",
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{old_publish}");
    let (status, restored) = write(
        &api,
        "/restore",
        json!({"expected_version":3,"revision_id":first}),
        "restore-one",
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{restored}");
    assert_eq!(restored["data"]["version"], 4);
    assert_eq!(restored["data"]["draft"]["document"], doc);
    let restored_id = restored["data"]["draft_revision_id"].as_str().unwrap();
    assert_ne!(restored_id, first);
    assert_eq!(restored["data"]["published_revision_id"], first);
    let (status, again) = write(
        &api,
        "/publish",
        json!({"expected_version":4,"revision_id":restored_id}),
        "publish-restored",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(
        public(&api, &key).await.1["data"]["revision_id"],
        restored_id
    );
    let (status, history) = api
        .call(Method::GET, "/revisions?page_size=500", None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{history}");
    assert_eq!(history["meta"]["per_page"], 50);
    assert_eq!(history["data"].as_array().unwrap().len(), 3);
    assert_eq!(history["data"][0]["was_published"], true);
    assert_eq!(history["data"][0]["restored_from_revision_id"], first);
}

#[sqlx::test]
async fn admin_only_and_foreign_revisions_are_hidden(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let state = save(&api).await;
    let revision = state["draft_revision_id"].as_str().unwrap();
    for path in [
        "/preview",
        "/preview?revision_id=bad",
        "/preview?revision_id=",
    ] {
        let (status, body) = api.call(Method::GET, path, None, &api.owner).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert!(body["errors"]["revision_id"].is_array());
    }
    let member = common::member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "member@example.com",
        &["setting.get", "setting.update"],
    )
    .await;
    let admin = common::member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "admin@example.com",
        &["workspace_admin"],
    )
    .await;
    assert_eq!(
        api.call(Method::GET, "", None, &member).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        api.call(Method::GET, "", None, &admin).await.0,
        StatusCode::OK
    );
    let body = json!({"expected_version":1,"revision_id":revision});
    for path in ["/publish", "/restore"] {
        assert_eq!(
            write_as(&api, path, body.clone(), path, &member).await.0,
            StatusCode::FORBIDDEN
        );
    }
    let other = api.as_new_owner("other@example.com", "other").await;
    assert_eq!(
        api.call(Method::GET, "", None, &other.owner).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        other
            .call(
                Method::GET,
                &format!("/preview?revision_id={revision}"),
                None,
                &other.owner
            )
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        write(
            &other,
            "/restore",
            json!({"expected_version":0,"revision_id":revision}),
            "foreign"
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let capabilities = api.with_base("/storefront/design-capabilities");
    let (status, registry) = capabilities.call(Method::GET, "", None, &admin).await;
    assert_eq!(status, StatusCode::OK, "{registry}");
    assert!(registry["data"]["document_schema"]["properties"]["sections"].is_object());
    assert_eq!(registry["data"]["components"].as_array().unwrap().len(), 2);
    assert_eq!(
        capabilities.call(Method::GET, "", None, &member).await.0,
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE workspace_members SET active=false WHERE workspace_id=$1::uuid AND user_id=(SELECT id FROM users WHERE email='admin@example.com')").bind(&api.ws).execute(&api.pool).await.unwrap();
    assert_eq!(
        api.call(Method::GET, "", None, &admin).await.0,
        StatusCode::FORBIDDEN
    );
}

#[sqlx::test]
async fn optimistic_concurrency_and_keyed_retries_are_atomic(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let body = json!({"expected_version":0,"document":document()});
    let (left, right) = tokio::join!(
        write(&api, "/revisions", body.clone(), "one"),
        write(&api, "/revisions", body.clone(), "two")
    );
    assert!(
        (left.0 == StatusCode::CREATED && right.0 == StatusCode::CONFLICT)
            || (right.0 == StatusCode::CREATED && left.0 == StatusCode::CONFLICT),
        "{left:?} {right:?}"
    );
    let (key, success) = if left.0 == StatusCode::CREATED {
        ("one", left)
    } else {
        ("two", right)
    };
    assert_eq!(write(&api, "/revisions", body.clone(), key).await, success);
    let mut different = body;
    different["document"]["theme"] = json!("neutral");
    assert_eq!(
        write(&api, "/revisions", different, key).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let no_key = api
        .call(
            Method::POST,
            "/revisions",
            Some(json!({"expected_version":1,"document":document()})),
            &api.owner,
        )
        .await;
    assert_eq!(no_key.0, StatusCode::UNPROCESSABLE_ENTITY);
    let id = success.1["data"]["draft_revision_id"].clone();
    let publish = json!({"expected_version":1,"revision_id":id});
    let (a, b) = tokio::join!(
        write(&api, "/publish", publish.clone(), "publish"),
        write(&api, "/publish", publish, "publish")
    );
    assert_eq!(a.0, StatusCode::OK, "{a:?}");
    assert_eq!(a, b);
    let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM storefront_design_revisions WHERE workspace_id=$1::uuid),(SELECT count(*) FROM storefront_design_events WHERE workspace_id=$1::uuid)").bind(&api.ws).fetch_one(&api.pool).await.unwrap();
    assert_eq!(counts, (1, 2));
}

#[sqlx::test]
async fn invalid_documents_and_foreign_categories_never_create_revisions(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let mut bad = document();
    bad["sections"][0]["props"]["html"] = json!("<script>x</script>");
    let mut duplicate = document();
    duplicate["sections"][1]["id"] = duplicate["sections"][0]["id"].clone();
    let mut bounds = document();
    bounds["sections"][1]["props"]["limit"] = json!(13);
    let mut version = document();
    version["schema_version"] = json!(2);
    let mut many = document();
    many["sections"] = json!(vec![document()["sections"][0].clone(); 13]);
    for (i, doc) in [bad, duplicate, bounds, version, many]
        .into_iter()
        .enumerate()
    {
        let (status, res) = write(
            &api,
            "/revisions",
            json!({"expected_version":0,"document":doc}),
            &format!("invalid-{i}"),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    }
    let other = api.as_new_owner("foreign@example.com", "foreign").await;
    let category = other
        .with_base("/erp/product-categories")
        .create(json!({"name":"Private"}), "product_category_id")
        .await;
    let mut foreign = document();
    foreign["sections"][1]["props"]["category_id"] = json!(category);
    assert_eq!(
        write(
            &api,
            "/revisions",
            json!({"expected_version":0,"document":foreign}),
            "foreign-category"
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        api.call(Method::GET, "", None, &api.owner).await.1["data"]["version"],
        0
    );
}

#[sqlx::test]
async fn revisions_events_and_actor_snapshots_survive_token_cleanup(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    save(&api).await;
    for query in [
        "TRUNCATE storefront_design_revisions CASCADE",
        "TRUNCATE storefront_design_events CASCADE",
    ] {
        assert!(
            sqlx::raw_sql(query).execute(&api.pool).await.is_err(),
            "{query}"
        );
    }
    for query in [
        "UPDATE storefront_design_revisions SET document='{}' WHERE workspace_id=$1::uuid",
        "DELETE FROM storefront_design_revisions WHERE workspace_id=$1::uuid",
        "UPDATE storefront_design_events SET kind='published' WHERE workspace_id=$1::uuid",
        "DELETE FROM storefront_design_events WHERE workspace_id=$1::uuid",
    ] {
        assert!(
            sqlx::query(query)
                .bind(&api.ws)
                .execute(&api.pool)
                .await
                .is_err(),
            "{query}"
        );
    }
    sqlx::query("DELETE FROM access_tokens WHERE user_id=(SELECT id FROM users WHERE email='ada@example.com')").execute(&api.pool).await.unwrap();
    let actor:(String,String)=sqlx::query_as("SELECT created_by::text,login_id::text FROM storefront_design_revisions WHERE workspace_id=$1::uuid").bind(&api.ws).fetch_one(&api.pool).await.unwrap();
    assert!(!actor.0.is_empty() && !actor.1.is_empty());
    sqlx::query("DELETE FROM workspaces WHERE id=$1::uuid")
        .bind(&api.ws)
        .execute(&api.pool)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM storefront_design_revisions WHERE workspace_id=$1::uuid",
    )
    .bind(&api.ws)
    .fetch_one(&api.pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test]
async fn preview_and_published_collections_match_existing_catalog(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let products = api.with_base("/products");
    products
        .create(json!({"name":"Honey","price":10}), "product_id")
        .await;
    let hidden = products
        .create(json!({"name":"Hidden","price":2}), "product_id")
        .await;
    sqlx::query("UPDATE products SET store_only=true WHERE id=$1::uuid")
        .bind(hidden)
        .execute(&api.pool)
        .await
        .unwrap();
    let key = catalog_key(&api).await;
    let (_, catalog) = common::send(
        &api.app,
        Method::GET,
        "/v1/storefront/products?page_size=6",
        None,
        Some(&key),
    )
    .await;
    let state = save(&api).await;
    let revision = state["draft_revision_id"].as_str().unwrap();
    let (status, preview) = api
        .call(
            Method::GET,
            &format!("/preview?revision_id={revision}"),
            None,
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(
        preview["data"]["collections"][0]["products"],
        catalog["data"]
    );
    assert_eq!(
        write(
            &api,
            "/publish",
            json!({"expected_version":1,"revision_id":revision}),
            "publish"
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        public(&api, &key).await.1["data"]["collections"][0]["products"],
        catalog["data"]
    );
    assert_eq!(public(&api, "unknown").await.0, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn failed_publish_rolls_back_pointer_version_and_event(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let state = save(&api).await;
    sqlx::raw_sql("CREATE FUNCTION reject_design_publish() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.kind='published' THEN RAISE EXCEPTION 'test event failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_design_publish BEFORE INSERT ON storefront_design_events FOR EACH ROW EXECUTE FUNCTION reject_design_publish();").execute(&api.pool).await.unwrap();
    let (status, _) = write(
        &api,
        "/publish",
        json!({"expected_version":1,"revision_id":state["draft_revision_id"]}),
        "publish",
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        api.call(Method::GET, "", None, &api.owner).await.1["data"],
        state
    );
    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM storefront_design_events WHERE workspace_id=$1::uuid",
    )
    .bind(&api.ws)
    .fetch_one(&api.pool)
    .await
    .unwrap();
    assert_eq!(events, 1);
}

/// Raw revision insert for current home page; bypasses Rust validation like legacy data would.
async fn raw_revision(api: &Api, document: &str) -> Result<String, sqlx::Error> {
    sqlx::query_scalar("INSERT INTO storefront_design_revisions(workspace_id,page_id,schema_version,document,created_by,login_id) SELECT workspace_id,id,1,$2::text::jsonb,uuidv7(),uuidv7() FROM storefront_pages WHERE workspace_id=$1::uuid RETURNING id::text")
        .bind(&api.ws)
        .bind(document)
        .fetch_one(&api.pool)
        .await
}

#[sqlx::test]
async fn published_design_that_no_longer_validates_is_hidden(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    save(&api).await;
    let mut stale = document();
    stale["schema_version"] = json!(2);
    let id = raw_revision(&api, &stale.to_string()).await.unwrap();
    sqlx::query(
        "UPDATE storefront_pages SET published_revision_id=$2::uuid WHERE workspace_id=$1::uuid",
    )
    .bind(&api.ws)
    .bind(&id)
    .execute(&api.pool)
    .await
    .unwrap();
    let key = catalog_key(&api).await;
    assert_eq!(
        public(&api, &key).await,
        (StatusCode::OK, json!({"data":null}))
    );
}

#[sqlx::test]
async fn document_size_check_is_a_64_kib_backstop(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    save(&api).await;
    let doc = |n: usize| json!({ "pad": "x".repeat(n) }).to_string();
    assert!(raw_revision(&api, &doc(40_000)).await.is_ok());
    assert!(raw_revision(&api, &doc(70_000)).await.is_err());
}

#[sqlx::test]
async fn invalid_document_reports_field_paths(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let mut duplicate = document();
    duplicate["sections"][1]["id"] = duplicate["sections"][0]["id"].clone();
    let (status, body) = write(
        &api,
        "/revisions",
        json!({"expected_version":0,"document":duplicate}),
        "duplicate",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["errors"]["document.sections.1.id"].is_array(),
        "{body}"
    );
    assert!(body["errors"]["document"].is_null(), "{body}");
}

#[sqlx::test]
async fn public_route_requires_catalog_read_scope(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let (status, body) = api
        .with_base("/app-keys")
        .call(
            Method::POST,
            "",
            Some(json!({"name":"Checkout","kind":"shop","scopes":["checkout:write"]})),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let key = body["data"]["key"].as_str().unwrap();
    assert_eq!(public(&api, key).await.0, StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn publish_race_with_different_keys_publishes_once(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let (status, empty) = api.call(Method::GET, "/revisions", None, &api.owner).await;
    assert_eq!(status, StatusCode::OK, "{empty}");
    assert_eq!(empty["data"], json!([]));
    assert_eq!(empty["meta"]["total"], 0);
    let state = save(&api).await;
    let id = state["draft_revision_id"].clone();
    let body = json!({"expected_version":1,"revision_id":id});
    let (a, b) = tokio::join!(
        write(&api, "/publish", body.clone(), "pub-a"),
        write(&api, "/publish", body, "pub-b")
    );
    let mut statuses = [a.0, b.0];
    statuses.sort();
    assert_eq!(
        statuses,
        [StatusCode::OK, StatusCode::CONFLICT],
        "{a:?} {b:?}"
    );
    let (published, version, events): (Option<String>, i64, i64) = sqlx::query_as("SELECT p.published_revision_id::text,p.version,(SELECT count(*) FROM storefront_design_events e WHERE e.workspace_id=p.workspace_id AND e.kind='published') FROM storefront_pages p WHERE p.workspace_id=$1::uuid")
        .bind(&api.ws)
        .fetch_one(&api.pool)
        .await
        .unwrap();
    assert_eq!(published.as_deref(), id.as_str());
    assert_eq!((version, events), (2, 1));
}
