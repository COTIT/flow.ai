//! GET /v1/workspace/{ws} + POST /v1/workspace/{ws}/update (settings workspace form) and
//! GET/DELETE /v1/workspace/{ws}/invites (pending invites modal).

use crate::common;

use axum::{
    Router,
    http::{Method, StatusCode},
};
use common::{app_with_mail, member_with, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

struct World {
    app: Router,
    pool: PgPool,
    owner: String,
    ws: String,
}

async fn world(pool: PgPool) -> World {
    let (app, _) = app_with_mail(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    common::verify_email(&pool, "ada@example.com").await;
    World {
        app,
        pool,
        owner,
        ws,
    }
}

impl World {
    async fn get(&self, token: &str) -> (StatusCode, Value) {
        send(
            &self.app,
            Method::GET,
            &format!("/v1/workspace/{}", self.ws),
            None,
            Some(token),
        )
        .await
    }

    async fn update(&self, body: Value, token: &str) -> (StatusCode, Value) {
        let uri = format!("/v1/workspace/{}/update", self.ws);
        send(&self.app, Method::POST, &uri, Some(body), Some(token)).await
    }

    async fn invites(&self, query: &str, token: &str) -> (StatusCode, Value) {
        let uri = format!("/v1/workspace/{}/invites{query}", self.ws);
        send(&self.app, Method::GET, &uri, None, Some(token)).await
    }

    async fn invite(&self, email: &str) -> String {
        let uri = format!("/v1/workspace/{}/invites", self.ws);
        let (status, res) = send(
            &self.app,
            Method::POST,
            &uri,
            Some(json!({ "email": email })),
            Some(&self.owner),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        res["data"]["invite_id"].as_str().unwrap().to_owned()
    }
}

fn full_profile() -> Value {
    json!({
        "company_name": "Acme BV", "vat_number": "NL123456789B01", "coc_number": "12345678",
        "email": "info@acme.nl", "phone_number": "+31 20 123 4567", "website": "https://acme.nl",
        "language": "nl", "logo_file_id": null,
        "address": { "address": "Damrak", "house_number": "1", "house_number_addition": "A",
                     "address2": null, "zipcode": "1012 LG", "city": "Amsterdam", "state": null,
                     "country": "NL" },
        "account": { "account_name": "Acme", "account_number": "NL91ABNA0417164300",
                     "bic_swift": "ABNANL2A", "credit_limit": 100 },
    })
}

// ================= GET

#[sqlx::test]
async fn get_has_resource_shape_with_empty_profile_objects(pool: PgPool) {
    let w = world(pool).await;

    let (status, res) = w.get(&w.owner).await;

    assert_eq!(status, StatusCode::OK, "{res}");
    let d = &res["data"];
    assert_eq!(d["id"], w.ws);
    assert_eq!(d["workspace_name"], "acme");
    assert_eq!(d["workspace_type"], "retail");
    assert_eq!(d["pending_invites_count"], 0);
    assert_eq!(d["users_count"], 1);
    assert_eq!(d["meta"], Value::Null);
    assert_eq!(d["relationships"], json!([]));
    assert!(d["created_at"].is_string() && d["last_updated_at"].is_string());
    // settings.store.js replaces its form with this object: address/account must be objects.
    let p = &d["workspace_profile"];
    assert_eq!(p["company_name"], Value::Null);
    assert_eq!(p["logo"], Value::Null);
    assert_eq!(p["address"]["city"], Value::Null);
    assert_eq!(p["account"]["bic_swift"], Value::Null);
}

#[sqlx::test]
async fn counts_pending_invites_and_all_members(pool: PgPool) {
    let w = world(pool).await;
    w.invite("a@example.com").await;
    w.invite("b@example.com").await;
    sqlx::query(
        "UPDATE invites SET expires_at = now() - interval '1 second' WHERE email = 'b@example.com'",
    )
    .execute(&w.pool)
    .await
    .unwrap();
    member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &[]).await;
    sqlx::query("UPDATE workspace_members SET active = false WHERE user_id = (SELECT id FROM users WHERE email = 'bob@example.com') AND workspace_id = $1::uuid")
        .bind(&w.ws)
        .execute(&w.pool)
        .await
        .unwrap();

    let d = w.get(&w.owner).await.1["data"].clone();

    // Laravel counted every pending row; the invite list shows expired ones as pending too.
    assert_eq!(d["pending_invites_count"], 2);
    assert_eq!(d["users_count"], 2, "inactive members count (Laravel)");
}

#[sqlx::test]
async fn get_and_update_need_their_permissions(pool: PgPool) {
    let w = world(pool).await;
    let none = member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &[]).await;
    let reader = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "carol@example.com",
        &["workspace.get"],
    )
    .await;
    let writer = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "dan@example.com",
        &["workspace.update"],
    )
    .await;
    let (eve, _) = signup(&w.app, "eve@example.com", "eves").await;

    assert_eq!(w.get(&none).await.0, StatusCode::FORBIDDEN);
    assert_eq!(w.get(&reader).await.0, StatusCode::OK);
    assert_eq!(w.get(&eve).await.0, StatusCode::NOT_FOUND);
    // 403 before body validation.
    assert_eq!(
        w.update(json!({ "email": "bad" }), &reader).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        w.update(json!({ "company_name": "X" }), &writer).await.0,
        StatusCode::OK
    );
}

// ================= update

#[sqlx::test]
async fn update_stores_profile_and_merges_partial_updates(pool: PgPool) {
    let w = world(pool).await;

    let (status, res) = w.update(full_profile(), &w.owner).await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let p = w.get(&w.owner).await.1["data"]["workspace_profile"].clone();
    assert_eq!(p["company_name"], "Acme BV");
    assert_eq!(p["language"], "nl");
    assert_eq!(p["address"]["city"], "Amsterdam");
    assert_eq!(p["address"]["house_number_addition"], "A");
    assert_eq!(p["account"]["account_number"], "NL91ABNA0417164300");

    // Absent keys kept, null clears, unknown keys and workspace_type ignored.
    w.update(
        json!({ "company_name": null, "workspace_type": "manufacturing", "hacker": 1 }),
        &w.owner,
    )
    .await;
    let d = w.get(&w.owner).await.1["data"].clone();
    assert_eq!(d["workspace_profile"]["company_name"], Value::Null);
    assert_eq!(d["workspace_profile"]["email"], "info@acme.nl");
    assert_eq!(d["workspace_type"], "retail");
    assert!(d["workspace_profile"].get("hacker").is_none());
}

#[sqlx::test]
async fn profile_shows_in_me_workspaces_once_set(pool: PgPool) {
    let w = world(pool).await;
    let list = |res: Value| res["data"][0].clone();
    let before = list(
        send(
            &w.app,
            Method::GET,
            "/v1/me/workspaces",
            None,
            Some(&w.owner),
        )
        .await
        .1,
    );
    assert!(
        before.get("workspace_profile").is_none(),
        "omitted while unset (Laravel)"
    );

    w.update(json!({ "company_name": "Acme BV" }), &w.owner)
        .await;

    let after = list(
        send(
            &w.app,
            Method::GET,
            "/v1/me/workspaces",
            None,
            Some(&w.owner),
        )
        .await
        .1,
    );
    assert_eq!(after["workspace_profile"]["company_name"], "Acme BV");
}

#[sqlx::test]
async fn me_workspaces_never_shows_bank_details(pool: PgPool) {
    // Any member reads /me/workspaces; IBAN/credit limit stay behind workspace.get.
    let w = world(pool).await;
    w.update(full_profile(), &w.owner).await;
    let bob = member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &[]).await;

    let res = send(&w.app, Method::GET, "/v1/me/workspaces", None, Some(&bob))
        .await
        .1;

    let item = res["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == w.ws)
        .unwrap()
        .clone();
    assert_eq!(item["workspace_profile"]["company_name"], "Acme BV");
    assert!(item["workspace_profile"].get("account").is_none(), "{item}");
}

#[sqlx::test]
async fn empty_update_writes_nothing(pool: PgPool) {
    let w = world(pool).await;

    assert_eq!(w.update(json!({}), &w.owner).await.0, StatusCode::OK);

    let list = send(
        &w.app,
        Method::GET,
        "/v1/me/workspaces",
        None,
        Some(&w.owner),
    )
    .await
    .1;
    assert!(list["data"][0].get("workspace_profile").is_none());
}

#[sqlx::test]
async fn iban_and_numbers_stored_normalized(pool: PgPool) {
    let w = world(pool).await;
    let mut body = full_profile();
    body["account"]["account_number"] = json!("nl91 abna 0417 1643 00");
    body["account"]["credit_limit"] = json!(" 250.5 ");
    body["address"]["house_number"] = json!(" 12 ");

    assert_eq!(w.update(body, &w.owner).await.0, StatusCode::OK);

    let p = w.get(&w.owner).await.1["data"]["workspace_profile"].clone();
    assert_eq!(p["account"]["account_number"], "NL91ABNA0417164300");
    assert_eq!(p["account"]["credit_limit"], json!(250.5));
    assert_eq!(p["address"]["house_number"], "12");
}

#[sqlx::test]
async fn oversized_or_unicode_values_rejected(pool: PgPool) {
    let w = world(pool).await;
    let mut body = full_profile();
    body["address"]["house_number"] = json!("1".repeat(21));
    body["address"]["city"] = json!(5);
    body["account"]["account_number"] = json!("GB82WE\u{FB06}12345698765432");

    let (status, res) = w.update(body, &w.owner).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let e = &res["errors"];
    assert!(e.get("address.house_number").is_some(), "{e}");
    assert_eq!(
        e["address.city"].as_array().unwrap().len(),
        1,
        "one error per field: {e}"
    );
    assert!(
        e.get("account.account_number").is_some(),
        "ligature must not pass as ST: {e}"
    );
}

#[sqlx::test]
async fn update_validates_every_field(pool: PgPool) {
    let w = world(pool).await;

    let (status, res) = w
        .update(
            json!({
                "company_name": "x".repeat(56), "email": "not-mail", "website": "acme",
                "language": "de", "phone_number": "12", "logo_file_id": "file-1", "meta": "x",
                "address": { "address": "Damrak", "house_number": "one", "country": "NL" },
                "account": { "account_number": "NL00ABNA0417164300", "bic_swift": "AB", "credit_limit": -1 },
            }),
            &w.owner,
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let e = &res["errors"];
    for key in [
        "company_name",
        "email",
        "website",
        "language",
        "phone_number",
        "logo_file_id",
        "meta",
        "address.house_number",
        "address.zipcode",
        "address.city",
        "account.account_number",
        "account.bic_swift",
        "account.credit_limit",
    ] {
        assert!(e.get(key).is_some(), "no error for {key}: {e}");
    }
    assert_eq!(e["phone_number"], json!(["Phone number is not valid"]));
    // Nothing written on a failed update.
    assert_eq!(
        w.get(&w.owner).await.1["data"]["workspace_profile"]["company_name"],
        Value::Null
    );
}

#[sqlx::test]
async fn update_accepts_nulls_and_boundaries(pool: PgPool) {
    let w = world(pool).await;

    let (status, res) = w
        .update(
            json!({
                "company_name": "é".repeat(55), "address": null, "account": null, "email": null,
                "website": "http://a.io", "language": "en", "meta": { "k": "v" },
            }),
            &w.owner,
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{res}");
    let d = w.get(&w.owner).await.1["data"].clone();
    assert_eq!(d["meta"], json!({ "k": "v" }));
    assert_eq!(
        d["workspace_profile"]["address"]["city"],
        Value::Null,
        "still an object"
    );
}

// ================= pending invites

#[sqlx::test]
async fn pending_invites_listed_without_codes(pool: PgPool) {
    let w = world(pool).await;
    let id = w.invite("new@example.com").await;

    let (status, res) = w
        .invites("?filter[invite_status]=pending&tab=users", &w.owner)
        .await;

    assert_eq!(status, StatusCode::OK, "{res}");
    let item = &res["data"][0];
    assert_eq!(item["invite_id"], id);
    assert_eq!(item["email"], "new@example.com");
    assert_eq!(item["invite_status"], "pending");
    assert_eq!(item["invited_by"], "ada@example.com");
    assert!(item["expired_at"].is_string() && item["created_at"].is_string());
    assert_eq!(item["relationships"], json!({ "team": null }));
    assert!(
        item.get("invite_code").is_none(),
        "only the hash is stored; never echo codes"
    );
    assert_eq!(res["meta"]["total"], 1);

    let (_, res) = w.invites("?filter[invite_status]=accepted", &w.owner).await;
    assert_eq!(res["data"], json!([]));
}

#[sqlx::test]
async fn invite_list_shows_live_team_only_and_pages(pool: PgPool) {
    let w = world(pool).await;
    let team = common::add_team(&w.pool, &w.ws, "Sales", &["relation.list"], false).await;
    let uri = format!("/v1/workspace/{}/invites", w.ws);
    let body = json!({ "email": "a@example.com", "team_id": team });
    send(&w.app, Method::POST, &uri, Some(body), Some(&w.owner)).await;

    let res = w.invites("", &w.owner).await.1;
    assert_eq!(res["data"][0]["relationships"]["team"]["name"], "Sales");
    assert_eq!(
        res["data"][0]["relationships"]["team"]["permissions"],
        json!(["relation.list"])
    );

    sqlx::query("UPDATE teams SET deleted_at = now()")
        .execute(&w.pool)
        .await
        .unwrap();
    let res = w.invites("?page=1&page_size=1", &w.owner).await.1;
    assert_eq!(res["data"][0]["relationships"]["team"], Value::Null);
    let res = w.invites("?page=7", &w.owner).await.1;
    assert_eq!(res["data"], json!([]));
    assert_eq!(res["meta"]["total"], 1, "real total past the end");
}

#[sqlx::test]
async fn delete_invite_kills_the_link(pool: PgPool) {
    let w = world(pool).await;
    let id = w.invite("new@example.com").await;
    let uri = format!("/v1/workspace/{}/invites/{id}", w.ws);

    let (status, res) = send(&w.app, Method::DELETE, &uri, None, Some(&w.owner)).await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    assert_eq!(w.invites("", &w.owner).await.1["data"], json!([]));

    assert_eq!(
        send(&w.app, Method::DELETE, &uri, None, Some(&w.owner))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test]
async fn invite_list_and_delete_scoped_and_permissioned(pool: PgPool) {
    let w = world(pool).await;
    let id = w.invite("new@example.com").await;
    let (eve, eves) = signup(&w.app, "eve@example.com", "eves").await;
    let none = member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &[]).await;

    assert_eq!(w.invites("", &none).await.0, StatusCode::FORBIDDEN);
    let mine = format!("/v1/workspace/{}/invites/{id}", w.ws);
    assert_eq!(
        send(&w.app, Method::DELETE, &mine, None, Some(&none))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    // Eve owns another workspace: deleting Ada's invite through her own workspace is a 404.
    let through_eves = format!("/v1/workspace/{eves}/invites/{id}");
    assert_eq!(
        send(&w.app, Method::DELETE, &through_eves, None, Some(&eve))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(w.invites("", &w.owner).await.1["meta"]["total"], 1);
}
