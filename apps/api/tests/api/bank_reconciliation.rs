//! Bank statements and statement-line reconciliation (Flow ERP M5-C, masterplan §13 BANKING,
//! §15 BANKING routes, import profile `bank_statement`): statements enter through imports
//! only, opening + lines = closing, bank transaction ids never insert twice, exact matching to
//! posted bank journal lines under shared capacity, typed unmatch, reversed journal lines,
//! foreign-currency refusal, permissions, isolation, replay and setup-book refusal.

use crate::{
    common::{self, member_with, send_raw, signup},
    payables::{bank_location, book},
    purchasing::{W, code, uuid, world},
};

use axum::http::{Method, StatusCode};
use genesy_360_api::jobs::{self, kinds::Ctx, runner::Step};
use serde_json::{Value, json};
use sqlx::PgPool;

impl W {
    /// Run queued import apply jobs to the end.
    async fn drain_imports(&self) {
        let state = common::state(
            self.pool().clone(),
            genesy_360_api::mail::Mailer::capture().0,
        );
        let ctx = Ctx {
            invoice_state: Some(state.clone()),
            assistant_api_key: None,
            db: state.db.clone(),
            files: state.files.clone(),
            http: genesy_360_api::outbound::Outbound::new(&[]).unwrap(),
            credentials: state.credentials.clone(),
            payments: state.payments.clone(),
        };
        for _ in 0..100 {
            let Some(job) =
                jobs::repo::claim(&state.db, &[jobs::kinds::IMPORT_APPLY], 2, jobs::LEASE)
                    .await
                    .unwrap()
            else {
                return;
            };
            match jobs::kinds::dispatch(&ctx, job.clone()).await.unwrap() {
                Step::Done => jobs::repo::finish(&state.db, job.id, job.token)
                    .await
                    .unwrap(),
                Step::More => jobs::repo::requeue(&state.db, job.id, job.token)
                    .await
                    .unwrap(),
            };
        }
        panic!("import apply did not finish");
    }

    /// Create, upload and validate a `bank_statement` import; the validated batch.
    async fn staged_statement(&self, mapping: Value, rows: &[Value]) -> (StatusCode, Value) {
        let (s, b) = self
            .req(
                Method::POST,
                "/imports",
                Some(json!({ "client_ref": uuid(), "payload": {
                    "profile": "bank_statement", "filename": "statement.csv",
                    "file_hash": "b".repeat(64), "expected_rows": rows.len(),
                    "mapping": mapping } })),
            )
            .await;
        if s != StatusCode::CREATED {
            return (s, b);
        }
        let mut batch = b["data"].clone();
        let id = batch["id"].as_str().unwrap().to_owned();
        for (i, chunk) in rows.chunks(20).enumerate() {
            let (s, b) = self
                .req(
                    Method::PUT,
                    &format!("/imports/{id}/chunks/{i}"),
                    Some(
                        json!({ "client_ref": uuid(), "expected_version": batch["version"],
                                 "payload": { "rows": chunk } }),
                    ),
                )
                .await;
            assert_eq!(s, StatusCode::OK, "{b}");
            batch = b["data"].clone();
        }
        let (s, b) = self
            .req(
                Method::POST,
                &format!("/imports/{id}/validate"),
                Some(
                    json!({ "client_ref": uuid(), "expected_version": batch["version"],
                             "payload": { "expected_chunks": rows.len().div_ceil(20) } }),
                ),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        (s, b["data"].clone())
    }

    async fn apply_import(&self, token: &str, batch: &Value) -> (StatusCode, Value) {
        let id = batch["id"].as_str().unwrap();
        self.req_as(
            token,
            Method::POST,
            &format!("/imports/{id}/apply"),
            Some(json!({ "client_ref": uuid(), "expected_version": batch["version"],
                         "payload": { "validation_fingerprint": batch["validation_fingerprint"] } })),
        )
        .await
    }

    /// Import a balanced statement for `bank`; the stored statement.
    async fn statement(
        &self,
        bank: &str,
        reference: &str,
        opening: &str,
        closing: &str,
        rows: &[Value],
    ) -> Value {
        let (s, batch) = self
            .staged_statement(
                mapping(
                    bank,
                    reference,
                    &self.today(-5).await,
                    &self.today(0).await,
                    opening,
                    closing,
                ),
                rows,
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{batch}");
        assert_eq!(batch["counts"]["invalid"], 0, "{batch}");
        let (s, b) = self.apply_import(&self.api.owner.clone(), &batch).await;
        assert_eq!(s, StatusCode::ACCEPTED, "{b}");
        self.drain_imports().await;
        let (s, list) = self
            .req(Method::GET, "/accounting/bank-statements", None)
            .await;
        assert_eq!(s, StatusCode::OK, "{list}");
        list["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|st| st["reference"] == reference)
            .unwrap_or_else(|| panic!("statement {reference} not stored: {list}"))
            .clone()
    }

    async fn lines(&self, statement: &Value) -> Vec<Value> {
        let id = statement["id"].as_str().unwrap();
        let (s, b) = self
            .req(
                Method::GET,
                &format!("/accounting/bank-statements/{id}/lines"),
                None,
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        b["data"].as_array().unwrap().clone()
    }

    /// The posted bank journal line of supplier payment `payment`.
    async fn bank_line(&self, payment: &Value) -> String {
        sqlx::query_scalar(
            "SELECT l.id::text FROM journal_lines l
             JOIN journal_entries e ON e.id = l.entry_id
             JOIN accounting_effects f ON f.id = e.effect_id
             JOIN bank_accounts b ON b.id = l.bank_account_id AND b.account_id = l.account_id
             WHERE f.workspace_id = $1::uuid AND f.source_id = $2::uuid
               AND f.effect_kind <> 'reversed'",
        )
        .bind(&self.api.ws)
        .bind(payment["id"].as_str().unwrap())
        .fetch_one(self.pool())
        .await
        .unwrap()
    }

    /// Supplier money through `bank` (functional currency); the payment.
    async fn bank_money(&self, bank: &str, direction: &str, amount: &str) -> Value {
        self.paid(
            self.money(&self.eur.clone(), direction, amount, bank_location(bank))
                .await,
        )
        .await
    }

    async fn reconcile(&self, line: &Value, allocations: &[(&str, &str)]) -> (StatusCode, Value) {
        self.reconcile_as(&self.api.owner.clone(), &uuid(), line, allocations)
            .await
    }

    async fn reconcile_as(
        &self,
        token: &str,
        client_ref: &str,
        line: &Value,
        allocations: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        self.req_as(
            token,
            Method::POST,
            "/accounting/bank-reconciliations",
            Some(reconcile_body(client_ref, line, allocations)),
        )
        .await
    }

    async fn unmatch(&self, rec: &Value) -> (StatusCode, Value) {
        let id = rec["id"].as_str().unwrap();
        self.req(
            Method::POST,
            &format!("/accounting/bank-reconciliations/{id}/reverse"),
            Some(
                json!({ "client_ref": uuid(), "expected_version": rec["version"],
                         "payload": { "reason": "Matched the wrong payment" } }),
            ),
        )
        .await
    }
}

fn mapping(
    bank: &str,
    reference: &str,
    start: &str,
    end: &str,
    opening: &str,
    closing: &str,
) -> Value {
    json!({ "bank_account_id": bank, "reference": reference, "start_date": start,
            "end_date": end, "opening_balance": opening, "closing_balance": closing })
}

fn row(n: usize, date: &str, amount: &str, tx: Option<&str>) -> Value {
    json!({ "row_number": n, "external_key": tx,
            "fields": { "booking_date": date, "amount": amount, "description": "Bees BV" } })
}

fn reconcile_body(client_ref: &str, line: &Value, allocations: &[(&str, &str)]) -> Value {
    json!({ "client_ref": client_ref, "payload": {
        "statement_line_id": line["id"],
        "allocations": allocations.iter()
            .map(|(j, a)| json!({ "journal_line_id": j, "amount": a }))
            .collect::<Vec<_>>() } })
}

/// Active book with payables roles and a functional-currency bank account.
async fn banked(pool: PgPool) -> (W, String) {
    let w = world(pool).await;
    book(&w).await;
    let bank = w.bank(&w.eur.clone(), "1100").await;
    (w, bank)
}

#[sqlx::test]
async fn statement_import_stores_lines_and_keeps_equal_rows(pool: PgPool) {
    let (w, bank) = banked(pool).await;
    let d = w.today(-1).await;
    // Two equal rows without a bank id are two real movements (§13: not duplicate proof).
    let st = w
        .statement(
            &bank,
            "ST-1",
            "100.00",
            "45.00",
            &[
                row(1, &d, "-40.00", Some("T1")),
                row(2, &d, "-25.00", None),
                row(3, &d, "-25.00", None),
                row(4, &d, "35.00", Some("T4")),
            ],
        )
        .await;
    assert_eq!(st["bank_account_id"], bank.as_str());
    assert_eq!(st["opening_balance"], "100.00");
    assert_eq!(st["closing_balance"], "45.00");
    assert_eq!(st["line_count"], 4);
    assert_eq!(st["unreconciled_lines"], 4);
    assert_eq!(st["unreconciled_amount"], "-55.00");
    assert_eq!(st["state"], "open");
    assert_eq!(st["file_hash"], "b".repeat(64));
    let id = st["id"].as_str().unwrap();
    let got = w.get(&format!("/accounting/bank-statements/{id}")).await;
    assert_eq!(got["id"], st["id"]);
    let lines = w.lines(&st).await;
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0]["row_number"], 1);
    assert_eq!(lines[0]["transaction_id"], "T1");
    assert_eq!(lines[0]["amount"], "-40.00");
    assert_eq!(lines[0]["residual"], "-40.00");
    assert_eq!(lines[0]["state"], "unreconciled");
    assert_eq!(lines[2]["amount"], "-25.00");
    // Statements and lines are fixed once stored.
    let err =
        sqlx::query("UPDATE bank_statement_lines SET amount = 1 WHERE statement_id = $1::uuid")
            .bind(id)
            .execute(w.pool())
            .await;
    assert!(err.is_err());
}

#[sqlx::test]
async fn opening_plus_lines_must_equal_closing(pool: PgPool) {
    let (w, bank) = banked(pool).await;
    let (start, end) = (w.today(-5).await, w.today(0).await);
    let (s, batch) = w
        .staged_statement(
            mapping(&bank, "ST-U", &start, &end, "100.00", "61.00"),
            &[row(1, &end, "-40.00", None)],
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(batch["counts"]["invalid"], 1, "{batch}");
    let id = batch["id"].as_str().unwrap();
    let rows = w.get(&format!("/imports/{id}/rows")).await;
    assert_eq!(
        rows[0]["issues"][0]["code"], "statement_unbalanced",
        "{rows}"
    );
    let (s, b) = w.apply_import(&w.api.owner.clone(), &batch).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(w.count("bank_statements").await, 0);

    // Row rules: date outside the statement, too many decimals, zero.
    let (_, batch) = w
        .staged_statement(
            mapping(&bank, "ST-R", &start, &end, "0", "0"),
            &[
                row(1, &w.today(-9).await, "1.00", None),
                row(2, &end, "1.001", None),
                row(3, &end, "0", None),
            ],
        )
        .await;
    assert_eq!(batch["counts"]["invalid"], 3, "{batch}");

    // The database refuses an unbalanced statement too.
    let err = sqlx::query(
        "INSERT INTO bank_statements (workspace_id, bank_account_id, import_batch_id, file_hash,
             reference, start_date, end_date, opening_balance, closing_balance, created_by)
         SELECT $1::uuid, $2::uuid, $3::uuid, 'x', 'ST-DB', current_date, current_date, 0, 5,
                created_by_user_id FROM workspaces WHERE id = $1::uuid",
    )
    .bind(&w.api.ws)
    .bind(&bank)
    .bind(id)
    .execute(w.pool())
    .await;
    assert!(err.is_err());
}

#[sqlx::test]
async fn a_bank_transaction_id_or_statement_is_never_stored_twice(pool: PgPool) {
    let (w, bank) = banked(pool).await;
    let d = w.today(-1).await;
    w.statement(
        &bank,
        "ST-1",
        "0",
        "-40.00",
        &[row(1, &d, "-40.00", Some("T1"))],
    )
    .await;
    let (start, end) = (w.today(-5).await, w.today(0).await);
    // Same bank transaction id in a later statement.
    let (_, batch) = w
        .staged_statement(
            mapping(&bank, "ST-2", &start, &end, "-40.00", "-80.00"),
            &[row(1, &d, "-40.00", Some("T1"))],
        )
        .await;
    assert_eq!(batch["counts"]["invalid"], 1, "{batch}");
    let id = batch["id"].as_str().unwrap();
    let rows = w.get(&format!("/imports/{id}/rows")).await;
    assert_eq!(rows[0]["issues"][0]["code"], "duplicate_key", "{rows}");
    // Same id twice in one file.
    let (_, batch) = w
        .staged_statement(
            mapping(&bank, "ST-3", &start, &end, "0", "-2.00"),
            &[
                row(1, &d, "-1.00", Some("T9")),
                row(2, &d, "-1.00", Some("T9")),
            ],
        )
        .await;
    assert_eq!(batch["counts"]["invalid"], 2, "{batch}");
    // Same statement reference for the account.
    let (_, batch) = w
        .staged_statement(
            mapping(&bank, "ST-1", &start, &end, "0", "-1.00"),
            &[row(1, &d, "-1.00", None)],
        )
        .await;
    assert_eq!(batch["counts"]["invalid"], 1, "{batch}");
    let id = batch["id"].as_str().unwrap();
    let rows = w.get(&format!("/imports/{id}/rows")).await;
    assert_eq!(
        rows[0]["issues"][0]["code"], "duplicate_statement",
        "{rows}"
    );
    assert_eq!(w.count("bank_statements").await, 1);
    assert_eq!(w.count("bank_statement_lines").await, 1);
}

#[sqlx::test]
async fn exact_match_reconciles_and_refuses_partial_over_and_twice(pool: PgPool) {
    let (w, bank) = banked(pool).await;
    let p40 = w.bank_money(&bank, "payment", "40.00").await;
    let p25 = w.bank_money(&bank, "payment", "25.00").await;
    let (j40, j25) = (w.bank_line(&p40).await, w.bank_line(&p25).await);
    let d = w.today(0).await;
    let st = w
        .statement(
            &bank,
            "ST-1",
            "100.00",
            "-5.00",
            &[
                row(1, &d, "-40.00", None),
                row(2, &d, "-25.00", None),
                row(3, &d, "-40.00", None),
            ],
        )
        .await;
    let lines = w.lines(&st).await;

    // Partial and over the statement line: Σ allocations must equal it exactly.
    for amount in ["-30.00", "-50.00", "40.00"] {
        let (s, b) = w.reconcile(&lines[0], &[(&j40, amount)]).await;
        assert_eq!(
            (s, code(&b)),
            (StatusCode::UNPROCESSABLE_ENTITY, "amount_mismatch"),
            "{b}"
        );
    }
    // Over the journal line's remaining capacity.
    let (s, b) = w.reconcile(&lines[0], &[(&j25, "-40.00")]).await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::UNPROCESSABLE_ENTITY, "capacity_exceeded"),
        "{b}"
    );
    // Too many decimals.
    let (s, b) = w.reconcile(&lines[0], &[(&j40, "-40.001")]).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");

    let (s, rec) = w.reconcile(&lines[0], &[(&j40, "-40.00")]).await;
    assert_eq!(s, StatusCode::CREATED, "{rec}");
    let rec = rec["data"].clone();
    assert_eq!(rec["statement_line_id"], lines[0]["id"]);
    assert_eq!(rec["allocations"][0]["journal_line_id"], j40.as_str());
    assert_eq!(rec["allocations"][0]["amount"], "-40.00");
    assert!(rec["reversed_at"].is_null());

    // The journal line is used up: another statement line can't take it.
    let (s, b) = w.reconcile(&lines[2], &[(&j40, "-40.00")]).await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::UNPROCESSABLE_ENTITY, "capacity_exceeded"),
        "{b}"
    );
    // The statement line has a live match.
    let (s, b) = w.reconcile(&lines[0], &[(&j40, "-40.00")]).await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::CONFLICT, "already_reconciled"),
        "{b}"
    );
    // Same journal line twice in one request.
    let (s, b) = w
        .reconcile(&lines[1], &[(&j25, "-12.50"), (&j25, "-12.50")])
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");

    let (s, b) = w.reconcile(&lines[1], &[(&j25, "-25.00")]).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let lines = w.lines(&st).await;
    assert_eq!(lines[0]["state"], "reconciled");
    assert_eq!(lines[0]["matched_amount"], "-40.00");
    assert_eq!(lines[0]["residual"], "0.00");
    assert_eq!(lines[0]["allocations"][0]["journal_reversed"], false);
    assert_eq!(lines[2]["state"], "unreconciled");
    let st = w
        .get(&format!(
            "/accounting/bank-statements/{}",
            st["id"].as_str().unwrap()
        ))
        .await;
    assert_eq!(st["reconciled_lines"], 2);
    assert_eq!(st["unreconciled_lines"], 1);
    assert_eq!(st["unreconciled_amount"], "-40.00");
    assert_eq!(st["state"], "open");
}

#[sqlx::test]
async fn split_journal_line_and_full_statement(pool: PgPool) {
    let (w, bank) = banked(pool).await;
    let p = w.bank_money(&bank, "payment", "40.00").await;
    let j = w.bank_line(&p).await;
    let d = w.today(0).await;
    // The bank cleared one payment in two movements.
    let st = w
        .statement(
            &bank,
            "ST-1",
            "0",
            "-40.00",
            &[row(1, &d, "-15.00", None), row(2, &d, "-25.00", None)],
        )
        .await;
    let lines = w.lines(&st).await;
    let (s, b) = w.reconcile(&lines[0], &[(&j, "-15.00")]).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let (s, b) = w.reconcile(&lines[1], &[(&j, "-25.01")]).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let (s, b) = w.reconcile(&lines[1], &[(&j, "-25.00")]).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let st = w
        .get(&format!(
            "/accounting/bank-statements/{}",
            st["id"].as_str().unwrap()
        ))
        .await;
    assert_eq!(st["state"], "reconciled");
    assert_eq!(st["unreconciled_amount"], "0.00");
}

#[sqlx::test]
async fn unmatch_restores_line_and_capacity_without_deleting(pool: PgPool) {
    let (w, bank) = banked(pool).await;
    let p = w.bank_money(&bank, "refund_received", "15.00").await;
    let j = w.bank_line(&p).await;
    let d = w.today(0).await;
    let st = w
        .statement(
            &bank,
            "ST-1",
            "0",
            "30.00",
            &[row(1, &d, "15.00", None), row(2, &d, "15.00", None)],
        )
        .await;
    let lines = w.lines(&st).await;
    let (s, rec) = w.reconcile(&lines[0], &[(&j, "15.00")]).await;
    assert_eq!(s, StatusCode::CREATED, "{rec}");
    let (s, rev) = w.unmatch(&rec["data"]).await;
    assert_eq!(s, StatusCode::OK, "{rev}");
    assert_eq!(rev["data"]["reverse_reason"], "Matched the wrong payment");
    assert!(!rev["data"]["reversed_at"].is_null());
    let after = w.lines(&st).await;
    assert_eq!(after[0]["state"], "unreconciled");
    assert_eq!(after[0]["residual"], "15.00");
    // Capacity is back: the other line takes the journal line.
    let (s, b) = w.reconcile(&lines[1], &[(&j, "15.00")]).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    // A reversed match can't be reversed again; it stays on record.
    let (s, b) = w.unmatch(&rev["data"]).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let (s, b) = w.unmatch(&rec["data"]).await;
    assert!(
        matches!(s, StatusCode::CONFLICT | StatusCode::UNPROCESSABLE_ENTITY),
        "{b}"
    );
    assert_eq!(w.count("bank_reconciliations").await, 2);
    let err = sqlx::query("DELETE FROM bank_reconciliations WHERE workspace_id = $1::uuid")
        .bind(&w.api.ws)
        .execute(w.pool())
        .await;
    assert!(err.is_err());
}

#[sqlx::test]
async fn concurrent_matches_never_exceed_journal_line_capacity(pool: PgPool) {
    let (w, bank) = banked(pool).await;
    let p = w.bank_money(&bank, "payment", "40.00").await;
    let j = w.bank_line(&p).await;
    let d = w.today(0).await;
    let st = w
        .statement(
            &bank,
            "ST-1",
            "0",
            "-80.00",
            &[row(1, &d, "-40.00", None), row(2, &d, "-40.00", None)],
        )
        .await;
    let lines = w.lines(&st).await;
    let alloc = [(j.as_str(), "-40.00")];
    let (a, b) = tokio::join!(
        w.reconcile(&lines[0], &alloc),
        w.reconcile(&lines[1], &alloc)
    );
    let created = [a.0, b.0]
        .iter()
        .filter(|s| **s == StatusCode::CREATED)
        .count();
    assert_eq!(created, 1, "{} / {}", a.1, b.1);
    let used: i64 = sqlx::query_scalar(
        "SELECT coalesce(sum(a.amount), 0)::bigint FROM bank_reconciliation_allocations a
         JOIN bank_reconciliations r ON r.id = a.reconciliation_id
         WHERE a.journal_line_id = $1::uuid AND r.reversed_at IS NULL",
    )
    .bind(&j)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(used, -4000);
}

#[sqlx::test]
async fn reversed_journal_lines_are_refused_and_flagged(pool: PgPool) {
    let (w, bank) = banked(pool).await;
    let gone = w.bank_money(&bank, "payment", "25.00").await;
    let kept = w.bank_money(&bank, "payment", "40.00").await;
    let (jg, jk) = (w.bank_line(&gone).await, w.bank_line(&kept).await);
    let (s, b) = w.reverse_payment(&gone).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let d = w.today(0).await;
    let st = w
        .statement(
            &bank,
            "ST-1",
            "0",
            "-65.00",
            &[row(1, &d, "-25.00", None), row(2, &d, "-40.00", None)],
        )
        .await;
    let lines = w.lines(&st).await;
    let (s, b) = w.reconcile(&lines[0], &[(&jg, "-25.00")]).await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::UNPROCESSABLE_ENTITY, "journal_line_reversed"),
        "{b}"
    );

    // Matched first, reversed later: the match stays and reads flag it.
    let (s, b) = w.reconcile(&lines[1], &[(&jk, "-40.00")]).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let kept = w
        .get(&format!(
            "/supplier-payments/{}",
            kept["id"].as_str().unwrap()
        ))
        .await;
    let (s, b) = w.reverse_payment(&kept).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let lines = w.lines(&st).await;
    assert_eq!(lines[1]["state"], "reconciled");
    assert_eq!(lines[1]["allocations"][0]["journal_reversed"], true);
}

#[sqlx::test]
async fn foreign_currency_bank_account_and_other_bank_lines_are_refused(pool: PgPool) {
    let (w, bank) = banked(pool).await;
    let usd = w.usd().await;
    let usd_bank = w.bank(&usd, "1110").await;
    let (start, end) = (w.today(-5).await, w.today(0).await);
    let (s, b) = w
        .staged_statement(
            mapping(&usd_bank, "ST-USD", &start, &end, "0", "1.00"),
            &[row(1, &end, "1.00", None)],
        )
        .await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::UNPROCESSABLE_ENTITY, "currency_mismatch"),
        "{b}"
    );

    // A journal line on another bank account doesn't match this statement.
    let other = w.bank(&w.eur.clone(), "1120").await;
    let p = w.bank_money(&other, "payment", "40.00").await;
    let j = w.bank_line(&p).await;
    let st = w
        .statement(
            &bank,
            "ST-1",
            "0",
            "-40.00",
            &[row(1, &end, "-40.00", None)],
        )
        .await;
    let lines = w.lines(&st).await;
    let (s, b) = w.reconcile(&lines[0], &[(&j, "-40.00")]).await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::UNPROCESSABLE_ENTITY, "not_this_bank_account"),
        "{b}"
    );
}

#[sqlx::test]
async fn permissions_isolation_and_replay(pool: PgPool) {
    let (w, bank) = banked(pool).await;
    let p = w.bank_money(&bank, "payment", "40.00").await;
    let j = w.bank_line(&p).await;
    let d = w.today(0).await;
    let st = w
        .statement(&bank, "ST-1", "0", "-40.00", &[row(1, &d, "-40.00", None)])
        .await;
    let lines = w.lines(&st).await;

    // Read-only member: reads yes, reconcile / reverse / statement import apply no.
    let reader = member_with(
        &w.api.app,
        w.pool(),
        &w.api.ws,
        "reader@example.com",
        &[
            "accounting.read",
            "import.create",
            "import.upload",
            "import.validate",
            "import.apply",
            "import.get",
        ],
    )
    .await;
    let sid = st["id"].as_str().unwrap();
    let (s, _) = w
        .req_as(
            &reader,
            Method::GET,
            &format!("/accounting/bank-statements/{sid}/lines"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, b) = w
        .reconcile_as(&reader, &uuid(), &lines[0], &[(&j, "-40.00")])
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    let (_, batch) = w
        .staged_statement(
            mapping(&bank, "ST-2", &w.today(-5).await, &d, "-40.00", "-41.00"),
            &[row(1, &d, "-1.00", None)],
        )
        .await;
    let (s, b) = w.apply_import(&reader, &batch).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");

    // Replay: same client_ref and body → same answer; other body → 409.
    let cref = uuid();
    let owner = w.api.owner.clone();
    let (s, first) = w
        .reconcile_as(&owner, &cref, &lines[0], &[(&j, "-40.00")])
        .await;
    assert_eq!(s, StatusCode::CREATED, "{first}");
    let (s, again) = w
        .reconcile_as(&owner, &cref, &lines[0], &[(&j, "-40.00")])
        .await;
    assert_eq!((s, &again), (StatusCode::CREATED, &first));
    let (s, b) = w
        .reconcile_as(&owner, &cref, &lines[0], &[(&j, "-4.00")])
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    let rec = first["data"].clone();
    let (s, b) = w
        .req_as(
            &reader,
            Method::POST,
            &format!(
                "/accounting/bank-reconciliations/{}/reverse",
                rec["id"].as_str().unwrap()
            ),
            Some(
                json!({ "client_ref": uuid(), "expected_version": rec["version"],
                         "payload": { "reason": "no" } }),
            ),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");

    // Another workspace (active book too) sees none of it.
    let (other, ows) = signup(&w.api.app, "eve@example.com", "eve").await;
    activate(w.pool(), &ows).await;
    let call = |method: Method, tail: String, body: Option<Value>| {
        let (app, other, ows) = (w.api.app.clone(), other.clone(), ows.clone());
        async move {
            let (s, _) = send_raw(
                &app,
                method,
                &format!("/v1/workspace/{ows}{tail}"),
                body,
                Some(&other),
                &[("idempotency-key", uuid().as_str())],
            )
            .await;
            s
        }
    };
    assert_eq!(
        call(
            Method::GET,
            format!("/accounting/bank-statements/{sid}"),
            None
        )
        .await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            Method::GET,
            format!("/accounting/bank-statements/{sid}/lines"),
            None
        )
        .await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            Method::POST,
            "/accounting/bank-reconciliations".into(),
            Some(reconcile_body(&uuid(), &lines[0], &[(&j, "-40.00")]))
        )
        .await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            Method::POST,
            format!(
                "/accounting/bank-reconciliations/{}/reverse",
                rec["id"].as_str().unwrap()
            ),
            Some(
                json!({ "client_ref": uuid(), "expected_version": 1, "payload": { "reason": "x" } })
            )
        )
        .await,
        StatusCode::NOT_FOUND
    );
    let (s, list) = send_raw(
        &w.api.app,
        Method::GET,
        &format!("/v1/workspace/{ows}/accounting/bank-statements"),
        None,
        Some(&other),
        &[],
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let list: Value = serde_json::from_slice(&list).unwrap();
    assert_eq!(list["data"], json!([]));
}

async fn activate(pool: &PgPool, ws: &str) {
    sqlx::query(
        "UPDATE accounting_settings SET state = 'active', timezone = 'UTC',
                functional_currency_id = (SELECT id FROM currencies WHERE workspace_id = $1::uuid
                                          ORDER BY id LIMIT 1),
                cutover_at = now(), activated_at = now(),
                activated_by = (SELECT created_by_user_id FROM workspaces WHERE id = $1::uuid)
         WHERE workspace_id = $1::uuid",
    )
    .bind(ws)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test]
async fn setup_books_refuse_statements_and_matching(pool: PgPool) {
    let w = world(pool).await;
    let bank = w.bank(&w.eur.clone(), "1100").await;
    let d = w.today(0).await;
    let (s, b) = w
        .staged_statement(
            mapping(&bank, "ST-1", &d, &d, "0", "1.00"),
            &[row(1, &d, "1.00", None)],
        )
        .await;
    assert_eq!(
        (s, code(&b)),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsupported_active_erp_operation"
        ),
        "{b}"
    );
    let (s, b) = w
        .req(
            Method::POST,
            "/accounting/bank-reconciliations",
            Some(reconcile_body(
                &uuid(),
                &json!({ "id": uuid() }),
                &[(&uuid(), "1.00")],
            )),
        )
        .await;
    assert_eq!(
        (s, code(&b)),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsupported_active_erp_operation"
        ),
        "{b}"
    );
}
