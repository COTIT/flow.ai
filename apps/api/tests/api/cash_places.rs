//! Cash places (Flow ERP M4): active place balance (counts never add cash), authorized cash
//! adjustments from the Office (member) and the privileged Till (member + till context, D14),
//! GET /accounting/cash-places. Masterplan §19 L (cash places) and M (Till privilege).

use crate::{
    common::{Api, member_with, send_full},
    till_login::{PIN, login, register, till_user, token, world},
};

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// Accounting roles (mirrors `accounting::dto::ROLES`).
const ROLES: [&str; 26] = [
    "receivables",
    "unbilled_receivables",
    "payables",
    "inventory",
    "grni",
    "revenue",
    "sales_returns",
    "cogs",
    "tax_payable",
    "recoverable_tax",
    "customer_deposits",
    "supplier_advances",
    "bank",
    "cash",
    "provider_clearing",
    "inventory_adjustment",
    "pending_purchase_cost_variance",
    "purchase_return_variance",
    "cash_over_short",
    "realized_fx",
    "unrealized_fx",
    "rounding",
    "opening_equity",
    "retained_earnings",
    "financial_discrepancy",
    "financial_loss",
];

/// A workspace with one register, Mary logged in on it, and a vault.
struct W {
    api: Api,
    reg: String,
    key: String,
    till: String,
    vault: String,
}

async fn w(pool: PgPool) -> W {
    let api = world(pool).await;
    let (reg, key) = register(&api, "Till 1").await;
    let mary = till_user(&api, "Mary", PIN).await;
    let till = token(&api, &key, &mary).await;
    let (s, b) = api
        .call(
            Method::POST,
            "/vaults",
            Some(json!({"name": "Safe"})),
            &api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let vault = b["data"]["id"].as_str().unwrap().to_owned();
    W {
        api,
        reg,
        key,
        till,
        vault,
    }
}

/// Request under `/v1/workspace/{ws}{path}` with a fresh Idempotency-Key unless `key` given.
async fn req(
    w: &W,
    method: Method,
    path: &str,
    body: Option<Value>,
    bearer: Option<&str>,
    till: Option<&str>,
    key: Option<&str>,
) -> (StatusCode, Value) {
    let uri = format!("/v1/workspace/{}{path}", w.api.ws);
    let fresh = Uuid::now_v7().to_string();
    let mut headers = vec![("idempotency-key", key.unwrap_or(&fresh))];
    if let Some(t) = till {
        headers.push(("x-till-session", t));
    }
    let (s, _, b) = send_full(&w.api.app, method, &uri, body, bearer, &headers).await;
    (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
}

/// `{denomination, count}` lines for `cents` as €10 notes plus cents.
fn count(cents: i64) -> Value {
    json!([{"denomination": 1000, "count": cents / 1000}, {"denomination": 1, "count": cents % 1000}])
}

async fn shift(w: &W, action: &str, cents: i64) -> Value {
    let (s, b) = w
        .api
        .call(
            Method::POST,
            &format!("/shift/{action}"),
            Some(json!({"count": count(cents)})),
            &w.till,
        )
        .await;
    assert!(s.is_success(), "{action}: {b}");
    b["data"].clone()
}

async fn places(w: &W, bearer: &str) -> (StatusCode, Value) {
    req(
        w,
        Method::GET,
        "/accounting/cash-places",
        None,
        Some(bearer),
        None,
        None,
    )
    .await
}

/// Active balance of `kind`/`id` from GET cash-places.
async fn balance(w: &W, kind: &str, id: &str) -> i64 {
    let (s, b) = places(w, &w.api.owner).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    b["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["kind"] == kind && p["id"] == id)
        .unwrap_or_else(|| panic!("{kind} {id} missing: {b}"))["balance"]
        .as_i64()
        .unwrap()
}

/// Till review + post of a drawer count; the post's status and body.
async fn till_adjust(w: &W, member: &str, till: &str, counted: i64) -> (StatusCode, Value) {
    let place = json!({"kind": "drawer", "id": w.reg});
    let (s, r) = req(
        w,
        Method::POST,
        "/till/cash-adjustments/review",
        Some(json!({"target_cash_place": place, "counted_amount": counted})),
        Some(member),
        Some(till),
        None,
    )
    .await;
    if s != StatusCode::OK {
        return (s, r);
    }
    let body = json!({"client_ref": Uuid::now_v7(), "payload": {
        "target_cash_place": place, "counted_amount": counted, "count_evidence_ref": "count sheet 1",
        "reason": "Opening float", "review_fingerprint": r["data"]["review_fingerprint"]}});
    req(
        w,
        Method::POST,
        "/till/cash-adjustments",
        Some(body),
        Some(member),
        Some(till),
        None,
    )
    .await
}

fn office_payload(kind: &str, id: &str, counted: &str) -> Value {
    json!({"cash_place": {"kind": kind, "id": id}, "counted_amount": counted,
           "accounting_date": "2027-01-15"})
}

/// Office review; status and body.
async fn office_review(
    w: &W,
    bearer: &str,
    kind: &str,
    id: &str,
    counted: &str,
) -> (StatusCode, Value) {
    req(
        w,
        Method::POST,
        "/accounting/cash-adjustments/review",
        Some(office_payload(kind, id, counted)),
        Some(bearer),
        None,
        None,
    )
    .await
}

/// Office post body for a reviewed adjustment.
fn office_body(kind: &str, id: &str, counted: &str, fingerprint: &Value) -> Value {
    let mut p = office_payload(kind, id, counted);
    p["count_evidence_ref"] = json!("safe count 7");
    p["reason"] = json!("Safe recount");
    p["review_fingerprint"] = fingerprint.clone();
    json!({"client_ref": Uuid::now_v7(), "payload": p})
}

async fn office_post(w: &W, bearer: &str, body: Value, key: Option<&str>) -> (StatusCode, Value) {
    req(
        w,
        Method::POST,
        "/accounting/cash-adjustments",
        Some(body),
        Some(bearer),
        None,
        key,
    )
    .await
}

/// Review then post from the Office as the owner.
async fn office_adjust(w: &W, kind: &str, id: &str, counted: &str) -> (StatusCode, Value) {
    let (s, r) = office_review(w, &w.api.owner, kind, id, counted).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    let body = office_body(kind, id, counted, &r["data"]["review_fingerprint"]);
    office_post(w, &w.api.owner, body, None).await
}

/// Create through an accounting C envelope; the `data`.
async fn acc(w: &W, path: &str, body: Value) -> Value {
    let (s, b) = req(
        w,
        Method::POST,
        &format!("/accounting{path}"),
        Some(json!({"client_ref": Uuid::now_v7(), "payload": body})),
        Some(&w.api.owner),
        None,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{path}: {b}");
    b["data"].clone()
}

/// Configure and activate the book: EUR, UTC, every role on its own account, 2027 open.
/// Returns role → account id.
async fn activate(w: &W) -> std::collections::HashMap<&'static str, String> {
    let mut roles = std::collections::HashMap::new();
    let mut mappings = serde_json::Map::new();
    for (i, role) in ROLES.iter().enumerate() {
        let a = acc(
            w,
            "/accounts",
            json!({"code": format!("R{i:02}"), "name": role, "type": "asset"}),
        )
        .await;
        let id = a["id"].as_str().unwrap().to_owned();
        mappings.insert((*role).into(), json!(id));
        roles.insert(*role, id);
    }
    let eur: Uuid = sqlx::query_scalar(
        "SELECT id FROM currencies WHERE workspace_id = $1::uuid AND asset_type = 'eur'",
    )
    .bind(&w.api.ws)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    let (s, b) = req(
        w,
        Method::GET,
        "/accounting/setup",
        None,
        Some(&w.api.owner),
        None,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = req(
        w,
        Method::PUT,
        "/accounting/setup",
        Some(
            json!({"client_ref": Uuid::now_v7(), "expected_version": b["data"]["version"],
                    "payload": {"functional_currency_id": eur, "timezone": "UTC",
                                "account_mappings": mappings}}),
        ),
        Some(&w.api.owner),
        None,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let year = acc(
        w,
        "/fiscal-years",
        json!({"start_date": "2027-01-01", "end_date": "2028-01-01"}),
    )
    .await;
    acc(
        w,
        "/periods",
        json!({"fiscal_year_id": year["id"], "start_date": "2027-01-01", "end_date": "2027-02-01"}),
    )
    .await;
    // fixture: disposable SQL activation without an opening; `POST /accounting/activate` is proven in activation.rs.
    sqlx::query(
        "UPDATE accounting_settings SET state = 'active', cutover_at = now(), activated_at = now(),
                activated_by = (SELECT created_by_user_id FROM workspaces WHERE id = $1::uuid)
         WHERE workspace_id = $1::uuid",
    )
    .bind(&w.api.ws)
    .execute(&w.api.pool)
    .await
    .unwrap();
    roles
}

async fn n(w: &W, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql)
        .bind(&w.api.ws)
        .fetch_one(&w.api.pool)
        .await
        .unwrap()
}

// §19 L: opening 50 + repeated counts never make 150; an adjustment needs review.
#[sqlx::test]
async fn repeated_counts_never_raise_book_cash(pool: PgPool) {
    let w = w(pool).await;
    for _ in 0..2 {
        assert_eq!(
            shift(&w, "open", 5000).await["expected"],
            5000,
            "physical float"
        );
        shift(&w, "close", 5000).await;
    }
    shift(&w, "open", 5000).await;
    assert_eq!(
        balance(&w, "drawer", &w.reg).await,
        0,
        "counts are observations"
    );

    shift(&w, "close", 5000).await;
    // No open shift: an adjustment may still repair the opening.
    let (s, b) = till_adjust(&w, &w.api.owner, &w.till, 5000).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(b["data"]["expected"], 0);
    assert_eq!(b["data"]["counted"], 5000);
    assert_eq!(b["data"]["difference"], 5000);
    assert_eq!(
        b["data"]["journal_entry_id"],
        Value::Null,
        "setup book: no journal"
    );
    assert_eq!(balance(&w, "drawer", &w.reg).await, 5000);

    let opened = shift(&w, "open", 5000).await;
    assert_eq!(
        opened["expected"], 5000,
        "adjustment stays out of the shift's physical figure"
    );
    assert_eq!(
        balance(&w, "drawer", &w.reg).await,
        5000,
        "another count adds nothing"
    );
    let (s, b) = till_adjust(&w, &w.api.owner, &w.till, 5000).await;
    assert_eq!(
        s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "nothing to adjust: {b}"
    );
    assert_eq!(
        n(
            &w,
            "SELECT count(*) FROM cash_ledger WHERE workspace_id = $1::uuid AND kind = 'adjustment'"
        )
        .await,
        1
    );
}

#[sqlx::test]
async fn adjustment_needs_the_current_review(pool: PgPool) {
    let w = w(pool).await;
    let (s, r) = office_review(&w, &w.api.owner, "vault", &w.vault, "12.34").await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["data"]["expected"], 0);
    assert_eq!(r["data"]["difference"], 1234);
    let stale = json!("00".repeat(32));
    let (s, b) = office_post(
        &w,
        &w.api.owner,
        office_body("vault", &w.vault, "12.34", &stale),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    // Review for another count does not fit this post.
    let (s, b) = office_post(
        &w,
        &w.api.owner,
        office_body("vault", &w.vault, "12.35", &r["data"]["review_fingerprint"]),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    let mut missing = office_body("vault", &w.vault, "12.34", &Value::Null);
    missing["payload"]
        .as_object_mut()
        .unwrap()
        .remove("review_fingerprint");
    let (s, b) = office_post(&w, &w.api.owner, missing, None).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(balance(&w, "vault", &w.vault).await, 0);
    // More decimals than the currency has: refused, nothing rounded.
    let (s, b) = office_review(&w, &w.api.owner, "vault", &w.vault, "1.005").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let (s, b) = office_review(&w, &w.api.owner, "vault", &w.vault, "-1").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
}

#[sqlx::test]
async fn setup_book_moves_the_place_without_a_journal(pool: PgPool) {
    let w = w(pool).await;
    let (s, b) = office_adjust(&w, "vault", &w.vault, "50.00").await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(b["data"]["journal_entry_id"], Value::Null);
    assert_eq!(b["data"]["actor"]["kind"], "member");
    assert_eq!(b["data"]["till_user_id"], Value::Null);
    assert_eq!(balance(&w, "vault", &w.vault).await, 5000);
    // Office may repair a drawer with no shift ever opened.
    let (s, b) = office_adjust(&w, "drawer", &w.reg, "7.00").await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(balance(&w, "drawer", &w.reg).await, 700);
    assert_eq!(
        n(
            &w,
            "SELECT count(*) FROM journal_entries WHERE workspace_id = $1::uuid"
        )
        .await,
        0
    );
    assert_eq!(
        n(
            &w,
            "SELECT count(*) FROM accounting_effects WHERE workspace_id = $1::uuid
               AND source_kind = 'cash_adjustment' AND effect_kind = 'posted'"
        )
        .await,
        2
    );
    // Vault balance feeds the transfer check: the till can now take €50 from the safe.
    shift(&w, "open", 0).await;
    let (s, b) = w
        .api
        .call(
            Method::POST,
            "/transfers",
            Some(json!({"vault_id": w.vault, "direction": "from_vault", "count": count(5000)})),
            &w.till,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
}

#[sqlx::test]
async fn active_book_posts_cash_over_short_once(pool: PgPool) {
    let w = w(pool).await;
    let roles = activate(&w).await;
    let (s, r) = office_review(&w, &w.api.owner, "vault", &w.vault, "50.00").await;
    assert_eq!(s, StatusCode::OK, "{r}");
    let body = office_body("vault", &w.vault, "50.00", &r["data"]["review_fingerprint"]);
    let key = Uuid::now_v7().to_string();
    let (s, first) = office_post(&w, &w.api.owner, body.clone(), Some(&key)).await;
    assert_eq!(s, StatusCode::CREATED, "{first}");
    let entry = first["data"]["journal_entry_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (s, again) = office_post(&w, &w.api.owner, body.clone(), Some(&key)).await;
    assert_eq!(s, StatusCode::CREATED, "{again}");
    assert_eq!(again, first, "replay answers the committed result");
    let mut other = body.clone();
    other["payload"]["reason"] = json!("Different");
    let (s, b) = office_post(&w, &w.api.owner, other, None).await;
    assert_eq!(s, StatusCode::CONFLICT, "same client_ref, other body: {b}");

    let lines: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT account_id::text, debit, credit FROM journal_lines WHERE entry_id = $1::uuid
         ORDER BY line_no",
    )
    .bind(&entry)
    .fetch_all(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(
        lines,
        vec![
            (roles["cash"].clone(), 5000, 0),
            (roles["cash_over_short"].clone(), 0, 5000)
        ]
    );
    assert_eq!(balance(&w, "vault", &w.vault).await, 5000);
    assert_eq!(
        n(
            &w,
            "SELECT count(*) FROM journal_entries WHERE workspace_id = $1::uuid"
        )
        .await,
        1
    );

    // Short by €20: cash_over_short debit, cash credit.
    let (s, b) = office_adjust(&w, "vault", &w.vault, "30.00").await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(b["data"]["difference"], -2000);
    let lines: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT account_id::text, debit, credit FROM journal_lines WHERE entry_id = $1::uuid
         ORDER BY line_no",
    )
    .bind(b["data"]["journal_entry_id"].as_str().unwrap())
    .fetch_all(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(
        lines,
        vec![
            (roles["cash_over_short"].clone(), 2000, 0),
            (roles["cash"].clone(), 0, 2000)
        ]
    );
    assert_eq!(balance(&w, "vault", &w.vault).await, 3000);
}

#[sqlx::test]
async fn active_book_refuses_a_foreign_currency_place(pool: PgPool) {
    let w = w(pool).await;
    activate(&w).await;
    let other: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM currencies WHERE workspace_id = $1::uuid AND asset_type <> 'eur' LIMIT 1",
    )
    .bind(&w.api.ws)
    .fetch_optional(&w.api.pool)
    .await
    .unwrap();
    let usd = match other {
        Some(id) => id,
        None => sqlx::query_scalar(
            "INSERT INTO currencies (workspace_id, name, asset_type, decimals, default_currency, active)
             VALUES ($1::uuid, 'Franc', 'chf', 2, false, true) RETURNING id",
        )
        .bind(&w.api.ws)
        .fetch_one(&w.api.pool)
        .await
        .unwrap(),
    };
    let (s, b) = w
        .api
        .call(
            Method::POST,
            "/vaults",
            Some(json!({"name": "Dollar box", "currency_id": usd})),
            &w.api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let box_id = b["data"]["id"].as_str().unwrap().to_owned();
    let (s, b) = office_review(&w, &w.api.owner, "vault", &box_id, "5.00").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "unsupported_active_erp_operation", "{b}");
}

#[sqlx::test]
async fn places_and_adjustments_stay_in_their_workspace(pool: PgPool) {
    let w = w(pool).await;
    let other = w.api.as_new_owner("eve@example.com", "evil").await;
    // Eve, owner elsewhere, is not a member here.
    let (s, _) = places(&w, &other.owner).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = office_review(&w, &other.owner, "vault", &w.vault, "1.00").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // Eve's own workspace sees none of our places; our vault id is invalid there.
    let (s, _, b) = send_full(
        &w.api.app,
        Method::POST,
        &format!(
            "/v1/workspace/{}/accounting/cash-adjustments/review",
            other.ws
        ),
        Some(office_payload("vault", &w.vault, "1.00")),
        Some(&other.owner),
        &[],
    )
    .await;
    assert_eq!(
        s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        String::from_utf8_lossy(&b)
    );
    let (s, _, b) = send_full(
        &w.api.app,
        Method::GET,
        &format!("/v1/workspace/{}/accounting/cash-places", other.ws),
        None,
        Some(&other.owner),
        &[],
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let b: Value = serde_json::from_slice(&b).unwrap();
    assert_eq!(b["data"], json!([]), "{b}");
}

#[sqlx::test]
async fn ledger_rules_hold_in_the_database(pool: PgPool) {
    let w = w(pool).await;
    shift(&w, "open", 1000).await;
    office_adjust(&w, "vault", &w.vault, "1.00").await;
    assert_eq!(
        n(
            &w,
            "SELECT count(*) FROM cash_ledger WHERE workspace_id = $1::uuid
               AND kind = 'open_count' AND actor_kind = 'till'"
        )
        .await,
        2,
        "count rows are till actor"
    );
    for sql in [
        "UPDATE cash_ledger SET amount = 0",
        "DELETE FROM cash_ledger",
        "UPDATE cash_adjustments SET reason = 'x'",
        "DELETE FROM cash_adjustments",
        "TRUNCATE cash_adjustments",
    ] {
        assert!(
            sqlx::query(sql).execute(&w.api.pool).await.is_err(),
            "{sql}"
        );
    }
    // Each typed CHECK, against a copy of the real adjustment row.
    // (kind, amount, actor_kind, member_user_id, shift_id) overrides on the vault row.
    let copy = |kind: &str, amount: &str, actor: &str, member: &str, shift: &str| {
        format!(
            "INSERT INTO cash_ledger (workspace_id, register_id, shift_id, vault_id, kind, amount,
                 actor_kind, member_user_id, till_user_id, source_kind, source_id, effect_id)
             SELECT workspace_id, register_id, {shift}, vault_id, {kind}, {amount}, {actor},
                    {member}, till_user_id, source_kind, source_id, effect_id
             FROM cash_ledger WHERE kind = 'adjustment'"
        )
    };
    let same = ("kind", "amount", "actor_kind", "member_user_id", "shift_id");
    for (what, (k, a, act, m, sh)) in [
        (
            "member without member id",
            ("kind", "amount", "actor_kind", "NULL", "shift_id"),
        ),
        (
            "receipt not positive",
            (
                "'receipt'",
                "-1",
                "actor_kind",
                "member_user_id",
                "shift_id",
            ),
        ),
        (
            "payment positive",
            ("'payment'", "1", "actor_kind", "member_user_id", "shift_id"),
        ),
        (
            "refund positive",
            ("'refund'", "1", "actor_kind", "member_user_id", "shift_id"),
        ),
        (
            "vault sale",
            (
                "'sale'",
                "amount",
                "actor_kind",
                "member_user_id",
                "shift_id",
            ),
        ),
        (
            "adjustment zero",
            ("kind", "0", "actor_kind", "member_user_id", "shift_id"),
        ),
        (
            "member on a count",
            (
                "'open_count'",
                "amount",
                "actor_kind",
                "member_user_id",
                "shift_id",
            ),
        ),
        (
            "system with member id",
            ("kind", "amount", "'system'", "member_user_id", "shift_id"),
        ),
        (
            "till actor without till user",
            ("kind", "amount", "'till'", "NULL", "shift_id"),
        ),
    ] {
        let sql = copy(k, a, act, m, sh);
        assert!(
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .execute(&w.api.pool)
                .await
                .is_err(),
            "{what}"
        );
    }
    let (k, a, act, m, sh) = same;
    assert!(
        sqlx::query(sqlx::AssertSqlSafe(copy(k, a, act, m, sh)))
            .execute(&w.api.pool)
            .await
            .is_ok(),
        "the copy itself is valid"
    );
}

// §19 M: Till PIN, owner link or PIN reset never authorize money.
#[sqlx::test]
async fn till_adjustment_needs_member_login_with_adjust(pool: PgPool) {
    let w = w(pool).await;
    let owner_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = 'ada@example.com'")
        .fetch_one(&w.api.pool)
        .await
        .unwrap();
    let (s, b) = w
        .api
        .call(
            Method::POST,
            "/users",
            Some(json!({"name": "Ada till", "pin": PIN, "user_id": owner_id})),
            &w.api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let linked = b["data"]["id"].as_str().unwrap().to_owned();
    let linked_till = token(&w.api, &w.key, &linked).await;
    let place = json!({"kind": "drawer", "id": w.reg});
    let review = json!({"target_cash_place": place, "counted_amount": 100});
    let path = "/till/cash-adjustments/review";
    let (s, _) = req(
        &w,
        Method::POST,
        path,
        Some(review.clone()),
        None,
        Some(&linked_till),
        None,
    )
    .await;
    assert_eq!(
        s,
        StatusCode::UNAUTHORIZED,
        "owner-linked PIN login is no member login"
    );
    let (s, _) = req(
        &w,
        Method::POST,
        path,
        Some(review.clone()),
        Some(&linked_till),
        Some(&linked_till),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "till token as member bearer");

    // Manager resets the linked user's PIN: still no adjust right.
    let manager = member_with(
        &w.api.app,
        &w.api.pool,
        &w.api.ws,
        "max@example.com",
        &["cashregister.update"],
    )
    .await;
    let (s, b) = w
        .api
        .call(
            Method::PATCH,
            &format!("/users/{linked}"),
            Some(json!({"pin": "9999"})),
            &manager,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = login(&w.api, &w.key, &linked, "9999").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let reset_till = b["data"]["token"].as_str().unwrap().to_owned();
    let (s, _) = till_adjust(&w, &manager, &reset_till, 100).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "PIN reset grants nothing");
    let refund_only = member_with(
        &w.api.app,
        &w.api.pool,
        &w.api.ws,
        "rex@example.com",
        &["cashregister.refund"],
    )
    .await;
    let (s, _) = till_adjust(&w, &refund_only, &w.till, 100).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "refund is not adjust");

    let adjuster = member_with(
        &w.api.app,
        &w.api.pool,
        &w.api.ws,
        "amy@example.com",
        &["cashregister.adjust"],
    )
    .await;
    let (s, b) = till_adjust(&w, &adjuster, &w.till, 100).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let amy: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = 'amy@example.com'")
        .fetch_one(&w.api.pool)
        .await
        .unwrap();
    assert_eq!(
        b["data"]["actor"],
        json!({"kind": "member", "user_id": amy})
    );
    assert!(
        b["data"]["till_user_id"].is_string(),
        "cashier context kept: {b}"
    );
    assert_eq!(
        n(&w, "SELECT count(*) FROM cash_ledger WHERE workspace_id = $1::uuid AND kind = 'adjustment'
               AND actor_kind = 'member' AND member_user_id IS NOT NULL AND till_user_id IS NOT NULL").await,
        1
    );
}

#[sqlx::test]
async fn till_adjustment_targets_only_its_own_drawer(pool: PgPool) {
    let w = w(pool).await;
    let (other_reg, _) = register(&w.api, "Till 2").await;
    let review = |place: Value| json!({"target_cash_place": place, "counted_amount": 100});
    let path = "/till/cash-adjustments/review";
    let (s, _) = req(
        &w,
        Method::POST,
        path,
        Some(review(json!({"kind": "drawer", "id": other_reg}))),
        Some(&w.api.owner),
        Some(&w.till),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "another register's drawer");
    let (s, _) = req(
        &w,
        Method::POST,
        path,
        Some(review(json!({"kind": "vault", "id": w.vault}))),
        Some(&w.api.owner),
        Some(&w.till),
        None,
    )
    .await;
    assert_eq!(
        s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "vaults are Office-only"
    );

    // A till session of another workspace beside our member: refused.
    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    let eve_api = eve.with_base("/till");
    crate::till_login::store(&eve_api).await;
    let (_, eve_key) = register(&eve_api, "Eve till").await;
    let eve_user = till_user(&eve_api, "Eve", PIN).await;
    let eve_till = token(&eve_api, &eve_key, &eve_user).await;
    let (s, _) = req(
        &w,
        Method::POST,
        path,
        Some(review(json!({"kind": "drawer", "id": w.reg}))),
        Some(&w.api.owner),
        Some(&eve_till),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "dual context across workspaces");
}

#[sqlx::test]
async fn office_routes_need_their_permissions(pool: PgPool) {
    let w = w(pool).await;
    let lister = member_with(
        &w.api.app,
        &w.api.pool,
        &w.api.ws,
        "lea@example.com",
        &["cashregister.list"],
    )
    .await;
    let (s, _) = office_review(&w, &lister, "vault", &w.vault, "1.00").await;
    assert_eq!(s, StatusCode::FORBIDDEN, "adjust needs cashregister.adjust");
    let (s, _) = office_review(&w, &w.till, "vault", &w.vault, "1.00").await;
    assert_eq!(
        s,
        StatusCode::UNAUTHORIZED,
        "till token is no member bearer"
    );
    let adjuster = member_with(
        &w.api.app,
        &w.api.pool,
        &w.api.ws,
        "amy@example.com",
        &["cashregister.adjust"],
    )
    .await;
    let (s, b) = office_review(&w, &adjuster, "vault", &w.vault, "1.00").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, _) = places(&w, &adjuster).await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "cash-places needs accounting.read"
    );
    let reader = member_with(
        &w.api.app,
        &w.api.pool,
        &w.api.ws,
        "rob@example.com",
        &["accounting.read"],
    )
    .await;
    let (s, b) = places(&w, &reader).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(
        b["data"].as_array().unwrap().len(),
        2,
        "one drawer, one vault: {b}"
    );
}
