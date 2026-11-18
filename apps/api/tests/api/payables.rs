//! Payables (Flow ERP M5-A, masterplan §11/§13/§15, §19 D/E/F): supplier bills matched to
//! booked receipts and landed components under the shared capacity, posting GRNI relief / FX /
//! pending variance / recoverable tax / AP, expense-only bills, duplicate supplier invoice
//! numbers, typed bill reversal, permissions, isolation and replay.

use crate::{
    common,
    inventory_valuation::{assert_books, role_balance, snapshot},
    purchasing::{W, active, code, uuid, world},
};

use axum::http::{Method, StatusCode};
use common::{member_with, send};
use serde_json::{Value, json};
use sqlx::PgPool;

/// Active book (M3 fixture) plus the payables roles and one plain expense account.
pub(crate) async fn book(w: &W) -> String {
    active(w).await;
    for (code, kind, role) in [
        ("2000", "liability", Some("payables")),
        ("1400", "asset", Some("recoverable_tax")),
        ("7900", "expense", Some("realized_fx")),
        ("5100", "expense", Some("pending_purchase_cost_variance")),
        ("5200", "expense", Some("purchase_return_variance")),
        ("6100", "expense", None),
    ] {
        sqlx::query(
            "WITH a AS (INSERT INTO accounts (workspace_id, code, name, type, created_by)
                        SELECT $1::uuid, $2, $2, $3, created_by_user_id
                        FROM workspaces WHERE id = $1::uuid RETURNING id)
             INSERT INTO account_mappings (workspace_id, role, account_id)
             SELECT $1::uuid, $4, id FROM a WHERE $4 IS NOT NULL",
        )
        .bind(&w.api.ws)
        .bind(code)
        .bind(kind)
        .bind(role)
        .execute(w.pool())
        .await
        .unwrap();
    }
    sqlx::query_scalar(
        "SELECT id::text FROM accounts WHERE workspace_id = $1::uuid AND code = '6100'",
    )
    .bind(&w.api.ws)
    .fetch_one(w.pool())
    .await
    .unwrap()
}

impl W {
    pub(crate) async fn usd(&self) -> String {
        let (s, res) = self
            .req(
                Method::POST,
                "/currencies",
                Some(json!({ "name": "USD", "asset_type": "USD", "decimals": 2,
                             "default_currency": false, "active": true })),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{res}");
        res["data"]["currency_id"].as_str().unwrap().to_owned()
    }

    async fn rate(&self, from: &str, rate: &str, days: i32) {
        let (s, res) = self
            .req(
                Method::POST,
                "/currency-rates",
                Some(
                    json!({ "from_currency_id": from, "to_currency_id": self.eur,
                             "rate": rate, "effective_on": self.today(days).await }),
                ),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{res}");
    }

    /// Booked receipt of `q` × `product` at `price` in `currency`, received `days` from today.
    async fn booked(
        &self,
        currency: &str,
        product: &str,
        q: &str,
        price: &str,
        days: i32,
    ) -> Value {
        let mut body = self.order_body(&[(product, q, price)]);
        body["currency_id"] = json!(currency);
        let po = self.draft(body).await;
        let po = self.confirm(&po).await;
        let (s, r) = self
            .receive_on(&po, &self.today(days).await, &[(0, q, &self.shop)])
            .await;
        assert_eq!(s, StatusCode::CREATED, "{r}");
        self.book(&r["data"]).await
    }

    fn bill_body(&self, currency: &str, number: &str, lines: Value) -> Value {
        json!({ "supplier_id": self.supplier, "currency_id": currency,
                "supplier_invoice_number": number, "invoice_date": "2026-11-01",
                "accounting_date": "2026-11-01", "lines": lines })
    }

    async fn bill_create(&self, payload: Value) -> (StatusCode, Value) {
        self.req(
            Method::POST,
            "/supplier-bills",
            Some(json!({ "client_ref": uuid(), "payload": payload })),
        )
        .await
    }

    /// Draft bill; dated today.
    async fn bill(&self, currency: &str, number: &str, lines: Value) -> Value {
        let mut body = self.bill_body(currency, number, lines);
        let today = self.today(0).await;
        body["invoice_date"] = json!(today);
        body["accounting_date"] = json!(today);
        let (s, b) = self.bill_create(body).await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        b["data"].clone()
    }

    async fn act(&self, path: &str, version: &Value, payload: Value) -> (StatusCode, Value) {
        self.req(
            Method::POST,
            path,
            Some(json!({ "client_ref": uuid(), "expected_version": version,
                         "payload": payload })),
        )
        .await
    }

    /// Match bill line `line` to receipt lines (id, quantity).
    async fn matched(
        &self,
        bill: &Value,
        line: usize,
        to: &[(&Value, &str)],
    ) -> (StatusCode, Value) {
        let id = bill["id"].as_str().unwrap();
        let allocations: Vec<Value> = to
            .iter()
            .map(|(rl, q)| {
                json!({ "bill_line_id": bill["lines"][line]["id"], "receipt_line_id": rl,
                        "quantity": q })
            })
            .collect();
        self.act(
            &format!("/supplier-bills/{id}/match"),
            &bill["version"],
            json!({ "allocations": allocations }),
        )
        .await
    }

    /// Review then post; (status, body) of the first refusal or the post.
    async fn post_bill(&self, bill: &Value) -> (StatusCode, Value) {
        let id = bill["id"].as_str().unwrap();
        let (s, review) = self
            .req(
                Method::POST,
                &format!("/supplier-bills/{id}/post/review"),
                None,
            )
            .await;
        if s != StatusCode::OK {
            return (s, review);
        }
        self.act(
            &format!("/supplier-bills/{id}/post"),
            &bill["version"],
            json!({ "review_fingerprint": review["data"]["review_fingerprint"] }),
        )
        .await
    }

    async fn posted(&self, bill: &Value) -> Value {
        let (s, b) = self.post_bill(bill).await;
        assert_eq!(s, StatusCode::OK, "{b}");
        b["data"].clone()
    }

    async fn reverse_bill(&self, bill: &Value) -> (StatusCode, Value) {
        let id = bill["id"].as_str().unwrap();
        let body =
            json!({ "accounting_date": self.today(0).await, "reason": "Wrong supplier invoice" });
        let (s, review) = self
            .req(
                Method::POST,
                &format!("/supplier-bills/{id}/reverse/review"),
                Some(body.clone()),
            )
            .await;
        if s != StatusCode::OK {
            return (s, review);
        }
        let mut payload = body;
        payload["review_fingerprint"] = review["data"]["review_fingerprint"].clone();
        self.act(
            &format!("/supplier-bills/{id}/reverse"),
            &bill["version"],
            payload,
        )
        .await
    }

    /// Open AP item amounts of a bill: (amount, functional, residual).
    async fn ap(&self, bill: &Value) -> (i64, i64, i64) {
        sqlx::query_as(
            "SELECT i.amount, i.functional_amount, i.residual FROM accounting_open_items i
             JOIN supplier_bills b ON b.ap_item_id = i.id WHERE b.id = $1::uuid",
        )
        .bind(bill["id"].as_str().unwrap())
        .fetch_one(self.pool())
        .await
        .unwrap()
    }
}

fn goods(product: &str, q: &str, price: &str) -> Value {
    json!({ "product_id": product, "quantity": q, "unit_price": price })
}

// --- §19 F: GRNI relieved at the receipt rate, FX explicit, partial bills exact -------------

#[sqlx::test]
async fn foreign_bills_relieve_grni_at_receipt_rate_with_explicit_fx(pool: PgPool) {
    let w = world(pool).await;
    book(&w).await;
    let usd = w.usd().await;
    w.rate(&usd, "1.10", -10).await;
    let r = w.booked(&usd, &w.wax, "10", "10.00", -5).await;
    let rl = r["lines"][0]["id"].clone();
    assert_eq!(role_balance(w.pool(), &w.api.ws, "grni").await, -11000);
    w.rate(&usd, "1.20", -2).await;

    // Bill A: 4 of 10 at the receipt price.
    let a = w
        .bill(&usd, "INV-A", json!([goods(&w.wax, "4", "10.00")]))
        .await;
    let (s, a) = w.matched(&a, 0, &[(&rl, "4")]).await;
    assert_eq!(s, StatusCode::OK, "{a}");
    let id = a["data"]["id"].as_str().unwrap();
    let (s, review) = w
        .req(
            Method::POST,
            &format!("/supplier-bills/{id}/post/review"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{review}");
    let m = &review["data"]["lines"][0]["matches"][0];
    assert_eq!(
        (&m["grni_functional"], &m["fx"], &m["variance"]),
        (&json!("44.00"), &json!("4.00"), &json!("0.00"))
    );
    assert_eq!(review["data"]["functional_total"], "48.00");
    let a = w.posted(&a["data"]).await;
    assert_eq!(a["state"], "posted");
    assert_eq!(w.ap(&a).await, (4000, 4800, 4000));
    assert_eq!(role_balance(w.pool(), &w.api.ws, "grni").await, -6600);

    // Bill B: the last 6 take the exact rest of GRNI.
    let b = w
        .bill(&usd, "INV-B", json!([goods(&w.wax, "6", "10.00")]))
        .await;
    let (s, b) = w.matched(&b, 0, &[(&rl, "6")]).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let b = w.posted(&b["data"]).await;
    assert_eq!(w.ap(&b).await, (6000, 7200, 6000));
    assert_eq!(role_balance(w.pool(), &w.api.ws, "grni").await, 0);
    assert_eq!(role_balance(w.pool(), &w.api.ws, "payables").await, -12000);
    assert_eq!(role_balance(w.pool(), &w.api.ws, "realized_fx").await, 1000);
    assert_books(w.pool(), &w.api.ws).await;
}

#[sqlx::test]
async fn price_difference_goes_to_pending_variance(pool: PgPool) {
    let w = world(pool).await;
    book(&w).await;
    let r = w.booked(&w.eur, &w.wax, "3", "10.00", 0).await;
    let rl = r["lines"][0]["id"].clone();
    let bill = w
        .bill(&w.eur, "INV-1", json!([goods(&w.wax, "3", "11.00")]))
        .await;
    let (_, bill) = w.matched(&bill, 0, &[(&rl, "3")]).await;
    w.posted(&bill["data"]).await;
    assert_eq!(role_balance(w.pool(), &w.api.ws, "grni").await, 0);
    assert_eq!(
        role_balance(w.pool(), &w.api.ws, "pending_purchase_cost_variance").await,
        300
    );
    assert_eq!(role_balance(w.pool(), &w.api.ws, "payables").await, -3300);
}

// --- §19 E: shared capacity --------------------------------------------------------------

#[sqlx::test]
async fn matching_never_exceeds_booked_capacity(pool: PgPool) {
    let w = world(pool).await;
    book(&w).await;
    let r = w.booked(&w.eur, &w.wax, "10", "10.00", 0).await;
    let rl = r["lines"][0]["id"].clone();
    let a = w
        .bill(&w.eur, "A", json!([goods(&w.wax, "6", "10.00")]))
        .await;
    let (s, a) = w.matched(&a, 0, &[(&rl, "6")]).await;
    assert_eq!(s, StatusCode::OK, "{a}");
    // Draft A reserves 6: B sees 4.
    let b = w
        .bill(&w.eur, "B", json!([goods(&w.wax, "5", "10.00")]))
        .await;
    let (s, res) = w.matched(&b, 0, &[(&rl, "5")]).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    // A line must be matched in full before posting.
    let (s, res) = w.post_bill(&b).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let a = w.posted(&a["data"]).await;
    // Reversing A releases its 6.
    let (s, res) = w.reverse_bill(&a).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["state"], "reversed");
    assert_eq!(w.ap(&a).await.2, 0, "AP item closed by the reversal");
    assert_eq!(role_balance(w.pool(), &w.api.ws, "grni").await, -10000);
    assert_eq!(role_balance(w.pool(), &w.api.ws, "payables").await, 0);
    let (s, res) = w.matched(&b, 0, &[(&rl, "5")]).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    // Only a posted bill reverses: a draft or an already reversed one is refused.
    let (s, res) = w.reverse_bill(&res["data"]).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "draft: {res}");
    let (s, res) = w.reverse_bill(&a).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "reversed: {res}");
    assert_books(w.pool(), &w.api.ws).await;
}

#[sqlx::test]
async fn concurrent_matches_never_spend_the_same_capacity(pool: PgPool) {
    let w = world(pool).await;
    book(&w).await;
    let r = w.booked(&w.eur, &w.wax, "6", "10.00", 0).await;
    let rl = r["lines"][0]["id"].clone();
    let a = w
        .bill(&w.eur, "A", json!([goods(&w.wax, "6", "10.00")]))
        .await;
    let b = w
        .bill(&w.eur, "B", json!([goods(&w.wax, "6", "10.00")]))
        .await;
    let six = [(&rl, "6")];
    let (x, y) = tokio::join!(w.matched(&a, 0, &six), w.matched(&b, 0, &six));
    let mut codes = [x.0, y.0];
    codes.sort();
    assert_eq!(
        codes,
        [StatusCode::OK, StatusCode::UNPROCESSABLE_ENTITY],
        "{} / {}",
        x.1,
        y.1
    );
}

// --- landed components, expense bills, tax ---------------------------------------------------

#[sqlx::test]
async fn landed_bill_line_clears_landed_grni(pool: PgPool) {
    let w = world(pool).await;
    book(&w).await;
    let po = w.order(&[(&w.wax, "2", "10.00")]).await;
    let r = w.receive(&po, &[(0, "2")]).await;
    let (s, r) = w
        .book_with(
            &r,
            json!([{ "client_component_ref": uuid(), "amount": "7.00", "currency_id": w.eur,
                     "evidence_ref": "DHL-1" }]),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(role_balance(w.pool(), &w.api.ws, "grni").await, -2700);
    let landed = r["data"]["landed_costs"][0]["id"].clone();
    let rl = r["data"]["lines"][0]["id"].clone();
    let bill = w
        .bill(
            &w.eur,
            "DHL-INV",
            json!([goods(&w.wax, "2", "10.00"),
                   { "landed_cost_id": landed, "quantity": "1", "unit_price": "7.50" }]),
        )
        .await;
    // The same component on a second bill: refused.
    let (s, res) = w
        .bill_create(w.bill_body(
            &w.eur,
            "DHL-2",
            json!([{ "landed_cost_id": landed, "quantity": "1", "unit_price": "7.00" }]),
        ))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let (_, bill) = w.matched(&bill, 0, &[(&rl, "2")]).await;
    w.posted(&bill["data"]).await;
    assert_eq!(
        role_balance(w.pool(), &w.api.ws, "grni").await,
        0,
        "no stranded GRNI"
    );
    assert_eq!(
        role_balance(w.pool(), &w.api.ws, "pending_purchase_cost_variance").await,
        50
    );
    assert_eq!(role_balance(w.pool(), &w.api.ws, "payables").await, -2750);
}

async fn tax_profile(w: &W, rate: &str, recoverable: bool) -> String {
    let id: String = sqlx::query_scalar(
        "WITH n AS (SELECT uuidv7() AS id),
              p AS (INSERT INTO tax_profiles (id, workspace_id, lineage_id, country,
                        transaction_types, effective_from, tax_point, created_by, state,
                        evidence_ref, verified_by, verified_at, verify_fingerprint)
                    SELECT n.id, $1::uuid, n.id, 'TZ', ARRAY['purchase'], current_date - 30,
                           'invoice', created_by_user_id, 'draft', 'TRA', NULL, NULL, NULL
                    FROM n, workspaces WHERE workspaces.id = $1::uuid RETURNING id)
         SELECT id::text FROM p",
    )
    .bind(&w.api.ws)
    .fetch_one(w.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO tax_rules (workspace_id, profile_id, line_no, code, name, rate, recoverable)
         VALUES ($1::uuid, $2::uuid, 1, 'VAT', 'VAT', $3::numeric, $4)",
    )
    .bind(&w.api.ws)
    .bind(&id)
    .bind(rate)
    .bind(recoverable)
    .execute(w.pool())
    .await
    .unwrap();
    sqlx::query(
        "UPDATE tax_profiles SET state = 'verified', verified_by = created_by,
                verified_at = now(), verify_fingerprint = sha256('x'::bytea)
         WHERE id = $1::uuid",
    )
    .bind(&id)
    .execute(w.pool())
    .await
    .unwrap();
    id
}

#[sqlx::test]
async fn expense_bill_books_tax_and_expense_without_stock(pool: PgPool) {
    let w = world(pool).await;
    let rent = book(&w).await;
    let vat = tax_profile(&w, "18", true).await;
    let before = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM stock_movements WHERE workspace_id = $1::uuid",
    )
    .bind(&w.api.ws)
    .fetch_one(w.pool())
    .await
    .unwrap();
    let bill = w
        .bill(
            &w.eur,
            "RENT-11",
            json!([{ "expense_account_id": rent, "quantity": "1", "unit_price": "100.00",
                     "tax_profile_id": vat, "description": "Rent November" }]),
        )
        .await;
    let bill = w.posted(&bill).await;
    assert_eq!(bill["totals"]["tax"], "18.00");
    assert_eq!(bill["totals"]["total"], "118.00");
    assert_eq!(w.ap(&bill).await, (11800, 11800, 11800));
    assert_eq!(
        role_balance(w.pool(), &w.api.ws, "recoverable_tax").await,
        1800
    );
    assert_eq!(role_balance(w.pool(), &w.api.ws, "payables").await, -11800);
    let after = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM stock_movements WHERE workspace_id = $1::uuid",
    )
    .bind(&w.api.ws)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(before, after, "a bill never adds stock");
    // Nonrecoverable tax on goods would change booked inventory cost: refused.
    let r = w.booked(&w.eur, &w.wax, "1", "10.00", 0).await;
    let nonrec = tax_profile(&w, "10", false).await;
    let mut line = goods(&w.wax, "1", "10.00");
    line["tax_profile_id"] = json!(nonrec);
    let b = w.bill(&w.eur, "G-1", json!([line])).await;
    let (_, b) = w.matched(&b, 0, &[(&r["lines"][0]["id"], "1")]).await;
    let (s, res) = w.post_bill(&b["data"]).await;
    assert_eq!(
        (s, code(&res)),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsupported_active_erp_operation"
        ),
        "{res}"
    );
}

// --- duplicate supplier invoice numbers --------------------------------------------------------

#[sqlx::test]
async fn duplicate_supplier_invoice_number_is_refused(pool: PgPool) {
    let w = world(pool).await;
    let rent = book(&w).await;
    let line = json!([{ "expense_account_id": rent, "quantity": "1", "unit_price": "5" }]);
    let first = w.bill(&w.eur, "INV-7", line.clone()).await;
    let (s, res) = w
        .bill_create(w.bill_body(&w.eur, " inv-7 ", line.clone()))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(
        res["errors"]["payload.supplier_invoice_number"].is_array(),
        "{res}"
    );
    // Another currency is another document.
    let usd = w.usd().await;
    let (s, res) = w
        .bill_create(w.bill_body(&usd, "INV-7", line.clone()))
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    // Reused only by an explicit correction of the reversed bill.
    let first = w.posted(&first).await;
    let (s, res) = w.reverse_bill(&first).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let (s, res) = w
        .bill_create(w.bill_body(&w.eur, "INV-7", line.clone()))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let mut body = w.bill_body(&w.eur, "INV-7", line);
    body["corrects_bill_id"] = first["id"].clone();
    let (s, res) = w.bill_create(body).await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
}

// --- security, replay, stale ----------------------------------------------------------------

#[sqlx::test]
async fn bills_check_permissions_isolation_replay_and_staleness(pool: PgPool) {
    let w = world(pool).await;
    let rent = book(&w).await;
    let body = w.bill_body(
        &w.eur,
        "X-1",
        json!([{ "expense_account_id": rent, "quantity": "2", "unit_price": "3" }]),
    );
    let cmd = json!({ "client_ref": uuid(), "payload": body });
    let (s, first) = w
        .req(Method::POST, "/supplier-bills", Some(cmd.clone()))
        .await;
    assert_eq!(s, StatusCode::CREATED, "{first}");
    let (s, again) = w.req(Method::POST, "/supplier-bills", Some(cmd)).await;
    assert_eq!(s, StatusCode::CREATED, "replay: {again}");
    assert_eq!(first["data"]["id"], again["data"]["id"]);
    let bill = first["data"].clone();
    let id = bill["id"].as_str().unwrap();

    let bob = member_with(&w.api.app, w.pool(), &w.api.ws, "bob@example.com", &[]).await;
    for (method, path) in [
        (Method::GET, "/supplier-bills".to_owned()),
        (Method::GET, format!("/supplier-bills/{id}")),
        (Method::POST, format!("/supplier-bills/{id}/post/review")),
        (Method::POST, format!("/supplier-bills/{id}/match")),
    ] {
        let (s, b) = w.req_as(&bob, method, &path, Some(json!({}))).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{path}: {b}");
    }
    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    let (s, _) = send(
        &w.api.app,
        Method::GET,
        &format!("/v1/workspace/{}/supplier-bills/{id}", eve.ws),
        None,
        Some(&eve.owner),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Stale fingerprint / version: 409, nothing written.
    let before = snapshot(w.pool(), &w.api.ws).await;
    let (s, res) = w
        .act(
            &format!("/supplier-bills/{id}/post"),
            &bill["version"],
            json!({ "review_fingerprint": "00" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{res}");
    let (s, res) = w
        .act(
            &format!("/supplier-bills/{id}/post"),
            &json!(99),
            json!({ "review_fingerprint": "00" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{res}");
    assert_eq!(snapshot(w.pool(), &w.api.ws).await, before);
    let posted = w.posted(&bill).await;
    let (s, list) = w.req(Method::GET, "/supplier-bills", None).await;
    assert_eq!(s, StatusCode::OK, "{list}");
    assert_eq!(list["data"][0]["id"], posted["id"]);
    // Posted facts are fixed in the database too.
    let err = sqlx::query("UPDATE supplier_bills SET total = 1 WHERE id = $1::uuid")
        .bind(id)
        .execute(w.pool())
        .await;
    assert!(err.is_err());
}

// --- supplier money (§11 SUPPLIER MONEY, §19 F settlement) -----------------------------------

impl W {
    /// Bank account in `currency` on a new asset GL account.
    pub(crate) async fn bank(&self, currency: &str, code: &str) -> String {
        sqlx::query_scalar(
            "WITH a AS (INSERT INTO accounts (workspace_id, code, name, type, created_by)
                        SELECT $1::uuid, $3, $3, 'asset', created_by_user_id
                        FROM workspaces WHERE id = $1::uuid RETURNING id, created_by)
             INSERT INTO bank_accounts (workspace_id, name, currency_id, account_id, created_by)
             SELECT $1::uuid, 'Bank ' || $3, $2::uuid, id, created_by FROM a RETURNING id::text",
        )
        .bind(&self.api.ws)
        .bind(currency)
        .bind(code)
        .fetch_one(self.pool())
        .await
        .unwrap()
    }

    async fn vault(&self) -> String {
        sqlx::query_scalar(
            "INSERT INTO cash_vaults (workspace_id, name, currency_id)
             VALUES ($1::uuid, 'Safe', $2::uuid) RETURNING id::text",
        )
        .bind(&self.api.ws)
        .bind(&self.eur)
        .fetch_one(self.pool())
        .await
        .unwrap()
    }

    async fn vault_balance(&self, vault: &str) -> i64 {
        sqlx::query_scalar(
            "SELECT coalesce(sum(amount), 0)::bigint FROM cash_ledger WHERE vault_id = $1::uuid",
        )
        .bind(vault)
        .fetch_one(self.pool())
        .await
        .unwrap()
    }

    pub(crate) async fn money(&self, currency: &str, direction: &str, amount: &str, location: Value) -> Value {
        json!({ "supplier_id": self.supplier, "currency_id": currency, "direction": direction,
                "amount": amount, "date": self.today(0).await, "location": location,
                "evidence_ref": "Statement 11/7" })
    }

    pub(crate) async fn pay(&self, payload: Value) -> (StatusCode, Value) {
        let (s, review) = self
            .req(
                Method::POST,
                "/supplier-payments/review",
                Some(payload.clone()),
            )
            .await;
        if s != StatusCode::OK {
            return (s, review);
        }
        let mut payload = payload;
        payload["review_fingerprint"] = review["data"]["review_fingerprint"].clone();
        self.req(
            Method::POST,
            "/supplier-payments",
            Some(json!({ "client_ref": uuid(), "payload": payload })),
        )
        .await
    }

    pub(crate) async fn paid(&self, payload: Value) -> Value {
        let (s, b) = self.pay(payload).await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        b["data"].clone()
    }

    async fn allocate(&self, payment: &Value, target: &Value, amount: &str) -> (StatusCode, Value) {
        let id = payment["id"].as_str().unwrap();
        let body = json!({ "target_open_item_id": target, "amount": amount });
        let (s, review) = self
            .req(
                Method::POST,
                &format!("/supplier-payments/{id}/allocate/review"),
                Some(body.clone()),
            )
            .await;
        if s != StatusCode::OK {
            return (s, review);
        }
        let mut payload = body;
        payload["review_fingerprint"] = review["data"]["review_fingerprint"].clone();
        self.act(
            &format!("/supplier-payments/{id}/allocate"),
            &payment["version"],
            payload,
        )
        .await
    }

    pub(crate) async fn reverse_payment(&self, payment: &Value) -> (StatusCode, Value) {
        let id = payment["id"].as_str().unwrap();
        let body = json!({ "accounting_date": self.today(0).await, "reason": "Entered twice" });
        let (s, review) = self
            .req(
                Method::POST,
                &format!("/supplier-payments/{id}/reverse/review"),
                Some(body.clone()),
            )
            .await;
        if s != StatusCode::OK {
            return (s, review);
        }
        let mut payload = body;
        payload["review_fingerprint"] = review["data"]["review_fingerprint"].clone();
        self.act(
            &format!("/supplier-payments/{id}/reverse"),
            &payment["version"],
            payload,
        )
        .await
    }
}

pub(crate) fn bank_location(bank: &str) -> Value {
    json!({ "kind": "bank_account", "bank_account_id": bank, "reference": "TX-1",
            "evidence_ref": "Statement 11", "confirmation_method": "bank_statement" })
}

#[sqlx::test]
async fn foreign_payment_settles_ap_with_realized_fx(pool: PgPool) {
    let w = world(pool).await;
    book(&w).await;
    let usd = w.usd().await;
    w.rate(&usd, "1.10", -10).await;
    let r = w.booked(&usd, &w.wax, "10", "10.00", -5).await;
    w.rate(&usd, "1.20", -2).await;
    let bill = w
        .bill(&usd, "INV-F", json!([goods(&w.wax, "10", "10.00")]))
        .await;
    let (_, bill) = w.matched(&bill, 0, &[(&r["lines"][0]["id"], "10")]).await;
    let bill = w.posted(&bill["data"]).await;
    assert_eq!(w.ap(&bill).await, (10000, 12000, 10000));

    // Paid later at 1.25: USD 100 leaves a USD bank account as 125.00 functional.
    w.rate(&usd, "1.25", -1).await;
    let bank = w.bank(&usd, "1110").await;
    let eur_bank = w.bank(&w.eur, "1120").await;
    let (s, res) = w
        .pay(
            w.money(&usd, "payment", "100.00", bank_location(&eur_bank))
                .await,
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "other currency: {res}");
    let pay = w
        .paid(
            w.money(&usd, "payment", "100.00", bank_location(&bank))
                .await,
        )
        .await;
    assert_eq!(pay["functional_amount"], "125.00");
    assert_eq!(pay["unapplied"], "100.00");
    // Too much, or onto the payment's own kind of item: refused.
    let (s, res) = w.allocate(&pay, &bill["ap_item_id"], "100.01").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let (s, res) = w.allocate(&pay, &pay["open_item_id"], "1").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let (s, res) = w.allocate(&pay, &bill["ap_item_id"], "100.00").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["unapplied"], "0.00");
    assert_eq!(res["data"]["allocations"][0]["fx"], "5.00");
    assert_eq!(w.ap(&bill).await.2, 0);
    assert_eq!(role_balance(w.pool(), &w.api.ws, "payables").await, 0);
    assert_eq!(role_balance(w.pool(), &w.api.ws, "realized_fx").await, 1500);
    let bank_gl: i64 = sqlx::query_scalar(
        "SELECT coalesce(sum(l.functional_debit - l.functional_credit), 0)::bigint
         FROM journal_lines l JOIN bank_accounts b ON b.account_id = l.account_id
         WHERE b.id = $1::uuid",
    )
    .bind(&bank)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(bank_gl, -12500);

    // Settled: neither the bill nor the payment can be reversed now.
    let (s, res) = w.reverse_bill(&bill).await;
    assert_eq!(
        (s, code(&res)),
        (StatusCode::UNPROCESSABLE_ENTITY, "dependent_effects_exist"),
        "{res}"
    );
    let pay = w
        .get(&format!(
            "/supplier-payments/{}",
            pay["id"].as_str().unwrap()
        ))
        .await;
    let (s, res) = w.reverse_payment(&pay).await;
    assert_eq!(
        (s, code(&res)),
        (StatusCode::UNPROCESSABLE_ENTITY, "dependent_effects_exist"),
        "{res}"
    );
    assert_books(w.pool(), &w.api.ws).await;
}

#[sqlx::test]
async fn cash_payment_refund_and_reversal_keep_place_and_book_equal(pool: PgPool) {
    let w = world(pool).await;
    book(&w).await;
    let vault = w.vault().await;
    let cash = json!({ "kind": "cash_place", "cash_place": { "kind": "vault", "id": vault } });
    let (s, res) = w
        .pay({
            let mut p = w.money(&w.eur, "payment", "50.00", cash.clone()).await;
            p["date"] = json!(w.today(-1).await);
            p
        })
        .await;
    assert_eq!(
        s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "business day only: {res}"
    );
    let pay = w
        .paid(w.money(&w.eur, "payment", "50.00", cash.clone()).await)
        .await;
    assert_eq!(w.vault_balance(&vault).await, -5000);
    assert_eq!(role_balance(w.pool(), &w.api.ws, "cash").await, -5000);
    // Supplier sends it back: a refund received, applied to the unapplied payment.
    let refund = w
        .paid(w.money(&w.eur, "refund_received", "50.00", cash).await)
        .await;
    assert_eq!(w.vault_balance(&vault).await, 0);
    let (s, res) = w.allocate(&refund, &pay["open_item_id"], "50.00").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["unapplied"], "0.00");
    assert_eq!(role_balance(w.pool(), &w.api.ws, "payables").await, 0);
    assert_eq!(role_balance(w.pool(), &w.api.ws, "realized_fx").await, 0);

    // A wrong bank record: reversed exactly; twice refused.
    let bank = w.bank(&w.eur, "1120").await;
    let wrong = w
        .paid(
            w.money(&w.eur, "payment", "30.00", bank_location(&bank))
                .await,
        )
        .await;
    let (s, res) = w.reverse_payment(&wrong).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["state"], "reversed");
    assert_eq!(res["data"]["unapplied"], "0.00");
    let (s, res) = w.reverse_payment(&res["data"]).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(role_balance(w.pool(), &w.api.ws, "payables").await, 0);
    let (s, list) = w.req(Method::GET, "/supplier-payments", None).await;
    assert_eq!(s, StatusCode::OK, "{list}");
    assert_eq!(list["meta"]["total"], 3, "{list}");

    let bob = member_with(&w.api.app, w.pool(), &w.api.ws, "bob@example.com", &[]).await;
    let (s, _) = w
        .req_as(
            &bob,
            Method::POST,
            "/supplier-payments/review",
            Some(json!({})),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

// --- purchase returns (§19 D, E) -------------------------------------------------------------

impl W {
    /// Draft return of receipt `r`'s line (q, billed match).
    async fn return_draft(
        &self,
        r: &Value,
        q: &str,
        billed: Option<&Value>,
        apply_credit: bool,
    ) -> (StatusCode, Value) {
        let mut line = json!({ "receipt_line_id": r["lines"][0]["id"], "quantity": q });
        if let Some(m) = billed {
            line["source_bill_allocation_id"] = m.clone();
        }
        self.req(
            Method::POST,
            "/purchase-returns",
            Some(json!({ "client_ref": uuid(), "payload": {
                "receipt_id": r["id"], "purpose": "vendor_return", "reason": "Damaged",
                "accounting_date": self.today(0).await, "apply_credit": apply_credit,
                "lines": [line] } })),
        )
        .await
    }

    async fn post_return(&self, ret: &Value) -> (StatusCode, Value) {
        let id = ret["id"].as_str().unwrap();
        let (s, review) = self
            .req(
                Method::POST,
                &format!("/purchase-returns/{id}/post/review"),
                None,
            )
            .await;
        if s != StatusCode::OK {
            return (s, review);
        }
        self.act(
            &format!("/purchase-returns/{id}/post"),
            &ret["version"],
            json!({ "review_fingerprint": review["data"]["review_fingerprint"] }),
        )
        .await
    }

    async fn returned(&self, r: &Value, q: &str, billed: Option<&Value>, apply: bool) -> Value {
        let (s, ret) = self.return_draft(r, q, billed, apply).await;
        assert_eq!(s, StatusCode::CREATED, "{ret}");
        let (s, ret) = self.post_return(&ret["data"]).await;
        assert_eq!(s, StatusCode::OK, "{ret}");
        ret["data"].clone()
    }

    async fn reverse_return(&self, ret: &Value, both: bool) -> (StatusCode, Value) {
        let id = ret["id"].as_str().unwrap();
        let body = json!({ "accounting_date": self.today(0).await, "reason": "Supplier refused",
                           "reverse_financial": true, "reverse_physical": both });
        let (s, review) = self
            .req(
                Method::POST,
                &format!("/purchase-returns/{id}/reverse/review"),
                Some(body.clone()),
            )
            .await;
        if s != StatusCode::OK {
            return (s, review);
        }
        let mut payload = body;
        payload["review_fingerprint"] = review["data"]["review_fingerprint"].clone();
        self.act(
            &format!("/purchase-returns/{id}/reverse"),
            &ret["version"],
            payload,
        )
        .await
    }
}

/// §19 D set-up: buy 10@10 and 10@20, issue 19 → Q1/V15; (receipt 2).
async fn case_d(w: &W) -> Value {
    w.booked(&w.eur, &w.wax, "10", "10.00", 0).await;
    let r2 = w.booked(&w.eur, &w.wax, "10", "20.00", 0).await;
    let (s, res) = w.till_sell(&w.wax, "19").await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    assert_eq!(
        crate::inventory_valuation::pool_qv(w.pool(), &w.wax, &w.shop).await,
        ("1".into(), 1500)
    );
    r2
}

#[sqlx::test]
async fn unbilled_return_removes_current_average_with_explicit_variance(pool: PgPool) {
    let w = world(pool).await;
    book(&w).await;
    let r2 = case_d(&w).await;
    let grni = role_balance(w.pool(), &w.api.ws, "grni").await;
    let ret = w.returned(&r2, "1", None, false).await;
    assert_eq!(ret["lines"][0]["issue_value"], "15.00");
    assert_eq!(ret["lines"][0]["grni_functional"], "20.00");
    assert_eq!(ret["lines"][0]["variance"], "-5.00");
    assert_eq!(
        crate::inventory_valuation::pool_qv(w.pool(), &w.wax, &w.shop).await,
        ("0".into(), 0)
    );
    assert_eq!(role_balance(w.pool(), &w.api.ws, "grni").await, grni + 2000);
    assert_eq!(
        role_balance(w.pool(), &w.api.ws, "purchase_return_variance").await,
        -500
    );
    assert_books(w.pool(), &w.api.ws).await;
    // Undo: goods back at 15.00, GRNI and capacity restored.
    let (s, res) = w.reverse_return(&ret, false).await;
    assert_eq!(
        (s, code(&res)),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsupported_active_erp_operation"
        ),
        "{res}"
    );
    let (s, res) = w.reverse_return(&ret, true).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["state"], "reversed");
    assert_eq!(
        crate::inventory_valuation::pool_qv(w.pool(), &w.wax, &w.shop).await,
        ("1".into(), 1500)
    );
    assert_eq!(role_balance(w.pool(), &w.api.ws, "grni").await, grni);
    assert_eq!(
        role_balance(w.pool(), &w.api.ws, "purchase_return_variance").await,
        0
    );
    let b = w
        .bill(&w.eur, "R2", json!([goods(&w.wax, "10", "20.00")]))
        .await;
    let (s, res) = w.matched(&b, 0, &[(&r2["lines"][0]["id"], "10")]).await;
    assert_eq!(s, StatusCode::OK, "capacity restored: {res}");
    assert_books(w.pool(), &w.api.ws).await;
}

#[sqlx::test]
async fn billed_return_credits_the_supplier_at_bill_basis(pool: PgPool) {
    let w = world(pool).await;
    book(&w).await;
    let r2 = case_d(&w).await;
    let bill = w
        .bill(&w.eur, "R2", json!([goods(&w.wax, "10", "20.00")]))
        .await;
    let (_, bill) = w.matched(&bill, 0, &[(&r2["lines"][0]["id"], "10")]).await;
    let bill = w.posted(&bill["data"]).await;
    let m = bill["lines"][0]["matches"][0]["id"].clone();
    // An unbilled return of billed goods is refused: the billed side must be named.
    let (s, res) = w.return_draft(&r2, "1", None, false).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let ret = w.returned(&r2, "1", Some(&m), true).await;
    assert_eq!(ret["lines"][0]["credit_nominal"], "20.00");
    assert_eq!(ret["lines"][0]["issue_value"], "15.00");
    assert_eq!(ret["lines"][0]["variance"], "-5.00");
    assert_eq!(
        crate::inventory_valuation::pool_qv(w.pool(), &w.wax, &w.shop).await,
        ("0".into(), 0)
    );
    // Applied to the bill: AP 200 − 20.
    assert_eq!(w.ap(&bill).await.2, 18000);
    assert_eq!(role_balance(w.pool(), &w.api.ws, "payables").await, -18000);
    // Returned goods are never billed again.
    let again = w
        .bill(&w.eur, "R2-B", json!([goods(&w.wax, "1", "20.00")]))
        .await;
    let (s, res) = w.matched(&again, 0, &[(&r2["lines"][0]["id"], "1")]).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    // The bill can't be reversed under a posted return.
    let bill = w
        .get(&format!("/supplier-bills/{}", bill["id"].as_str().unwrap()))
        .await;
    let (s, res) = w.reverse_bill(&bill).await;
    assert_eq!(code(&res), "dependent_effects_exist", "{s}: {res}");
    // Reverse the return: its own application undone, credit closed, AP back to 200.
    let (s, res) = w.reverse_return(&ret, true).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(w.ap(&bill).await.2, 20000);
    assert_eq!(role_balance(w.pool(), &w.api.ws, "payables").await, -20000);
    assert_books(w.pool(), &w.api.ws).await;
}

#[sqlx::test]
async fn unbilled_return_shares_capacity_with_bills(pool: PgPool) {
    let w = world(pool).await;
    book(&w).await;
    let r = w.booked(&w.eur, &w.wax, "10", "10.00", 0).await;
    let rl = r["lines"][0]["id"].clone();
    w.returned(&r, "4", None, false).await;
    let b = w
        .bill(&w.eur, "E", json!([goods(&w.wax, "7", "10.00")]))
        .await;
    let (s, res) = w.matched(&b, 0, &[(&rl, "7")]).await;
    assert_eq!(
        s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "capacity 6, not 10: {res}"
    );
    let b = w
        .bill(&w.eur, "E2", json!([goods(&w.wax, "6", "10.00")]))
        .await;
    let (s, b) = w.matched(&b, 0, &[(&rl, "6")]).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    // A draft reservation is spent capacity too.
    let (s, res) = w.return_draft(&r, "1", None, false).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
}

#[sqlx::test]
async fn concurrent_return_and_bill_never_spend_the_same_capacity(pool: PgPool) {
    let w = world(pool).await;
    book(&w).await;
    let r = w.booked(&w.eur, &w.wax, "6", "10.00", 0).await;
    let rl = r["lines"][0]["id"].clone();
    let (s, ret) = w.return_draft(&r, "6", None, false).await;
    assert_eq!(s, StatusCode::CREATED, "{ret}");
    let b = w
        .bill(&w.eur, "C", json!([goods(&w.wax, "6", "10.00")]))
        .await;
    let six = [(&rl, "6")];
    let (x, y) = tokio::join!(w.post_return(&ret["data"]), w.matched(&b, 0, &six));
    let mut codes = [x.0, y.0];
    codes.sort();
    assert_eq!(
        codes,
        [StatusCode::OK, StatusCode::UNPROCESSABLE_ENTITY],
        "{} / {}",
        x.1,
        y.1
    );
}

// --- purchase cost adjustments (late price: pending variance → inventory/COGS) --------------

impl W {
    async fn adjust(&self, bill: &Value, rl: &Value, inv: &str, cogs: &str) -> (StatusCode, Value) {
        let (s, adj) = self
            .req(
                Method::POST,
                "/purchase-cost-adjustments",
                Some(json!({ "client_ref": uuid(), "payload": {
                    "bill_id": bill["id"], "reason": "Late price", "evidence_ref": "INV-1",
                    "allocations": [{ "receipt_line_id": rl, "inventory_amount": inv,
                                      "cogs_amount": cogs }] } })),
            )
            .await;
        if s != StatusCode::CREATED {
            return (s, adj);
        }
        let adj = adj["data"].clone();
        let id = adj["id"].as_str().unwrap();
        let (s, review) = self
            .req(
                Method::POST,
                &format!("/purchase-cost-adjustments/{id}/post/review"),
                None,
            )
            .await;
        if s != StatusCode::OK {
            return (s, review);
        }
        self.act(
            &format!("/purchase-cost-adjustments/{id}/post"),
            &adj["version"],
            json!({ "review_fingerprint": review["data"]["review_fingerprint"] }),
        )
        .await
    }

    async fn reverse_adjustment(&self, adj: &Value) -> (StatusCode, Value) {
        let id = adj["id"].as_str().unwrap();
        let body = json!({ "accounting_date": self.today(0).await, "reason": "Wrong split" });
        let (s, review) = self
            .req(
                Method::POST,
                &format!("/purchase-cost-adjustments/{id}/reverse/review"),
                Some(body.clone()),
            )
            .await;
        if s != StatusCode::OK {
            return (s, review);
        }
        let mut payload = body;
        payload["review_fingerprint"] = review["data"]["review_fingerprint"].clone();
        self.act(
            &format!("/purchase-cost-adjustments/{id}/reverse"),
            &adj["version"],
            payload,
        )
        .await
    }
}

#[sqlx::test]
async fn late_price_variance_splits_into_inventory_and_cogs(pool: PgPool) {
    use crate::inventory_valuation::pool_qv;
    let w = world(pool).await;
    book(&w).await;
    let r = w.booked(&w.eur, &w.wax, "3", "10.00", 0).await;
    let rl = r["lines"][0]["id"].clone();
    let bill = w
        .bill(&w.eur, "LATE", json!([goods(&w.wax, "3", "11.00")]))
        .await;
    let (_, bill) = w.matched(&bill, 0, &[(&rl, "3")]).await;
    let bill = w.posted(&bill["data"]).await;
    let (s, res) = w.till_sell(&w.wax, "1").await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    assert_eq!(pool_qv(w.pool(), &w.wax, &w.shop).await, ("2".into(), 2000));
    let cogs = role_balance(w.pool(), &w.api.ws, "cogs").await;
    // More than pending, or against its sign: refused.
    let (s, res) = w.adjust(&bill, &rl, "2.01", "1.00").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let (s, res) = w.adjust(&bill, &rl, "-1.00", "0").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let (s, adj) = w.adjust(&bill, &rl, "2.00", "1.00").await;
    assert_eq!(s, StatusCode::OK, "{adj}");
    assert_eq!(adj["data"]["bill_pending_variance"], "0.00");
    assert_eq!(pool_qv(w.pool(), &w.wax, &w.shop).await, ("2".into(), 2200));
    assert_eq!(role_balance(w.pool(), &w.api.ws, "cogs").await, cogs + 100);
    assert_eq!(
        role_balance(w.pool(), &w.api.ws, "pending_purchase_cost_variance").await,
        0
    );
    assert_books(w.pool(), &w.api.ws).await;
    // Exact undo while nothing moved; after a sale it would need a new adjustment.
    let (s, res) = w.reverse_adjustment(&adj["data"]).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(pool_qv(w.pool(), &w.wax, &w.shop).await, ("2".into(), 2000));
    assert_eq!(
        role_balance(w.pool(), &w.api.ws, "pending_purchase_cost_variance").await,
        300
    );
    let (s, adj) = w.adjust(&bill, &rl, "2.00", "1.00").await;
    assert_eq!(s, StatusCode::OK, "{adj}");
    let (s, res) = w.till_sell(&w.wax, "1").await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let (s, res) = w.reverse_adjustment(&adj["data"]).await;
    assert_eq!(
        (s, code(&res)),
        (StatusCode::UNPROCESSABLE_ENTITY, "dependent_effects_exist"),
        "{res}"
    );
    // The bill can't be reversed under a posted adjustment.
    let bill = w
        .get(&format!("/supplier-bills/{}", bill["id"].as_str().unwrap()))
        .await;
    let (_, res) = w.reverse_bill(&bill).await;
    assert_eq!(code(&res), "dependent_effects_exist", "{res}");
    assert_books(w.pool(), &w.api.ws).await;
}
