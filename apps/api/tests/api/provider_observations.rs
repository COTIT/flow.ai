//! Provider observations in active books (Flow ERP M4 C12; masterplan D16, §12, §19 N/X):
//! one immutable observation per confirmed fact, canonical posting or a pending exception,
//! excess outflow to recovery, exception retry/resolve routes, book gate before invoice locks.

use axum::http::{Method, StatusCode};
use genesy_360_api::{gateway::repo::Operation, invoice_gateway as gateway, provider_observations};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::settlement::{Paying, paying};

/// A Globadyme operation row on the paying invoice (`note` makes it a refund of `parent`).
pub(crate) async fn operation(
    p: &Paying,
    note: Option<Uuid>,
    parent: Option<Uuid>,
    amount: i64,
) -> Operation {
    let ws: Uuid = p.w.ws.parse().unwrap();
    let invoice: Uuid = p.invoice.parse().unwrap();
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO payment_accounts (workspace_id,environment,merchant_id,principal_hash) VALUES ($1,'sandbox','merchant-test',decode(repeat('00',32),'hex')) ON CONFLICT DO NOTHING")
        .bind(ws).execute(&p.w.pool).await.unwrap();
    sqlx::query("INSERT INTO order_payments (id,workspace_id,invoice_id,credit_note_id,parent_id,kind,provider,environment,merchant_id,account_revision,request_id,status,amount,currency,decimals,customer_id,country,payer_name,captured_transaction_id) VALUES ($1,$2,$3,$4,$5,$6,'globadyme','sandbox','merchant-test',1,$1,CASE WHEN $4::uuid IS NULL THEN 'paid' ELSE 'refunded' END,$7,'EUR',2,$8,'NL','Client',$9)")
        .bind(id).bind(ws).bind(invoice).bind(note).bind(parent)
        .bind(if note.is_some() { "refund" } else { "payment" }).bind(amount).bind(id.to_string())
        .bind(format!("tx-{id}"))
        .execute(&p.w.pool).await.unwrap();
    genesy_360_api::gateway::repo::find(&p.w.pool, ws, id)
        .await
        .unwrap()
        .unwrap()
}

pub(crate) async fn collect(p: &Paying, op: &Operation) {
    let ws = p.w.ws.parse().unwrap();
    let mut tx = p.w.pool.begin().await.unwrap();
    gateway::apply_collection(
        &p._f.delivery_state,
        &mut tx,
        ws,
        op,
        &format!("tx-{}", op.id),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

async fn scalar(p: &Paying, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql)
        .bind(p.w.ws.parse::<Uuid>().unwrap())
        .fetch_one(&p.w.pool)
        .await
        .unwrap()
}

async fn exception(p: &Paying) -> Value {
    let (s, b) =
        p.w.call(Method::GET, "/exceptions?state=all", None, &p.w.owner)
            .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    b["data"][0].clone()
}

async fn open_receivable(p: &Paying) {
    sqlx::query(
        "WITH e AS (INSERT INTO accounting_effects (workspace_id, source_kind, source_id,
                 effect_kind, fingerprint, created_by)
             SELECT $1::uuid, 'invoice', $2::uuid, 'issued', decode(repeat('00', 32), 'hex'),
                    created_by_user_id FROM workspaces WHERE id = $1::uuid RETURNING id, created_by)
         INSERT INTO accounting_open_items (workspace_id, classification, side, account_role,
             account_id, source_kind, source_id, relation_id, invoice_id, currency_id,
             issue_date, origin, amount, functional_amount, residual, functional_residual,
             effect_id, created_by)
         SELECT $1::uuid, 'receivable', 'debit', 'receivables', $3::uuid, 'invoice', i.id,
                i.relation_id, i.id, i.currency_id, current_date, 'posted', i.total, i.total,
                i.total, i.total, e.id, e.created_by
         FROM invoices i, e WHERE i.id = $2::uuid",
    )
    .bind(&p.w.ws)
    .bind(&p.invoice)
    .bind(&p.w.roles["receivables"])
    .execute(&p.w.pool)
    .await
    .unwrap();
}

/// A customer_credit item of `amount` on the invoice (as C11's credit note will open).
async fn credit_item(p: &Paying, amount: i64) -> Uuid {
    sqlx::query_scalar(
        "WITH e AS (INSERT INTO accounting_effects (workspace_id, source_kind, source_id,
                 effect_kind, fingerprint, created_by)
             SELECT $1::uuid, 'credit_note', $2::uuid, 'issued', decode(repeat('00', 32), 'hex'),
                    created_by_user_id FROM workspaces WHERE id = $1::uuid RETURNING id, created_by)
         INSERT INTO accounting_open_items (workspace_id, classification, side, account_role,
             account_id, source_kind, source_id, relation_id, invoice_id, currency_id,
             issue_date, origin, amount, functional_amount, residual, functional_residual,
             effect_id, created_by)
         SELECT $1::uuid, 'customer_credit', 'credit', 'receivables', $3::uuid, 'credit_note',
                i.id, i.relation_id, i.id, i.currency_id, current_date, 'posted', $4, $4, $4, $4,
                e.id, e.created_by
         FROM invoices i, e WHERE i.id = $2::uuid RETURNING id",
    )
    .bind(&p.w.ws)
    .bind(&p.invoice)
    .bind(&p.w.roles["receivables"])
    .bind(amount)
    .fetch_one(&p.w.pool)
    .await
    .unwrap()
}

/// A paying invoice in an active book whose open period holds today (provider facts post on
/// the day they are confirmed); `open` adds the invoice's receivable item.
pub(crate) async fn book(pool: PgPool, open: bool) -> Paying {
    let p = paying(pool, false).await;
    today_period(&p.w).await;
    if open {
        open_receivable(&p).await;
    }
    p
}

async fn paid(p: &Paying) -> (i64, String) {
    sqlx::query_as("SELECT paid_total, status FROM invoices WHERE id = $1::uuid")
        .bind(&p.invoice)
        .fetch_one(&p.w.pool)
        .await
        .unwrap()
}

/// Activate `api`'s book (roles mapped, EUR, 2027 plus a period holding today): provider
/// facts post on the day they are confirmed.
pub(crate) async fn active_today(api: &crate::common::Api) -> crate::accounting::World {
    let eur: String = sqlx::query_scalar(
        "SELECT id::text FROM currencies WHERE workspace_id = $1::uuid AND asset_type = 'eur'",
    )
    .bind(&api.ws)
    .fetch_one(&api.pool)
    .await
    .unwrap();
    let mut w = crate::accounting::World {
        app: api.app.clone(),
        pool: api.pool.clone(),
        owner: api.owner.clone(),
        ws: api.ws.clone(),
        eur,
        roles: Default::default(),
        expense: String::new(),
        income: String::new(),
    };
    w.ready().await;
    today_period(&w).await;
    w.activate().await;
    w
}

async fn today_period(w: &crate::accounting::World) {
    use chrono::Datelike;
    let today = chrono::Utc::now().date_naive();
    let first = |y, m| chrono::NaiveDate::from_ymd_opt(y, m, 1).unwrap();
    let (y, m) = (today.year(), today.month());
    let next = if m == 12 {
        first(y + 1, 1)
    } else {
        first(y, m + 1)
    };
    if y < 2027 {
        let year = w
            .create(
                "/fiscal-years",
                json!({"start_date": first(y, 1), "end_date": first(y + 1, 1)}),
            )
            .await;
        w.create(
            "/periods",
            json!({"fiscal_year_id": year["id"], "start_date": first(y, m), "end_date": next}),
        )
        .await;
    }
}

fn action(version: i64, payload: Value) -> Value {
    json!({"client_ref": Uuid::now_v7(), "expected_version": version, "payload": payload})
}

/// §19 N: observed 100 without its prerequisite → paid 0 / due 100 / attention; resolving the
/// prerequisite and retrying posts it once → paid 100 / due 0.
#[sqlx::test]
async fn pending_observation_commits_and_retry_posts_it_once(pool: PgPool) {
    let p = book(pool, false).await;
    let op = operation(&p, None, None, 10_000).await;
    collect(&p, &op).await;
    assert_eq!(
        scalar(
            &p,
            "SELECT count(*) FROM provider_observations WHERE workspace_id=$1"
        )
        .await,
        1
    );
    assert_eq!(paid(&p).await.0, 0);
    let e = exception(&p).await;
    assert_eq!(e["kind"], "pending_posting");
    assert_eq!(e["reason_code"], "no_receivable");
    assert_eq!(e["disposition"], "pending_observation");
    assert_eq!(e["observed_amount"], 10_000);
    let attention: bool = sqlx::query_scalar("SELECT attention FROM order_payments WHERE id=$1")
        .bind(op.id)
        .fetch_one(&p.w.pool)
        .await
        .unwrap();
    assert!(attention);
    assert_eq!(scalar(&p, "SELECT count(*) FROM accounting_effects WHERE workspace_id=$1 AND source_kind='invoice_payment'").await, 0);
    // Still missing: retry counts the attempt and refuses.
    let id = e["id"].as_str().unwrap();
    let (s, b) =
        p.w.call(
            Method::POST,
            &format!("/exceptions/{id}/retry"),
            Some(action(1, json!({}))),
            &p.w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(exception(&p).await["attempts"], 2);
    // Prerequisite now present: posts once.
    open_receivable(&p).await;
    let (s, b) =
        p.w.call(
            Method::POST,
            &format!("/exceptions/{id}/retry"),
            Some(action(1, json!({}))),
            &p.w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["state"], "resolved");
    assert_eq!(b["data"]["resolution"], "posted");
    assert_eq!(b["data"]["disposition"], "posted");
    assert_eq!(paid(&p).await, (10_000, "paid".into()));
    assert_eq!(p.gl("provider_clearing").await, 10_000);
    let attention: bool = sqlx::query_scalar("SELECT attention FROM order_payments WHERE id=$1")
        .bind(op.id)
        .fetch_one(&p.w.pool)
        .await
        .unwrap();
    assert!(!attention);
    let (s, _) =
        p.w.call(
            Method::POST,
            &format!("/exceptions/{id}/retry"),
            Some(action(2, json!({}))),
            &p.w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    // The same provider fact again is a no-op.
    collect(&p, &op).await;
    assert_eq!(
        scalar(
            &p,
            "SELECT count(*) FROM provider_observations WHERE workspace_id=$1"
        )
        .await,
        1
    );
    assert_eq!(p.gl("provider_clearing").await, 10_000);
}

/// §19 N: late excess collection becomes unapplied customer funds, never negative due.
#[sqlx::test]
async fn excess_collection_stays_unapplied_customer_funds(pool: PgPool) {
    let p = book(pool, true).await;
    let op = operation(&p, None, None, 12_000).await;
    collect(&p, &op).await;
    assert_eq!(paid(&p).await, (10_000, "paid".into()));
    assert_eq!(scalar(&p, "SELECT residual FROM accounting_open_items WHERE workspace_id=$1 AND classification='receivable'").await, 0);
    assert_eq!(p.gl("customer_deposits").await, -2_000);
    assert_eq!(p.gl("provider_clearing").await, 12_000);
    assert_eq!(
        scalar(
            &p,
            "SELECT count(*) FROM accounting_exceptions WHERE workspace_id=$1"
        )
        .await,
        0
    );
}

/// §19 X: a failure after the first native effect rolls every native effect back; the confirmed
/// observation stays with a pending exception.
#[sqlx::test]
async fn failed_posting_rolls_back_effects_but_keeps_the_observation(pool: PgPool) {
    let p = book(pool, true).await;
    sqlx::query("CREATE FUNCTION c12_boom() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected'; END $$")
        .execute(&p.w.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER c12_boom BEFORE INSERT ON accounting_allocations FOR EACH ROW EXECUTE FUNCTION c12_boom()")
        .execute(&p.w.pool).await.unwrap();
    let op = operation(&p, None, None, 10_000).await;
    collect(&p, &op).await;
    assert_eq!(
        scalar(
            &p,
            "SELECT count(*) FROM provider_observations WHERE workspace_id=$1"
        )
        .await,
        1
    );
    assert_eq!(scalar(&p, "SELECT count(*) FROM accounting_effects WHERE workspace_id=$1 AND source_kind<>'invoice'").await, 0);
    assert_eq!(
        scalar(
            &p,
            "SELECT count(*) FROM journal_entries WHERE workspace_id=$1"
        )
        .await,
        0
    );
    assert_eq!(scalar(&p, "SELECT count(*) FROM accounting_open_items WHERE workspace_id=$1 AND classification='customer_deposit'").await, 0);
    let e = exception(&p).await;
    assert_eq!(
        (e["kind"].as_str(), e["reason_code"].as_str()),
        (Some("pending_posting"), Some("posting_failed"))
    );
    assert_eq!(paid(&p).await.0, 0);
}

/// Excess refund: eligible credit relieved, rest to a recovery item with a discrepancy;
/// reviewed write-off moves it to financial_loss once.
#[sqlx::test]
async fn excess_refund_goes_to_recovery_and_writes_off_once(pool: PgPool) {
    let p = book(pool, true).await;
    let source = operation(&p, None, None, 10_000).await;
    collect(&p, &source).await;
    let credit = credit_item(&p, 3_000).await;
    let note: Uuid = p.invoice.parse().unwrap();
    let refund = operation(&p, Some(note), Some(source.id), 5_000).await;
    let ws = p.w.ws.parse().unwrap();
    let mut tx = p.w.pool.begin().await.unwrap();
    gateway::apply_refund(&p._f.delivery_state, &mut tx, ws, &refund, "r-1")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let residual: i64 =
        sqlx::query_scalar("SELECT residual FROM accounting_open_items WHERE id=$1")
            .bind(credit)
            .fetch_one(&p.w.pool)
            .await
            .unwrap();
    assert_eq!(residual, 0);
    assert_eq!(p.gl("financial_discrepancy").await, 2_000);
    assert_eq!(p.gl("provider_clearing").await, 10_000 - 5_000);
    let e = exception(&p).await;
    assert_eq!(
        (e["kind"].as_str(), e["reason_code"].as_str()),
        (Some("discrepancy"), Some("excess_refund"))
    );
    assert_eq!(e["recovery_residual"], 2_000);
    let id = e["id"].as_str().unwrap();
    // A discrepancy is resolved, not retried; a pending posting is retried, not resolved.
    let (s, _) =
        p.w.call(
            Method::POST,
            &format!("/exceptions/{id}/retry"),
            Some(action(1, json!({}))),
            &p.w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let body =
        json!({"resolution": "write_off", "reason": "Card scheme loss", "evidence_ref": "case-7"});
    let (s, r) =
        p.w.call(
            Method::POST,
            &format!("/exceptions/{id}/resolve/review"),
            Some(body.clone()),
            &p.w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["data"]["amount"], "20.00");
    let mut payload = body.clone();
    payload["review_fingerprint"] = json!("stale");
    let (s, _) =
        p.w.call(
            Method::POST,
            &format!("/exceptions/{id}/resolve"),
            Some(action(1, payload)),
            &p.w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let mut payload = body;
    payload["review_fingerprint"] = r["data"]["review_fingerprint"].clone();
    let cmd = action(1, payload);
    let key = Uuid::now_v7().to_string();
    let (s, first) =
        p.w.raw(
            Method::POST,
            &format!("/exceptions/{id}/resolve"),
            Some(cmd.clone()),
            &p.w.owner,
            &key,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, again) =
        p.w.raw(
            Method::POST,
            &format!("/exceptions/{id}/resolve"),
            Some(cmd),
            &p.w.owner,
            &key,
        )
        .await;
    assert_eq!((s, &again), (StatusCode::OK, &first));
    assert_eq!(p.gl("financial_loss").await, 2_000);
    assert_eq!(p.gl("financial_discrepancy").await, 0);
    let e = exception(&p).await;
    assert_eq!(
        (e["state"].as_str(), e["resolution"].as_str()),
        (Some("resolved"), Some("write_off"))
    );
    assert_eq!(e["recovery_residual"], 0);
}

/// Chargeback with no open credit: full outflow to recovery, assigned to the customer as a
/// receivable; the same fact twice is one observation.
#[sqlx::test]
async fn chargeback_recovery_is_assigned_to_a_customer_receivable(pool: PgPool) {
    let p = book(pool, true).await;
    let source = operation(&p, None, None, 10_000).await;
    collect(&p, &source).await;
    let ws = p.w.ws.parse().unwrap();
    for _ in 0..2 {
        let mut tx = p.w.pool.begin().await.unwrap();
        provider_observations::ingest(&mut tx, ws, &source, "chargeback", "cb-tx", 10_000)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }
    assert_eq!(scalar(&p, "SELECT count(*) FROM provider_observations WHERE workspace_id=$1 AND economic_kind='chargeback'").await, 1);
    assert_eq!(p.gl("financial_discrepancy").await, 10_000);
    let e = exception(&p).await;
    assert_eq!(e["reason_code"], "chargeback");
    let id = e["id"].as_str().unwrap();
    let body = json!({"resolution": "assign_receivable", "reason": "Customer disputes", "evidence_ref": "cb-1"});
    let (s, r) =
        p.w.call(
            Method::POST,
            &format!("/exceptions/{id}/resolve/review"),
            Some(body.clone()),
            &p.w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    let mut payload = body;
    payload["review_fingerprint"] = r["data"]["review_fingerprint"].clone();
    let (s, b) =
        p.w.call(
            Method::POST,
            &format!("/exceptions/{id}/resolve"),
            Some(action(1, payload)),
            &p.w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(p.gl("financial_discrepancy").await, 0);
    assert_eq!(scalar(&p, "SELECT residual FROM accounting_open_items WHERE workspace_id=$1 AND source_kind='accounting_exception' AND classification='receivable'").await, 10_000);
}

/// Web-shop collection before any invoice: unapplied customer funds keyed by order (guest:
/// no customer); its refund relieves them.
#[sqlx::test]
async fn shop_collection_is_an_order_deposit_and_its_refund_relieves_it(pool: PgPool) {
    let p = book(pool, true).await;
    let ws: Uuid = p.w.ws.parse().unwrap();
    let order: Uuid = sqlx::query_scalar(
        "INSERT INTO orders (workspace_id, number, source, status, currency_id, warehouse_id)
         SELECT $1, 'W-1', 'storefront', 'open', $2::uuid, id FROM warehouses
         WHERE workspace_id = $1 ORDER BY id LIMIT 1 RETURNING id",
    )
    .bind(ws)
    .bind(&p.w.eur)
    .fetch_one(&p.w.pool)
    .await
    .unwrap();
    let pay = Uuid::now_v7();
    let back = Uuid::now_v7();
    sqlx::query("INSERT INTO order_payments (id,workspace_id,order_id,kind,provider,status,amount,currency,provider_id) VALUES ($1,$2,$3,'payment','fake','paid',2500,'EUR','pi_1')")
        .bind(pay).bind(ws).bind(order).execute(&p.w.pool).await.unwrap();
    sqlx::query("INSERT INTO order_payments (id,workspace_id,order_id,parent_id,kind,provider,status,amount,currency,provider_id) VALUES ($1,$2,$3,$4,'refund','fake','refunded',1000,'EUR','re_1')")
        .bind(back).bind(ws).bind(order).bind(pay).execute(&p.w.pool).await.unwrap();
    for (id, kind, ext, amount) in [
        (pay, "collection", "pi_1", 2_500),
        (back, "refund", "re_1", 1_000),
    ] {
        let op = genesy_360_api::gateway::repo::find(&p.w.pool, ws, id)
            .await
            .unwrap()
            .unwrap();
        let mut tx = p.w.pool.begin().await.unwrap();
        provider_observations::ingest(&mut tx, ws, &op, kind, ext, amount)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }
    let (residual, relation): (i64, Option<Uuid>) = sqlx::query_as(
        "SELECT residual, relation_id FROM accounting_open_items WHERE workspace_id=$1 AND order_id=$2 AND classification='customer_deposit'",
    ).bind(ws).bind(order).fetch_one(&p.w.pool).await.unwrap();
    assert_eq!((residual, relation), (1_500, None));
    assert_eq!(p.gl("customer_deposits").await, -1_500);
    assert_eq!(p.gl("provider_clearing").await, 1_500);
    assert_eq!(
        scalar(
            &p,
            "SELECT count(*) FROM accounting_exceptions WHERE workspace_id=$1"
        )
        .await,
        0
    );
}

/// §6: gateway admission takes the book gate before the invoice lock, so a writer holding the
/// gate exclusively never meets it holding the invoice.
#[sqlx::test]
async fn gateway_takes_the_book_gate_before_invoice_locks(pool: PgPool) {
    let p = book(pool, true).await;
    let token = p._f.api.get(&p.invoice).await["payment_hash"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut gate = p.w.pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('erp.book|' || $1, 0))")
        .bind(&p.w.ws)
        .execute(&mut *gate)
        .await
        .unwrap();
    let app = p.w.app.clone();
    let call = tokio::spawn(async move {
        crate::common::send(
            &app,
            Method::POST,
            &format!("/v1/invoices/fetch-from-hashed/{token}/payment"),
            Some(json!({"country":"NL","payer_name":"Client"})),
            None,
        )
        .await
        .0
    });
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let mut other = p.w.pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM invoices WHERE id = $1::uuid FOR UPDATE NOWAIT")
        .bind(&p.invoice)
        .execute(&mut *other)
        .await
        .expect("invoice not locked while gate waits");
    other.rollback().await.unwrap();
    gate.rollback().await.unwrap();
    call.await.unwrap();
}

/// Exceptions stay in their workspace; reads need accounting.read.
#[sqlx::test]
async fn exceptions_stay_in_their_workspace(pool: PgPool) {
    let p = book(pool, false).await;
    let op = operation(&p, None, None, 10_000).await;
    collect(&p, &op).await;
    let id = exception(&p).await["id"].as_str().unwrap().to_owned();
    let (other, _) = crate::common::signup(&p.w.app, "bob@example.com", "bobco").await;
    for (method, path, body) in [
        (Method::GET, format!("/exceptions/{id}"), None),
        (
            Method::POST,
            format!("/exceptions/{id}/retry"),
            Some(action(1, json!({}))),
        ),
        (
            Method::POST,
            format!("/exceptions/{id}/resolve/review"),
            Some(json!({"resolution":"write_off","reason":"r","evidence_ref":"e"})),
        ),
    ] {
        let (s, _) = p.w.call(method, &path, body, &other).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{path}");
    }
}

/// Retry and resolve need accounting.post; a read-only member can only read.
#[sqlx::test]
async fn read_only_members_cannot_retry_or_resolve(pool: PgPool) {
    let p = book(pool, false).await;
    let op = operation(&p, None, None, 10_000).await;
    collect(&p, &op).await;
    let id = exception(&p).await["id"].as_str().unwrap().to_owned();
    let reader = crate::common::member_with(
        &p.w.app,
        &p.w.pool,
        &p.w.ws,
        "rae@example.com",
        &["accounting.read"],
    )
    .await;
    let (s, _) =
        p.w.call(Method::GET, &format!("/exceptions/{id}"), None, &reader)
            .await;
    assert_eq!(s, StatusCode::OK);
    for (path, body) in [
        (format!("/exceptions/{id}/retry"), action(1, json!({}))),
        (
            format!("/exceptions/{id}/resolve/review"),
            json!({"resolution":"write_off","reason":"r","evidence_ref":"e"}),
        ),
        (
            format!("/exceptions/{id}/resolve"),
            action(
                1,
                json!({"resolution":"write_off","reason":"r","evidence_ref":"e","review_fingerprint":"x"}),
            ),
        ),
    ] {
        let (s, _) = p.w.call(Method::POST, &path, Some(body), &reader).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{path}");
    }
    assert_eq!(exception(&p).await["attempts"], 1);
}
