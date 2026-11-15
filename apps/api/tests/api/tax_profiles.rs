//! /v1/workspace/{ws}/tax-profiles: draft, edit, review, verify, correct; permanent replay.

use crate::common;

use axum::{
    Router,
    http::{Method, StatusCode},
};
use common::{app, member_with, send_full, signup};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

struct World {
    app: Router,
    pool: PgPool,
    owner: String,
    ws: String,
    category: String,
    relation: String,
}

async fn world(pool: PgPool) -> World {
    let app = app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let category: String = sqlx::query_scalar(
        "INSERT INTO product_categories (workspace_id, name) VALUES ($1::uuid, 'Seeds')
         RETURNING id::text",
    )
    .bind(&ws)
    .fetch_one(&pool)
    .await
    .unwrap();
    let relation: String = sqlx::query_scalar(
        "INSERT INTO relations (workspace_id, relation_type_id)
         SELECT $1::uuid, id FROM custom_types
         WHERE workspace_id = $1::uuid AND domain = 'relation' LIMIT 1
         RETURNING id::text",
    )
    .bind(&ws)
    .fetch_one(&pool)
    .await
    .unwrap();
    World {
        app,
        pool,
        owner,
        ws,
        category,
        relation,
    }
}

fn draft() -> Value {
    json!({
        "country": "TZ",
        "jurisdiction_scope": null,
        "effective_from": "2027-01-01",
        "transaction_types": ["sale", "purchase"],
        "product_categories": [],
        "components": [
            {"code": "VAT", "name": "VAT", "kind": "percentage", "rate": "18", "recoverable": true}
        ],
        "exemptions": [],
        "tax_point": "invoice",
        "evidence_ref": "TRA VAT Act 2014 s.5"
    })
}

impl World {
    async fn raw(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        token: &str,
        key: Option<&str>,
    ) -> (StatusCode, Value) {
        let uri = format!("/v1/workspace/{}/tax-profiles{path}", self.ws);
        let fresh = Uuid::now_v7().to_string();
        let headers: Vec<(&str, &str)> = match key {
            Some("") => vec![],
            Some(k) => vec![("idempotency-key", k)],
            None => vec![("idempotency-key", &fresh)],
        };
        let (s, _, b) = send_full(&self.app, method, &uri, body, Some(token), &headers).await;
        (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
    }

    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        self.raw(method, path, body, &self.owner, None).await
    }

    async fn create_with(&self, payload: Value) -> (StatusCode, Value) {
        self.call(
            Method::POST,
            "",
            Some(json!({"client_ref": Uuid::now_v7(), "payload": payload})),
        )
        .await
    }

    async fn create(&self, payload: Value) -> Value {
        let (s, b) = self.create_with(payload).await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        b["data"].clone()
    }

    async fn review(&self, id: &str) -> Value {
        let (s, b) = self
            .call(Method::POST, &format!("/{id}/verify/review"), None)
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        b["data"].clone()
    }

    async fn verify_with(&self, id: &str, version: i64, fp: &str) -> (StatusCode, Value) {
        self.call(
            Method::POST,
            &format!("/{id}/verify"),
            Some(
                json!({"client_ref": Uuid::now_v7(), "expected_version": version,
                        "payload": {"review_fingerprint": fp, "evidence_ref": "Reviewed by CPA"}}),
            ),
        )
        .await
    }

    async fn verify(&self, id: &str) -> Value {
        let r = self.review(id).await;
        let v = r["profile"]["version"].as_i64().unwrap();
        let (s, b) = self
            .verify_with(id, v, r["review_fingerprint"].as_str().unwrap())
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        b["data"].clone()
    }

    async fn patch(&self, id: &str, version: i64, payload: Value) -> (StatusCode, Value) {
        self.call(
            Method::PATCH,
            &format!("/{id}"),
            Some(
                json!({"client_ref": Uuid::now_v7(), "expected_version": version,
                        "payload": payload}),
            ),
        )
        .await
    }
}

fn errors(b: &Value) -> Vec<String> {
    let mut keys: Vec<String> = b["errors"]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

#[sqlx::test]
async fn create_get_and_list_a_draft(pool: PgPool) {
    let w = world(pool).await;
    let mut p = draft();
    p["product_categories"] = json!([w.category]);
    p["jurisdiction_scope"] = json!("TZ-02");
    p["components"] = json!([
        {"code": "VAT", "name": "VAT", "kind": "percentage", "rate": "18.00", "recoverable": true},
        {"code": "ZERO", "name": "Zero rated", "kind": "percentage", "rate": "0", "recoverable": false}
    ]);
    p["exemptions"] = json!([
        {"relation_id": w.relation, "reason": "Diplomatic mission", "evidence_ref": "EX-7"}
    ]);
    let d = w.create(p).await;
    assert_eq!(d["state"], "draft");
    assert_eq!(d["version"], 1);
    assert_eq!(d["revision"], 1);
    assert_eq!(d["lineage_id"], d["id"]);
    assert_eq!(d["jurisdiction_scope"], "TZ-02");
    assert_eq!(d["components"][0]["rate"], "18");
    assert_eq!(d["components"][1]["rate"], "0");
    assert_eq!(d["exemptions"][0]["relation_id"], w.relation);
    assert_eq!(d["product_categories"], json!([w.category]));
    assert_eq!(d["allowed_actions"], json!(["update", "verify"]));
    let id = d["id"].as_str().unwrap();
    let (s, got) = w.call(Method::GET, &format!("/{id}"), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(got["data"], d);
    let (s, list) = w.call(Method::GET, "?country=tz&state=draft", None).await;
    assert_eq!(s, StatusCode::OK, "{list}");
    assert_eq!(list["meta"]["total"], 1);
    assert_eq!(list["data"][0]["id"], d["id"]);
    let (_, none) = w.call(Method::GET, "?state=verified", None).await;
    assert_eq!(none["meta"]["total"], 0);
}

#[sqlx::test]
async fn create_replays_and_refuses_reused_refs(pool: PgPool) {
    let w = world(pool).await;
    let body = json!({"client_ref": Uuid::now_v7(), "payload": draft()});
    let (s1, b1) = w
        .raw(Method::POST, "", Some(body.clone()), &w.owner, Some("k1"))
        .await;
    let (s2, b2) = w
        .raw(Method::POST, "", Some(body.clone()), &w.owner, Some("k2"))
        .await;
    assert_eq!((s1, s2), (StatusCode::CREATED, StatusCode::CREATED));
    assert_eq!(b1, b2);
    let mut other = body.clone();
    other["payload"]["country"] = json!("KE");
    let (s, _) = w
        .raw(Method::POST, "", Some(other), &w.owner, Some("k3"))
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = w
        .raw(Method::POST, "", Some(body), &w.owner, Some(""))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM tax_profiles")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test]
async fn invalid_and_unsupported_input_is_refused(pool: PgPool) {
    let w = world(pool).await;
    let comp = |kind: &str, rate: &str| json!({"code": "VAT", "name": "VAT", "kind": kind, "rate": rate, "recoverable": false});
    let cases: Vec<(&str, Value, &str)> = vec![
        ("country", json!("XX1"), "payload.country"),
        (
            "jurisdiction_scope",
            json!("KE-01"),
            "payload.jurisdiction_scope",
        ),
        (
            "effective_from",
            json!("2027-13-01"),
            "payload.effective_from",
        ),
        ("effective_to", json!("2027-01-01"), "payload.effective_to"),
        ("transaction_types", json!([]), "payload.transaction_types"),
        (
            "transaction_types",
            json!(["refund"]),
            "payload.transaction_types.0",
        ),
        ("tax_point", json!("payment"), "payload.tax_point"),
        ("components", json!([]), "payload.components"),
        (
            "components",
            json!([comp("fixed", "1")]),
            "payload.components.0.kind",
        ),
        (
            "components",
            json!([comp("compound", "1")]),
            "payload.components.0.kind",
        ),
        (
            "components",
            json!([comp("percentage", "100.5")]),
            "payload.components.0.rate",
        ),
        (
            "components",
            json!([comp("percentage", "-1")]),
            "payload.components.0.rate",
        ),
        (
            "components",
            json!([comp("percentage", "1.123456789")]),
            "payload.components.0.rate",
        ),
        (
            "components",
            json!([comp("percentage", "1"), comp("percentage", "2")]),
            "payload.components.1.code",
        ),
        (
            "components",
            json!(vec![comp("percentage", "1"); 11]),
            "payload.components",
        ),
        (
            "product_categories",
            json!([Uuid::now_v7()]),
            "payload.product_categories.0",
        ),
        (
            "exemptions",
            json!([{"reason": "x", "evidence_ref": "y"}]),
            "payload.exemptions.0",
        ),
        (
            "exemptions",
            json!([{"relation_id": Uuid::now_v7(), "reason": "x", "evidence_ref": "y"}]),
            "payload.exemptions.0.relation_id",
        ),
        ("corrects_id", json!(Uuid::now_v7()), "payload.corrects_id"),
    ];
    for (field, value, key) in cases {
        let mut p = draft();
        p[field] = value;
        let (s, b) = w.create_with(p).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{field}: {b}");
        assert!(errors(&b).contains(&key.to_owned()), "{field}: {b}");
    }
    let (s, b) = w.create_with(json!({"tax_point": "invoice"})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    for key in [
        "payload.components",
        "payload.country",
        "payload.effective_from",
    ] {
        assert!(errors(&b).contains(&key.to_owned()), "{b}");
    }
}

#[sqlx::test]
async fn permissions_and_workspace_privacy(pool: PgPool) {
    let w = world(pool).await;
    let id = w.create(draft()).await["id"].as_str().unwrap().to_owned();
    let none = member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &["product.list"]).await;
    let reader = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "cy@example.com",
        &["taxprofile.read"],
    )
    .await;
    let config = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "di@example.com",
        &["taxprofile.read", "taxprofile.configure"],
    )
    .await;
    let create = json!({"client_ref": Uuid::now_v7(), "payload": draft()});
    let (s, _) = w.raw(Method::GET, "", None, &none, None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = w
        .raw(Method::GET, &format!("/{id}"), None, &reader, None)
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = w
        .raw(Method::POST, "", Some(create.clone()), &reader, None)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = w.raw(Method::POST, "", Some(create), &config, None).await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, _) = w
        .raw(
            Method::POST,
            &format!("/{id}/verify/review"),
            None,
            &config,
            None,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // Another workspace's owner sees nothing.
    let (eve, _) = signup(&w.app, "eve@example.com", "evil").await;
    let (s, _) = w
        .raw(Method::GET, &format!("/{id}"), None, &eve, None)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let other = World {
        ws: sqlx::query_scalar("SELECT id::text FROM workspaces WHERE name = 'evil'")
            .fetch_one(&w.pool)
            .await
            .unwrap(),
        owner: eve,
        app: w.app.clone(),
        pool: w.pool.clone(),
        category: String::new(),
        relation: String::new(),
    };
    let (s, _) = other.call(Method::GET, &format!("/{id}"), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = other.patch(&id, 1, json!({"country": "KE"})).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let mut p = draft();
    p["product_categories"] = json!([w.category]);
    let (s, b) = other.create_with(p).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
}

#[sqlx::test]
async fn drafts_update_by_version(pool: PgPool) {
    let w = world(pool).await;
    let id = w.create(draft()).await["id"].as_str().unwrap().to_owned();
    let (s, b) = w
        .patch(
            &id,
            1,
            json!({"effective_to": "2028-01-01",
                   "components": [{"code": "VAT", "name": "VAT", "kind": "percentage",
                                   "rate": "16", "recoverable": false}]}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["version"], 2);
    assert_eq!(b["data"]["effective_to"], "2028-01-01");
    assert_eq!(b["data"]["components"][0]["rate"], "16");
    assert_eq!(b["data"]["country"], "TZ");
    let (s, b) = w.patch(&id, 1, json!({"country": "KE"})).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(b["code"], "stale_document");
    let (s, b) = w.patch(&id, 2, json!({"effective_to": "2026-01-01"})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(errors(&b).contains(&"payload.effective_to".to_owned()));
}

#[sqlx::test]
async fn verify_needs_the_current_review(pool: PgPool) {
    let w = world(pool).await;
    let id = w.create(draft()).await["id"].as_str().unwrap().to_owned();
    let r = w.review(&id).await;
    assert_eq!(r, w.review(&id).await, "review is stable");
    let fp = r["review_fingerprint"].as_str().unwrap().to_owned();
    assert_eq!(fp.len(), 64);
    // Draft changes after the review: the old fingerprint is stale.
    let (s, _) = w.patch(&id, 1, json!({"tax_point": "supply"})).await;
    assert_eq!(s, StatusCode::OK);
    let (s, b) = w.verify_with(&id, 2, &fp).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(b["code"], "stale_document");
    let v = w.verify(&id).await;
    assert_eq!(v["state"], "verified");
    assert_eq!(v["version"], 3);
    assert_eq!(v["evidence_ref"], "Reviewed by CPA");
    assert!(v["verified_at"].is_string());
    assert_eq!(v["allowed_actions"], json!(["correct"]));
    // Verified is final.
    let (s, b) = w.patch(&id, 3, json!({"country": "KE"})).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(b["code"], "tax_profile_verified");
    let (s, b) = w.verify_with(&id, 3, &fp).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(b["code"], "tax_profile_verified");
    let (s, _) = w
        .call(Method::POST, &format!("/{id}/verify/review"), None)
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
}

#[sqlx::test]
async fn overlapping_verified_profiles_are_refused(pool: PgPool) {
    let w = world(pool).await;
    let a = w.create(draft()).await["id"].as_str().unwrap().to_owned();
    w.verify(&a).await;
    // Same country, overlapping dates and types.
    let mut p = draft();
    p["effective_from"] = json!("2030-01-01");
    p["transaction_types"] = json!(["sale"]);
    let b = w.create(p).await["id"].as_str().unwrap().to_owned();
    let r = w.review(&b).await;
    let (s, body) = w
        .verify_with(&b, 1, r["review_fingerprint"].as_str().unwrap())
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "tax_profile_overlap");
    // Other region, other category, or disjoint dates: fine.
    for change in [
        json!({"jurisdiction_scope": "TZ-01"}),
        json!({"country": "KE"}),
    ] {
        let mut p = draft();
        for (k, v) in change.as_object().unwrap() {
            p[k] = v.clone();
        }
        let id = w.create(p).await["id"].as_str().unwrap().to_owned();
        w.verify(&id).await;
    }
    let mut early = draft();
    early["effective_from"] = json!("2026-01-01");
    early["effective_to"] = json!("2027-01-01");
    let id = w.create(early).await["id"].as_str().unwrap().to_owned();
    w.verify(&id).await;
}

#[sqlx::test]
async fn category_scopes_overlap_only_when_they_share_a_category(pool: PgPool) {
    let w = world(pool).await;
    let other: String = sqlx::query_scalar(
        "INSERT INTO product_categories (workspace_id, name) VALUES ($1::uuid, 'Tools')
         RETURNING id::text",
    )
    .bind(&w.ws)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    let mut seeds = draft();
    seeds["product_categories"] = json!([w.category]);
    let a = w.create(seeds.clone()).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    w.verify(&a).await;
    let mut tools = draft();
    tools["product_categories"] = json!([other]);
    let b = w.create(tools).await["id"].as_str().unwrap().to_owned();
    w.verify(&b).await;
    // Empty = every category: overlaps both.
    let c = w.create(draft()).await["id"].as_str().unwrap().to_owned();
    let r = w.review(&c).await;
    let (s, body) = w
        .verify_with(&c, 1, r["review_fingerprint"].as_str().unwrap())
        .await;
    assert_eq!(body["code"], "tax_profile_overlap", "{s} {body}");
}

#[sqlx::test]
async fn concurrent_overlapping_verifies_let_one_win(pool: PgPool) {
    let w = world(pool).await;
    let a = w.create(draft()).await["id"].as_str().unwrap().to_owned();
    let b = w.create(draft()).await["id"].as_str().unwrap().to_owned();
    let (ra, rb) = (w.review(&a).await, w.review(&b).await);
    let (x, y) = tokio::join!(
        w.verify_with(&a, 1, ra["review_fingerprint"].as_str().unwrap()),
        w.verify_with(&b, 1, rb["review_fingerprint"].as_str().unwrap()),
    );
    let mut got = [x.0, y.0];
    got.sort();
    assert_eq!(got, [StatusCode::OK, StatusCode::UNPROCESSABLE_ENTITY]);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM tax_profiles WHERE state = 'verified'")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test]
async fn a_correction_is_a_new_revision_that_supersedes(pool: PgPool) {
    let w = world(pool).await;
    let first = w.create(draft()).await;
    let a = first["id"].as_str().unwrap().to_owned();
    // A draft can't be corrected; only verified.
    let mut p = draft();
    p["corrects_id"] = json!(a);
    let (s, b) = w.create_with(p.clone()).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    w.verify(&a).await;
    p["components"][0]["rate"] = json!("16");
    let fix = w.create(p.clone()).await;
    assert_eq!(fix["revision"], 2);
    assert_eq!(fix["lineage_id"], first["lineage_id"]);
    assert_eq!(fix["corrects_id"], a);
    // One correction per revision.
    let (s, _) = w.create_with(p).await;
    assert_eq!(s, StatusCode::CONFLICT);
    // Same dates as revision 1: its own lineage doesn't count as overlap.
    let b = fix["id"].as_str().unwrap();
    w.verify(b).await;
    let (_, old) = w.call(Method::GET, &format!("/{a}"), None).await;
    assert_eq!(old["data"]["superseded_by_id"], b);
    assert_eq!(old["data"]["components"][0]["rate"], "18");
    assert_eq!(old["data"]["allowed_actions"], json!([]));
}
