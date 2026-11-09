//! Done when: ops files a kg correction of a bought delivery, a different ops or admin approves
//! it, past credits are re-split by the corrected kg as one new passbook line per farmer (the
//! batch total stays), the raw lot's stock follows the net kg, and a paid-out farmer goes
//! negative until the next credit. Officers, the requester and other workspaces are refused;
//! sold-out and dispatched lots too. Nothing earlier is ever changed.

use crate::{
    common,
    farm_sales::{Sold, balances, sold, till, ws_call},
};

use axum::http::{Method, StatusCode};
use common::farm::{Farm, client_ref};
use serde_json::{Value, json};
use sqlx::PgPool;

/// The batch of a sold scenario: (batch id, raw lot id, share id and harvest per person).
struct Batch {
    id: String,
    raw: String,
    /// Share id and harvest id per person, in `Sold::persons` order.
    shares: [(String, String); 3],
}

async fn batch(s: &Sold) -> Batch {
    let (status, res) =
        s.f.call(Method::GET, "/supply-batches", None, &s.f.ops)
            .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let b = &res["data"][0];
    let line = |p: &str| {
        let l = b["lines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["person_id"] == p)
            .unwrap();
        (
            l["share_id"].as_str().unwrap().to_owned(),
            l["harvest_report_id"].as_str().unwrap().to_owned(),
        )
    };
    assert_eq!(
        b["lines"][0]["farmer_name"].as_str().map(str::is_empty),
        Some(false)
    );
    Batch {
        id: b["id"].as_str().unwrap().to_owned(),
        raw: b["lot"]["id"].as_str().unwrap().to_owned(),
        shares: [
            line(&s.persons[0]),
            line(&s.persons[1]),
            line(&s.persons[2]),
        ],
    }
}

async fn file(f: &Farm, batch: &str, lines: Value, token: &str) -> (StatusCode, Value) {
    let body =
        json!({ "client_ref": client_ref(), "reason": "Scale read 20 kg high", "lines": lines });
    f.call(
        Method::POST,
        &format!("/supply-batches/{batch}/corrections"),
        Some(body),
        token,
    )
    .await
}

/// Filed by ops; the correction id.
async fn filed(f: &Farm, batch: &str, lines: Value) -> String {
    let (status, res) = file(f, batch, lines, &f.ops).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["status"], "pending");
    res["data"]["id"].as_str().unwrap().to_owned()
}

async fn decide(f: &Farm, id: &str, action: &str, token: &str) -> (StatusCode, Value) {
    f.call(
        Method::POST,
        &format!("/supply-corrections/{id}/{action}"),
        None,
        token,
    )
    .await
}

async fn approved(f: &Farm, id: &str) -> Value {
    let (status, res) = decide(f, id, "approve", &f.owner).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["status"], "approved");
    res["data"].clone()
}

async fn raw_stock(f: &Farm, lot: &str) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT coalesce(sum(stock_lvl), 0)::numeric(19,2)::text FROM stocks WHERE lot_id = $1::uuid",
    )
    .bind(lot)
    .fetch_one(&f.pool)
    .await
    .unwrap()
}

async fn listen(f: &Farm) {
    sqlx::query(
        "INSERT INTO webhook_endpoints (workspace_id, url, events, secret_sealed, secret_key_id)
         VALUES ($1::uuid, 'https://hooks.example.com/c', '{supply.corrected}', '\\x00', 'k')",
    )
    .bind(&f.ws)
    .execute(&f.pool)
    .await
    .unwrap();
}

// The sale: 1,800,000 over 250 : 120.50 : 30 kg.
const SALE: [i64; 3] = [1_123_596, 541_573, 134_831];

#[sqlx::test]
async fn a_wrong_farmer_is_two_lines_and_moves_the_money(pool: PgPool) {
    let s = sold(pool, false).await;
    let b = batch(&s).await;
    listen(&s.f).await;
    let (baraka_share, _) = &b.shares[1];
    // 20 of Baraka's 120.50 kg were Upendo's.
    let (status, res) = file(
        &s.f,
        &b.id,
        json!([{ "share_id": baraka_share, "kg": "100.50" },
               { "share_id": baraka_share, "person_id": s.persons[2], "kg": "20" }]),
        &s.f.ops,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let d = &res["data"];
    assert_eq!(d["net_kg"], "0.00");
    assert_eq!(d["reason"], "Scale read 20 kg high");
    let line = |p: &str| {
        d["lines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["person_id"] == p)
            .unwrap()
            .clone()
    };
    assert_eq!(line(&s.persons[1])["kg_before"], "120.50");
    assert_eq!(line(&s.persons[1])["kg_after"], "100.50");
    assert_eq!(line(&s.persons[1])["kg_delta"], "-20.00");
    assert_eq!(line(&s.persons[2])["kg_before"], "0.00");
    assert_eq!(line(&s.persons[2])["kg_after"], "20.00");
    // Preview: 1,800,000 over 250 : 100.50 : 50 = 1,123,596 / 451,685 / 224,719.
    let farmer = |p: &str| {
        d["farmers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["person_id"] == p)
            .cloned()
            .unwrap_or(Value::Null)
    };
    assert_eq!(farmer(&s.persons[1])["change"]["minor"], -89_888);
    assert_eq!(farmer(&s.persons[1])["balance_after"]["minor"], 451_685);
    assert_eq!(farmer(&s.persons[2])["change"]["minor"], 89_888);
    assert_eq!(farmer(&s.persons[0]), Value::Null, "no change, not listed");
    assert_eq!(balances(&s).await, SALE, "nothing moves before approval");

    let id = d["id"].as_str().unwrap();
    approved(&s.f, id).await;
    assert_eq!(balances(&s).await, [1_123_596, 451_685, 224_719]);
    assert_eq!(
        raw_stock(&s.f, &b.raw).await,
        "393.00",
        "net 0: stock stays"
    );

    // One new line per farmer whose money moved, with the reason; the sale lines stay as they were.
    let (_, pb) =
        s.f.call(
            Method::GET,
            &format!("/persons/{}/passbook", s.persons[1]),
            None,
            &s.f.ops,
        )
        .await;
    let rows = pb["data"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["kind"], "supply_correction");
    assert_eq!(rows[0]["amount"]["minor"], -89_888);
    assert_eq!(rows[0]["correction_id"], id);
    assert_eq!(rows[0]["reason"], "Scale read 20 kg high");
    assert_eq!(rows[1]["kind"], "sale_credit");
    assert_eq!(rows[1]["amount"]["minor"], SALE[1]);
    let neema: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM farm_passbook_entries WHERE person_id = $1::uuid AND kind = 'supply_correction'",
    )
    .bind(&s.persons[0])
    .fetch_one(&s.f.pool)
    .await
    .unwrap();
    assert_eq!(neema, 0, "a zero change writes nothing");

    let events: Vec<Value> =
        sqlx::query_scalar("SELECT payload FROM webhook_events WHERE type = 'supply.corrected'")
            .fetch_all(&s.f.pool)
            .await
            .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["correction_id"], id);
    assert_eq!(events[0]["batch_id"], b.id.as_str());
    assert_eq!(events[0]["farmers"], 2);
    assert_eq!(events[0]["net_kg"], "0.00");
    assert!(
        !events[0].to_string().contains(&s.persons[1]),
        "no person ids"
    );
}

#[sqlx::test]
async fn a_cut_frees_stock_and_committed_kg_and_splits_later_sales(pool: PgPool) {
    let s = sold(pool, false).await;
    let b = batch(&s).await;
    let id = filed(
        &s.f,
        &b.id,
        json!([{ "share_id": b.shares[1].0, "kg": "100.50" }]),
    )
    .await;
    let d = approved(&s.f, &id).await;
    assert_eq!(d["net_kg"], "-20.00");
    // 1,800,000 over 250 : 100.50 : 30.
    assert_eq!(balances(&s).await, [1_182_654, 475_427, 141_919]);
    assert_eq!(raw_stock(&s.f, &b.raw).await, "373.00");
    let committed: String = sqlx::query_scalar("SELECT farm_harvest_committed_kg($1::uuid)::text")
        .bind(&b.shares[1].1)
        .fetch_one(&s.f.pool)
        .await
        .unwrap();
    assert_eq!(committed, "100.50");
    let moved: (String, String) = sqlx::query_as(
        "SELECT related_domain, quantity_change::text FROM stock_movements
         WHERE related_id = $1::uuid",
    )
    .bind(&id)
    .fetch_one(&s.f.pool)
    .await
    .unwrap();
    assert_eq!(moved, ("farm_supply_correction".into(), "-20.00".into()));
}

#[sqlx::test]
async fn a_paid_farmer_goes_negative_and_the_next_credit_recovers(pool: PgPool) {
    let s = sold(pool, false).await;
    let f = &s.f;
    let b = batch(&s).await;
    let ilu = f.ilundo.clone();
    let juma = f
        .staff("juma@ruaha.test", "field_officer", Some(&ilu))
        .await;
    let juma_id = f.user_id("juma@ruaha.test").await;
    let float = json!({ "kind": "float_out", "staff_user_id": juma_id, "village_id": f.ilundo,
                        "currency_id": s.tzs, "amount": 2_000_000 });
    let (status, _) = f
        .call(Method::POST, "/cash/movements", Some(float), &f.ops)
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let pay = |amount: i64| {
        json!({ "client_ref": client_ref(), "person_id": s.persons[1], "amount": amount,
                "currency_id": s.tzs, "id_type": "nida", "name_confirmed": true })
    };
    let (status, res) = f
        .call(Method::POST, "/payouts", Some(pay(SALE[1])), &juma)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");

    // Baraka delivered 20 kg, not 120.50: 1,800,000 over 250 : 20 : 30 gives him 120,000.
    let id = filed(f, &b.id, json!([{ "share_id": b.shares[1].0, "kg": "20" }])).await;
    let (_, preview) = f
        .call(
            Method::GET,
            &format!("/supply-corrections/{id}"),
            None,
            &f.owner,
        )
        .await;
    let baraka = preview["data"]["farmers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["person_id"] == s.persons[1].as_str())
        .unwrap()
        .clone();
    assert_eq!(baraka["balance_after"]["minor"], -421_573);
    approved(f, &id).await;
    assert_eq!(balances(&s).await, [1_500_000, -421_573, 180_000]);
    let (status, res) = f.call(Method::POST, "/payouts", Some(pay(1)), &juma).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "no payout while owing: {res}"
    );

    // The till sells 1 jar (600,000) split by the corrected kg: Baraka gets 40,000.
    let t = till(&s).await;
    let body = json!({ "client_ref": client_ref(), "payment": { "method": "card" },
                       "lines": [{ "product_id": s.product, "quantity": "1", "unit_price": 2400 }] });
    let (status, res) = ws_call(f, &s.shop_ws, Method::POST, "/till/sales", Some(body), &t).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(balances(&s).await, [2_000_000, -381_573, 240_000]);
}

#[sqlx::test]
async fn four_eyes_officers_and_double_decisions_are_refused(pool: PgPool) {
    let s = sold(pool, false).await;
    let f = &s.f;
    let b = batch(&s).await;
    let lines = json!([{ "share_id": b.shares[1].0, "kg": "100.50" }]);
    let (status, _) = file(f, &b.id, lines.clone(), &f.officer_ilu).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "officer files");
    let id = filed(f, &b.id, lines).await;
    let (status, _) = decide(f, &id, "approve", &f.ops).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "requester approves");
    let (status, _) = decide(f, &id, "reject", &f.ops).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "requester rejects");
    let (status, _) = decide(f, &id, "approve", &f.officer_ilu).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "officer approves");
    approved(f, &id).await;
    let (status, _) = decide(f, &id, "reject", &f.owner).await;
    assert_eq!(status, StatusCode::CONFLICT, "decided once");

    let other = filed(f, &b.id, json!([{ "share_id": b.shares[2].0, "kg": "25" }])).await;
    let before = balances(&s).await;
    let (status, res) = decide(f, &other, "reject", &f.owner).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["status"], "rejected");
    assert_eq!(balances(&s).await, before, "a rejection moves nothing");
    let (status, _) = decide(f, &other, "approve", &f.owner).await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (_, list) = f
        .call(
            Method::GET,
            "/supply-corrections?status=pending",
            None,
            &f.owner,
        )
        .await;
    assert_eq!(list["data"].as_array().unwrap().len(), 0);
    let (_, list) = f
        .call(Method::GET, "/supply-corrections", None, &f.officer_ilu)
        .await;
    assert_eq!(list["data"].as_array().unwrap().len(), 2, "staff read");

    // The schema refuses one person deciding their own request, whatever the code does.
    let ops_id = f.user_id("ops@ruaha.test").await;
    let third = filed(f, &b.id, json!([{ "share_id": b.shares[1].0, "kg": "90" }])).await;
    let err = sqlx::query(
        "INSERT INTO farm_supply_correction_decisions
             (correction_id, workspace_id, decision, requested_by, decided_by)
         VALUES ($1::uuid, $2::uuid, 'rejected', $3::uuid, $3::uuid)",
    )
    .bind(&third)
    .bind(&f.ws)
    .bind(&ops_id)
    .execute(&f.pool)
    .await;
    assert!(err.is_err());
}

#[sqlx::test]
async fn another_workspace_is_refused(pool: PgPool) {
    let s = sold(pool.clone(), false).await;
    let b = batch(&s).await;
    let id = filed(
        &s.f,
        &b.id,
        json!([{ "share_id": b.shares[1].0, "kg": "100.50" }]),
    )
    .await;
    let other = Farm::new_named(pool, "other.test", "other").await;
    let lines = json!([{ "share_id": b.shares[1].0, "kg": "1" }]);
    // Their admin on our workspace: not a member.
    let (status, _) = file(&s.f, &b.id, lines.clone(), &other.owner).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = decide(&s.f, &id, "approve", &other.owner).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // Our ids in their own workspace.
    let (status, _) = file(&other, &b.id, lines, &other.owner).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    for (method, tail) in [
        (Method::GET, format!("/supply-corrections/{id}")),
        (Method::POST, format!("/supply-corrections/{id}/approve")),
        (Method::POST, format!("/supply-corrections/{id}/reject")),
    ] {
        let (status, _) = other.call(method, &tail, None, &other.owner).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{tail}");
    }
    let (_, list) = other
        .call(Method::GET, "/supply-corrections", None, &other.owner)
        .await;
    assert_eq!(list["data"].as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn a_cut_on_a_dispatched_lot_is_refused_a_move_is_not(pool: PgPool) {
    // The jars packed from the raw lot crossed a supply link.
    let s = sold(pool, true).await;
    let b = batch(&s).await;
    let (status, res) = file(
        &s.f,
        &b.id,
        json!([{ "share_id": b.shares[1].0, "kg": "100.50" }]),
        &s.f.ops,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(
        res["errors"]["lines"][0]
            .as_str()
            .unwrap()
            .contains("supply link"),
        "{res}"
    );
    let id = filed(
        &s.f,
        &b.id,
        json!([{ "share_id": b.shares[1].0, "kg": "100.50" },
               { "share_id": b.shares[1].0, "person_id": s.persons[2], "kg": "20" }]),
    )
    .await;
    approved(&s.f, &id).await;
}

#[sqlx::test]
async fn a_sold_out_lot_is_refused(pool: PgPool) {
    let s = sold(pool, false).await;
    let b = batch(&s).await;
    let lines = json!([{ "share_id": b.shares[1].0, "kg": "100.50" }]);
    let id = filed(&s.f, &b.id, lines.clone()).await;
    // The rest of the raw lot goes.
    let (status, res) =
        s.f.ws_call(
            Method::POST,
            &format!("/lots/{}/stock-mutations", b.raw),
            Some(json!({ "warehouse_id": s.warehouse, "quantity": "-393", "reason": "sold" })),
            &s.f.owner,
        )
        .await;
    assert!(status.is_success(), "{res}");
    let (status, res) = file(&s.f, &b.id, lines, &s.f.ops).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(
        res["errors"]["batch"][0]
            .as_str()
            .unwrap()
            .contains("sold out"),
        "{res}"
    );
    let (status, _) = decide(&s.f, &id, "approve", &s.f.owner).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "rechecked at approval"
    );
}

#[sqlx::test]
async fn kg_stays_between_zero_and_the_harvest(pool: PgPool) {
    let s = sold(pool, false).await;
    let f = &s.f;
    let b = batch(&s).await;
    let share = &b.shares[1].0;
    // Baraka's harvest is 200 kg.
    let (status, res) = file(
        f,
        &b.id,
        json!([{ "share_id": share, "kg": "200.01" }]),
        &f.ops,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    for bad in [
        json!([{ "share_id": share, "kg": "-1" }]),
        json!([]),
        json!([{ "share_id": share, "kg": "120.50" }]),
        json!([{ "share_id": "00000000-0000-0000-0000-000000000000", "kg": "1" }]),
    ] {
        let (status, res) = file(f, &b.id, bad.clone(), &f.ops).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {res}");
    }
    let id = filed(f, &b.id, json!([{ "share_id": share, "kg": "200" }])).await;
    approved(f, &id).await;
    // Two cuts to 0 filed against 200 kg: the second would take the share below 0.
    let one = filed(f, &b.id, json!([{ "share_id": share, "kg": "0" }])).await;
    let two = filed(f, &b.id, json!([{ "share_id": share, "kg": "0" }])).await;
    approved(f, &one).await;
    let (status, res) = decide(f, &two, "approve", &f.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
}

#[sqlx::test]
async fn corrections_replay_and_are_append_only(pool: PgPool) {
    let s = sold(pool, false).await;
    let f = &s.f;
    let b = batch(&s).await;
    let body = json!({ "client_ref": client_ref(), "reason": "Wrong farmer",
                       "lines": [{ "share_id": b.shares[1].0, "kg": "100.50" }] });
    let url = format!("/supply-batches/{}/corrections", b.id);
    let (status, first) = f.call(Method::POST, &url, Some(body.clone()), &f.ops).await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let (status, again) = f.call(Method::POST, &url, Some(body.clone()), &f.ops).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(again["data"]["id"], first["data"]["id"]);
    let mut other = body.clone();
    other["reason"] = json!("Another reason");
    let (status, _) = f.call(Method::POST, &url, Some(other), &f.ops).await;
    assert_eq!(status, StatusCode::CONFLICT);
    approved(f, first["data"]["id"].as_str().unwrap()).await;

    for sql in [
        "UPDATE farm_supply_corrections SET reason = 'x'",
        "DELETE FROM farm_supply_correction_lines",
        "UPDATE farm_supply_correction_decisions SET decision = 'rejected'",
        "DELETE FROM farm_supply_correction_splits",
        "UPDATE farm_passbook_entries SET amount = 1 WHERE kind = 'supply_correction'",
        "DELETE FROM farm_passbook_entries WHERE kind = 'supply_correction'",
        "UPDATE farm_supply_shares SET kg = 1",
    ] {
        assert!(sqlx::query(sql).execute(&f.pool).await.is_err(), "{sql}");
    }
}

#[sqlx::test]
async fn a_voice_request_gets_a_review_card(pool: PgPool) {
    use crate::assistant_farm_facts::propose;
    let s = sold(pool, false).await;
    let f = &s.f;
    let b = batch(&s).await;
    let tail = format!("/farm/supply-batches/{}/corrections", b.id);
    let body = json!({ "client_ref": client_ref(), "reason": "Scale read high",
                       "lines": [{ "share_id": b.shares[1].0, "kg": "100.50" }] });
    let (status, facts) = propose(
        f,
        &f.ops,
        "farm_supply_correction_request",
        &tail,
        body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{facts}");
    assert_eq!(facts["kind"], "supply_correction");
    assert_eq!(facts["locked"]["label"], "ILU-H-1");
    assert_eq!(
        facts["lines"],
        json!([{ "farmer": "Baraka Grower", "kg_before": "120.50", "kg_after": "100.50" }])
    );
    assert_eq!(facts["net_kg"], "-20.00");
    assert_eq!(facts["reason"], "Scale read high");
    let baraka = facts["farmers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == "Baraka Grower")
        .unwrap()
        .clone();
    assert_eq!(baraka["change_minor"], 475_427 - SALE[1]);
    assert_eq!(baraka["balance_after_minor"], 475_427);
    assert_eq!(facts["currency"], json!({ "code": "TZS", "decimals": 2 }));
    assert!(
        !facts.to_string().contains(&s.persons[1]),
        "no raw person id"
    );

    let (status, _) = propose(
        f,
        &f.officer_ilu,
        "farm_supply_correction_request",
        &tail,
        body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "officers don't correct");
    let mut same = body;
    same["lines"][0]["kg"] = json!("120.50");
    let (status, res) = propose(f, &f.ops, "farm_supply_correction_request", &tail, same).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "native rules before the card: {res}"
    );
}

/// Test-only trigger: inserting a passbook line of the kind waits on advisory lock 7001.
const PAUSE_CORRECTION: &str = "CREATE FUNCTION e2e_pause() RETURNS trigger LANGUAGE plpgsql AS $$
    BEGIN PERFORM pg_advisory_xact_lock_shared(7001); RETURN NEW; END $$;
    CREATE TRIGGER e2e_pause AFTER INSERT ON farm_passbook_entries
        FOR EACH ROW WHEN (NEW.kind = 'supply_correction') EXECUTE FUNCTION e2e_pause();";
const PAUSE_SALE: &str = "CREATE FUNCTION e2e_pause() RETURNS trigger LANGUAGE plpgsql AS $$
    BEGIN PERFORM pg_advisory_xact_lock_shared(7001); RETURN NEW; END $$;
    CREATE TRIGGER e2e_pause AFTER INSERT ON farm_passbook_entries
        FOR EACH ROW WHEN (NEW.kind = 'sale_credit') EXECUTE FUNCTION e2e_pause();";

/// Installs `trigger` and holds lock 7001 on its own connection, so the transaction that
/// inserts the matching passbook line pauses right after it read the weights.
async fn pause_on(
    pool: &PgPool,
    trigger: &'static str,
) -> sqlx::pool::PoolConnection<sqlx::Postgres> {
    sqlx::raw_sql(trigger).execute(pool).await.unwrap();
    let mut holder = pool.acquire().await.unwrap();
    sqlx::query("SELECT pg_advisory_lock(7001)")
        .execute(&mut *holder)
        .await
        .unwrap();
    holder
}

/// Waits until `n` backends of this database wait on a lock (5 s at most).
async fn waiting(pool: &PgPool, n: i64) -> bool {
    for _ in 0..100 {
        let w: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity
             WHERE datname = current_database() AND wait_event_type = 'Lock'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        if w >= n {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    false
}

/// Every attribution of the batch holds exactly `split(total, effective kg)` per farmer, and
/// every correction's re-split of an attribution sums to 0.
async fn splits_match_corrected_kg(f: &Farm, batch: &str) {
    let weights: Vec<(String, rust_decimal::Decimal)> = sqlx::query_as(
        "SELECT person_id::text, sum(kg) FROM farm_supply_share_kg WHERE batch_id = $1::uuid
         GROUP BY person_id HAVING sum(kg) > 0 ORDER BY person_id",
    )
    .bind(batch)
    .fetch_all(&f.pool)
    .await
    .unwrap();
    let list: Vec<(String, i128)> = weights
        .iter()
        .map(|(p, kg)| {
            use rust_decimal::prelude::ToPrimitive;
            (
                p.clone(),
                (*kg * rust_decimal::Decimal::ONE_HUNDRED)
                    .to_i128()
                    .unwrap(),
            )
        })
        .collect();
    let attributions: Vec<(String, i64)> = sqlx::query_as(
        "SELECT id::text, total FROM farm_sale_attributions WHERE batch_id = $1::uuid",
    )
    .bind(batch)
    .fetch_all(&f.pool)
    .await
    .unwrap();
    assert!(attributions.len() >= 2, "the first sale and the racing one");
    for (a, total) in attributions {
        let held: Vec<(String, i64)> = sqlx::query_as(
            "SELECT person_id::text, sum(amount)::bigint FROM (
                 SELECT person_id, amount FROM farm_passbook_entries WHERE attribution_id = $1::uuid
                 UNION ALL
                 SELECT person_id, amount FROM farm_supply_correction_splits
                 WHERE attribution_id = $1::uuid) x
             GROUP BY person_id HAVING sum(amount) <> 0 ORDER BY person_id",
        )
        .bind(&a)
        .fetch_all(&f.pool)
        .await
        .unwrap();
        let want: Vec<(String, i64)> = list
            .iter()
            .zip(largest_remainder(total, &list))
            .filter(|(_, m)| *m != 0)
            .map(|((p, _), m)| (p.clone(), m))
            .collect();
        assert_eq!(held, want, "attribution {a} (total {total})");
    }
    let unbalanced: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (SELECT 1 FROM farm_supply_correction_splits
         GROUP BY correction_id, attribution_id HAVING sum(amount) <> 0) x",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(unbalanced, 0);
}

/// A till sale of 1 jar on its own task.
async fn sell_one(s: &Sold) -> tokio::task::JoinHandle<(StatusCode, Value)> {
    let t = till(s).await;
    let (app, ws, product) = (s.f.app.clone(), s.shop_ws.clone(), s.product.clone());
    tokio::spawn(async move {
        let body = json!({ "client_ref": client_ref(), "payment": { "method": "card" },
                           "lines": [{ "product_id": product, "quantity": "1", "unit_price": 2400 }] });
        common::send(
            &app,
            Method::POST,
            &format!("/v1/workspace/{ws}/till/sales"),
            Some(body),
            Some(&t),
        )
        .await
    })
}

/// The owner approves `id` on its own task.
fn approve_later(f: &Farm, id: &str) -> tokio::task::JoinHandle<(StatusCode, Value)> {
    let (app, url, owner) = (
        f.app.clone(),
        f.url(&format!("/supply-corrections/{id}/approve")),
        f.owner.clone(),
    );
    tokio::spawn(async move { common::send(&app, Method::POST, &url, None, Some(&owner)).await })
}

#[sqlx::test]
async fn a_sale_during_an_approval_waits_and_splits_by_the_corrected_kg(pool: PgPool) {
    let s = sold(pool.clone(), false).await;
    let b = batch(&s).await;
    let id = filed(
        &s.f,
        &b.id,
        json!([{ "share_id": b.shares[1].0, "kg": "20" }]),
    )
    .await;
    // The approval stops after its re-split read, holding the batch.
    let mut holder = pause_on(&pool, PAUSE_CORRECTION).await;
    let approval = approve_later(&s.f, &id);
    assert!(waiting(&pool, 1).await, "approval paused");
    let sale = sell_one(&s).await;
    assert!(waiting(&pool, 2).await, "the sale waits for the approval");
    sqlx::query("SELECT pg_advisory_unlock_all()")
        .execute(&mut *holder)
        .await
        .unwrap();
    let (status, res) = approval.await.unwrap();
    assert_eq!(status, StatusCode::OK, "{res}");
    let (status, res) = sale.await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{res}");
    splits_match_corrected_kg(&s.f, &b.id).await;
}

#[sqlx::test]
async fn an_approval_during_a_sale_waits_and_re_splits_it(pool: PgPool) {
    let s = sold(pool.clone(), false).await;
    let b = batch(&s).await;
    let id = filed(
        &s.f,
        &b.id,
        json!([{ "share_id": b.shares[1].0, "kg": "20" }]),
    )
    .await;
    // The sale stops after it read the old weights, holding its stock and the batch (shared).
    let mut holder = pause_on(&pool, PAUSE_SALE).await;
    let sale = sell_one(&s).await;
    assert!(waiting(&pool, 1).await, "sale paused");
    let approval = approve_later(&s.f, &id);
    assert!(waiting(&pool, 2).await, "the approval waits for the sale");
    sqlx::query("SELECT pg_advisory_unlock_all()")
        .execute(&mut *holder)
        .await
        .unwrap();
    let (status, res) = sale.await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let (status, res) = approval.await.unwrap();
    assert_eq!(status, StatusCode::OK, "{res}");
    splits_match_corrected_kg(&s.f, &b.id).await;
}

/// Independent oracle of the credit split: floor of each share, leftover units to the largest
/// remainders, ties to the smaller person id (the list is sorted by person id).
fn largest_remainder(total: i64, weights: &[(String, i128)]) -> Vec<i64> {
    let sum: i128 = weights.iter().map(|w| w.1).sum();
    let t = i128::from(total);
    let mut out: Vec<i128> = weights.iter().map(|w| (t * w.1).div_euclid(sum)).collect();
    let mut left = t - out.iter().sum::<i128>();
    let mut order: Vec<usize> = (0..weights.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse((t * weights[i].1).rem_euclid(sum)));
    for i in order {
        if left == 0 {
            break;
        }
        out[i] += 1;
        left -= 1;
    }
    out.into_iter().map(|v| v as i64).collect()
}
