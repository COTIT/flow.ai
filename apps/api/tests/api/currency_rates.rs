//! /v1/workspace/{ws}/currency-rates: workspace-managed exchange rates (append-only history)
//! and the read-only convert (exact integer minor units, half up once).

use crate::common;

use axum::{
    Router,
    http::{Method, StatusCode},
};
use common::{app_with_mail, member_with, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

const NOT_FOUND: &str = "The requested resource could not be found.";
const UNAUTHORIZED: &str = "This action is unauthorized.";

struct World {
    app: Router,
    pool: PgPool,
    owner: String,
    ws: String,
    eur: String,
    usd: String,
}

/// Ada owns `acme` (seeded EUR default, GBP, USD; all 2 decimals).
async fn world(pool: PgPool) -> World {
    let (app, _) = app_with_mail(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let code = |c: &'static str| {
        let (pool, ws) = (pool.clone(), ws.clone());
        async move {
            sqlx::query_scalar::<_, String>(
                "SELECT id::text FROM currencies WHERE workspace_id = $1::uuid AND asset_type = $2",
            )
            .bind(ws)
            .bind(c)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    let (eur, usd) = (code("eur").await, code("usd").await);
    World {
        app,
        pool,
        owner,
        ws,
        eur,
        usd,
    }
}

impl World {
    async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        token: &str,
    ) -> (StatusCode, Value) {
        let uri = format!("/v1/workspace/{}{path}", self.ws);
        send(&self.app, method, &uri, body, Some(token)).await
    }

    /// New currency in acme; returns its id.
    async fn currency(&self, code: &str, decimals: i64) -> String {
        let body = json!({"name": code, "asset_type": code, "decimals": decimals,
                          "default_currency": false, "active": true});
        let (status, res) = self
            .call(Method::POST, "/currencies", Some(body), &self.owner)
            .await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        res["data"]["currency_id"].as_str().unwrap().to_owned()
    }

    async fn set(&self, from: &str, to: &str, rate: &str, on: &str) -> String {
        let body = json!({"from_currency_id": from, "to_currency_id": to, "rate": rate,
                          "effective_on": on});
        let (status, res) = self
            .call(Method::POST, "/currency-rates", Some(body), &self.owner)
            .await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        res["data"]["currency_rate_id"].as_str().unwrap().to_owned()
    }

    async fn convert(&self, query: &str, token: &str) -> (StatusCode, Value) {
        self.call(
            Method::GET,
            &format!("/currency-rates/convert?{query}"),
            None,
            token,
        )
        .await
    }

    async fn converted(&self, from: &str, to: &str, amount: i64, on: Option<&str>) -> Value {
        let on = on.map_or(String::new(), |d| format!("&on={d}"));
        let q = format!("from_currency_id={from}&to_currency_id={to}&amount_minor={amount}{on}");
        let (status, res) = self.convert(&q, &self.owner).await;
        assert_eq!(status, StatusCode::OK, "{q}: {res}");
        res["data"].clone()
    }
}

// ================= happy path

#[sqlx::test]
async fn set_then_list_newest_first_in_rate_shape(pool: PgPool) {
    let w = world(pool).await;
    let tzs = w.currency("TZS", 0).await;
    let old = w.set(&w.eur, &tzs, "2900", "2026-10-01").await;
    let new = w.set(&w.eur, &tzs, "2950.5", "2026-11-01").await;

    let (status, res) = w.call(Method::GET, "/currency-rates", None, &w.owner).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["meta"]["total"], 2);
    let rows = res["data"].as_array().unwrap();
    assert_eq!(rows[0]["id"], new.as_str());
    assert_eq!(rows[1]["id"], old.as_str());
    let r = &rows[0];
    let keys: Vec<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "created_at",
            "effective_on",
            "from",
            "id",
            "note",
            "rate",
            "to"
        ]
    );
    assert_eq!(r["rate"], "2950.5", "exact decimal text, no float");
    assert_eq!(r["effective_on"], "2026-11-01");
    assert_eq!(
        r["from"],
        json!({"id": w.eur, "code": "EUR", "decimals": 2})
    );
    assert_eq!(r["to"], json!({"id": tzs, "code": "TZS", "decimals": 0}));
    assert_eq!(r["note"], Value::Null);
}

#[sqlx::test]
async fn list_filters_by_pair(pool: PgPool) {
    let w = world(pool).await;
    let tzs = w.currency("TZS", 0).await;
    w.set(&w.eur, &tzs, "2900", "2026-10-01").await;
    let usd_rate = w.set(&w.usd, &tzs, "2700", "2026-10-01").await;

    let path = format!(
        "/currency-rates?from_currency_id={}&to_currency_id={tzs}",
        w.usd
    );
    let (status, res) = w.call(Method::GET, &path, None, &w.owner).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["meta"]["total"], 1);
    assert_eq!(res["data"][0]["id"], usd_rate.as_str());
}

#[sqlx::test]
async fn convert_direct_says_rate_date_and_source(pool: PgPool) {
    let w = world(pool).await;
    let tzs = w.currency("TZS", 0).await;
    w.set(&w.eur, &tzs, "2950.5", "2026-11-01").await;

    // EUR 100.00 at 1 EUR = 2950.5 TZS = TZS 295,050.
    let d = w.converted(&w.eur, &tzs, 10_000, None).await;
    assert_eq!(
        d,
        json!({
            "amount_minor": 10_000,
            "from": {"id": w.eur, "code": "EUR", "decimals": 2},
            "to": {"id": tzs, "code": "TZS", "decimals": 0},
            "converted_minor": 295_050,
            "rate": "2950.5",
            "rate_effective_on": "2026-11-01",
            "direction": "direct",
            "rounding": "half_up",
            "source": "workspace",
        })
    );
}

#[sqlx::test]
async fn convert_inverse_when_only_the_other_direction_is_set(pool: PgPool) {
    let w = world(pool).await;
    let tzs = w.currency("TZS", 0).await;
    w.set(&w.eur, &tzs, "2950.5", "2026-11-01").await;

    let d = w.converted(&tzs, &w.eur, 295_050, None).await;
    assert_eq!(d["converted_minor"], 10_000);
    assert_eq!(d["direction"], "inverse");
    assert_eq!(d["rate"], "2950.5", "the stored rate, not a rounded 1/rate");
    // TZS 1,000 / 2950.5 = EUR 0.33892… -> 34 cents.
    assert_eq!(
        w.converted(&tzs, &w.eur, 1_000, None).await["converted_minor"],
        34
    );
}

#[sqlx::test]
async fn direct_rate_beats_inverse(pool: PgPool) {
    let w = world(pool).await;
    let tzs = w.currency("TZS", 0).await;
    w.set(&w.eur, &tzs, "3000", "2026-11-01").await;
    w.set(&tzs, &w.eur, "0.0004", "2026-10-01").await;

    let d = w.converted(&tzs, &w.eur, 10_000, None).await;
    assert_eq!(d["direction"], "direct");
    assert_eq!(d["converted_minor"], 400, "TZS 10,000 × 0.0004 = EUR 4.00");
}

#[sqlx::test]
async fn latest_rate_on_or_before_the_day_wins(pool: PgPool) {
    let w = world(pool).await;
    let tzs = w.currency("TZS", 0).await;
    w.set(&w.eur, &tzs, "2900", "2026-10-01").await;
    w.set(&w.eur, &tzs, "3000", "2026-11-01").await;

    let mid = w.converted(&w.eur, &tzs, 100, Some("2026-10-15")).await;
    assert_eq!(
        (mid["rate"].clone(), mid["rate_effective_on"].clone()),
        (json!("2900"), json!("2026-10-01"))
    );
    let edge = w.converted(&w.eur, &tzs, 100, Some("2026-11-01")).await;
    assert_eq!(edge["rate"], "3000", "the effective day itself counts");

    // A fix is a newer row for the same day: it wins, the old row stays in the history.
    w.set(&w.eur, &tzs, "3100", "2026-11-01").await;
    let today = w.converted(&w.eur, &tzs, 100, None).await;
    assert_eq!(today["rate"], "3100");
    assert_eq!(today["converted_minor"], 3_100);
    let (_, list) = w.call(Method::GET, "/currency-rates", None, &w.owner).await;
    assert_eq!(list["meta"]["total"], 3);

    let (status, res) = w
        .convert(
            &format!(
                "from_currency_id={}&to_currency_id={tzs}&amount_minor=100&on=2026-09-30",
                w.eur
            ),
            &w.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["to_currency_id"][0],
        "No exchange rate from EUR to TZS on or before 2026-09-30. An admin can set one."
    );
}

#[sqlx::test]
async fn same_currency_is_identity_without_a_rate(pool: PgPool) {
    let w = world(pool).await;
    let d = w.converted(&w.eur, &w.eur, -1_234, None).await;
    assert_eq!(d["converted_minor"], -1_234);
    assert_eq!(d["rate"], "1");
    assert_eq!(d["direction"], "same");
    assert_eq!(d["rate_effective_on"], Value::Null);
}

#[sqlx::test]
async fn rounding_is_half_up_once_and_negatives_mirror(pool: PgPool) {
    let w = world(pool).await;
    let jpy = w.currency("JPY", 0).await;
    w.set(&w.eur, &jpy, "150", "2026-11-01").await;

    for (cents, yen) in [(1, 2), (-1, -2), (3, 5), (0, 0), (10_000, 15_000)] {
        // 0.01 EUR × 150 = 1.5 JPY -> 2; 0.03 × 150 = 4.5 -> 5.
        assert_eq!(
            w.converted(&w.eur, &jpy, cents, None).await["converted_minor"],
            yen,
            "{cents}"
        );
    }
    // 0 -> 2 decimals: JPY 1 / 150 = EUR 0.00666… -> 1 cent; JPY 1 at 0.4999 cents rounds down.
    assert_eq!(
        w.converted(&jpy, &w.eur, 1, None).await["converted_minor"],
        1
    );
    assert_eq!(
        w.converted(&jpy, &w.eur, 74, None).await["converted_minor"],
        49,
        "74/150 = 0.4933 EUR = 49.33 cents"
    );
}

// ================= errors

#[sqlx::test]
async fn no_rate_is_422_and_no_third_currency_hop(pool: PgPool) {
    let w = world(pool).await;
    let tzs = w.currency("TZS", 0).await;
    // EUR→USD and USD→TZS exist; EUR→TZS must not be derived through USD.
    w.set(&w.eur, &w.usd, "1.08", "2026-11-01").await;
    w.set(&w.usd, &tzs, "2700", "2026-11-01").await;

    let q = format!(
        "from_currency_id={}&to_currency_id={tzs}&amount_minor=100&on=2026-11-05",
        w.eur
    );
    let (status, res) = w.convert(&q, &w.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["to_currency_id"][0],
        "No exchange rate from EUR to TZS on or before 2026-11-05. An admin can set one."
    );
}

#[sqlx::test]
async fn create_rules(pool: PgPool) {
    let w = world(pool).await;
    let gone = w.currency("CHF", 2).await;
    let (status, _) = w
        .call(
            Method::DELETE,
            &format!("/currencies/{gone}"),
            None,
            &w.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (eur, usd) = (w.eur.as_str(), w.usd.as_str());
    let unknown = uuid::Uuid::now_v7().to_string();

    for (body, field, message) in [
        (
            json!({}),
            "from_currency_id",
            "The from currency id field is required.",
        ),
        (
            json!({}),
            "to_currency_id",
            "The to currency id field is required.",
        ),
        (json!({}), "rate", "The rate field is required."),
        (
            json!({}),
            "effective_on",
            "The effective on field is required.",
        ),
        (
            json!({"from_currency_id": eur, "to_currency_id": eur, "rate": "1", "effective_on": "2026-11-01"}),
            "to_currency_id",
            "The to currency must differ from the from currency.",
        ),
        (
            json!({"from_currency_id": eur, "to_currency_id": usd, "rate": "0", "effective_on": "2026-11-01"}),
            "rate",
            "The rate must be greater than 0.",
        ),
        (
            json!({"from_currency_id": eur, "to_currency_id": usd, "rate": "-1", "effective_on": "2026-11-01"}),
            "rate",
            "The rate must be greater than 0.",
        ),
        (
            json!({"from_currency_id": eur, "to_currency_id": usd, "rate": "1.123456789", "effective_on": "2026-11-01"}),
            "rate",
            "The rate must have 0-8 decimal places.",
        ),
        (
            json!({"from_currency_id": eur, "to_currency_id": usd, "rate": 1.08, "effective_on": "2999-01-01"}),
            "effective_on",
            "The effective on must not be in the future.",
        ),
        (
            json!({"from_currency_id": eur, "to_currency_id": usd, "rate": "1", "effective_on": "1 Nov"}),
            "effective_on",
            "The effective on must be a date (YYYY-MM-DD).",
        ),
        (
            json!({"from_currency_id": "nope", "to_currency_id": usd, "rate": "1", "effective_on": "2026-11-01"}),
            "from_currency_id",
            "The selected from currency id is invalid.",
        ),
        (
            json!({"from_currency_id": eur, "to_currency_id": unknown, "rate": "1", "effective_on": "2026-11-01"}),
            "to_currency_id",
            "The selected to currency id is invalid.",
        ),
        (
            json!({"from_currency_id": gone, "to_currency_id": usd, "rate": "1", "effective_on": "2026-11-01"}),
            "from_currency_id",
            "The selected from currency id is invalid.",
        ),
        (
            json!({"from_currency_id": eur, "to_currency_id": usd, "rate": "1", "effective_on": "2026-11-01", "note": "x".repeat(501)}),
            "note",
            "The note must not be greater than 500 characters.",
        ),
    ] {
        let (status, res) = w
            .call(
                Method::POST,
                "/currency-rates",
                Some(body.clone()),
                &w.owner,
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {res}");
        assert_eq!(res["errors"][field][0], message, "{body}: {res}");
    }
}

#[sqlx::test]
async fn create_boundaries_pass(pool: PgPool) {
    let w = world(pool).await;
    let today: String = sqlx::query_scalar("SELECT current_date::text")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    for (rate, on) in [("0.00000001", "2000-01-01"), ("1000000000", today.as_str())] {
        let body = json!({"from_currency_id": w.eur, "to_currency_id": w.usd, "rate": rate,
                          "effective_on": on, "note": "x".repeat(500)});
        let (status, res) = w
            .call(Method::POST, "/currency-rates", Some(body), &w.owner)
            .await;
        assert_eq!(status, StatusCode::CREATED, "{rate} {on}: {res}");
    }
    let body = json!({"from_currency_id": w.eur, "to_currency_id": w.usd,
                      "rate": "1000000000.01", "effective_on": "2026-11-01"});
    let (status, res) = w
        .call(Method::POST, "/currency-rates", Some(body), &w.owner)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
}

#[sqlx::test]
async fn convert_query_rules(pool: PgPool) {
    let w = world(pool).await;
    let (eur, usd) = (&w.eur, &w.usd);
    for (q, field, message) in [
        (
            format!("to_currency_id={usd}&amount_minor=1"),
            "from_currency_id",
            "The from currency id field is required.",
        ),
        (
            format!("from_currency_id={eur}&to_currency_id={usd}"),
            "amount_minor",
            "The amount minor field is required.",
        ),
        (
            format!("from_currency_id={eur}&to_currency_id={usd}&amount_minor=1.5"),
            "amount_minor",
            "The amount minor must be a whole number of minor units.",
        ),
        (
            format!("from_currency_id={eur}&to_currency_id={usd}&amount_minor=1&on=nov"),
            "on",
            "The on must be a date (YYYY-MM-DD).",
        ),
        (
            format!("from_currency_id=nope&to_currency_id={usd}&amount_minor=1"),
            "from_currency_id",
            "The selected from currency id is invalid.",
        ),
    ] {
        let (status, res) = w.convert(&q, &w.owner).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{q}: {res}");
        assert_eq!(res["errors"][field][0], message, "{q}: {res}");
    }
}

#[sqlx::test]
async fn convert_overflow_is_422(pool: PgPool) {
    let w = world(pool).await;
    w.set(&w.eur, &w.usd, "1000000000", "2026-11-01").await;
    let q = format!(
        "from_currency_id={}&to_currency_id={}&amount_minor={}",
        w.eur,
        w.usd,
        i64::MAX
    );
    let (status, res) = w.convert(&q, &w.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["amount_minor"][0],
        "The amount minor is too large to convert."
    );
}

#[sqlx::test]
async fn rates_are_append_only(pool: PgPool) {
    let w = world(pool).await;
    let id = w.set(&w.eur, &w.usd, "1.08", "2026-11-01").await;
    for sql in [
        "UPDATE currency_rates SET rate = 2 WHERE id = $1::uuid",
        "DELETE FROM currency_rates WHERE id = $1::uuid",
    ] {
        let err = sqlx::query(sql)
            .bind(&id)
            .execute(&w.pool)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("cannot be changed"),
            "{sql}: {err}"
        );
    }
    let err = sqlx::query("TRUNCATE currency_rates")
        .execute(&w.pool)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("cannot be changed"), "{err}");
}

// ================= access

#[sqlx::test]
async fn setting_needs_currency_update_and_403_comes_before_422(pool: PgPool) {
    let w = world(pool).await;
    let bob = member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &[]).await;
    let (status, res) = w
        .call(Method::POST, "/currency-rates", Some(json!({})), &bob)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{res}");
    assert_eq!(res["message"], UNAUTHORIZED);

    let cara = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "cara@example.com",
        &["currency.update"],
    )
    .await;
    let body = json!({"from_currency_id": w.eur, "to_currency_id": w.usd, "rate": "1.08",
                      "effective_on": "2026-11-01"});
    let (status, res) = w
        .call(Method::POST, "/currency-rates", Some(body), &cara)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
}

#[sqlx::test]
async fn any_member_reads_and_converts(pool: PgPool) {
    let w = world(pool).await;
    w.set(&w.eur, &w.usd, "1.08", "2026-11-01").await;
    let bob = member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &[]).await;

    let (status, res) = w.call(Method::GET, "/currency-rates", None, &bob).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let q = format!(
        "from_currency_id={}&to_currency_id={}&amount_minor=100",
        w.eur, w.usd
    );
    let (status, res) = w.convert(&q, &bob).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["converted_minor"], 108);
}

#[sqlx::test]
async fn other_workspace_is_refused(pool: PgPool) {
    let w = world(pool).await;
    w.set(&w.eur, &w.usd, "1.08", "2026-11-01").await;
    let (eve, eve_ws) = signup(&w.app, "eve@example.com", "evil").await;

    // Eve can't reach acme at all.
    for path in [
        "/currency-rates".to_owned(),
        format!(
            "/currency-rates/convert?from_currency_id={}&to_currency_id={}&amount_minor=1",
            w.eur, w.usd
        ),
    ] {
        let (status, res) = w.call(Method::GET, &path, None, &eve).await;
        assert_eq!(
            (status, res["message"].clone()),
            (StatusCode::NOT_FOUND, json!(NOT_FOUND)),
            "{path}"
        );
    }
    let (status, _) = w
        .call(Method::POST, "/currency-rates", Some(json!({})), &eve)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Acme's currency ids through Eve's own workspace: unknown there.
    let uri = format!("/v1/workspace/{eve_ws}/currency-rates");
    let body = json!({"from_currency_id": w.eur, "to_currency_id": w.usd, "rate": "9",
                      "effective_on": "2026-11-01"});
    let (status, res) = send(&w.app, Method::POST, &uri, Some(body), Some(&eve)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["from_currency_id"][0],
        "The selected from currency id is invalid."
    );
    let q = format!(
        "{uri}/convert?from_currency_id={}&to_currency_id={}&amount_minor=1",
        w.eur, w.usd
    );
    let (status, _) = send(&w.app, Method::GET, &q, None, Some(&eve)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Eve's own pair (same codes) has no rate: acme's rate never leaks across.
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id::text FROM currencies WHERE workspace_id = $1::uuid
         AND asset_type IN ('eur','usd') ORDER BY asset_type",
    )
    .bind(&eve_ws)
    .fetch_all(&w.pool)
    .await
    .unwrap();
    let q = format!(
        "{uri}/convert?from_currency_id={}&to_currency_id={}&amount_minor=1",
        ids[0], ids[1]
    );
    let (status, res) = send(&w.app, Method::GET, &q, None, Some(&eve)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let (_, listed) = send(&w.app, Method::GET, &uri, None, Some(&eve)).await;
    assert_eq!(listed["meta"]["total"], 0);
}

#[sqlx::test]
async fn no_token_is_401(pool: PgPool) {
    let w = world(pool).await;
    let uri = format!("/v1/workspace/{}/currency-rates", w.ws);
    let (status, _) = send(&w.app, Method::GET, &uri, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ================= assistant review card

impl World {
    /// Propose `currency_rates_create` with `body` as `token`; (status, facts or error body).
    async fn propose(&self, token: &str, body: Value) -> (StatusCode, Value) {
        let base = format!("/v1/workspace/{}/assistant", self.ws);
        let (status, res) = send(
            &self.app,
            Method::POST,
            &format!("{base}/sessions"),
            Some(json!({"language": "en"})),
            Some(token),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        let proposal = json!({"session_id": res["data"]["id"], "operation_id": "currency_rates_create",
                              "method": "POST",
                              "path": format!("/v1/workspace/{}/currency-rates", self.ws),
                              "body": body});
        let (status, res) = send(
            &self.app,
            Method::POST,
            &format!("{base}/actions"),
            Some(proposal),
            Some(token),
        )
        .await;
        if status == StatusCode::CREATED {
            let facts = &res["data"]["review_facts"];
            assert_eq!(facts["operation_id"], "currency_rates_create", "{res}");
            (status, facts["facts"].clone())
        } else {
            (status, res)
        }
    }
}

#[sqlx::test]
async fn review_card_shows_pair_old_and_new_rate_and_day(pool: PgPool) {
    let w = world(pool).await;
    let tzs = w.currency("TZS", 0).await;
    let body = |rate: &str, on: &str| {
        json!({"from_currency_id": w.eur, "to_currency_id": tzs, "rate": rate, "effective_on": on,
               "note": "bank board"})
    };

    // First rate for the pair: nothing before.
    let (status, facts) = w.propose(&w.owner, body("2950.50", "2026-11-01")).await;
    assert_eq!(status, StatusCode::CREATED, "{facts}");
    assert_eq!(
        facts,
        json!({
            "kind": "currency_rate",
            "locked": {"label": "EUR → TZS",
                       "from": {"id": w.eur, "name": "EUR", "code": "EUR", "decimals": 2},
                       "to": {"id": tzs, "name": "TZS", "code": "TZS", "decimals": 0},
                       "previous": null},
            "changes": [{"field": "rate", "before": null, "after": "2950.5"},
                        {"field": "effective_on", "before": null, "after": "2026-11-01"}],
            "effective_on": "2026-11-01",
            "note": "bank board",
            "source": "workspace",
            "priced": null,
        })
    );

    // With a rate in force on that day, the card shows it as before.
    w.set(&w.eur, &tzs, "2900", "2026-10-01").await;
    let (_, facts) = w.propose(&w.owner, body("3000", "2026-11-01")).await;
    assert_eq!(
        facts["locked"]["previous"],
        json!({"rate": "2900", "effective_on": "2026-10-01"})
    );
    assert_eq!(
        facts["changes"],
        json!([{"field": "rate", "before": "2900", "after": "3000"},
               {"field": "effective_on", "before": "2026-10-01", "after": "2026-11-01"}])
    );
    // Back-dated before every rate: nothing before again.
    let (_, facts) = w.propose(&w.owner, body("2800", "2026-09-01")).await;
    assert_eq!(facts["locked"]["previous"], Value::Null);
}

#[sqlx::test]
async fn review_card_refuses_before_any_fact(pool: PgPool) {
    let w = world(pool).await;
    let tzs = w.currency("TZS", 0).await;
    let ok = json!({"from_currency_id": w.eur, "to_currency_id": tzs, "rate": "3000",
                    "effective_on": "2026-11-01"});

    // No `currency.update`: 403 before any currency name is read.
    let bob = member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &[]).await;
    let (status, res) = w.propose(&bob, ok.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{res}");

    // The native rules run first: future day, same currency, another workspace's currency.
    let mut future = ok.clone();
    future["effective_on"] = json!("2999-01-01");
    let (status, res) = w.propose(&w.owner, future).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["effective_on"][0],
        "The effective on must not be in the future."
    );
    signup(&w.app, "eve@example.com", "evil").await;
    let other: String = sqlx::query_scalar(
        "SELECT c.id::text FROM currencies c JOIN workspaces ws ON ws.id = c.workspace_id
         WHERE c.workspace_id <> $1::uuid LIMIT 1",
    )
    .bind(&w.ws)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    let mut foreign = ok.clone();
    foreign["to_currency_id"] = json!(other);
    let (status, res) = w.propose(&w.owner, foreign).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["to_currency_id"][0],
        "The selected to currency id is invalid."
    );
}
