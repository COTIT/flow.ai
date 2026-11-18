//! Accounting core tables: CHECKs, composite workspace FKs, calendar triggers, posted-fact
//! immutability and the deferred balance trigger, exercised in raw SQL.

use sqlx::{PgPool, Row};
use uuid::Uuid;

struct Book {
    ws: Uuid,
    user: Uuid,
    currency: Uuid,
}

async fn book(pool: &PgPool, name: &str) -> Book {
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, first_name, last_name, password_hash)
         VALUES ($1, 'O', 'W', 'h') RETURNING id",
    )
    .bind(format!("{name}@x.io"))
    .fetch_one(pool)
    .await
    .unwrap();
    let ws: Uuid = sqlx::query_scalar(
        "INSERT INTO workspaces (name, created_by_user_id) VALUES ($2, $1) RETURNING id",
    )
    .bind(user)
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap();
    let currency = sqlx::query_scalar(
        "INSERT INTO currencies (workspace_id, name, asset_type, decimals, active)
         VALUES ($1, 'Euro', 'EUR', 2, true) RETURNING id",
    )
    .bind(ws)
    .fetch_one(pool)
    .await
    .unwrap();
    Book { ws, user, currency }
}

/// SQLSTATE of a failed statement.
fn code(e: sqlx::Error) -> String {
    e.as_database_error()
        .and_then(|d| d.code())
        .map(|c| c.into_owned())
        .unwrap_or_else(|| panic!("not a database error: {e}"))
}

async fn account(pool: &PgPool, b: &Book, code: &str, ty: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO accounts (workspace_id, code, name, type, created_by)
         VALUES ($1, $2, $2, $3, $4) RETURNING id",
    )
    .bind(b.ws)
    .bind(code)
    .bind(ty)
    .bind(b.user)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn period(pool: &PgPool, b: &Book) -> Uuid {
    let year: Uuid = sqlx::query_scalar(
        "INSERT INTO accounting_fiscal_years (workspace_id, start_date, end_date, created_by)
         VALUES ($1, '2027-01-01', '2028-01-01', $2) RETURNING id",
    )
    .bind(b.ws)
    .bind(b.user)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query_scalar(
        "INSERT INTO accounting_periods (workspace_id, fiscal_year_id, start_date, end_date, created_by)
         VALUES ($1, $2, '2027-01-01', '2027-02-01', $3) RETURNING id",
    )
    .bind(b.ws)
    .bind(year)
    .bind(b.user)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn draft(conn: &mut sqlx::PgConnection, b: &Book) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO journal_entries (workspace_id, kind, accounting_date, currency_id, reason, created_by)
         VALUES ($1, 'manual', '2027-01-15', $2, 'r', $3) RETURNING id",
    )
    .bind(b.ws)
    .bind(b.currency)
    .bind(b.user)
    .fetch_one(conn)
    .await
    .unwrap()
}

async fn line(
    conn: &mut sqlx::PgConnection,
    b: &Book,
    entry: Uuid,
    no: i16,
    acct: Uuid,
    debit: i64,
    credit: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO journal_lines (workspace_id, entry_id, line_no, account_id, debit, credit,
                                    functional_debit, functional_credit)
         VALUES ($1, $2, $3, $4, $5, $6, $5, $6)",
    )
    .bind(b.ws)
    .bind(entry)
    .bind(no)
    .bind(acct)
    .bind(debit)
    .bind(credit)
    .execute(conn)
    .await
    .map(|_| ())
}

/// Mark `entry` posted with number `n` (effect + period filled in).
async fn post(conn: &mut sqlx::PgConnection, b: &Book, entry: Uuid, period: Uuid, n: i64) {
    let effect: Uuid = sqlx::query_scalar(
        "INSERT INTO accounting_effects (workspace_id, source_kind, source_id, effect_kind,
                                         fingerprint, created_by)
         VALUES ($1, 'manual_journal', $2, 'posted', sha256('x'), $3) RETURNING id",
    )
    .bind(b.ws)
    .bind(entry)
    .bind(b.user)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE journal_entries SET state = 'posted', number = $2, period_id = $3, effect_id = $4,
                posted_by = $5, posted_at = now()
         WHERE id = $1",
    )
    .bind(entry)
    .bind(n)
    .bind(period)
    .bind(effect)
    .bind(b.user)
    .execute(conn)
    .await
    .unwrap();
}

#[sqlx::test]
async fn every_workspace_gets_one_setup_book(pool: PgPool) {
    let b = book(&pool, "alpha").await;
    let row = sqlx::query("SELECT state, version FROM accounting_settings WHERE workspace_id = $1")
        .bind(b.ws)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("state"), "setup");
    assert_eq!(row.get::<i64, _>("version"), 1);
    let half_active = sqlx::query(
        "UPDATE accounting_settings SET state = 'active', functional_currency_id = $2
         WHERE workspace_id = $1",
    )
    .bind(b.ws)
    .bind(b.currency)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(
        code(half_active),
        "23514",
        "active needs timezone, cutover, activator"
    );
    let other = book(&pool, "beta").await;
    let foreign = sqlx::query(
        "UPDATE accounting_settings SET functional_currency_id = $2 WHERE workspace_id = $1",
    )
    .bind(b.ws)
    .bind(other.currency)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(code(foreign), "23503", "another book's currency");
}

#[sqlx::test]
async fn accounts_and_mappings_stay_in_one_book(pool: PgPool) {
    let b = book(&pool, "alpha").await;
    let other = book(&pool, "beta").await;
    let cash = account(&pool, &b, "1000", "asset").await;
    let dup = sqlx::query(
        "INSERT INTO accounts (workspace_id, code, name, type, created_by)
         VALUES ($1, '1000', 'x', 'asset', $2)",
    )
    .bind(b.ws)
    .bind(b.user)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(code(dup), "23505");
    let bad_type = sqlx::query(
        "INSERT INTO accounts (workspace_id, code, name, type, created_by)
         VALUES ($1, '9', 'x', 'revenue', $2)",
    )
    .bind(b.ws)
    .bind(b.user)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(code(bad_type), "23514");
    let cross_type_parent = sqlx::query(
        "INSERT INTO accounts (workspace_id, code, name, type, parent_id, created_by)
         VALUES ($1, '4000', 'Sales', 'income', $2, $3)",
    )
    .bind(b.ws)
    .bind(cash)
    .bind(b.user)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(code(cross_type_parent), "23503", "parent must share type");
    let foreign_parent = sqlx::query(
        "INSERT INTO accounts (workspace_id, code, name, type, parent_id, created_by)
         VALUES ($1, '1001', 'Till', 'asset', $2, $3)",
    )
    .bind(other.ws)
    .bind(cash)
    .bind(other.user)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(code(foreign_parent), "23503");
    let unknown_role = sqlx::query(
        "INSERT INTO account_mappings (workspace_id, role, account_id) VALUES ($1, 'misc', $2)",
    )
    .bind(b.ws)
    .bind(cash)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(code(unknown_role), "23514");
    let foreign_map = sqlx::query(
        "INSERT INTO account_mappings (workspace_id, role, account_id) VALUES ($1, 'cash', $2)",
    )
    .bind(other.ws)
    .bind(cash)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(code(foreign_map), "23503");
}

#[sqlx::test]
async fn calendar_ranges_never_overlap_and_periods_stay_inside_years(pool: PgPool) {
    let b = book(&pool, "alpha").await;
    let p = period(&pool, &b).await;
    let year: Uuid =
        sqlx::query_scalar("SELECT fiscal_year_id FROM accounting_periods WHERE id = $1")
            .bind(p)
            .fetch_one(&pool)
            .await
            .unwrap();
    let insert_year = |start: &'static str, end: &'static str| {
        sqlx::query(
            "INSERT INTO accounting_fiscal_years (workspace_id, start_date, end_date, created_by)
             VALUES ($1, $2::date, $3::date, $4)",
        )
        .bind(b.ws)
        .bind(start)
        .bind(end)
        .bind(b.user)
        .execute(&pool)
    };
    assert_eq!(
        code(insert_year("2027-06-01", "2028-06-01").await.unwrap_err()),
        "23P01"
    );
    assert_eq!(
        code(insert_year("2029-01-01", "2029-01-01").await.unwrap_err()),
        "23514"
    );
    assert_eq!(
        code(insert_year("2029-01-01", "2031-06-01").await.unwrap_err()),
        "23514",
        "max 24 months"
    );
    insert_year("2028-01-01", "2029-01-01")
        .await
        .expect("adjacent year");
    let insert_period = |start: &'static str, end: &'static str| {
        sqlx::query(
            "INSERT INTO accounting_periods (workspace_id, fiscal_year_id, start_date, end_date, created_by)
             VALUES ($1, $2, $3::date, $4::date, $5)",
        )
        .bind(b.ws)
        .bind(year)
        .bind(start)
        .bind(end)
        .bind(b.user)
        .execute(&pool)
    };
    assert_eq!(
        code(insert_period("2027-01-15", "2027-02-15").await.unwrap_err()),
        "23P01"
    );
    assert_eq!(
        code(insert_period("2027-12-01", "2028-01-02").await.unwrap_err()),
        "23514"
    );
    insert_period("2027-02-01", "2027-03-01")
        .await
        .expect("adjacent period");
    let other = book(&pool, "beta").await;
    let foreign_year = sqlx::query(
        "INSERT INTO accounting_periods (workspace_id, fiscal_year_id, start_date, end_date, created_by)
         VALUES ($1, $2, '2027-03-01', '2027-04-01', $3)",
    )
    .bind(other.ws)
    .bind(year)
    .bind(other.user)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(["23503", "23514"].contains(&code(foreign_year).as_str()));
}

#[sqlx::test]
async fn commands_and_effects_are_unique_per_identity(pool: PgPool) {
    let b = book(&pool, "alpha").await;
    let client_ref = Uuid::now_v7();
    let insert = || {
        sqlx::query(
            "INSERT INTO accounting_commands (workspace_id, operation, client_ref, user_id,
                                              idempotency_key, request_hash)
             VALUES ($1, 'accounting_journals_create', $2, $3, 'k', sha256('a'))",
        )
        .bind(b.ws)
        .bind(client_ref)
        .bind(b.user)
        .execute(&pool)
    };
    insert().await.unwrap();
    assert_eq!(code(insert().await.unwrap_err()), "23505");
    let bad_status =
        sqlx::query("UPDATE accounting_commands SET status = 422 WHERE client_ref = $1")
            .bind(client_ref)
            .execute(&pool)
            .await
            .unwrap_err();
    assert_eq!(code(bad_status), "23514", "only 2xx answers are stored");
    let source = Uuid::now_v7();
    let effect = || {
        sqlx::query(
            "INSERT INTO accounting_effects (workspace_id, source_kind, source_id, effect_kind,
                                             fingerprint, created_by)
             VALUES ($1, 'manual_journal', $2, 'reversed', sha256('a'), $3)",
        )
        .bind(b.ws)
        .bind(source)
        .bind(b.user)
        .execute(&pool)
    };
    effect().await.unwrap();
    assert_eq!(code(effect().await.unwrap_err()), "23505");
    let edit =
        sqlx::query("UPDATE accounting_effects SET effect_kind = 'posted' WHERE source_id = $1")
            .bind(source)
            .execute(&pool)
            .await
            .unwrap_err();
    assert_eq!(code(edit), "23514", "effects are append-only");
}

#[sqlx::test]
async fn lines_hold_one_nonnegative_side_and_own_accounts(pool: PgPool) {
    let b = book(&pool, "alpha").await;
    let other = book(&pool, "beta").await;
    let a = account(&pool, &b, "1000", "asset").await;
    let foreign = account(&pool, &other, "1000", "asset").await;
    let mut conn = pool.acquire().await.unwrap();
    let e = draft(&mut conn, &b).await;
    assert_eq!(
        code(line(&mut conn, &b, e, 1, a, 5, 5).await.unwrap_err()),
        "23514",
        "both sides"
    );
    assert_eq!(
        code(line(&mut conn, &b, e, 1, a, 0, 0).await.unwrap_err()),
        "23514",
        "no side"
    );
    assert_eq!(
        code(line(&mut conn, &b, e, 1, a, -5, 0).await.unwrap_err()),
        "23514",
        "negative"
    );
    assert_eq!(
        code(line(&mut conn, &b, e, 1, foreign, 5, 0).await.unwrap_err()),
        "23503"
    );
    line(&mut conn, &b, e, 1, a, 5, 0).await.unwrap();
    assert_eq!(
        code(line(&mut conn, &b, e, 1, a, 0, 5).await.unwrap_err()),
        "23505",
        "line_no"
    );
}

#[sqlx::test]
async fn posting_unbalanced_fails_at_commit(pool: PgPool) {
    let b = book(&pool, "alpha").await;
    let p = period(&pool, &b).await;
    let (a, c) = (
        account(&pool, &b, "1000", "asset").await,
        account(&pool, &b, "3000", "equity").await,
    );
    let mut tx = pool.begin().await.unwrap();
    let e = draft(&mut tx, &b).await;
    line(&mut tx, &b, e, 1, a, 500, 0).await.unwrap();
    line(&mut tx, &b, e, 2, c, 0, 499).await.unwrap();
    post(&mut tx, &b, e, p, 1).await;
    assert_eq!(
        code(tx.commit().await.unwrap_err()),
        "23514",
        "deferred balance trigger"
    );

    let mut tx = pool.begin().await.unwrap();
    let e = draft(&mut tx, &b).await;
    line(&mut tx, &b, e, 1, a, 500, 0).await.unwrap();
    post(&mut tx, &b, e, p, 1).await;
    assert_eq!(
        code(tx.commit().await.unwrap_err()),
        "23514",
        "one line never balances"
    );

    let mut tx = pool.begin().await.unwrap();
    let e = draft(&mut tx, &b).await;
    line(&mut tx, &b, e, 1, a, 500, 0).await.unwrap();
    line(&mut tx, &b, e, 2, c, 0, 500).await.unwrap();
    post(&mut tx, &b, e, p, 1).await;
    tx.commit().await.expect("balanced entry commits");
}

#[sqlx::test]
async fn posted_entries_and_lines_are_immutable_but_workspace_delete_cascades(pool: PgPool) {
    let b = book(&pool, "alpha").await;
    let p = period(&pool, &b).await;
    let (a, c) = (
        account(&pool, &b, "1000", "asset").await,
        account(&pool, &b, "3000", "equity").await,
    );
    let mut tx = pool.begin().await.unwrap();
    let e = draft(&mut tx, &b).await;
    line(&mut tx, &b, e, 1, a, 500, 0).await.unwrap();
    line(&mut tx, &b, e, 2, c, 0, 500).await.unwrap();
    post(&mut tx, &b, e, p, 7).await;
    tx.commit().await.unwrap();

    for sql in [
        "UPDATE journal_entries SET reason = 'edited' WHERE id = $1",
        "UPDATE journal_entries SET state = 'draft' WHERE id = $1",
        "DELETE FROM journal_entries WHERE id = $1",
        "UPDATE journal_lines SET debit = 600, functional_debit = 600 WHERE entry_id = $1 AND line_no = 1",
        "DELETE FROM journal_lines WHERE entry_id = $1",
    ] {
        let err = sqlx::query(sql).bind(e).execute(&pool).await.unwrap_err();
        assert_eq!(code(err), "23514", "{sql}");
    }
    let mut conn = pool.acquire().await.unwrap();
    assert_eq!(
        code(line(&mut conn, &b, e, 3, a, 1, 0).await.unwrap_err()),
        "23514",
        "no new lines"
    );
    drop(conn);

    sqlx::query("DELETE FROM workspaces WHERE id = $1")
        .bind(b.ws)
        .execute(&pool)
        .await
        .expect("workspace delete still cascades");
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM journal_lines")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}
