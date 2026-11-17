//! Settlement tables (M4): open-item CHECKs, allocation/link/carrying immutability, bank
//! account typing, exercised in raw SQL.

use sqlx::PgPool;
use uuid::Uuid;

struct Book {
    ws: Uuid,
    user: Uuid,
    eur: Uuid,
    asset: Uuid,
    income: Uuid,
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
    let eur = sqlx::query_scalar(
        "INSERT INTO currencies (workspace_id, name, asset_type, decimals, active)
         VALUES ($1, 'Euro', 'EUR', 2, true) RETURNING id",
    )
    .bind(ws)
    .fetch_one(pool)
    .await
    .unwrap();
    let account = |code: &'static str, ty: &'static str| {
        sqlx::query_scalar(
            "INSERT INTO accounts (workspace_id, code, name, type, created_by)
             VALUES ($1, $2, $2, $3, $4) RETURNING id",
        )
        .bind(ws)
        .bind(code)
        .bind(ty)
        .bind(user)
        .fetch_one(pool)
    };
    let asset = account("1300", "asset").await.unwrap();
    let income = account("8000", "income").await.unwrap();
    Book {
        ws,
        user,
        eur,
        asset,
        income,
    }
}

fn code(e: sqlx::Error) -> String {
    e.as_database_error()
        .and_then(|d| d.code())
        .map(|c| c.into_owned())
        .unwrap_or_else(|| panic!("not a database error: {e}"))
}

async fn effect(pool: &PgPool, b: &Book) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO accounting_effects (workspace_id, source_kind, source_id, effect_kind,
             fingerprint, created_by)
         VALUES ($1, 'test', uuidv7(), 'posted', decode(repeat('00', 32), 'hex'), $2) RETURNING id",
    )
    .bind(b.ws)
    .bind(b.user)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn item(
    pool: &PgPool,
    b: &Book,
    class: &str,
    side: &str,
    role: &str,
) -> Result<Uuid, sqlx::Error> {
    let e = effect(pool, b).await;
    sqlx::query_scalar(
        "INSERT INTO accounting_open_items (workspace_id, classification, side, account_role,
             account_id, source_kind, source_id, currency_id, issue_date, origin, amount,
             functional_amount, residual, functional_residual, effect_id, created_by)
         VALUES ($1, $2, $3, $4, $5, 'test', uuidv7(), $6, '2027-01-10', 'posted', 1000, 1000,
                 0, 0, $7, $8) RETURNING id",
    )
    .bind(b.ws)
    .bind(class)
    .bind(side)
    .bind(role)
    .bind(b.asset)
    .bind(b.eur)
    .bind(e)
    .bind(b.user)
    .fetch_one(pool)
    .await
}

#[sqlx::test]
async fn open_items_tie_side_and_role_to_classification(pool: PgPool) {
    let b = book(&pool, "a").await;
    let ok = item(&pool, &b, "receivable", "debit", "unbilled_receivables")
        .await
        .unwrap();
    // Opening residual is the whole amount whatever the insert said.
    let residual: i64 =
        sqlx::query_scalar("SELECT residual FROM accounting_open_items WHERE id = $1")
            .bind(ok)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(residual, 1000);
    for (class, side, role) in [
        ("receivable", "credit", "receivables"),
        ("receivable", "debit", "customer_deposits"),
        ("customer_deposit", "debit", "customer_deposits"),
        ("payable", "credit", "receivables"),
        ("gift", "debit", "receivables"),
    ] {
        let e = item(&pool, &b, class, side, role).await.unwrap_err();
        assert_eq!(code(e), "23514", "{class}/{side}/{role}");
    }
}

#[sqlx::test]
async fn open_items_change_only_their_projections_and_never_go(pool: PgPool) {
    let b = book(&pool, "a").await;
    let id = item(&pool, &b, "receivable", "debit", "receivables")
        .await
        .unwrap();
    for sql in [
        "UPDATE accounting_open_items SET amount = 5 WHERE id = $1",
        "UPDATE accounting_open_items SET residual = 1001 WHERE id = $1",
        "DELETE FROM accounting_open_items WHERE id = $1",
    ] {
        let e = sqlx::query(sql).bind(id).execute(&pool).await.unwrap_err();
        assert_eq!(code(e), "23514", "{sql}");
    }
    let e = sqlx::query("TRUNCATE accounting_open_items CASCADE")
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(code(e), "23514");
    // A deleted workspace still cascades.
    sqlx::query("DELETE FROM workspaces WHERE id = $1")
        .bind(b.ws)
        .execute(&pool)
        .await
        .unwrap();
}

#[sqlx::test]
async fn allocations_and_links_are_append_only(pool: PgPool) {
    let b = book(&pool, "a").await;
    let target = item(&pool, &b, "receivable", "debit", "receivables")
        .await
        .unwrap();
    let e = effect(&pool, &b).await;
    let a: Uuid = sqlx::query_scalar(
        "INSERT INTO accounting_allocations (workspace_id, kind, target_item_id, accounting_date,
             currency_id, amount, effect_id, created_by)
         VALUES ($1, 'rounding', $2, '2027-01-10', $3, 5, $4, $5) RETURNING id",
    )
    .bind(b.ws)
    .bind(target)
    .bind(b.eur)
    .bind(e)
    .bind(b.user)
    .fetch_one(&pool)
    .await
    .unwrap();
    for sql in [
        "UPDATE accounting_allocations SET amount = 6 WHERE id = $1",
        "DELETE FROM accounting_allocations WHERE id = $1",
    ] {
        assert_eq!(
            code(sqlx::query(sql).bind(a).execute(&pool).await.unwrap_err()),
            "23514"
        );
    }
    // Application needs a source item.
    let e2 = effect(&pool, &b).await;
    let bad = sqlx::query(
        "INSERT INTO accounting_allocations (workspace_id, kind, target_item_id, accounting_date,
             currency_id, amount, effect_id, created_by)
         VALUES ($1, 'application', $2, '2027-01-10', $3, 5, $4, $5)",
    )
    .bind(b.ws)
    .bind(target)
    .bind(b.eur)
    .bind(e2)
    .bind(b.user)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(code(bad), "23514");

    let link = |disposition: &'static str, effect: Option<Uuid>| {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO accounting_source_links (workspace_id, source_kind, source_id, effect_id,
                 disposition, fingerprint)
             VALUES ($1, 'op', uuidv7(), $2, $3, decode(repeat('00', 32), 'hex')) RETURNING id",
        )
        .bind(b.ws)
        .bind(effect)
        .bind(disposition)
        .fetch_one(&pool)
    };
    assert_eq!(code(link("posted", None).await.unwrap_err()), "23514");
    let pending = link("pending_observation", None).await.unwrap();
    // Pending may become posted once; nothing else changes.
    sqlx::query(
        "UPDATE accounting_source_links SET disposition = 'posted', effect_id = $2 WHERE id = $1",
    )
    .bind(pending)
    .bind(e)
    .execute(&pool)
    .await
    .unwrap();
    let back = sqlx::query(
        "UPDATE accounting_source_links SET disposition = 'pending_observation' WHERE id = $1",
    )
    .bind(pending)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(code(back), "23514");
}

#[sqlx::test]
async fn carrying_reversals_link_their_original(pool: PgPool) {
    let b = book(&pool, "a").await;
    let it = item(&pool, &b, "receivable", "debit", "receivables")
        .await
        .unwrap();
    let e = effect(&pool, &b).await;
    let bad = sqlx::query(
        "INSERT INTO accounting_carrying_adjustments (workspace_id, item_id, accounting_date,
             functional_delta, kind, effect_id, created_by)
         VALUES ($1, $2, '2027-01-31', 10, 'reversal', $3, $4)",
    )
    .bind(b.ws)
    .bind(it)
    .bind(e)
    .bind(b.user)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(code(bad), "23514");
}

#[sqlx::test]
async fn bank_accounts_sit_on_an_asset_account_of_their_book(pool: PgPool) {
    let b = book(&pool, "a").await;
    let other = book(&pool, "b").await;
    let insert = |ws: Uuid, account: Uuid, name: &'static str| {
        sqlx::query(
            "INSERT INTO bank_accounts (workspace_id, name, currency_id, account_id, created_by)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(ws)
        .bind(name)
        .bind(b.eur)
        .bind(account)
        .bind(b.user)
        .execute(&pool)
    };
    insert(b.ws, b.asset, "Main").await.unwrap();
    assert_eq!(
        code(insert(b.ws, b.income, "Income").await.unwrap_err()),
        "23503"
    );
    assert_eq!(
        code(insert(b.ws, other.asset, "Theirs").await.unwrap_err()),
        "23503"
    );
    assert_eq!(
        code(insert(b.ws, b.asset, "Main").await.unwrap_err()),
        "23505"
    );
}
