//! Till cash: drawer shifts (open/close with counted vs expected), vaults, drawer↔vault
//! transfers and the append-only cash ledger. Money in integer minor units.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{Api, member_with};
use serde_json::{Value, json};
use sqlx::PgPool;

const PIN: &str = "4821";

struct Till {
    api: Api,
    /// Session token on register 1.
    t1: String,
    /// Session token on register 2.
    t2: String,
    vault: String,
}

async fn till(pool: PgPool) -> Till {
    let api = Api::new(pool, "/till").await;
    sqlx::query(
        "INSERT INTO warehouses (workspace_id, name, slug, is_store)
         VALUES ($1::uuid, 'Shop', 'shop', true)",
    )
    .bind(&api.ws)
    .execute(&api.pool)
    .await
    .unwrap();
    let t1 = session(&api, "Till 1").await;
    let t2 = session(&api, "Till 2").await;
    let (status, res) = api
        .call(
            Method::POST,
            "/vaults",
            Some(json!({ "name": "Safe" })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let vault = res["data"]["id"].as_str().unwrap().to_owned();
    Till { api, t1, t2, vault }
}

/// A register and a till user logged in on it; the session token.
async fn session(api: &Api, name: &str) -> String {
    let store: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM warehouses WHERE workspace_id = $1::uuid AND is_store")
            .bind(&api.ws)
            .fetch_one(&api.pool)
            .await
            .unwrap();
    let (_, reg) = api
        .call(
            Method::POST,
            "/registers",
            Some(json!({ "name": name, "warehouse_id": store })),
            &api.owner,
        )
        .await;
    let key = reg["data"]["key"].as_str().unwrap().to_owned();
    let (_, user) = api
        .call(
            Method::POST,
            "/users",
            Some(json!({ "name": format!("{name} user"), "pin": PIN })),
            &api.owner,
        )
        .await;
    let (status, res) = api
        .call(
            Method::POST,
            "/login",
            Some(json!({ "till_user_id": user["data"]["id"], "pin": PIN })),
            &key,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"]["token"].as_str().unwrap().to_owned()
}

fn count(pairs: &[(i64, i64)]) -> Value {
    Value::Array(
        pairs
            .iter()
            .map(|(d, c)| json!({ "denomination": d, "count": c }))
            .collect(),
    )
}

impl Till {
    async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        t: &str,
    ) -> (StatusCode, Value) {
        self.api.call(method, path, body, t).await
    }

    async fn open(&self, t: &str, pairs: &[(i64, i64)]) -> (StatusCode, Value) {
        self.call(
            Method::POST,
            "/shift/open",
            Some(json!({ "count": count(pairs) })),
            t,
        )
        .await
    }

    async fn close(&self, t: &str, pairs: &[(i64, i64)]) -> (StatusCode, Value) {
        self.call(
            Method::POST,
            "/shift/close",
            Some(json!({ "count": count(pairs) })),
            t,
        )
        .await
    }

    async fn transfer(
        &self,
        t: &str,
        direction: &str,
        vault: &str,
        pairs: &[(i64, i64)],
    ) -> (StatusCode, Value) {
        let body = json!({ "vault_id": vault, "direction": direction, "count": count(pairs) });
        self.call(Method::POST, "/transfers", Some(body), t).await
    }

    async fn vault_balance(&self) -> i64 {
        let (status, res) = self
            .call(Method::GET, "/vaults", None, &self.api.owner)
            .await;
        assert_eq!(status, StatusCode::OK, "{res}");
        res["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == self.vault.as_str())
            .unwrap()["balance"]
            .as_i64()
            .unwrap()
    }
}

#[sqlx::test]
async fn open_transfer_close_balances_in_minor_units(pool: PgPool) {
    let t = till(pool).await;
    let (status, res) = t.call(Method::GET, "/shift", None, &t.t1).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["data"], Value::Null, "no shift yet");

    // Float: 2 × €20 + 5 × €1 + 3 × €0.05 = 4515 cents.
    let (status, res) = t.open(&t.t1, &[(2000, 2), (100, 5), (5, 3)]).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["opening"], 4515);
    assert_eq!(res["data"]["expected"], 4515);
    assert_eq!(res["data"]["closed_at"], Value::Null);

    // Drop €20 into the safe.
    let (status, res) = t.transfer(&t.t1, "to_vault", &t.vault, &[(2000, 1)]).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["amount"], 2000);
    assert_eq!(t.vault_balance().await, 2000);
    let (_, res) = t.call(Method::GET, "/shift", None, &t.t1).await;
    assert_eq!(res["data"]["expected"], 2515);

    // Count at close is 5 cents short.
    let (status, res) = t.close(&t.t1, &[(2000, 1), (100, 5), (5, 2)]).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["expected"], 2515);
    assert_eq!(res["data"]["counted"], 2510);
    assert_eq!(res["data"]["difference"], -5);
    assert!(res["data"]["closed_at"].is_string());

    let (_, res) = t.call(Method::GET, "/shift", None, &t.t1).await;
    assert_eq!(res["data"], Value::Null, "closed");

    // Office sees the closed shift.
    let (status, res) = t.call(Method::GET, "/shifts", None, &t.api.owner).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"][0]["difference"], -5);
    assert_eq!(res["meta"]["total"], 1);
}

#[sqlx::test]
async fn one_open_shift_per_register_and_close_needs_one(pool: PgPool) {
    let t = till(pool).await;
    assert_eq!(t.close(&t.t1, &[]).await.0, StatusCode::CONFLICT);
    assert_eq!(
        t.open(&t.t1, &[]).await.0,
        StatusCode::CREATED,
        "empty float is fine"
    );
    let (status, res) = t.open(&t.t1, &[(100, 1)]).await;
    assert_eq!(status, StatusCode::CONFLICT, "{res}");
    // Register 2 is its own drawer.
    assert_eq!(t.open(&t.t2, &[(100, 1)]).await.0, StatusCode::CREATED);
    let (_, res) = t.call(Method::GET, "/shift", None, &t.t2).await;
    assert_eq!(res["data"]["expected"], 100);
    let (_, res) = t.call(Method::GET, "/shift", None, &t.t1).await;
    assert_eq!(
        res["data"]["expected"], 0,
        "register 1 never sees register 2's cash"
    );
}

#[sqlx::test]
async fn transfers_need_an_open_shift_and_enough_cash(pool: PgPool) {
    let t = till(pool).await;
    assert_eq!(
        t.transfer(&t.t1, "to_vault", &t.vault, &[(100, 1)]).await.0,
        StatusCode::CONFLICT,
        "no shift"
    );
    t.open(&t.t1, &[(100, 3)]).await;
    let (status, res) = t.transfer(&t.t1, "to_vault", &t.vault, &[(100, 4)]).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let (status, _) = t.transfer(&t.t1, "from_vault", &t.vault, &[(100, 1)]).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "empty vault");
    assert_eq!(
        t.transfer(&t.t1, "to_vault", &t.vault, &[(100, 3)]).await.0,
        StatusCode::CREATED
    );
    assert_eq!(
        t.transfer(&t.t1, "from_vault", &t.vault, &[(100, 2)])
            .await
            .0,
        StatusCode::CREATED
    );
    assert_eq!(t.vault_balance().await, 100);
    let (_, res) = t.call(Method::GET, "/shift", None, &t.t1).await;
    assert_eq!(res["data"]["expected"], 200);
}

#[sqlx::test]
async fn parallel_drops_never_overdraw_the_drawer(pool: PgPool) {
    let t = till(pool).await;
    t.open(&t.t1, &[(100, 5)]).await;
    let mut tasks = Vec::new();
    for _ in 0..10 {
        let app = t.api.app.clone();
        let uri = format!("/v1/workspace/{}/till/transfers", t.api.ws);
        let body =
            json!({ "vault_id": t.vault, "direction": "to_vault", "count": count(&[(100, 1)]) });
        let token = t.t1.clone();
        tasks.push(tokio::spawn(async move {
            common::send(&app, Method::POST, &uri, Some(body), Some(&token))
                .await
                .0
        }));
    }
    let mut ok = 0;
    for task in tasks {
        if task.await.unwrap() == StatusCode::CREATED {
            ok += 1;
        }
    }
    assert_eq!(ok, 5);
    assert_eq!(t.vault_balance().await, 500);
}

#[sqlx::test]
async fn counts_are_validated(pool: PgPool) {
    let t = till(pool).await;
    for (body, field) in [
        (json!({}), "count"),
        (json!({ "count": "x" }), "count"),
        (
            json!({ "count": [{ "denomination": 0, "count": 1 }] }),
            "count.0.denomination",
        ),
        (
            json!({ "count": [{ "denomination": 5, "count": -1 }] }),
            "count.0.count",
        ),
        (
            json!({ "count": [{ "denomination": 1.5, "count": 1 }] }),
            "count.0.denomination",
        ),
        (
            json!({ "count": [{ "denomination": 5, "count": 1 }, { "denomination": 5, "count": 2 }] }),
            "count.1.denomination",
        ),
    ] {
        let (status, res) = t
            .call(Method::POST, "/shift/open", Some(body.clone()), &t.t1)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert!(res["errors"][field].is_array(), "{body}: {res}");
    }
    let many: Vec<(i64, i64)> = (1..=51).map(|d| (d, 1)).collect();
    let (status, _) = t.open(&t.t1, &many).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "51 denominations");
    t.open(&t.t1, &[(100, 1)]).await;
    let (status, res) = t.transfer(&t.t1, "sideways", &t.vault, &[(100, 1)]).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(res["errors"]["direction"].is_array(), "{res}");
    let (status, _) = t.transfer(&t.t1, "to_vault", &t.vault, &[(100, 0)]).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "nothing to move");
}

#[sqlx::test]
async fn ledger_is_append_only(pool: PgPool) {
    let t = till(pool).await;
    t.open(&t.t1, &[(100, 1)]).await;
    for sql in [
        "UPDATE cash_ledger SET amount = 0",
        "DELETE FROM cash_ledger",
        "TRUNCATE cash_ledger",
    ] {
        let err = sqlx::query(sql).execute(&t.api.pool).await.unwrap_err();
        assert!(err.to_string().contains("append-only"), "{sql}: {err}");
    }
}

#[sqlx::test]
async fn another_workspace_and_its_vaults_are_refused(pool: PgPool) {
    let t = till(pool).await;
    let eve = till_of(&t, "eve@example.com", "evil").await;
    t.open(&t.t1, &[(100, 5)]).await;

    // Acme's session can't drop into evil's vault.
    let (status, res) = t.transfer(&t.t1, "to_vault", &eve.1, &[(100, 1)]).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["vault_id"].is_array(), "{res}");
    // Acme's session under evil's path: 404.
    let uri = format!("/v1/workspace/{}/till/shift", eve.0.ws);
    let (status, _) = common::send(&t.api.app, Method::GET, &uri, None, Some(&t.t1)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // Evil's office sees none of acme's vaults or shifts.
    let (_, vaults) = eve.0.call(Method::GET, "/vaults", None, &eve.0.owner).await;
    assert_eq!(vaults["data"].as_array().unwrap().len(), 1);
    let (_, shifts) = eve.0.call(Method::GET, "/shifts", None, &eve.0.owner).await;
    assert_eq!(shifts["data"].as_array().unwrap().len(), 0);
}

/// Another owner's workspace with one vault: `(api, vault id)`.
async fn till_of(t: &Till, email: &str, ws: &str) -> (Api, String) {
    let eve = t.api.as_new_owner(email, ws).await;
    let (status, res) = eve
        .call(
            Method::POST,
            "/vaults",
            Some(json!({ "name": "Evil safe" })),
            &eve.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let vault = res["data"]["id"].as_str().unwrap().to_owned();
    (eve, vault)
}

#[sqlx::test]
async fn office_cash_routes_need_cashregister_rights(pool: PgPool) {
    let t = till(pool).await;
    let bob = member_with(&t.api.app, &t.api.pool, &t.api.ws, "bob@example.com", &[]).await;
    for (method, path) in [
        (Method::GET, "/vaults"),
        (Method::POST, "/vaults"),
        (Method::GET, "/shifts"),
    ] {
        let (status, _) = t.call(method.clone(), path, Some(json!({})), &bob).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}");
    }
    // Till tokens can't use office routes; member tokens can't use till routes.
    assert_eq!(
        t.call(Method::GET, "/vaults", None, &t.t1).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        t.call(Method::GET, "/shift", None, &t.api.owner).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test]
async fn office_shifts_filter_by_open_register_and_time(pool: PgPool) {
    let t = till(pool).await;
    t.open(&t.t1, &[]).await;
    t.close(&t.t1, &[]).await;
    t.open(&t.t2, &[]).await;
    let list = |q: String| {
        let t = &t;
        async move {
            let (status, res) = t
                .call(Method::GET, &format!("/shifts{q}"), None, &t.api.owner)
                .await;
            (status, res)
        }
    };
    let (_, all) = list(String::new()).await;
    assert_eq!(all["meta"]["total"], 2);
    let (open, closed) = (&all["data"][0], &all["data"][1]);
    assert_eq!(open["closed_at"], Value::Null, "newest first: register 2");

    let (_, res) = list("?open=true".into()).await;
    assert_eq!(res["meta"]["total"], 1);
    assert_eq!(res["data"][0]["id"], open["id"]);
    let (_, res) = list("?open=false".into()).await;
    assert_eq!(res["meta"]["total"], 1);
    assert_eq!(res["data"][0]["id"], closed["id"]);
    let reg = closed["register_id"].as_str().unwrap();
    let (_, res) = list(format!("?register_id={reg}")).await;
    assert_eq!(res["meta"]["total"], 1);
    assert_eq!(res["data"][0]["id"], closed["id"]);

    // `from` inclusive, `to` exclusive, on opened_at.
    let at = open["opened_at"].as_str().unwrap();
    let (_, res) = list(format!("?from={at}")).await;
    assert_eq!(res["meta"]["total"], 1, "from = opened_at includes it");
    let (_, res) = list(format!("?to={at}")).await;
    assert_eq!(res["meta"]["total"], 1, "to = opened_at excludes it");
    assert_eq!(res["data"][0]["id"], closed["id"]);
    let (_, res) = list("?from=2000-01-01T00:00:00Z&to=2000-01-02T00:00:00Z".into()).await;
    assert_eq!(res["meta"]["total"], 0);

    for q in [
        "?from=yesterday",
        "?to=2026-13-01T00:00:00Z",
        "?open=maybe",
        "?register_id=nope",
    ] {
        let (status, res) = list(q.into()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{q}: {res}");
    }

    let eve = till_of(&t, "eve@example.com", "evil").await;
    let (_, res) = eve
        .0
        .call(
            Method::GET,
            &format!("/shifts?open=true&register_id={reg}"),
            None,
            &eve.0.owner,
        )
        .await;
    assert_eq!(res["meta"]["total"], 0, "acme's register under evil");
}

/// The till's live total: the API adds up a count and compares it with the drawer, writing
/// nothing. The till shows only what comes back.
#[sqlx::test]
async fn a_count_quote_totals_and_compares_with_the_drawer(pool: PgPool) {
    let t = till(pool).await;
    let quote = |pairs: &[(i64, i64)]| {
        let body = json!({ "count": count(pairs) });
        let t = &t;
        async move {
            t.call(Method::POST, "/shift/count-quote", Some(body), &t.t1)
                .await
        }
    };
    let (status, res) = quote(&[(2000, 2), (100, 5), (5, 3)]).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(
        res["data"],
        json!({ "total": 4515, "lines": [4000, 500, 15], "expected": null, "difference": null }),
        "no shift yet"
    );

    t.open(&t.t1, &[(2000, 2), (100, 5), (5, 3)]).await;
    let (_, res) = quote(&[(2000, 2), (100, 5), (5, 2)]).await;
    assert_eq!(
        res["data"],
        json!({ "total": 4510, "lines": [4000, 500, 10], "expected": 4515, "difference": -5 })
    );
    // Nothing booked: the shift still expects its float, and close works as before.
    let (_, shift) = t.call(Method::GET, "/shift", None, &t.t1).await;
    assert_eq!(shift["data"]["expected"], 4515);
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM cash_ledger")
        .fetch_one(&t.api.pool)
        .await
        .unwrap();
    assert_eq!(rows, 3, "only the opening count's lines");

    // The same rules as every count.
    let (status, res) = t
        .call(
            Method::POST,
            "/shift/count-quote",
            Some(json!({ "count": [{ "denomination": 0, "count": 1 }] })),
            &t.t1,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(res["errors"]["count.0.denomination"].is_array(), "{res}");
}

/// A typed note or coin goes as decimal text (`value`); the API converts it with the register's
/// decimals. The till never parses money.
#[sqlx::test]
async fn a_typed_value_is_converted_by_the_api(pool: PgPool) {
    let t = till(pool).await;
    let quote = |count: Value| {
        let t = &t;
        async move {
            t.call(
                Method::POST,
                "/shift/count-quote",
                Some(json!({ "count": count })),
                &t.t1,
            )
            .await
        }
    };
    let (status, res) =
        quote(json!([{ "value": "0.50", "count": 3 }, { "value": " 20 ", "count": 1 }])).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["total"], 2150);
    for (count, field) in [
        (json!([{ "value": "0.505", "count": 1 }]), "count.0.value"),
        (json!([{ "value": "0", "count": 1 }]), "count.0.value"),
        (json!([{ "value": "abc", "count": 1 }]), "count.0.value"),
        (json!([{ "count": 1 }]), "count.0.denomination"),
        (
            json!([{ "denomination": 50, "value": "0.50", "count": 1 }]),
            "count.0.value",
        ),
        (
            json!([{ "denomination": 50, "count": 1 }, { "value": "0.50", "count": 2 }]),
            "count.1.value",
        ),
    ] {
        let (status, res) = quote(count.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{count}: {res}");
        assert!(res["errors"][field].is_array(), "{count}: {res}");
    }
    // Opening with a typed value books it in minor units.
    let (status, res) = t
        .call(
            Method::POST,
            "/shift/open",
            Some(json!({ "count": [{ "value": "0.50", "count": 3 }] })),
            &t.t1,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["opening"], 150);
}

#[sqlx::test]
async fn a_count_quote_under_another_workspace_is_a_404(pool: PgPool) {
    let t = till(pool).await;
    let eve = till_of(&t, "eve@example.com", "evil").await;
    let uri = format!("/v1/workspace/{}/till/shift/count-quote", eve.0.ws);
    let body = json!({ "count": count(&[(100, 1)]) });
    let (status, _) = common::send(&t.api.app, Method::POST, &uri, Some(body), Some(&t.t1)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// Shift amounts name the register's currency: ISO code and minor-unit exponent.
#[sqlx::test]
async fn shifts_name_the_register_currency(pool: PgPool) {
    let t = till(pool).await;
    let money = |d: &Value| (d["currency"].clone(), d["decimals"].clone());
    let (status, res) = t.open(&t.t1, &[(100, 5)]).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(money(&res["data"]), (json!("EUR"), json!(2)), "open");
    let (_, res) = t.call(Method::GET, "/shift", None, &t.t1).await;
    assert_eq!(money(&res["data"]), (json!("EUR"), json!(2)), "current");
    let (status, res) = t.close(&t.t1, &[(100, 5)]).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(money(&res["data"]), (json!("EUR"), json!(2)), "close");

    // Register 2 runs on shillings: 0 decimals.
    let (status, tzs) = common::send(
        &t.api.app,
        Method::POST,
        &format!("/v1/workspace/{}/currencies", t.api.ws),
        Some(json!({ "name": "Shilling", "asset_type": "tzs", "decimals": 0, "default_currency": false, "active": true })),
        Some(&t.api.owner),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{tzs}");
    sqlx::query("UPDATE till_registers SET currency_id = $2::uuid WHERE workspace_id = $1::uuid AND name = 'Till 2'")
        .bind(&t.api.ws)
        .bind(tzs["data"]["currency_id"].as_str().unwrap())
        .execute(&t.api.pool)
        .await
        .unwrap();
    let (status, res) = t.open(&t.t2, &[(100, 5)]).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(
        money(&res["data"]),
        (json!("TZS"), json!(0)),
        "open in shillings"
    );

    let (_, res) = t.call(Method::GET, "/shifts", None, &t.api.owner).await;
    assert_eq!(
        money(&res["data"][0]),
        (json!("TZS"), json!(0)),
        "office list"
    );
    assert_eq!(
        money(&res["data"][1]),
        (json!("EUR"), json!(2)),
        "office list"
    );
}
