//! Farm tenancy: villages, farm memberships (role + optional village), the farm guard, and
//! isolation of farm-only users from the rest of the workspace. RUAHA ids in
//! docs/ruaha-rls-mapping.md.

mod common;

use axum::http::{Method, StatusCode};
use common::{farm::Farm, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

fn ids(list: &Value) -> Vec<String> {
    list["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap().to_owned())
        .collect()
}

// ---- villages ----

#[sqlx::test]
async fn ops_creates_a_village_and_a_retry_replays(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    let id = "019a0000-0000-7000-8000-0000000000aa";
    let body = json!({ "id": id, "code": "IGOMAA", "name": "Igomaa", "latitude": "-7.80", "longitude": "35.60" });

    let (status, res) = f
        .call(Method::POST, "/villages", Some(body.clone()), &f.ops)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["id"], id);
    assert_eq!(res["data"]["latitude"], "-7.800000");
    assert_eq!(res["data"]["primary_language"], "sw");

    let (status, again) = f.call(Method::POST, "/villages", Some(body), &f.ops).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["data"], res["data"]);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM farm_villages WHERE id = $1::uuid")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test]
async fn a_duplicate_village_code_is_refused(pool: PgPool) {
    let f = Farm::new(pool).await;
    let (status, res) = f
        .call(
            Method::POST,
            "/villages",
            Some(json!({ "code": "ILUNDO", "name": "Again" })),
            &f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["code"].is_array(), "{res}");
}

#[sqlx::test]
async fn officers_and_farmers_cannot_create_villages(pool: PgPool) {
    let f = Farm::new(pool).await;
    let neema = f.farmer("neema@ruaha.test", &f.ilundo).await;
    for token in [&f.officer_ilu, &neema] {
        let (status, res) = f
            .call(
                Method::POST,
                "/villages",
                Some(json!({ "code": "X", "name": "X" })),
                token,
            )
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{res}");
    }
}

#[sqlx::test]
async fn ops_cannot_create_a_village_in_another_workspace(pool: PgPool) {
    let f = Farm::new(pool).await;
    let (_, other_ws) = signup(&f.app, "other@else.test", "elsewhere").await;
    let uri = format!("/v1/workspace/{other_ws}/farm/villages");
    let (status, _) = send(
        &f.app,
        Method::POST,
        &uri,
        Some(json!({ "code": "X", "name": "X" })),
        Some(&f.ops),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn every_farm_member_reads_the_villages(pool: PgPool) {
    let f = Farm::new(pool).await;
    let neema = f.farmer("neema@ruaha.test", &f.ilundo).await;
    for token in [&f.owner, &f.ops, &f.officer_mga, &neema] {
        let (status, res) = f.call(Method::GET, "/villages", None, token).await;
        assert_eq!(status, StatusCode::OK, "{res}");
        assert_eq!(res["meta"]["total"], 2, "{res}");
    }
}

#[sqlx::test]
async fn ops_renames_a_village_officers_cannot(pool: PgPool) {
    let f = Farm::new(pool).await;
    let tail = format!("/villages/{}", f.ilundo);
    let (status, res) = f
        .call(
            Method::PATCH,
            &tail,
            Some(json!({ "name": "Ilundo B" })),
            &f.officer_ilu,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{res}");
    let (status, res) = f
        .call(
            Method::PATCH,
            &tail,
            Some(json!({ "name": "Ilundo B" })),
            &f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["name"], "Ilundo B");
    assert_eq!(res["data"]["code"], "ILUNDO");
}

// ---- memberships ----

#[sqlx::test]
async fn a_field_officer_is_scoped_to_the_named_village(pool: PgPool) {
    let f = Farm::new(pool).await;
    f.member("juma@ruaha.test").await;
    let juma = f.user_id("juma@ruaha.test").await;
    let m = f
        .grant(&f.ops, &juma, "field_officer", Some(&f.ilundo))
        .await;
    assert_eq!(m["role"], "field_officer");
    assert_eq!(m["village_id"], f.ilundo.as_str());
    assert!(m["revoked_at"].is_null());
    assert_eq!(m["user"]["id"], juma.as_str());
}

#[sqlx::test]
async fn a_repeat_grant_adds_no_second_membership(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    f.member("juma@ruaha.test").await;
    let juma = f.user_id("juma@ruaha.test").await;
    let first = f
        .grant(&f.ops, &juma, "field_officer", Some(&f.ilundo))
        .await;
    let (status, again) = f
        .call(
            Method::POST,
            "/memberships",
            Some(json!({ "user_id": juma, "role": "field_officer", "village_id": f.ilundo })),
            &f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["data"]["id"], first["id"]);
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM farm_memberships WHERE user_id = $1::uuid")
            .bind(&juma)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test]
async fn ops_is_granted_across_the_whole_workspace(pool: PgPool) {
    let f = Farm::new(pool).await;
    f.member("asha@ruaha.test").await;
    let asha = f.user_id("asha@ruaha.test").await;
    let m = f.grant(&f.owner, &asha, "ops", None).await;
    assert_eq!(m["role"], "ops");
    assert!(m["village_id"].is_null());
}

#[sqlx::test]
async fn granting_a_revoked_role_again_restores_it(pool: PgPool) {
    let f = Farm::new(pool).await;
    f.member("juma@ruaha.test").await;
    let juma = f.user_id("juma@ruaha.test").await;
    let m = f
        .grant(&f.ops, &juma, "field_officer", Some(&f.ilundo))
        .await;
    let tail = format!("/memberships/{}", m["id"].as_str().unwrap());
    let (status, _) = f.call(Method::DELETE, &tail, None, &f.ops).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = f.call(Method::DELETE, &tail, None, &f.ops).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "revoke is idempotent");

    let back = f
        .grant(&f.ops, &juma, "field_officer", Some(&f.ilundo))
        .await;
    assert_eq!(back["id"], m["id"]);
    assert!(back["revoked_at"].is_null(), "{back}");
}

#[sqlx::test]
async fn grant_input_rules(pool: PgPool) {
    let f = Farm::new(pool).await;
    f.member("juma@ruaha.test").await;
    let juma = f.user_id("juma@ruaha.test").await;
    let unknown = "019a0000-0000-7000-8000-000000000999";
    for (body, field) in [
        // farmers get their login from an officer, never from this route
        (
            json!({ "user_id": juma, "role": "farmer", "village_id": f.ilundo }),
            "role",
        ),
        (
            json!({ "user_id": juma, "role": "field_officer" }),
            "village_id",
        ),
        (
            json!({ "user_id": juma, "role": "admin", "village_id": f.ilundo }),
            "village_id",
        ),
        (
            json!({ "user_id": juma, "role": "ops", "village_id": f.ilundo }),
            "village_id",
        ),
        (
            json!({ "user_id": juma, "role": "field_officer", "village_id": unknown }),
            "village_id",
        ),
    ] {
        let (status, res) = f
            .call(Method::POST, "/memberships", Some(body), &f.owner)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
        assert!(res["errors"][field].is_array(), "{field}: {res}");
    }
}

#[sqlx::test]
async fn staff_roles_need_an_active_workspace_member(pool: PgPool) {
    let f = Farm::new(pool).await;
    signup(&f.app, "stranger@else.test", "stranger").await;
    let stranger = f.user_id("stranger@else.test").await;
    let (status, res) = f
        .call(
            Method::POST,
            "/memberships",
            Some(json!({ "user_id": stranger, "role": "ops" })),
            &f.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["user_id"].is_array(), "{res}");
}

#[sqlx::test]
async fn ops_cannot_mint_admin_or_self_elevate(pool: PgPool) {
    let f = Farm::new(pool).await;
    f.member("juma@ruaha.test").await;
    let juma = f.user_id("juma@ruaha.test").await;
    let ops_id = f.user_id("ops@ruaha.test").await;
    for body in [
        json!({ "user_id": juma, "role": "admin" }),
        json!({ "user_id": juma, "role": "ops" }),
        json!({ "user_id": ops_id, "role": "admin" }),
        json!({ "user_id": ops_id, "role": "field_officer", "village_id": f.ilundo }),
    ] {
        let (status, res) = f
            .call(Method::POST, "/memberships", Some(body), &f.ops)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{res}");
    }
}

#[sqlx::test]
async fn ops_cannot_touch_admin_memberships(pool: PgPool) {
    let f = Farm::new(pool).await;
    let (_, list) = f.call(Method::GET, "/memberships", None, &f.owner).await;
    let admin = list["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "admin")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (status, _) = f
        .call(
            Method::DELETE,
            &format!("/memberships/{admin}"),
            None,
            &f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // No route changes a membership's role: only grant and revoke exist.
    let (status, _) = f
        .call(
            Method::PATCH,
            &format!("/memberships/{admin}"),
            Some(json!({ "role": "admin" })),
            &f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}

#[sqlx::test]
async fn a_farm_admin_cannot_touch_their_own_membership(pool: PgPool) {
    let f = Farm::new(pool).await;
    f.member("amina@ruaha.test").await;
    let amina_id = f.user_id("amina@ruaha.test").await;
    let m = f.grant(&f.owner, &amina_id, "admin", None).await;
    let amina = signup_token(&f, "amina@ruaha.test").await;
    let (status, _) = f
        .call(
            Method::DELETE,
            &format!("/memberships/{}", m["id"].as_str().unwrap()),
            None,
            &amina,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// Fresh token for an existing user, through the normal login route.
async fn signup_token(f: &Farm, email: &str) -> String {
    let (status, res) = send(
        &f.app,
        Method::POST,
        "/v1/login",
        Some(json!({ "email": email, "password": common::PASSWORD })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"]["access_token"].as_str().unwrap().to_owned()
}

#[sqlx::test]
async fn officers_list_their_own_memberships_managers_list_all(pool: PgPool) {
    let f = Farm::new(pool).await;
    let (_, mine) = f
        .call(Method::GET, "/memberships", None, &f.officer_ilu)
        .await;
    assert_eq!(mine["meta"]["total"], 1, "{mine}");
    let (_, all) = f.call(Method::GET, "/memberships", None, &f.ops).await;
    assert_eq!(all["meta"]["total"], 4, "{all}");
    assert_eq!(ids(&all).len(), 4);
}

#[sqlx::test]
async fn a_grant_is_on_the_audit_trail_with_its_actor(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    f.member("juma@ruaha.test").await;
    let juma = f.user_id("juma@ruaha.test").await;
    let m = f
        .grant(&f.ops, &juma, "field_officer", Some(&f.ilundo))
        .await;
    let ops_id = f.user_id("ops@ruaha.test").await;
    let (action, actor): (String, Option<String>) = sqlx::query_as(
        "SELECT action, actor_id::text FROM farm_audit_log
         WHERE table_name = 'farm_memberships' AND record_id = $1::uuid",
    )
    .bind(m["id"].as_str().unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (action.as_str(), actor.as_deref()),
        ("insert", Some(ops_id.as_str()))
    );
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM farm_audit_log WHERE table_name = 'farm_villages' AND actor_id IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 0, "every village write names its actor");
}

#[sqlx::test]
async fn the_audit_trail_cannot_be_changed(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    for sql in [
        "UPDATE farm_audit_log SET action = 'delete'",
        "DELETE FROM farm_audit_log",
    ] {
        let err = sqlx::query(sql).execute(&pool).await.unwrap_err();
        assert!(
            err.to_string().contains("audit trail cannot be changed"),
            "{err}"
        );
    }
    drop(f);
}

// ---- the guard ----

#[sqlx::test]
async fn a_farm_only_user_is_shut_out_of_the_rest_of_the_workspace(pool: PgPool) {
    let f = Farm::new(pool).await;
    let neema = f.farmer("neema@ruaha.test", &f.ilundo).await;
    for tail in ["/products", "/invoices", "/relations"] {
        let uri = format!("/v1/workspace/{}{tail}", f.ws);
        let (status, res) = send(&f.app, Method::GET, &uri, None, Some(&neema)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{tail}: {res}");
    }
    let (status, _) = f.call(Method::GET, "/villages", None, &neema).await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test]
async fn outsiders_get_404_and_anonymous_401(pool: PgPool) {
    let f = Farm::new(pool).await;
    let (outsider, _) = signup(&f.app, "out@else.test", "outside").await;
    let (status, _) = f.call(Method::GET, "/villages", None, &outsider).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&f.app, Method::GET, &f.url("/villages"), None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = send(
        &f.app,
        Method::GET,
        "/v1/workspace/not-a-uuid/farm/villages",
        None,
        Some(&f.ops),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn a_staff_role_lapses_with_workspace_membership(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    sqlx::query(
        "UPDATE workspace_members SET active = false
         WHERE user_id = (SELECT id FROM users WHERE email = 'ops@ruaha.test')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let (status, _) = f.call(Method::GET, "/villages", None, &f.ops).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn revoked_access_ends_on_the_next_request(pool: PgPool) {
    let f = Farm::new(pool).await;
    let (_, list) = f
        .call(Method::GET, "/memberships", None, &f.officer_mga)
        .await;
    let id = list["data"][0]["id"].as_str().unwrap().to_owned();
    let (status, _) = f
        .call(Method::DELETE, &format!("/memberships/{id}"), None, &f.ops)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    // Peter is still a workspace member, so the guard lets him in with no farm role:
    // he sees no memberships of his own.
    let (status, res) = f
        .call(Method::GET, "/memberships", None, &f.officer_mga)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        res["data"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["revoked_at"].is_null())
            .count(),
        0
    );
}
