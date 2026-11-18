//! Canonical settlement (Flow ERP M4 C10): bank accounts, active-book invoice receipts with a
//! money location, allocation-derived paid status, explicit cash rounding.

use axum::http::{Method, StatusCode};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::accounting::world;

#[sqlx::test]
async fn bank_accounts_sit_on_an_asset_account_in_the_book_currency(pool: PgPool) {
    let w = world(pool).await;
    let bank_gl = w.account("1100", "asset").await;
    let made = w
        .create(
            "/bank-accounts",
            json!({"name": "Main", "currency_id": w.eur, "account_id": bank_gl}),
        )
        .await;
    assert_eq!(made["name"], "Main");
    assert_eq!(made["account_id"], bank_gl.as_str());
    assert_eq!(made["active"], true);
    let (s, b) = w.call(Method::GET, "/bank-accounts", None, &w.owner).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"][0]["id"], made["id"]);

    let post = |payload| {
        w.call(
            Method::POST,
            "/bank-accounts",
            Some(json!({"client_ref": Uuid::now_v7(), "payload": payload})),
            &w.owner,
        )
    };
    // Income account, duplicate name, unknown currency, missing fields.
    let (s, b) = post(json!({"name": "Bad", "currency_id": w.eur, "account_id": w.income})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(b["errors"]["payload.account_id"].is_array(), "{b}");
    let (s, b) = post(json!({"name": "Main", "currency_id": w.eur, "account_id": bank_gl})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(b["errors"]["payload.name"].is_array(), "{b}");
    let (s, b) =
        post(json!({"name": "X", "currency_id": Uuid::now_v7(), "account_id": bank_gl})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(b["errors"]["payload.currency_id"].is_array(), "{b}");
    let (s, _) = post(json!({})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn bank_accounts_stay_in_their_workspace(pool: PgPool) {
    let w = world(pool.clone()).await;
    let bank_gl = w.account("1100", "asset").await;
    w.create(
        "/bank-accounts",
        json!({"name": "Main", "currency_id": w.eur, "account_id": bank_gl}),
    )
    .await;
    let (other, other_ws) = crate::common::signup(&w.app, "bob@example.com", "bobco").await;
    let uri = format!("/v1/workspace/{other_ws}/accounting/bank-accounts");
    let (s, _, b) =
        crate::common::send_full(&w.app, Method::GET, &uri, None, Some(&other), &[]).await;
    assert_eq!(s, StatusCode::OK);
    let b: serde_json::Value = serde_json::from_slice(&b).unwrap();
    assert_eq!(b["data"].as_array().unwrap().len(), 0);
    // Bob can't read Ada's book, nor use her GL account in his.
    let (s, _) = w.call(Method::GET, "/bank-accounts", None, &other).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let body = json!({"client_ref": Uuid::now_v7(),
        "payload": {"name": "Steal", "currency_id": w.eur, "account_id": bank_gl}});
    let (s, _, _) = crate::common::send_full(
        &w.app,
        Method::POST,
        &uri,
        Some(body),
        Some(&other),
        &[("idempotency-key", &Uuid::now_v7().to_string())],
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

use crate::accounting::World;
use crate::common::invoicing::{self, F};

/// An invoice of 100.00 (no VAT, no stock) sent in a setup book, then the book configured,
/// a receivable item opened for it (as C9's issue will) and the book activated.
struct Paying {
    _f: F,
    w: World,
    invoice: String,
    receivable: Uuid,
    vault: Uuid,
}

async fn paying(pool: PgPool, open: bool) -> Paying {
    let f = invoicing::setup(pool.clone()).await;
    let invoice = invoicing::draft(
        &f,
        json!([{ "title": "Advice", "quantity": 1, "price": "100", "vat": "0" }]),
        false,
    )
    .await;
    let (s, b) = invoicing::send(&f, &invoice, invoicing::message(&f, "manual")).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let eur: String = sqlx::query_scalar(
        "SELECT id::text FROM currencies WHERE workspace_id = $1::uuid AND asset_type = 'eur'",
    )
    .bind(&f.api.ws)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut w = World {
        app: f.api.app.clone(),
        pool: pool.clone(),
        owner: f.api.owner.clone(),
        ws: f.api.ws.clone(),
        eur,
        roles: Default::default(),
        expense: String::new(),
        income: String::new(),
    };
    w.ready().await;
    let receivable = if open {
        sqlx::query_scalar(
            "WITH e AS (INSERT INTO accounting_effects (workspace_id, source_kind, source_id,
                     effect_kind, fingerprint, created_by)
                 SELECT $1::uuid, 'invoice', $2::uuid, 'issued', decode(repeat('00', 32), 'hex'),
                        created_by_user_id FROM workspaces WHERE id = $1::uuid RETURNING id, created_by)
             INSERT INTO accounting_open_items (workspace_id, classification, side, account_role,
                 account_id, source_kind, source_id, relation_id, invoice_id, currency_id,
                 issue_date, origin, amount, functional_amount, residual, functional_residual,
                 effect_id, created_by)
             SELECT $1::uuid, 'receivable', 'debit', 'receivables', $3::uuid, 'invoice', i.id,
                    i.relation_id, i.id, i.currency_id, '2027-01-05', 'posted', i.total, i.total,
                    i.total, i.total, e.id, e.created_by
             FROM invoices i, e WHERE i.id = $2::uuid RETURNING id",
        )
        .bind(&w.ws)
        .bind(&invoice)
        .bind(&w.roles["receivables"])
        .fetch_one(&pool)
        .await
        .unwrap()
    } else {
        Uuid::nil()
    };
    w.activate().await;
    let vault = sqlx::query_scalar(
        "INSERT INTO cash_vaults (workspace_id, name, currency_id) VALUES ($1::uuid, 'Safe', $2::uuid)
         RETURNING id",
    )
    .bind(&w.ws)
    .bind(&w.eur)
    .fetch_one(&pool)
    .await
    .unwrap();
    Paying {
        _f: f,
        w,
        invoice,
        receivable,
        vault,
    }
}

impl Paying {
    async fn pay(&self, body: serde_json::Value, key: &str) -> (StatusCode, serde_json::Value) {
        let uri = format!(
            "/v1/workspace/{}/invoices/{}/payments",
            self.w.ws, self.invoice
        );
        let (s, _, b) = crate::common::send_full(
            &self.w.app,
            Method::POST,
            &uri,
            Some(body),
            Some(&self.w.owner),
            &[("idempotency-key", key)],
        )
        .await;
        (
            s,
            serde_json::from_slice(&b).unwrap_or(serde_json::Value::Null),
        )
    }

    fn cash(&self, amount: &str) -> serde_json::Value {
        json!({"date": "2027-01-15", "amount": amount, "payment_method": "cash",
               "attribution": {"kind": "cash_place", "cash_place": {"kind": "vault", "id": self.vault}}})
    }

    async fn state(&self) -> (i64, String, i64) {
        sqlx::query_as(
            "SELECT i.paid_total, i.status, o.residual FROM invoices i
             LEFT JOIN accounting_open_items o ON o.id = $2
             WHERE i.id = $1::uuid",
        )
        .bind(&self.invoice)
        .bind(self.receivable)
        .fetch_one(&self.w.pool)
        .await
        .unwrap()
    }

    /// Σ functional debit − credit on the account mapped to `role`.
    async fn gl(&self, role: &str) -> i64 {
        sqlx::query_scalar(
            "SELECT COALESCE(sum(functional_debit - functional_credit), 0)::bigint FROM journal_lines
             WHERE workspace_id = $1::uuid AND account_id = $2::uuid",
        )
        .bind(&self.w.ws)
        .bind(&self.w.roles[role])
        .fetch_one(&self.w.pool)
        .await
        .unwrap()
    }

    async fn vault_balance(&self) -> i64 {
        sqlx::query_scalar(
            "SELECT COALESCE(sum(amount), 0)::bigint FROM cash_ledger WHERE vault_id = $1",
        )
        .bind(self.vault)
        .fetch_one(&self.w.pool)
        .await
        .unwrap()
    }
}

fn key() -> String {
    Uuid::now_v7().to_string()
}

#[sqlx::test]
async fn cash_receipt_moves_place_gl_and_settlement_together(pool: PgPool) {
    // §19 L: Office cash receipt names its vault; place and cash GL rise equally.
    let p = paying(pool, true).await;
    let (s, b) = p.pay(p.cash("40"), &key()).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(p.state().await, (4_000, "partially_paid".into(), 6_000));
    assert_eq!(p.vault_balance().await, 4_000);
    assert_eq!(p.gl("cash").await, 4_000);
    assert_eq!(p.gl("receivables").await, -4_000);
    assert_eq!(p.gl("customer_deposits").await, 0);
    let (s, b) = p.pay(p.cash("60"), &key()).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(p.state().await, (10_000, "paid".into(), 0));
    assert_eq!(p.vault_balance().await, p.gl("cash").await);
    let row: (String, Option<Uuid>, bool) = sqlx::query_as(
        "SELECT location_kind, vault_id, effect_id IS NOT NULL FROM invoice_payments
         WHERE id = $1::uuid",
    )
    .bind(b["data"]["invoice_payment_id"].as_str().unwrap())
    .fetch_one(&p.w.pool)
    .await
    .unwrap();
    assert_eq!(row, ("cash_place".into(), Some(p.vault), true));
}

#[sqlx::test]
async fn overpayment_keeps_the_rest_as_customer_deposit(pool: PgPool) {
    let p = paying(pool, true).await;
    let (s, b) = p.pay(p.cash("120"), &key()).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(p.state().await, (10_000, "paid".into(), 0));
    assert_eq!(p.gl("customer_deposits").await, -2_000);
}

#[sqlx::test]
async fn cash_rounding_is_explicit_and_exact(pool: PgPool) {
    // §19 K: G100/D99.95 with explicit 0.05 settles; 99.98 alone stays 0.02 open.
    let p = paying(pool, true).await;
    let mut body = p.cash("99.98");
    body["cash_rounding"] = json!("0.02");
    let (s, b) = p.pay(body, &key()).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(b["errors"]["cash_rounding"].is_array(), "{b}");
    let (s, _) = p.pay(p.cash("99.98"), &key()).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(p.state().await, (9_998, "partially_paid".into(), 2));
}

#[sqlx::test]
async fn explicit_cash_rounding_settles_the_rounded_due(pool: PgPool) {
    let q = paying(pool, true).await;
    let mut body = q.cash("99.95");
    body["cash_rounding"] = json!("0.05");
    let (s, b) = q.pay(body, &key()).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(q.state().await, (10_000, "paid".into(), 0));
    assert_eq!(q.gl("rounding").await, 5);
    assert_eq!(q.vault_balance().await, 9_995);
}

#[sqlx::test]
async fn noncash_receipts_need_proof_and_the_right_account(pool: PgPool) {
    let p = paying(pool, true).await;
    let bank_gl = p.w.account("1100", "asset").await;
    let bank =
        p.w.create(
            "/bank-accounts",
            json!({"name": "Main", "currency_id": p.w.eur, "account_id": bank_gl}),
        )
        .await;
    let proof = json!({"reference": "TX-1", "evidence_ref": "statement 11",
                       "confirmation_method": "bank_statement"});
    let mut attribution = proof.clone();
    attribution["kind"] = json!("bank_account");
    attribution["bank_account_id"] = bank["id"].clone();
    let body = json!({"date": "2027-01-15", "amount": "30", "payment_method": "pin",
                      "attribution": attribution});
    let (s, b) = p.pay(body, &key()).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let bank_line: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM journal_lines WHERE workspace_id = $1::uuid AND account_id = $2::uuid
         AND bank_account_id = $3::uuid AND functional_debit = 3000",
    )
    .bind(&p.w.ws)
    .bind(&bank_gl)
    .bind(bank["id"].as_str().unwrap())
    .fetch_one(&p.w.pool)
    .await
    .unwrap();
    assert_eq!(bank_line, 1);

    let mut clearing = proof.clone();
    clearing["kind"] = json!("provider_clearing");
    let body = json!({"date": "2027-01-15", "amount": "20", "payment_method": "mollie",
                      "attribution": clearing});
    let (s, b) = p.pay(body, &key()).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(p.gl("provider_clearing").await, 2_000);
    assert_eq!(p.state().await, (5_000, "partially_paid".into(), 5_000));

    // No attribution, wrong pairing, missing proof: refused before any effect.
    for body in [
        json!({"date": "2027-01-15", "amount": "1", "payment_method": "cash"}),
        json!({"date": "2027-01-15", "amount": "1", "payment_method": "cash",
               "attribution": attribution}),
        json!({"date": "2027-01-15", "amount": "1", "payment_method": "pin",
               "attribution": {"kind": "bank_account", "bank_account_id": bank["id"]}}),
    ] {
        let (s, b) = p.pay(body, &key()).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    }
    assert_eq!(p.state().await, (5_000, "partially_paid".into(), 5_000));
}

#[sqlx::test]
async fn active_receipts_replay_forever_and_need_a_key(pool: PgPool) {
    let p = paying(pool, true).await;
    let k = key();
    let (s, first) = p.pay(p.cash("10"), &k).await;
    assert_eq!(s, StatusCode::CREATED, "{first}");
    let (s, again) = p.pay(p.cash("10"), &k).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(again, first);
    let (s, _) = p.pay(p.cash("11"), &k).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(p.vault_balance().await, 1_000);
    let uri = format!("/v1/workspace/{}/invoices/{}/payments", p.w.ws, p.invoice);
    let (s, _, _) = crate::common::send_full(
        &p.w.app,
        Method::POST,
        &uri,
        Some(p.cash("10")),
        Some(&p.w.owner),
        &[],
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn active_books_refuse_invoices_without_a_receivable(pool: PgPool) {
    let p = paying(pool, false).await;
    let (s, b) = p.pay(p.cash("10"), &key()).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "unsupported_active_erp_operation", "{b}");
    assert_eq!(p.vault_balance().await, 0);
}

#[sqlx::test]
async fn active_books_refuse_voiding_a_booked_receipt(pool: PgPool) {
    let q = paying(pool, true).await;
    let (s, b) = q.pay(q.cash("10"), &key()).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let id = b["data"]["invoice_payment_id"].as_str().unwrap().to_owned();
    let uri = format!(
        "/v1/workspace/{}/invoices/{}/payments/{id}",
        q.w.ws, q.invoice
    );
    let (s, _, b) = crate::common::send_full(
        &q.w.app,
        Method::DELETE,
        &uri,
        None,
        Some(&q.w.owner),
        &[("idempotency-key", &key())],
    )
    .await;
    assert_eq!(
        s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        String::from_utf8_lossy(&b)
    );
    assert_eq!(q.state().await, (1_000, "partially_paid".into(), 9_000));
}

#[sqlx::test]
async fn receipts_refuse_another_workspaces_place(pool: PgPool) {
    let p = paying(pool.clone(), true).await;
    let (_, other_ws) = crate::common::signup(&p.w.app, "bob@example.com", "bobco").await;
    let theirs: Uuid = sqlx::query_scalar(
        "INSERT INTO cash_vaults (workspace_id, name, currency_id)
         SELECT $1::uuid, 'Theirs', id FROM currencies WHERE workspace_id = $1::uuid
           AND asset_type = 'eur' RETURNING id",
    )
    .bind(&other_ws)
    .fetch_one(&pool)
    .await
    .unwrap();
    let body = json!({"date": "2027-01-15", "amount": "10", "payment_method": "cash",
        "attribution": {"kind": "cash_place", "cash_place": {"kind": "vault", "id": theirs}}});
    let (s, _) = p.pay(body, &key()).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert_eq!(p.state().await, (0, "pending".into(), 10_000));
}

#[sqlx::test]
async fn setup_books_keep_the_legacy_cash_tolerance(pool: PgPool) {
    let f = invoicing::setup(pool).await;
    let invoice = invoicing::draft(
        &f,
        json!([{ "title": "Advice", "quantity": 1, "price": "100", "vat": "0" }]),
        false,
    )
    .await;
    let (s, _) = invoicing::send(&f, &invoice, invoicing::message(&f, "manual")).await;
    assert_eq!(s, StatusCode::OK);
    let (s, b) = f
        .api
        .call(
            Method::POST,
            &format!("/{invoice}/payments"),
            Some(json!({"date": "2027-01-15", "amount": "99.98", "payment_method": "cash"})),
            &f.api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(invoicing::numbered(&f, &invoice).await.1, "paid");
}

impl Paying {
    /// A Till register of this workspace; with `open`, Mary opens its shift counting 50.00.
    async fn drawer(&self, name: &str, open: bool) -> String {
        use crate::till_login::{PIN, register, till_user, token};
        let till = self._f.api.with_base("/till");
        let (reg, key) = register(&till, name).await;
        if open {
            let mary = till_user(&till, &format!("Mary {name}"), PIN).await;
            let session = token(&till, &key, &mary).await;
            let (s, b) = till
                .call(
                    Method::POST,
                    "/shift/open",
                    Some(json!({"count": [{"denomination": 1000, "count": 5}]})),
                    &session,
                )
                .await;
            assert!(s.is_success(), "open shift: {b}");
        }
        reg
    }

    fn drawer_cash(&self, reg: &str, amount: &str) -> serde_json::Value {
        json!({"date": "2027-01-15", "amount": amount, "payment_method": "cash",
               "attribution": {"kind": "cash_place", "cash_place": {"kind": "drawer", "id": reg}}})
    }

    /// Book cash of the drawer: posted place rows, shift counts excluded.
    async fn drawer_balance(&self, reg: &str) -> i64 {
        sqlx::query_scalar(
            "SELECT COALESCE(sum(amount), 0)::bigint FROM cash_ledger
             WHERE register_id = $1::uuid AND kind NOT IN ('open_count', 'close_count')",
        )
        .bind(reg)
        .fetch_one(&self.w.pool)
        .await
        .unwrap()
    }
}

#[sqlx::test]
async fn drawer_receipt_lands_on_the_open_shift_and_the_cash_account(pool: PgPool) {
    // §19 L: Office cash receipt into a drawer raises place and cash GL by the same 40.00.
    let p = paying(pool, true).await;
    let reg = p.drawer("Till 1", true).await;
    let (s, b) = p.pay(p.drawer_cash(&reg, "40"), &key()).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let payment = b["data"]["invoice_payment_id"].as_str().unwrap().to_owned();
    let row: (String, i64, String, bool) = sqlx::query_as(
        "SELECT l.kind, l.amount, l.actor_kind, l.shift_id = s.id
         FROM cash_ledger l JOIN drawer_shifts s ON s.register_id = l.register_id
              AND s.closed_at IS NULL
         WHERE l.register_id = $1::uuid AND l.source_kind = 'invoice_payment'
           AND l.source_id = $2::uuid",
    )
    .bind(&reg)
    .bind(&payment)
    .fetch_one(&p.w.pool)
    .await
    .unwrap();
    assert_eq!(row, ("receipt".into(), 4_000, "member".into(), true));
    assert_eq!(p.drawer_balance(&reg).await, 4_000);
    assert_eq!(p.gl("cash").await, 4_000);
    // The receipt journal: Dr cash / Cr customer deposits 40.00, then applied to receivables.
    let received: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM journal_entries j JOIN accounting_effects e ON e.id = j.effect_id
         WHERE e.workspace_id = $1::uuid AND e.source_kind = 'invoice_payment'
           AND e.source_id = $2::uuid AND e.effect_kind = 'received' AND j.state = 'posted'",
    )
    .bind(&p.w.ws)
    .bind(&payment)
    .fetch_one(&p.w.pool)
    .await
    .unwrap();
    assert_eq!(received, 1);
    assert_eq!(p.gl("customer_deposits").await, 0);
    assert_eq!(p.state().await, (4_000, "partially_paid".into(), 6_000));
}

#[sqlx::test]
async fn drawer_without_an_open_shift_takes_no_cash(pool: PgPool) {
    let p = paying(pool, true).await;
    let reg = p.drawer("Till 2", false).await;
    let (s, b) = p.pay(p.drawer_cash(&reg, "40"), &key()).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(b["errors"]["attribution.cash_place"].is_array(), "{b}");
    assert_eq!(p.drawer_balance(&reg).await, 0);
    assert_eq!(p.gl("cash").await, 0);
    let effects: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM accounting_effects WHERE workspace_id = $1::uuid
         AND source_kind = 'invoice_payment'",
    )
    .bind(&p.w.ws)
    .fetch_one(&p.w.pool)
    .await
    .unwrap();
    assert_eq!(effects, 0);
    assert_eq!(p.state().await, (0, "pending".into(), 10_000));
}
