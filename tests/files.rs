//! /v1/workspace/{ws}/files (final batch F5a): uploads to the bucket (an in-memory store
//! here), metadata in `files`, downloads through short-lived signed URLs.

mod common;

use axum::http::{Method, StatusCode};
use common::{Api, member_with};
use genesy_360_api::{build_app, cleanup, mail::Mailer, storage::Store};
use serde_json::{Value, json};
use sqlx::PgPool;

const PDF_B64: &str = "JVBERi0xLjQKJcOkw7zDtsOfCg=="; // "%PDF-1.4\n%äüöß\n"

fn upload(name: &str, b64: &str) -> Value {
    json!({ "upload": b64, "document_name": name, "public": 1, "description": "d" })
}

async fn post(api: &Api, body: Value) -> (StatusCode, Value) {
    api.call(Method::POST, "", Some(body), &api.owner).await
}

async fn row(api: &Api, id: &str) -> (String, String, i64, Vec<u8>, String) {
    sqlx::query_as("SELECT key, status, size, sha256, content_type FROM files WHERE id = $1::uuid")
        .bind(id)
        .fetch_one(&api.pool)
        .await
        .unwrap()
}

/// An app whose memory store the test can read (tokens live in the database, so the owner
/// signed up through the default app works here too).
async fn with_store(pool: PgPool) -> (Api, Store) {
    let files = Store::memory();
    let mut state = common::state(pool.clone(), Mailer::capture().0);
    state.files = Some(files.clone());
    let mut api = Api::new(pool, "/files").await;
    api.app = build_app(state);
    (api, files)
}

// --- happy ---------------------------------------------------------------------------------

#[sqlx::test]
async fn a_base64_upload_is_stored_and_described(pool: PgPool) {
    let (api, files) = with_store(pool).await;
    let (status, res) = post(&api, upload("Invoice 1.pdf", PDF_B64)).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let id = res["data"]["file_id"].as_str().unwrap().to_owned();

    let (key, status, size, sha, content_type) = row(&api, &id).await;
    assert_eq!(key, format!("genesy/{}/{id}.pdf", api.ws));
    assert_eq!((status.as_str(), size), ("stored", 19));
    assert_eq!(content_type, "application/pdf");
    let (bytes, stored_type) = files.read(&key).unwrap();
    assert!(bytes.starts_with(b"%PDF-1.4"));
    assert_eq!(stored_type, "application/pdf");
    assert_eq!(sha.len(), 32);

    let file = api.get(&id).await;
    assert_eq!(file["id"], id);
    assert_eq!(file["document_name"], "Invoice 1.pdf");
    assert_eq!(file["extension"], "pdf");
    assert_eq!(file["size"], 19);
    assert_eq!(file["file_path"], format!("memory://{key}"));
    assert_eq!(file["association_data"], json!([]));
    assert_eq!(
        file["relationships"]["uploaded_by"]["email"],
        "ada@example.com"
    );
}

#[sqlx::test]
async fn list_and_delete(pool: PgPool) {
    let (api, files) = with_store(pool).await;
    let a = api.create(upload("a.png", "iVBORw0KGgo="), "file_id").await;
    let b = api.create(upload("b.pdf", PDF_B64), "file_id").await;
    let list = api.list("").await;
    let ids: Vec<&str> = list["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [b.as_str(), a.as_str()], "newest first");
    assert_eq!(list["meta"]["total"], 2);

    let key = row(&api, &a).await.0;
    let (status, res) = api
        .call(Method::DELETE, &format!("/{a}"), None, &api.owner)
        .await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    assert!(files.read(&key).is_none(), "object removed");
    assert_eq!(row(&api, &a).await.1, "deleted");
    let (status, _) = api
        .call(Method::GET, &format!("/{a}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(api.list("").await["meta"]["total"], 1);
}

// --- errors --------------------------------------------------------------------------------

#[sqlx::test]
async fn upload_rules(pool: PgPool) {
    let (api, _) = with_store(pool).await;
    let cases = [
        (
            json!({ "document_name": "a.pdf", "public": 1 }),
            "upload",
            "The upload field is required.",
        ),
        (
            json!({ "upload": "not base64!", "document_name": "a.pdf", "public": 1 }),
            "upload",
            "The upload must be a file or a base64 string.",
        ),
        (
            json!({ "upload": "{\"a\":1}", "document_name": "a.pdf", "public": 1 }),
            "upload",
            "The upload must be a file or a base64 string.",
        ),
        (
            json!({ "upload": PDF_B64, "public": 1 }),
            "document_name",
            "The document name field is required.",
        ),
        (
            json!({ "upload": PDF_B64, "document_name": "a.pdf" }),
            "public",
            "The public field is required.",
        ),
        (
            json!({ "upload": PDF_B64, "document_name": "a.pdf", "public": "maybe" }),
            "public",
            "The public field must be true or false.",
        ),
        (
            json!({ "upload": PDF_B64, "document_name": "a.pdf", "public": 1, "description": "x".repeat(256) }),
            "description",
            "The description must not be greater than 255 characters.",
        ),
        (
            json!({ "upload": PDF_B64, "document_name": "x".repeat(256), "public": 1 }),
            "document_name",
            "The document name must not be greater than 255 characters.",
        ),
    ];
    for (body, field, msg) in cases {
        let (status, res) = post(&api, body.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {res}");
        assert_eq!(res["errors"][field][0], msg, "{res}");
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM files")
        .fetch_one(&api.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

/// `meta` is capped on its text (16 KiB); quotes double in the stored JSON and still fit.
#[sqlx::test]
async fn meta_at_the_cap_is_stored(pool: PgPool) {
    let (api, _) = with_store(pool).await;
    let mut body = upload("a.pdf", PDF_B64);
    body["meta"] = json!({ "k": "\"".repeat(16_000) });
    let (status, res) = post(&api, body).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
}

#[sqlx::test]
async fn a_file_over_10_mb_is_refused(pool: PgPool) {
    use base64::Engine;
    let (api, _) = with_store(pool).await;
    let big = base64::engine::general_purpose::STANDARD.encode(vec![0u8; 10 * 1024 * 1024 + 1]);
    let (status, res) = post(&api, upload("big.bin", &big)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["upload"][0],
        "The upload must not be greater than 10240 kilobytes."
    );
}

#[sqlx::test]
async fn without_storage_file_routes_are_503(pool: PgPool) {
    let mut state = common::state(pool.clone(), Mailer::capture().0);
    state.files = None;
    let app = build_app(state);
    let (owner, ws) = common::signup(&app, "ada@example.com", "acme").await;
    let uri = format!("/v1/workspace/{ws}/files");
    let (status, res) = common::send(
        &app,
        Method::POST,
        &uri,
        Some(upload("a.pdf", PDF_B64)),
        Some(&owner),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{res}");
    assert_eq!(res, json!({ "message": "File storage isn't configured." }));
}

/// One upload per workspace at a time; a caller without `file.create` is refused before it
/// queues for the slot.
#[sqlx::test]
async fn uploads_take_the_workspace_heavy_slot(pool: PgPool) {
    let mut state = common::state(pool.clone(), Mailer::capture().0);
    state.slots = genesy_360_api::Slots::new(genesy_360_api::WorkspaceSlots {
        per_workspace: 100,
        per_user: 100,
        wait: std::time::Duration::from_millis(200),
    });
    let slots = state.slots.clone();
    let mut api = Api::new(pool, "/files").await;
    api.app = build_app(state);
    let bob = member_with(&api.app, &api.pool, &api.ws, "bob@example.com", &[]).await;
    let held = slots.heavy(api.ws.parse().unwrap()).await.unwrap();
    let body = upload("a.pdf", PDF_B64);
    let (status, _) = api.call(Method::POST, "", Some(body.clone()), &bob).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = post(&api, body.clone()).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    drop(held);
    let (status, res) = post(&api, body).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
}

// --- access --------------------------------------------------------------------------------

#[sqlx::test]
async fn access_rules(pool: PgPool) {
    let (api, _) = with_store(pool).await;
    let body = upload("a.pdf", PDF_B64);
    let id = api.create(body.clone(), "file_id").await;
    let routes = |id: &str| {
        [
            (Method::GET, String::new()),
            (Method::POST, String::new()),
            (Method::GET, format!("/{id}")),
            (Method::DELETE, format!("/{id}")),
        ]
    };
    let bob = member_with(&api.app, &api.pool, &api.ws, "bob@example.com", &[]).await;
    for (method, path) in routes(&id) {
        let (status, res) = api
            .call(method.clone(), &path, Some(body.clone()), &bob)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}: {res}");
    }
    let perms = ["file.list", "file.get", "file.create", "file.delete"];
    let carl = member_with(&api.app, &api.pool, &api.ws, "carl@example.com", &perms).await;
    let (status, res) = api.call(Method::POST, "", Some(body.clone()), &carl).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let mine = res["data"]["file_id"].as_str().unwrap().to_owned();
    for (method, path) in routes(&mine)
        .into_iter()
        .filter(|(m, p)| !(m == Method::POST && p.is_empty()))
    {
        let (status, res) = api.call(method.clone(), &path, None, &carl).await;
        assert_eq!(status, StatusCode::OK, "{method} {path}: {res}");
    }
    // Another workspace can't reach the file, through either workspace.
    let (eve, eve_ws) = common::signup(&api.app, "eve@example.com", "evil").await;
    let (status, _) = api.call(Method::GET, &format!("/{id}"), None, &eve).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    for method in [Method::GET, Method::DELETE] {
        let uri = format!("/v1/workspace/{eve_ws}/files/{id}");
        let (status, _) = common::send(&api.app, method.clone(), &uri, None, Some(&eve)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method}");
    }
    for bad in ["nope", "01900000-0000-7000-8000-000000000001"] {
        let (status, _) = api
            .call(Method::GET, &format!("/{bad}"), None, &api.owner)
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{bad}");
    }
}

// --- sweep ---------------------------------------------------------------------------------

/// An upload whose save never finished (pending for over an hour) and a delete that didn't
/// reach the bucket are finished by the sweep; stored files are left alone.
#[sqlx::test]
async fn the_sweep_finishes_interrupted_writes(pool: PgPool) {
    let (api, files) = with_store(pool).await;
    let stored = api.create(upload("keep.pdf", PDF_B64), "file_id").await;
    let pending = api.create(upload("lost.pdf", PDF_B64), "file_id").await;
    let deleting = api.create(upload("gone.pdf", PDF_B64), "file_id").await;
    sqlx::query("UPDATE files SET status = 'pending', updated_at = now() - interval '2 hours' WHERE id = $1::uuid")
        .bind(&pending)
        .execute(&api.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE files SET status = 'deleting', deleted_at = now(), updated_at = now() - interval '1 hour' WHERE id = $1::uuid")
        .bind(&deleting)
        .execute(&api.pool)
        .await
        .unwrap();
    let (pending_key, deleting_key, stored_key) = (
        row(&api, &pending).await.0,
        row(&api, &deleting).await.0,
        row(&api, &stored).await.0,
    );
    let swept = cleanup::sweep_files(&api.pool, &files).await.unwrap();
    assert_eq!(swept, 2);
    assert!(files.read(&pending_key).is_none() && files.read(&deleting_key).is_none());
    assert!(files.read(&stored_key).is_some());
    let left: Vec<(String, String)> =
        sqlx::query_as("SELECT id::text, status FROM files ORDER BY created_at")
            .fetch_all(&api.pool)
            .await
            .unwrap();
    // The lost upload ends as deleted too: claiming flipped it to deleting, so an upload
    // finishing late can't mark it stored.
    assert_eq!(
        left,
        [
            (stored, "stored".into()),
            (pending, "deleted".into()),
            (deleting, "deleted".into())
        ]
    );
}
