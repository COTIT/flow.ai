//! Generic master-data imports: bounded chunks, exact matching, only valid rows apply.
use crate::common::{self, member_with, send_raw, signup};
use axum::{
    Router,
    http::{Method, StatusCode},
};
use genesy_360_api::{
    AppState,
    jobs::{self, kinds::Ctx, runner::Step},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

struct World {
    app: Router,
    state: AppState,
    token: String,
    ws: String,
}

async fn world(db: PgPool) -> World {
    let state = common::state(db, genesy_360_api::mail::Mailer::capture().0);
    let app = genesy_360_api::build_app(state.clone());
    let (token, ws) = signup(&app, "imports@example.test", "imports").await;
    World {
        app,
        state,
        token,
        ws,
    }
}

impl World {
    fn uri(&self, tail: &str) -> String {
        format!("/v1/workspace/{}/imports{tail}", self.ws)
    }
    async fn call(
        &self,
        method: Method,
        tail: &str,
        body: Option<Value>,
        token: &str,
    ) -> (StatusCode, Value) {
        let key = Uuid::now_v7().to_string();
        self.keyed(method, tail, body, token, &key).await
    }
    async fn keyed(
        &self,
        method: Method,
        tail: &str,
        body: Option<Value>,
        token: &str,
        key: &str,
    ) -> (StatusCode, Value) {
        let (status, bytes) = send_raw(
            &self.app,
            method,
            &self.uri(tail),
            body,
            Some(token),
            &[("idempotency-key", key)],
        )
        .await;
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
    async fn create(&self, profile: &str, rows: i64, mapping: Value) -> Value {
        let (status, body) = self
            .call(
                Method::POST,
                "",
                Some(create_body(profile, rows, mapping)),
                &self.token,
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body["data"].clone()
    }
    async fn upload(&self, batch: &Value, index: usize, rows: Value) -> (StatusCode, Value) {
        let body = json!({"client_ref": Uuid::now_v7(), "expected_version": batch["version"], "payload": {"rows": rows}});
        self.call(
            Method::PUT,
            &format!("/{}/chunks/{index}", batch["id"].as_str().unwrap()),
            Some(body),
            &self.token,
        )
        .await
    }
    async fn get(&self, batch: &Value) -> Value {
        let (status, body) = self
            .call(
                Method::GET,
                &format!("/{}", batch["id"].as_str().unwrap()),
                None,
                &self.token,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["data"].clone()
    }
    async fn validate(&self, batch: &Value, chunks: i64) -> (StatusCode, Value) {
        let body = json!({"client_ref": Uuid::now_v7(), "expected_version": batch["version"], "payload": {"expected_chunks": chunks}});
        self.call(
            Method::POST,
            &format!("/{}/validate", batch["id"].as_str().unwrap()),
            Some(body),
            &self.token,
        )
        .await
    }
    async fn apply_as(&self, batch: &Value, token: &str) -> (StatusCode, Value) {
        let body = json!({"client_ref": Uuid::now_v7(), "expected_version": batch["version"], "payload": {"validation_fingerprint": batch["validation_fingerprint"]}});
        self.call(
            Method::POST,
            &format!("/{}/apply", batch["id"].as_str().unwrap()),
            Some(body),
            token,
        )
        .await
    }
    async fn rows(&self, batch: &Value, query: &str) -> Value {
        let (status, body) = self
            .call(
                Method::GET,
                &format!("/{}/rows{query}", batch["id"].as_str().unwrap()),
                None,
                &self.token,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }
    /// Upload all rows in chunks of 20 and validate; returns the validated batch.
    async fn staged(&self, profile: &str, mapping: Value, rows: Vec<Value>) -> Value {
        let batch = self.create(profile, rows.len() as i64, mapping).await;
        let chunks: Vec<_> = rows.chunks(20).collect();
        for (i, chunk) in chunks.iter().enumerate() {
            let (status, body) = self.upload(&batch, i, json!(chunk)).await;
            assert_eq!(status, StatusCode::OK, "{body}");
        }
        let (status, body) = self
            .validate(&self.get(&batch).await, chunks.len() as i64)
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["data"].clone()
    }
    async fn drain(&self) {
        let ctx = Ctx {
            invoice_state: Some(self.state.clone()),
            assistant_api_key: None,
            db: self.state.db.clone(),
            files: self.state.files.clone(),
            http: genesy_360_api::outbound::Outbound::new(&[]).unwrap(),
            credentials: self.state.credentials.clone(),
            payments: self.state.payments.clone(),
        };
        for _ in 0..100 {
            let Some(job) =
                jobs::repo::claim(&self.state.db, &[jobs::kinds::IMPORT_APPLY], 2, jobs::LEASE)
                    .await
                    .unwrap()
            else {
                return;
            };
            match jobs::kinds::dispatch(&ctx, job.clone()).await.unwrap() {
                Step::Done => jobs::repo::finish(&self.state.db, job.id, job.token)
                    .await
                    .unwrap(),
                Step::More => jobs::repo::requeue(&self.state.db, job.id, job.token)
                    .await
                    .unwrap(),
            };
        }
        panic!("import apply did not finish");
    }
    async fn count(&self, sql: &'static str) -> i64 {
        sqlx::query_scalar(sql)
            .bind(Uuid::parse_str(&self.ws).unwrap())
            .fetch_one(&self.state.db)
            .await
            .unwrap()
    }
}

fn create_body(profile: &str, rows: i64, mapping: Value) -> Value {
    json!({"client_ref": Uuid::now_v7(), "payload": {
        "profile": profile, "filename": "list.csv", "file_hash": "a".repeat(64),
        "expected_rows": rows, "mapping": mapping}})
}

fn product(n: usize, name: &str) -> Value {
    json!({"row_number": n, "fields": {"name": name, "price": "2.50"}})
}

fn create_products() -> Value {
    json!({"operation": "create", "match_key": "barcode"})
}

fn relation(n: usize, company: &str) -> Value {
    json!({"row_number": n, "fields": {"type": "business", "company_name": company,
        "email": format!("r{n}@example.test"), "address": "Main", "house_number": "1",
        "zipcode": "1000AA", "city": "Almere", "country": "nl"}})
}

#[sqlx::test]
async fn products_import_creates_only_valid_rows(db: PgPool) {
    let w = world(db).await;
    let batch = w
        .staged(
            "products",
            create_products(),
            vec![
                product(1, "Maize flour"),
                product(2, ""),
                product(3, "Rice"),
                // Natively valid; invalid only by the import's duplicate-key rule.
                json!({"row_number": 4, "fields": {"name": "Dup A", "price": "1", "barcode": "dup"}}),
                json!({"row_number": 5, "fields": {"name": "Dup B", "price": "1", "barcode": "dup"}}),
            ],
        )
        .await;
    assert_eq!(batch["state"], "validated");
    assert_eq!(batch["counts"]["valid"], 2, "{batch}");
    assert_eq!(batch["counts"]["invalid"], 3, "{batch}");
    let invalid = w.rows(&batch, "?status=invalid").await;
    assert_eq!(invalid["data"][0]["row_number"], 2);
    assert!(
        invalid["data"][0]["issues"][0]["message"]
            .as_str()
            .unwrap()
            .contains("name"),
        "{invalid}"
    );

    let (status, body) = w.apply_as(&batch, &w.token).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["data"]["state"], "applying");
    w.drain().await;

    let done = w.get(&batch).await;
    assert_eq!(done["state"], "completed", "{done}");
    assert_eq!(done["counts"]["applied"], 2, "{done}");
    assert_eq!(done["counts"]["invalid"], 3, "{done}");
    assert_eq!(
        w.count("SELECT count(*) FROM products WHERE workspace_id=$1 AND deleted_at IS NULL")
            .await,
        2
    );
    let applied = w.rows(&batch, "?status=applied").await;
    assert!(applied["data"][0]["result_id"].is_string(), "{applied}");
    assert_eq!(applied["meta"]["total"], 2);

    // A second drain (restart) applies nothing twice.
    sqlx::query("UPDATE import_batches SET state='applying' WHERE id=$1::uuid")
        .bind(batch["id"].as_str().unwrap())
        .execute(&w.state.db)
        .await
        .unwrap();
    genesy_360_api::imports::service::enqueue_apply(
        &w.state.db,
        Uuid::parse_str(&w.ws).unwrap(),
        Uuid::parse_str(batch["id"].as_str().unwrap()).unwrap(),
    )
    .await
    .unwrap();
    w.drain().await;
    assert_eq!(
        w.count("SELECT count(*) FROM products WHERE workspace_id=$1 AND deleted_at IS NULL")
            .await,
        2
    );
}

#[sqlx::test]
async fn products_update_matches_exact_key_only(db: PgPool) {
    let w = world(db).await;
    let seed = w
        .staged(
            "products",
            create_products(),
            vec![json!({"row_number": 1, "fields": {"name": "Beans", "price": "1", "barcode": "abc"}})],
        )
        .await;
    w.apply_as(&seed, &w.token).await;
    w.drain().await;

    let batch = w
        .staged(
            "products",
            json!({"operation": "update", "match_key": "barcode"}),
            vec![
                json!({"row_number": 1, "external_key": "abc", "fields": {"price": "9"}}),
                json!({"row_number": 2, "external_key": "ABC", "fields": {"price": "8"}}),
                json!({"row_number": 3, "external_key": "abc ", "fields": {"price": "7"}}),
                json!({"row_number": 4, "fields": {"price": "6"}}),
            ],
        )
        .await;
    assert_eq!(batch["counts"]["valid"], 1, "{batch}");
    assert_eq!(batch["counts"]["invalid"], 3, "{batch}");
    w.apply_as(&batch, &w.token).await;
    w.drain().await;
    let price: String = sqlx::query_scalar(
        "SELECT price::text FROM products WHERE workspace_id=$1::uuid AND barcode='abc'",
    )
    .bind(&w.ws)
    .fetch_one(&w.state.db)
    .await
    .unwrap();
    assert!(price.starts_with('9'), "{price}");
}

#[sqlx::test]
async fn duplicate_keys_in_one_batch_are_invalid(db: PgPool) {
    let w = world(db).await;
    let batch = w
        .staged(
            "products",
            create_products(),
            vec![
                json!({"row_number": 1, "fields": {"name": "A", "price": "1", "barcode": "dup"}}),
                json!({"row_number": 2, "fields": {"name": "B", "price": "1", "barcode": "dup"}}),
            ],
        )
        .await;
    assert_eq!(batch["counts"]["invalid"], 2, "{batch}");
    let (status, body) = w.apply_as(&batch, &w.token).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
}

#[sqlx::test]
async fn relations_import_creates_relations(db: PgPool) {
    let w = world(db).await;
    let mut bad = relation(2, "Bad Ltd");
    bad["fields"]["email"] = json!("not-an-email");
    let batch = w
        .staged("relations", json!({}), vec![relation(1, "Good Ltd"), bad])
        .await;
    assert_eq!(batch["counts"]["valid"], 1, "{batch}");
    assert_eq!(batch["counts"]["invalid"], 1, "{batch}");
    let (status, body) = w.apply_as(&batch, &w.token).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    w.drain().await;
    assert_eq!(w.get(&batch).await["state"], "completed");
    assert_eq!(
        w.count("SELECT count(*) FROM relations WHERE workspace_id=$1 AND deleted_at IS NULL")
            .await,
        1
    );
}

#[sqlx::test]
async fn chunk_limits_and_replay(db: PgPool) {
    let w = world(db).await;
    let batch = w.create("products", 40, create_products()).await;
    // 21 rows in one chunk.
    let many: Vec<Value> = (1..=21).map(|n| product(n, "x")).collect();
    assert_eq!(
        w.upload(&batch, 0, json!(many)).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    // Index beyond 24.
    assert_eq!(
        w.upload(&batch, 25, json!([product(1, "x")])).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    // Cell above 255 characters.
    assert_eq!(
        w.upload(&batch, 0, json!([product(1, &"x".repeat(256))]))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    // Unknown field for the profile.
    assert_eq!(
        w.upload(
            &batch,
            0,
            json!([{"row_number": 1, "fields": {"colour": "red"}}])
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    // Row number outside expected_rows.
    assert_eq!(
        w.upload(&batch, 0, json!([product(41, "x")])).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    // Encoded rows above 60 KiB.
    let fat: Vec<Value> = (1..=20)
        .map(|n| {
            let mut fields = serde_json::Map::new();
            for f in [
                "name",
                "price",
                "unit",
                "barcode",
                "ean13",
                "reference",
                "description",
            ] {
                fields.insert(f.into(), json!("é".repeat(255)));
            }
            json!({"row_number": n, "external_key": "é".repeat(255), "fields": fields})
        })
        .collect();
    let fat_status = w.upload(&batch, 0, json!(fat)).await.0;
    assert!(
        fat_status == StatusCode::PAYLOAD_TOO_LARGE,
        "fat chunk: {fat_status}"
    );
    // Same content again = replay; different content for the same index = 409.
    let rows = json!([product(1, "Rice")]);
    assert_eq!(w.upload(&batch, 0, rows.clone()).await.0, StatusCode::OK);
    assert_eq!(w.upload(&batch, 0, rows).await.0, StatusCode::OK);
    assert_eq!(
        w.upload(&batch, 0, json!([product(1, "Beans")])).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        w.count("SELECT count(*) FROM import_rows WHERE workspace_id=$1")
            .await,
        1
    );
    // Incomplete revision cannot be validated.
    let (status, body) = w.validate(&w.get(&batch).await, 2).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    // Create limits.
    let (status, _) = w
        .call(
            Method::POST,
            "",
            Some(create_body("products", 501, create_products())),
            &w.token,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = w
        .call(
            Method::POST,
            "",
            Some(create_body("opening_inventory", 1, json!({}))),
            &w.token,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn create_requires_idempotency_key_and_replays_by_client_ref(db: PgPool) {
    let w = world(db).await;
    let (status, _) = send_raw(
        &w.app,
        Method::POST,
        &w.uri(""),
        Some(create_body("products", 1, create_products())),
        Some(&w.token),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let body = create_body("products", 1, create_products());
    let (a, first) = w
        .keyed(Method::POST, "", Some(body.clone()), &w.token, "k-1")
        .await;
    let (b, again) = w.keyed(Method::POST, "", Some(body), &w.token, "k-2").await;
    assert_eq!((a, b), (StatusCode::CREATED, StatusCode::CREATED));
    assert_eq!(first["data"]["id"], again["data"]["id"]);
    assert_eq!(
        w.count("SELECT count(*) FROM import_batches WHERE workspace_id=$1")
            .await,
        1
    );
}

#[sqlx::test]
async fn stale_fingerprint_and_version_are_refused(db: PgPool) {
    let w = world(db).await;
    let mut batch = w
        .staged("products", create_products(), vec![product(1, "Rice")])
        .await;
    let real = batch["validation_fingerprint"].clone();
    batch["validation_fingerprint"] = json!("0".repeat(64));
    assert_eq!(w.apply_as(&batch, &w.token).await.0, StatusCode::CONFLICT);
    batch["validation_fingerprint"] = real;
    batch["version"] = json!(1);
    assert_eq!(w.apply_as(&batch, &w.token).await.0, StatusCode::CONFLICT);
    assert_eq!(
        w.count("SELECT count(*) FROM products WHERE workspace_id=$1")
            .await,
        0
    );
}

#[sqlx::test]
async fn permissions_are_import_plus_native(db: PgPool) {
    let w = world(db).await;
    let ws = w.ws.clone();
    let outsider = member_with(
        &w.app,
        &w.state.db,
        &ws,
        "none@example.test",
        &["product.list"],
    )
    .await;
    let (status, _) = w
        .call(
            Method::POST,
            "",
            Some(create_body("products", 1, create_products())),
            &outsider,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        w.call(Method::GET, "", None, &outsider).await.0,
        StatusCode::FORBIDDEN
    );

    let importer = member_with(
        &w.app,
        &w.state.db,
        &ws,
        "importer@example.test",
        &[
            "import.list",
            "import.get",
            "import.create",
            "import.upload",
            "import.validate",
            "import.apply",
        ],
    )
    .await;
    let batch = w
        .staged("products", create_products(), vec![product(1, "Rice")])
        .await;
    // import.apply without product.create.
    assert_eq!(w.apply_as(&batch, &importer).await.0, StatusCode::FORBIDDEN);

    // Grant lost between apply and job: nothing applied, batch failed.
    let (status, _) = w.apply_as(&batch, &w.token).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    sqlx::query("UPDATE import_batches SET applied_by=(SELECT id FROM users WHERE email='importer@example.test') WHERE id=$1::uuid")
        .bind(batch["id"].as_str().unwrap())
        .execute(&w.state.db)
        .await
        .unwrap();
    w.drain().await;
    assert_eq!(w.get(&batch).await["state"], "failed");
    assert_eq!(
        w.count("SELECT count(*) FROM products WHERE workspace_id=$1")
            .await,
        0
    );
}

#[sqlx::test]
async fn other_workspace_is_404(db: PgPool) {
    let w = world(db).await;
    let batch = w.create("products", 1, create_products()).await;
    let (stranger, _) = signup(&w.app, "other@example.test", "other").await;
    let (status, _) = w
        .call(
            Method::GET,
            &format!("/{}", batch["id"].as_str().unwrap()),
            None,
            &stranger,
        )
        .await;
    assert!(
        matches!(status, StatusCode::NOT_FOUND | StatusCode::FORBIDDEN),
        "{status}"
    );
    // Stranger's own workspace does not see it either.
    let (other_ws,): (Uuid,) = sqlx::query_as(
        "SELECT wm.workspace_id FROM workspace_members wm JOIN users u ON u.id=wm.user_id WHERE u.email='other@example.test'",
    )
    .fetch_one(&w.state.db)
    .await
    .unwrap();
    let (status, _) = common::send(
        &w.app,
        Method::GET,
        &format!(
            "/v1/workspace/{other_ws}/imports/{}",
            batch["id"].as_str().unwrap()
        ),
        None,
        Some(&stranger),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn validating_500_rows_is_synchronous_and_timed(db: PgPool) {
    let w = world(db).await;
    let rows: Vec<Value> = (1..=500)
        .map(|n| product(n, &format!("Item {n}")))
        .collect();
    let batch = w.create("products", 500, create_products()).await;
    for (i, chunk) in rows.chunks(20).enumerate() {
        assert_eq!(w.upload(&batch, i, json!(chunk)).await.0, StatusCode::OK);
    }
    let started = std::time::Instant::now();
    let (status, body) = w.validate(&w.get(&batch).await, 25).await;
    let took = started.elapsed();
    eprintln!("IMPORT_VALIDATE_500_MS={}", took.as_millis());
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["counts"]["valid"], 500);
    assert!(took.as_secs() < 10, "validate 500 rows took {took:?}");
}
