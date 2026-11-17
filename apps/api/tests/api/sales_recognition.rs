//! Sales recognition (Flow ERP M4, masterplan §12 RECOGNITION, §19 H/I): shipment and
//! recognition tables in raw SQL (parent-scoped lines, immutability, one recognition per
//! source, one billing link per recognition), then active-book recognition journals.

use crate::shipments::world;

use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// SQLSTATE of a failed statement.
fn code(e: sqlx::Error) -> String {
    e.as_database_error()
        .and_then(|d| d.code())
        .map(|c| c.into_owned())
        .unwrap_or_else(|| panic!("not a database error: {e}"))
}

async fn user_of(pool: &PgPool, ws: &str) -> Uuid {
    sqlx::query_scalar("SELECT created_by_user_id FROM workspaces WHERE id = $1::uuid")
        .bind(ws)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn shipment(pool: &PgPool, ws: &str, order: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO shipments (workspace_id, order_id, created_by)
         VALUES ($1::uuid, $2::uuid, (SELECT created_by_user_id FROM workspaces WHERE id = $1::uuid))
         RETURNING id",
    )
    .bind(ws)
    .bind(order)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn ship_line(
    pool: &PgPool,
    ws: &str,
    shipment: Uuid,
    order: &str,
    line: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO shipment_lines (workspace_id, shipment_id, order_id, order_line_id, quantity)
         VALUES ($1::uuid, $2, $3::uuid, $4::uuid, 1)",
    )
    .bind(ws)
    .bind(shipment)
    .bind(order)
    .bind(line)
    .execute(pool)
    .await
    .map(|_| ())
}

#[sqlx::test]
async fn shipment_lines_stay_inside_their_order(pool: PgPool) {
    let s = world(pool).await;
    let (a, a_line) = s.open_order("2").await;
    let (_, b_line) = s.open_order("2").await;
    let sh = shipment(s.pool(), s.ws(), &a).await;
    ship_line(s.pool(), s.ws(), sh, &a, &a_line).await.unwrap();
    // A line of another order (FK on order + line) and a shipment named under another order.
    let e = ship_line(s.pool(), s.ws(), sh, &a, &b_line).await.unwrap_err();
    assert_eq!(code(e), "23503");
    // Same line twice in one shipment.
    let e = ship_line(s.pool(), s.ws(), sh, &a, &a_line).await.unwrap_err();
    assert_eq!(code(e), "23505");
    // Another workspace can't name this order.
    let other = s.api.as_new_owner("eve@example.com", "evil").await;
    let e = sqlx::query(
        "INSERT INTO shipments (workspace_id, order_id, created_by) VALUES ($1::uuid, $2::uuid, $3)",
    )
    .bind(&other.ws)
    .bind(&a)
    .bind(user_of(s.pool(), &other.ws).await)
    .execute(s.pool())
    .await
    .unwrap_err();
    assert_eq!(code(e), "23503");
}

#[sqlx::test]
async fn posted_and_cancelled_shipments_are_fixed(pool: PgPool) {
    let s = world(pool).await;
    let (o, line) = s.open_order("2").await;
    let sh = shipment(s.pool(), s.ws(), &o).await;
    ship_line(s.pool(), s.ws(), sh, &o, &line).await.unwrap();
    // A draft changes freely.
    sqlx::query("UPDATE shipments SET note = 'x' WHERE id = $1")
        .bind(sh)
        .execute(s.pool())
        .await
        .unwrap();
    sqlx::query(
        "UPDATE shipments SET state = 'posted', accounting_date = current_date, posted_at = now(),
                posted_by = created_by WHERE id = $1",
    )
    .bind(sh)
    .execute(s.pool())
    .await
    .unwrap();
    for sql in [
        "UPDATE shipments SET note = 'y' WHERE id = $1",
        "DELETE FROM shipments WHERE id = $1",
        "UPDATE shipment_lines SET quantity = 2 WHERE shipment_id = $1",
        "DELETE FROM shipment_lines WHERE shipment_id = $1",
    ] {
        let e = sqlx::query(sql)
            .bind(sh)
            .execute(s.pool())
            .await
            .unwrap_err();
        assert_eq!(code(e), "23514", "{sql}");
    }
    // A posted shipment's lines take no new line either.
    let e = sqlx::query(
        "INSERT INTO shipment_lines (workspace_id, shipment_id, order_id, order_line_id, quantity)
         SELECT workspace_id, $1, order_id, $2::uuid, 1 FROM shipments WHERE id = $1",
    )
    .bind(sh)
    .bind(&line)
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

/// A recognition of `order` with a draft journal and an effect (FK targets only).
async fn recognition(pool: &PgPool, ws: &str, order: &str) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "WITH u AS (SELECT created_by_user_id AS id FROM workspaces WHERE id = $1::uuid),
              c AS (SELECT currency_id AS id FROM orders WHERE id = $2::uuid),
              e AS (INSERT INTO accounting_effects (workspace_id, source_kind, source_id, effect_kind,
                                                   fingerprint, created_by)
                    SELECT $1::uuid, 'retail_sale', $2::uuid, 'fulfilled', sha256('x'), u.id FROM u
                    ON CONFLICT DO NOTHING RETURNING id),
              j AS (INSERT INTO journal_entries (workspace_id, kind, accounting_date, currency_id,
                                                 reason, created_by)
                    SELECT $1::uuid, 'source', current_date, c.id, 'sale', u.id FROM u, c RETURNING id)
         INSERT INTO sales_recognitions (workspace_id, source_kind, source_id, order_id, currency_id,
                                         accounting_date, net, tax, functional_net, functional_tax,
                                         journal_entry_id, effect_id, created_by)
         SELECT $1::uuid, 'retail_sale', $2::uuid, $2::uuid, c.id, current_date, 100, 21, 100, 21,
                j.id, coalesce(e.id, (SELECT id FROM accounting_effects WHERE source_id = $2::uuid)),
                u.id
         FROM u, c, j LEFT JOIN e ON true
         RETURNING id",
    )
    .bind(ws)
    .bind(order)
    .fetch_one(pool)
    .await
}

#[sqlx::test]
async fn recognitions_are_immutable_and_once_per_source(pool: PgPool) {
    let s = world(pool).await;
    let (o, line) = s.open_order("2").await;
    let r = recognition(s.pool(), s.ws(), &o).await.unwrap();
    let gross: (i64, i64) =
        sqlx::query_as("SELECT gross, functional_gross FROM sales_recognitions WHERE id = $1")
            .bind(r)
            .fetch_one(s.pool())
            .await
            .unwrap();
    assert_eq!(gross, (121, 121));
    sqlx::query(
        "INSERT INTO sales_recognition_lines (workspace_id, recognition_id, order_line_id,
                                              product_id, quantity, net, tax, vat_rate)
         VALUES ($1::uuid, $2, $3::uuid, $4::uuid, 2, 100, 21, 21)",
    )
    .bind(s.ws())
    .bind(r)
    .bind(&line)
    .bind(&s.honey)
    .execute(s.pool())
    .await
    .unwrap();
    let e = recognition(s.pool(), s.ws(), &o).await.unwrap_err();
    assert_eq!(code(e), "23505");
    for sql in [
        "UPDATE sales_recognitions SET net = 1 WHERE id = $1",
        "DELETE FROM sales_recognitions WHERE id = $1",
        "UPDATE sales_recognition_lines SET net = 1 WHERE recognition_id = $1",
        "DELETE FROM sales_recognition_lines WHERE recognition_id = $1",
    ] {
        let e = sqlx::query(sql)
            .bind(r)
            .execute(s.pool())
            .await
            .unwrap_err();
        assert_eq!(code(e), "23514", "{sql}");
    }
}

#[sqlx::test]
async fn one_billing_link_per_recognition_released_with_its_draft(pool: PgPool) {
    let s = world(pool).await;
    let (o, _) = s.open_order("2").await;
    let r = recognition(s.pool(), s.ws(), &o).await.unwrap();
    let s = &s;
    let relation = s
        .api
        .with_base("/relations")
        .create(
            json!({ "company_name": "Shop BV", "type": "business" }),
            "relation_id",
        )
        .await;
    let invoice = || async {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO invoices (workspace_id, relation_id, currency_id)
             SELECT workspace_id, $2::uuid, currency_id FROM orders WHERE id = $1::uuid RETURNING id",
        )
        .bind(&o)
        .bind(&relation)
        .fetch_one(s.pool())
        .await
        .unwrap()
    };
    let (a, b) = (invoice().await, invoice().await);
    let link = |inv: Uuid| async move {
        sqlx::query(
            "INSERT INTO invoice_recognition_links (workspace_id, invoice_id, recognition_id)
             VALUES ($1::uuid, $2, $3)",
        )
        .bind(s.ws())
        .bind(inv)
        .bind(r)
        .execute(s.pool())
        .await
        .map(|_| ())
    };
    link(a).await.unwrap();
    assert_eq!(code(link(b).await.unwrap_err()), "23505");
    sqlx::query("DELETE FROM invoices WHERE id = $1")
        .bind(a)
        .execute(s.pool())
        .await
        .unwrap();
    link(b).await.unwrap();
    let state: String = sqlx::query("SELECT state FROM invoice_recognition_links")
        .fetch_one(s.pool())
        .await
        .unwrap()
        .get(0);
    assert_eq!(state, "draft");
}
