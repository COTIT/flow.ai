//! Supply links between two workspaces: the farm workspace (R) grants, the shop workspace (A)
//! accepts, either revokes. A link carries an allow-list of what A may show publicly; it
//! never opens R's records to A.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{farm::Farm, join_workspace, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

struct Two {
    r: Farm,
    /// AFM owner token and workspace.
    a: String,
    a_ws: String,
}

async fn two(pool: PgPool) -> Two {
    let r = Farm::new(pool).await;
    let (a, a_ws) = signup(&r.app, "owner@afm.test", "afm").await;
    Two { r, a, a_ws }
}

async fn call(
    t: &Two,
    method: Method,
    ws: &str,
    tail: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    send(
        &t.r.app,
        method,
        &format!("/v1/workspace/{ws}/supply-links{tail}"),
        body,
        Some(token),
    )
    .await
}

async fn grant(t: &Two, fields: Value) -> (StatusCode, Value) {
    let body = json!({ "to_workspace_id": t.a_ws, "fields": fields });
    call(t, Method::POST, &t.r.ws, "", Some(body), &t.r.owner).await
}

#[sqlx::test]
async fn r_grants_a_accepts_and_both_see_the_live_link(pool: PgPool) {
    let t = two(pool).await;
    let (status, res) = grant(&t, json!(["village", "farmer_count", "harvest_window"])).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let id = res["data"]["id"].as_str().unwrap().to_owned();
    assert_eq!(res["data"]["status"], "pending");
    assert_eq!(res["data"]["direction"], "outgoing");
    assert_eq!(res["data"]["partner"]["name"], "afm");
    assert_eq!(
        res["data"]["fields"],
        json!(["farmer_count", "harvest_window", "village"])
    );

    let (status, res) = call(
        &t,
        Method::POST,
        &t.a_ws,
        &format!("/{id}/accept"),
        None,
        &t.a,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["status"], "live");
    assert_eq!(res["data"]["direction"], "incoming");
    assert_eq!(res["data"]["partner"]["name"], "ruaha");

    let (_, list) = call(&t, Method::GET, &t.r.ws, "", None, &t.r.owner).await;
    assert_eq!(list["data"][0]["status"], "live");
    assert_eq!(list["data"][0]["direction"], "outgoing");
    let (_, list) = call(&t, Method::GET, &t.a_ws, "", None, &t.a).await;
    assert_eq!(list["data"][0]["id"], id.as_str());
    assert_eq!(list["meta"]["total"], 1);
}

#[sqlx::test]
async fn the_allow_list_and_target_are_checked(pool: PgPool) {
    let t = two(pool).await;
    for (fields, why) in [
        (json!(["farmer_names"]), "names wait for the PDPC gate"),
        (json!(["farmer_photos"]), "photos wait for the PDPC gate"),
        (json!(["phone"]), "unknown field"),
        (json!("village"), "not a list"),
    ] {
        let (status, res) = grant(&t, fields).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{why}: {res}");
        assert!(res["errors"]["fields"].is_array(), "{why}: {res}");
    }
    let (_, res) = grant(&t, json!(["farmer_names"])).await;
    assert_eq!(
        res["errors"]["fields"][0],
        "farmer_names can't be shared until the data-protection gate opens."
    );

    for (target, why) in [
        (json!(t.r.ws), "itself"),
        (json!("01900000-0000-7000-8000-000000000001"), "unknown"),
        (json!("nope"), "malformed"),
    ] {
        let body = json!({ "to_workspace_id": target, "fields": [] });
        let (status, res) = call(&t, Method::POST, &t.r.ws, "", Some(body), &t.r.owner).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{why}: {res}");
        assert!(res["errors"]["to_workspace_id"].is_array(), "{why}: {res}");
    }

    let (status, _) = grant(&t, json!([])).await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "nothing shown publicly is fine"
    );
    let (status, _) = grant(&t, json!(["village"])).await;
    assert_eq!(status, StatusCode::CONFLICT, "one open link per pair");
}

#[sqlx::test]
async fn only_the_receiving_admin_accepts_and_outsiders_see_nothing(pool: PgPool) {
    let t = two(pool).await;
    let (_, res) = grant(&t, json!(["village"])).await;
    let id = res["data"]["id"].as_str().unwrap().to_owned();
    let accept = format!("/{id}/accept");

    let (status, _) = call(&t, Method::POST, &t.r.ws, &accept, None, &t.r.owner).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "the granting side can't accept for A"
    );

    let (c, c_ws) = signup(&t.r.app, "owner@third.test", "third").await;
    let (status, _) = call(&t, Method::POST, &c_ws, &accept, None, &c).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "a third workspace");
    let (status, _) = call(&t, Method::DELETE, &c_ws, &format!("/{id}"), None, &c).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, list) = call(&t, Method::GET, &c_ws, "", None, &c).await;
    assert_eq!(list["data"], json!([]));
    let (status, _) = call(&t, Method::POST, &t.a_ws, &accept, None, &c).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "not a member of A");

    let (member, _) = signup(&t.r.app, "clerk@afm.test", "clerk-own").await;
    join_workspace(&t.r.pool, &t.a_ws, "clerk@afm.test").await;
    let (status, _) = call(&t, Method::POST, &t.a_ws, &accept, None, &member).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "a member who is not admin");

    let (status, _) = call(&t, Method::POST, &t.a_ws, &accept, None, &t.a).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(&t, Method::POST, &t.a_ws, &accept, None, &t.a).await;
    assert_eq!(status, StatusCode::CONFLICT, "already live");
}

#[sqlx::test]
async fn either_side_revokes_and_a_revoked_link_stays_dead(pool: PgPool) {
    let t = two(pool).await;
    let (_, res) = grant(&t, json!(["village"])).await;
    let id = res["data"]["id"].as_str().unwrap().to_owned();
    call(
        &t,
        Method::POST,
        &t.a_ws,
        &format!("/{id}/accept"),
        None,
        &t.a,
    )
    .await;

    let (status, _) = call(&t, Method::DELETE, &t.a_ws, &format!("/{id}"), None, &t.a).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(
        &t,
        Method::DELETE,
        &t.r.ws,
        &format!("/{id}"),
        None,
        &t.r.owner,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "again: still 204");
    let (_, list) = call(&t, Method::GET, &t.r.ws, "", None, &t.r.owner).await;
    assert_eq!(list["data"][0]["status"], "revoked");
    let (status, _) = call(
        &t,
        Method::POST,
        &t.a_ws,
        &format!("/{id}/accept"),
        None,
        &t.a,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a revoked link never comes back"
    );

    let (status, _) = grant(&t, json!(["village"])).await;
    assert_eq!(status, StatusCode::CREATED, "a new grant after the revoke");
}

#[sqlx::test]
async fn a_live_link_opens_none_of_rs_records_to_a(pool: PgPool) {
    let t = two(pool).await;
    let c = t.r.cast().await;
    let (_, res) = grant(&t, json!(["village", "farmer_count", "harvest_window"])).await;
    let id = res["data"]["id"].as_str().unwrap().to_owned();
    call(
        &t,
        Method::POST,
        &t.a_ws,
        &format!("/{id}/accept"),
        None,
        &t.a,
    )
    .await;

    for tail in [
        "/supply-batches".to_owned(),
        format!("/persons/{}/passbook", c.neema.person),
        format!("/persons/{}", c.neema.person),
        "/cash/reconciliation?day=2026-11-01".to_owned(),
    ] {
        let (status, _) = t.r.call(Method::GET, &tail, None, &t.a).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{tail}");
    }
    let (status, _) = send(
        &t.r.app,
        Method::GET,
        &format!("/v1/workspace/{}/lots", t.r.ws),
        None,
        Some(&t.a),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "R's lots");
}

/// A grant retried with its Idempotency-Key answers the first 201 again and leaves one link;
/// another body under the key is refused. (Revoke is already safe to repeat: 204 each time.)
#[sqlx::test]
async fn a_retried_grant_makes_one_link(pool: PgPool) {
    let t = two(pool).await;
    let send = |fields: Value, key: &'static str| {
        let (app, uri, token, a_ws) = (
            t.r.app.clone(),
            format!("/v1/workspace/{}/supply-links", t.r.ws),
            t.r.owner.clone(),
            t.a_ws.clone(),
        );
        async move {
            common::send_raw(
                &app,
                Method::POST,
                &uri,
                Some(json!({ "to_workspace_id": a_ws, "fields": fields })),
                Some(&token),
                &[("idempotency-key", key)],
            )
            .await
        }
    };
    let first = send(json!(["village"]), "g-1").await;
    assert_eq!(first.0, StatusCode::CREATED);
    let again = send(json!(["village"]), "g-1").await;
    assert_eq!(again, first, "byte-identical replay, not a 409");
    let (_, list) = call(&t, Method::GET, &t.r.ws, "", None, &t.r.owner).await;
    assert_eq!(list["meta"]["total"], 1);
    let (status, bytes) = send(json!(["farmer_count"]), "g-1").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap()["message"],
        "Idempotency-Key reused with different request payload"
    );
}

/// `meta.total` of the list `ws` sees under `query`.
async fn total(t: &Two, ws: &str, token: &str, query: &str) -> Value {
    let (status, res) = call(t, Method::GET, ws, query, None, token).await;
    assert_eq!(status, StatusCode::OK, "{query}: {res}");
    res["meta"]["total"].clone()
}

#[sqlx::test]
async fn the_list_filters_by_status_and_direction(pool: PgPool) {
    let t = two(pool).await;
    let (_, res) = grant(&t, json!(["village"])).await;
    let id = res["data"]["id"].as_str().unwrap().to_owned();

    let pending_in = "?status=pending&direction=incoming";
    assert_eq!(total(&t, &t.a_ws, &t.a, pending_in).await, 1, "A's badge");
    assert_eq!(total(&t, &t.r.ws, &t.r.owner, pending_in).await, 0);
    assert_eq!(
        total(&t, &t.r.ws, &t.r.owner, "?direction=outgoing").await,
        1
    );
    assert_eq!(total(&t, &t.a_ws, &t.a, "?status=live").await, 0);

    call(
        &t,
        Method::POST,
        &t.a_ws,
        &format!("/{id}/accept"),
        None,
        &t.a,
    )
    .await;
    assert_eq!(total(&t, &t.a_ws, &t.a, pending_in).await, 0);
    assert_eq!(total(&t, &t.a_ws, &t.a, "?status=live").await, 1);

    call(
        &t,
        Method::DELETE,
        &t.r.ws,
        &format!("/{id}"),
        None,
        &t.r.owner,
    )
    .await;
    assert_eq!(total(&t, &t.a_ws, &t.a, "?status=live").await, 0);
    assert_eq!(total(&t, &t.r.ws, &t.r.owner, "?status=revoked").await, 1);
    let (_, res) = call(&t, Method::GET, &t.a_ws, "?status=revoked", None, &t.a).await;
    assert_eq!(res["data"][0]["id"], id.as_str());

    let (c, c_ws) = signup(&t.r.app, "owner@third.test", "third").await;
    for q in [
        "?status=revoked",
        "?direction=incoming",
        "?direction=outgoing",
    ] {
        assert_eq!(total(&t, &c_ws, &c, q).await, 0, "third workspace, {q}");
    }

    for q in ["?status=open", "?direction=both"] {
        let (status, res) = call(&t, Method::GET, &t.a_ws, q, None, &t.a).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{q}: {res}");
    }
}
