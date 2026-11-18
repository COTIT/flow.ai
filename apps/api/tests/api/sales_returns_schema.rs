//! Sales return tables in raw SQL (Flow ERP M4, masterplan §12 CUSTOMER CREDIT / RESTOCK):
//! lines scoped to their return's recognition, posted money fixed except the reversal
//! projection, legacy return costs fixed except their consumed projection, restocks only
//! posted → reversed, workspace deletion still cascades.

use sqlx::PgPool;
use uuid::Uuid;

use crate::shipments::S;

fn code(e: sqlx::Error) -> String {
    e.as_database_error()
        .and_then(|d| d.code())
        .map(|c| c.into_owned())
        .unwrap_or_else(|| panic!("not a database error: {e}"))
}

/// Active book with one posted shipment of 2 Honey; (world, order, order line, recognition,
/// recognition line).
pub(crate) async fn shipped(pool: PgPool) -> (S, String, String, Uuid, Uuid) {
    let s = crate::shipments::world_without_stock(pool).await;
    crate::sales_recognition::activate_sales(s.pool(), s.ws()).await;
    crate::inventory_valuation::seed(s.pool(), s.ws(), &s.honey, &s.main, None, "10", 1000).await;
    let (o, line) = s.open_order("4").await;
    s.ship(&o, &[(&line, "2")]).await;
    let (rec, rec_line): (Uuid, Uuid) = sqlx::query_as(
        "SELECT r.id, l.id FROM sales_recognitions r
         JOIN sales_recognition_lines l ON l.recognition_id = r.id
         WHERE r.order_id = $1::uuid",
    )
    .bind(&o)
    .fetch_one(s.pool())
    .await
    .unwrap();
    (s, o, line, rec, rec_line)
}

async fn draft(s: &S, rec: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO sales_returns (workspace_id, client_ref, kind, recognition_id, reason,
             accounting_date, created_by)
         VALUES ($1::uuid, $2, 'receipt_credit', $3, 'damaged', current_date,
                 (SELECT created_by_user_id FROM workspaces WHERE id = $1::uuid))
         RETURNING id",
    )
    .bind(s.ws())
    .bind(Uuid::now_v7())
    .bind(rec)
    .fetch_one(s.pool())
    .await
    .unwrap()
}

async fn add_line(s: &S, ret: Uuid, rec: Uuid, line: Uuid) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO sales_return_lines (workspace_id, return_id, recognition_id,
             recognition_line_id, quantity)
         VALUES ($1::uuid, $2, $3, $4, 1) RETURNING id",
    )
    .bind(s.ws())
    .bind(ret)
    .bind(rec)
    .bind(line)
    .fetch_one(s.pool())
    .await
}

/// Mark `ret` posted with fabricated money pointing at the recognition's own journal/effect.
async fn post(s: &S, ret: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE sales_returns r SET state = 'posted', net = 1, tax = 0, rounding = 0,
                credit_item_id = (SELECT id FROM accounting_open_items
                                  WHERE recognition_id = r.recognition_id LIMIT 1),
                journal_entry_id = g.journal_entry_id, effect_id = g.effect_id,
                posted_by = r.created_by, posted_at = now()
         FROM sales_recognitions g WHERE g.id = r.recognition_id AND r.id = $1",
    )
    .bind(ret)
    .execute(s.pool())
    .await
    .map(|_| ())
}

#[sqlx::test]
async fn return_lines_stay_inside_their_recognition(pool: PgPool) {
    let (s, _, _, rec, rec_line) = shipped(pool).await;
    let ret = draft(&s, rec).await;
    add_line(&s, ret, rec, rec_line).await.unwrap();
    // Same line twice.
    assert_eq!(
        code(add_line(&s, ret, rec, rec_line).await.unwrap_err()),
        "23505"
    );
    // A line of another recognition, named under this return.
    let (o2, l2) = s.open_order("1").await;
    s.ship(&o2, &[(&l2, "1")]).await;
    let (rec2, line2): (Uuid, Uuid) = sqlx::query_as(
        "SELECT r.id, l.id FROM sales_recognitions r
         JOIN sales_recognition_lines l ON l.recognition_id = r.id WHERE r.order_id = $1::uuid",
    )
    .bind(&o2)
    .fetch_one(s.pool())
    .await
    .unwrap();
    assert_eq!(
        code(add_line(&s, ret, rec, line2).await.unwrap_err()),
        "23503"
    );
    assert_eq!(
        code(add_line(&s, ret, rec2, line2).await.unwrap_err()),
        "23503"
    );
    // Another workspace can't credit this recognition.
    let other = s.api.as_new_owner("eve@example.com", "evil").await;
    let e = sqlx::query(
        "INSERT INTO sales_returns (workspace_id, client_ref, kind, recognition_id, reason,
             accounting_date, created_by)
         VALUES ($1::uuid, $2, 'receipt_credit', $3, 'x', current_date, $4)",
    )
    .bind(&other.ws)
    .bind(Uuid::now_v7())
    .bind(rec)
    .bind(Uuid::now_v7())
    .execute(s.pool())
    .await
    .unwrap_err();
    assert_eq!(code(e), "23503");
}

#[sqlx::test]
async fn posted_returns_keep_their_money_and_reverse_only(pool: PgPool) {
    let (s, _, _, rec, rec_line) = shipped(pool).await;
    let ret = draft(&s, rec).await;
    add_line(&s, ret, rec, rec_line).await.unwrap();
    // Posted state needs its money and links.
    let e = sqlx::query("UPDATE sales_returns SET state = 'posted' WHERE id = $1")
        .bind(ret)
        .execute(s.pool())
        .await
        .unwrap_err();
    assert_eq!(code(e), "23514");
    // A draft line changes freely; money all or nothing.
    sqlx::query("UPDATE sales_return_lines SET net = 1, tax = 0 WHERE return_id = $1")
        .bind(ret)
        .execute(s.pool())
        .await
        .unwrap();
    let e = sqlx::query("UPDATE sales_return_lines SET net = NULL WHERE return_id = $1")
        .bind(ret)
        .execute(s.pool())
        .await
        .unwrap_err();
    assert_eq!(code(e), "23514");
    post(&s, ret).await.unwrap();
    for sql in [
        "UPDATE sales_returns SET net = 2 WHERE id = $1",
        "UPDATE sales_returns SET state = 'cancelled', cancelled_by = created_by,
                cancelled_at = now(), cancel_reason = 'x' WHERE id = $1",
        "DELETE FROM sales_returns WHERE id = $1",
        "UPDATE sales_return_lines SET quantity = 2 WHERE return_id = $1",
        "DELETE FROM sales_return_lines WHERE return_id = $1",
    ] {
        let e = sqlx::query(sql)
            .bind(ret)
            .execute(s.pool())
            .await
            .unwrap_err();
        assert_eq!(code(e), "23514", "{sql}");
    }
    assert_eq!(
        code(add_line(&s, ret, rec, rec_line).await.unwrap_err()),
        "23514"
    );
    // posted → reversed keeps money and moves only the reversal columns.
    let reverse = "UPDATE sales_returns SET state = 'reversed', reversal_effect_id = effect_id,
                          reversed_by = created_by, reversed_at = now(), reversal_reason = 'x',
                          version = version + 1, net = $2 WHERE id = $1";
    let e = sqlx::query(reverse)
        .bind(ret)
        .bind(5_i64)
        .execute(s.pool())
        .await
        .unwrap_err();
    assert_eq!(code(e), "23514");
    sqlx::query(reverse)
        .bind(ret)
        .bind(1_i64)
        .execute(s.pool())
        .await
        .unwrap();
    let e = sqlx::query("UPDATE sales_returns SET version = version + 1 WHERE id = $1")
        .bind(ret)
        .execute(s.pool())
        .await
        .unwrap_err();
    assert_eq!(code(e), "23514");
    // Workspace deletion still cascades.
    sqlx::query("DELETE FROM workspaces WHERE id = $1::uuid")
        .bind(s.ws())
        .execute(s.pool())
        .await
        .unwrap();
}

#[sqlx::test]
async fn legacy_costs_and_restocks_are_fixed_facts(pool: PgPool) {
    let (s, o, line, rec, rec_line) = shipped(pool).await;
    let cost: Uuid = sqlx::query_scalar(
        "INSERT INTO legacy_return_costs (workspace_id, client_ref, order_id, order_line_id,
             quantity, restoration_amount, evidence_ref, reason, reviewed_by)
         VALUES ($1::uuid, $2, $3::uuid, $4::uuid, 2, 300, 'note 7', 'pre-book sale', $2)
         RETURNING id",
    )
    .bind(s.ws())
    .bind(Uuid::now_v7())
    .bind(&o)
    .bind(&line)
    .fetch_one(s.pool())
    .await
    .unwrap();
    sqlx::query(
        "UPDATE legacy_return_costs SET consumed_quantity = 1, consumed_value = 150 WHERE id = $1",
    )
    .bind(cost)
    .execute(s.pool())
    .await
    .unwrap();
    for sql in [
        "UPDATE legacy_return_costs SET consumed_quantity = 3 WHERE id = $1",
        "UPDATE legacy_return_costs SET consumed_value = 301 WHERE id = $1",
        "UPDATE legacy_return_costs SET restoration_amount = 1 WHERE id = $1",
        "DELETE FROM legacy_return_costs WHERE id = $1",
    ] {
        let e = sqlx::query(sql)
            .bind(cost)
            .execute(s.pool())
            .await
            .unwrap_err();
        assert_eq!(code(e), "23514", "{sql}");
    }
    let ret = draft(&s, rec).await;
    let ret_line = add_line(&s, ret, rec, rec_line).await.unwrap();
    let restock: Uuid = sqlx::query_scalar(
        "INSERT INTO sales_return_restocks (workspace_id, client_ref, return_id, warehouse_id,
             accounting_date, created_by)
         VALUES ($1::uuid, $2, $3, $4::uuid, current_date, $2) RETURNING id",
    )
    .bind(s.ws())
    .bind(Uuid::now_v7())
    .bind(ret)
    .bind(&s.main)
    .fetch_one(s.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sales_return_restock_lines (workspace_id, restock_id, return_line_id,
             quantity, value, legacy_return_cost_id)
         VALUES ($1::uuid, $2, $3, 1, 150, $4)",
    )
    .bind(s.ws())
    .bind(restock)
    .bind(ret_line)
    .bind(cost)
    .execute(s.pool())
    .await
    .unwrap();
    for sql in [
        "UPDATE sales_return_restocks SET accounting_date = accounting_date - 1 WHERE id = $1",
        "UPDATE sales_return_restock_lines SET value = 1 WHERE restock_id = $1",
        "DELETE FROM sales_return_restock_lines WHERE restock_id = $1",
        "DELETE FROM sales_return_restocks WHERE id = $1",
    ] {
        let e = sqlx::query(sql)
            .bind(restock)
            .execute(s.pool())
            .await
            .expect_err(sql);
        assert_eq!(code(e), "23514", "{sql}");
    }
    sqlx::query(
        "UPDATE sales_return_restocks SET state = 'reversed', reversed_by = created_by,
                reversed_at = now(), reversal_reason = 'wrong shelf', version = 2 WHERE id = $1",
    )
    .bind(restock)
    .execute(s.pool())
    .await
    .unwrap();
    let e = sqlx::query("UPDATE sales_return_restocks SET version = 3 WHERE id = $1")
        .bind(restock)
        .execute(s.pool())
        .await
        .unwrap_err();
    assert_eq!(code(e), "23514");
    // Workspace deletion still cascades.
    sqlx::query("DELETE FROM workspaces WHERE id = $1::uuid")
        .bind(s.ws())
        .execute(s.pool())
        .await
        .unwrap();
}
