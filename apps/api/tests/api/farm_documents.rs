//! Plot title documents: staff of the plot's village file and read them; farmers see nothing;
//! append-only; the type comes from the bytes; short-lived download links.
//! RUAHA ids in docs/ruaha-rls-mapping.md.

use crate::common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use chrono::{DateTime, Utc};
use common::farm::{Cast, Farm};
use genesy_360_api::{build_app, mail::Mailer, storage::Store};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

const PDF: &[u8] = b"%PDF-1.7\n1 0 obj\n<<>>\nendobj\ntrailer\n%%EOF\n";
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n0000IHDR";
const HEIC: &[u8] = b"\x00\x00\x00\x18ftypheic\x00\x00\x00\x00mif1heic";

async fn setup(pool: PgPool) -> (Farm, Cast, Store) {
    let mut f = Farm::new(pool.clone()).await;
    let c = f.cast().await;
    let store = Store::memory();
    let mut state = common::state(pool, Mailer::capture().0);
    state.files = Some(store.clone());
    f.app = build_app(state);
    (f, c, store)
}

/// One multipart part: name, file name (a file part) and bytes.
type Part<'a> = (&'a str, Option<&'a str>, &'a [u8]);

async fn upload(f: &Farm, plot: &str, token: &str, parts: &[Part<'_>]) -> (StatusCode, Value) {
    let boundary = "farmDocBoundary";
    let mut body = Vec::new();
    for (name, file, data) in parts {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        let head = match file {
            // The client's claimed type is a lie on purpose: the server must sniff.
            Some(n) => format!(
                "Content-Disposition: form-data; name=\"{name}\"; filename=\"{n}\"\r\nContent-Type: image/png\r\n\r\n"
            ),
            None => format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n"),
        };
        body.extend_from_slice(head.as_bytes());
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    let req = Request::post(f.url(&format!("/plots/{plot}/documents")))
        .header("authorization", format!("Bearer {token}"))
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();
    let res = f.app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    common::contract::check(
        &Method::POST,
        &f.url(&format!("/plots/{plot}/documents")),
        status,
        &value,
    );
    (status, value)
}

async fn filed(f: &Farm, plot: &str, token: &str, parts: &[Part<'_>]) -> Value {
    let (status, res) = upload(f, plot, token, parts).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    res["data"].clone()
}

#[sqlx::test]
async fn an_officer_files_a_title_document_and_reads_it_back(pool: PgPool) {
    let (f, c, store) = setup(pool.clone()).await;
    let doc = filed(
        &f,
        &c.neema.plot,
        &f.officer_ilu,
        &[("file", Some("../deed scan.pdf"), PDF)],
    )
    .await;
    assert_eq!(doc["content_type"], "application/pdf");
    assert_eq!(doc["file_name"], "deed scan.pdf");
    assert_eq!(doc["size_bytes"], PDF.len());
    assert_eq!(
        doc["uploaded_by"]["id"],
        f.user_id("salima@ruaha.test").await.as_str()
    );

    let key: String = sqlx::query_scalar("SELECT storage_key FROM farm_plot_documents")
        .fetch_one(&pool)
        .await
        .unwrap();
    let prefix = format!("genesy/{}/farm/{}/{}/", f.ws, f.ilundo, c.neema.plot);
    assert!(key.starts_with(&prefix), "{key}");
    assert_eq!(store.read(&key).unwrap().0, PDF);

    let list = f
        .list(
            &format!("/plots/{}/documents", c.neema.plot),
            &f.officer_ilu,
        )
        .await;
    assert_eq!(list["meta"]["total"], 1);
    let (status, one) = f
        .call(
            Method::GET,
            &format!(
                "/plots/{}/documents/{}",
                c.neema.plot,
                doc["id"].as_str().unwrap()
            ),
            None,
            &f.officer_ilu,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{one}");
    assert!(one["data"]["download_url"].as_str().unwrap().contains(&key));
    let expires: DateTime<Utc> = one["data"]["download_expires_at"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let ttl = expires - Utc::now();
    assert!(
        ttl.num_seconds() > 0 && ttl.num_minutes() <= 15,
        "link lives at most 15 minutes: {ttl}"
    );
}

#[sqlx::test]
async fn the_type_comes_from_the_bytes(pool: PgPool) {
    let (f, c, _) = setup(pool).await;
    let png = filed(
        &f,
        &c.neema.plot,
        &f.officer_ilu,
        &[("file", Some("deed.pdf"), PNG)],
    )
    .await;
    assert_eq!(
        png["content_type"], "image/png",
        "a PNG named .pdf is a PNG"
    );
    let heic = filed(
        &f,
        &c.neema.plot,
        &f.officer_ilu,
        &[("file", Some("IMG_1.HEIC"), HEIC)],
    )
    .await;
    assert_eq!(heic["content_type"], "image/heic");
    for (name, bytes) in [
        ("notes.pdf", &b"just some text"[..]),
        ("page.html", &b"<html></html>"[..]),
    ] {
        let (status, res) = upload(
            &f,
            &c.neema.plot,
            &f.officer_ilu,
            &[("file", Some(name), bytes)],
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{name}: {res}");
    }
    let big = vec![b'%'; 10 * 1024 * 1024 + 1];
    let (status, _) = upload(
        &f,
        &c.neema.plot,
        &f.officer_ilu,
        &[("file", Some("big.pdf"), &big)],
    )
    .await;
    assert!(
        status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::PAYLOAD_TOO_LARGE,
        "{status}"
    );
}

#[sqlx::test]
async fn an_upload_cannot_name_its_uploader_or_any_other_field(pool: PgPool) {
    let (f, c, _) = setup(pool).await;
    let ops = f.user_id("ops@ruaha.test").await;
    let (status, res) = upload(
        &f,
        &c.neema.plot,
        &f.officer_ilu,
        &[
            ("uploaded_by", None, ops.as_bytes()),
            ("file", Some("deed.pdf"), PDF),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
}

#[sqlx::test]
async fn a_filed_document_is_append_only(pool: PgPool) {
    let (f, c, store) = setup(pool.clone()).await;
    let id = "019a0000-0000-7000-8000-0000000000d1";
    let doc = filed(
        &f,
        &c.neema.plot,
        &f.officer_ilu,
        &[("id", None, id.as_bytes()), ("file", Some("deed.pdf"), PDF)],
    )
    .await;
    assert_eq!(doc["id"], id);
    // A retry with the same id is the same document; it cannot replace the file.
    let (status, again) = upload(
        &f,
        &c.neema.plot,
        &f.officer_ilu,
        &[
            ("id", None, id.as_bytes()),
            ("file", Some("other.png"), PNG),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["data"]["file_name"], "deed.pdf");
    let key: String = sqlx::query_scalar("SELECT storage_key FROM farm_plot_documents")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        store.read(&key).unwrap().0,
        PDF,
        "the stored file is unchanged"
    );

    for sql in [
        "UPDATE farm_plot_documents SET file_name = 'renamed.pdf'",
        "DELETE FROM farm_plot_documents",
    ] {
        let err = sqlx::query(sql).execute(&pool).await.unwrap_err();
        assert!(
            err.to_string().contains("plot documents cannot be changed"),
            "{err}"
        );
    }
    let one = format!("/plots/{}/documents/{id}", c.neema.plot);
    for method in [Method::DELETE, Method::PUT, Method::PATCH] {
        let (status, _) = f.call(method.clone(), &one, Some(json!({})), &f.ops).await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "{method}");
    }
}

#[sqlx::test]
async fn a_document_row_cannot_point_outside_its_plot_folder(pool: PgPool) {
    let (f, c, _) = setup(pool.clone()).await;
    let err = sqlx::query(
        "INSERT INTO farm_plot_documents (id, workspace_id, village_id, plot_id, storage_key, file_name,
                                          content_type, size_bytes, sha256, uploaded_by)
         SELECT uuidv7(), $1::uuid, $2::uuid, $3::uuid, 'genesy/elsewhere/x.pdf', 'x.pdf',
                'application/pdf', 10, decode(repeat('00', 32), 'hex'), id
         FROM users WHERE email = 'salima@ruaha.test'",
    )
    .bind(&f.ws)
    .bind(&f.ilundo)
    .bind(&c.neema.plot)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("farm_plot_documents_key_in_plot"),
        "{err}"
    );
}

#[sqlx::test]
async fn only_staff_of_the_village_see_or_file_documents(pool: PgPool) {
    let (f, c, _) = setup(pool).await;
    let doc = filed(
        &f,
        &c.neema.plot,
        &f.officer_ilu,
        &[("file", Some("deed.pdf"), PDF)],
    )
    .await;
    let neema = f
        .farmer_for("neema@ruaha.test", &f.ilundo, &c.neema.person)
        .await;
    let list = format!("/plots/{}/documents", c.neema.plot);
    let one = format!("{list}/{}", doc["id"].as_str().unwrap());

    for token in [&f.officer_mga, &neema] {
        for tail in [&list, &one] {
            let (status, _) = f.call(Method::GET, tail, None, token).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{tail}");
        }
        let (status, _) =
            upload(&f, &c.neema.plot, token, &[("file", Some("deed.pdf"), PDF)]).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "no upload from outside the village staff"
        );
    }
    assert_eq!(
        f.list(&list, &f.ops).await["meta"]["total"],
        1,
        "ops sees the document"
    );

    let (outsider, _) = common::signup(&f.app, "out@else.test", "elsewhere").await;
    let (status, _) = f.call(Method::GET, &list, None, &outsider).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = common::send(&f.app, Method::GET, &f.url(&list), None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn a_client_id_of_another_plot_or_workspace_never_returns_that_document(pool: PgPool) {
    let (f, c, _) = setup(pool).await;
    let id = "019a0000-0000-7000-8000-0000000000d9";
    let filed = |plot: String, token: String| {
        let f = &f;
        async move {
            upload(
                f,
                &plot,
                &token,
                &[("id", None, id.as_bytes()), ("file", Some("deed.pdf"), PDF)],
            )
            .await
        }
    };
    // Same id, another plot of this workspace.
    let (status, _) = filed(c.asha.plot.clone(), f.officer_ilu.clone()).await;
    assert_eq!(status, StatusCode::CREATED);
    let (other_plot, body_a) = filed(c.neema.plot.clone(), f.officer_ilu.clone()).await;

    // Same id in another workspace: its owner files a document there first.
    let (owner2, ws2) = common::signup(&f.app, "other@else.test", "elsewhere").await;
    let owner2_id = f.user_id("other@else.test").await;
    let url2 = |tail: &str| format!("/v1/workspace/{ws2}/farm{tail}");
    let call2 = |method: Method, tail: String, body: Value| {
        let (app, uri, token) = (f.app.clone(), url2(&tail), owner2.clone());
        async move { common::send(&app, method, &uri, Some(body), Some(&token)).await }
    };
    call2(
        Method::POST,
        "/memberships".into(),
        json!({ "user_id": owner2_id, "role": "admin" }),
    )
    .await;
    let (_, v) = call2(
        Method::POST,
        "/villages".into(),
        json!({ "code": "X", "name": "X", "capacity": { "capacity_kw": "1" } }),
    )
    .await;
    let (_, crop) = call2(
        Method::POST,
        "/crops".into(),
        json!({ "code": "M", "name_en": "M", "name_sw": "M" }),
    )
    .await;
    let (status, reg) = call2(
        Method::POST,
        "/registrations".into(),
        json!({ "client_ref": common::farm::client_ref(), "village_id": v["data"]["id"],
                "person": { "given_name": "A", "family_name": "B", "phone": "0755000000" },
                "farm": { "label": "F", "latitude": "-7", "longitude": "35" },
                "plot": { "label": "P" },
                "cycles": [{ "crop_id": crop["data"]["id"], "area_ha": "1" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{reg}");
    let plot2 = reg["data"]["plot_id"].as_str().unwrap().to_owned();
    let mut f2 = Farm {
        ws: ws2.clone(),
        ..clone_farm(&f)
    };
    f2.officer_ilu = owner2.clone();
    let (status, _) = upload(
        &f2,
        &plot2,
        &owner2,
        &[
            ("id", None, b"019a0000-0000-7000-8000-0000000000da"),
            ("file", Some("deed.pdf"), PDF),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = upload(
        &f2,
        &plot2,
        &owner2,
        &[("id", None, id.as_bytes()), ("file", Some("deed.pdf"), PDF)],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "taken in workspace 1");
    let (_, body_b) = upload(
        &f,
        &c.rehema.plot,
        &f.officer_ilu,
        &[
            ("id", None, b"019a0000-0000-7000-8000-0000000000da"),
            ("file", Some("deed.pdf"), PDF),
        ],
    )
    .await;

    assert_eq!(other_plot, StatusCode::CONFLICT);
    assert_eq!(
        body_a, body_b,
        "same answer whichever plot or workspace holds the id"
    );
    assert!(
        !body_a.to_string().contains("deed.pdf"),
        "the other document is never returned: {body_a}"
    );
}

fn clone_farm(f: &Farm) -> Farm {
    Farm {
        app: f.app.clone(),
        pool: f.pool.clone(),
        ws: f.ws.clone(),
        owner: f.owner.clone(),
        ops: f.ops.clone(),
        officer_ilu: f.officer_ilu.clone(),
        officer_mga: f.officer_mga.clone(),
        ilundo: f.ilundo.clone(),
        mgama: f.mgama.clone(),
    }
}
