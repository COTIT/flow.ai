//! Narrow reads for the price calculator: only the columns a line needs, live rows of one
//! workspace.

use rust_decimal::Decimal;
use sqlx::PgExecutor;
use uuid::Uuid;

/// What a product line takes from its product.
pub struct PricingProduct {
    pub id: Uuid,
    pub name: String,
    pub impact_on_wholesale_price: Option<Decimal>,
    pub wholesale_price: Option<Decimal>,
    pub supplier_wholesale_price: Option<Decimal>,
    pub gross_price: Decimal,
}

/// Live products of `workspace_id` among `ids`, in one query.
pub async fn products(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<PricingProduct>, sqlx::Error> {
    sqlx::query_as!(
        PricingProduct,
        "SELECT id, name, impact_on_wholesale_price, wholesale_price, supplier_wholesale_price,
                gross_price
         FROM products
         WHERE id = ANY($1) AND workspace_id = $2 AND deleted_at IS NULL",
        ids,
        workspace_id,
    )
    .fetch_all(db)
    .await
}

/// True when `id` is a live relation of `workspace_id`.
pub async fn relation_exists(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM relations
                          WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL) AS "e!""#,
        id,
        workspace_id,
    )
    .fetch_one(db)
    .await
}

/// One line of `price_lines` (document columns repeat on every row). Money is NULL when
/// `overflow_at` is set; margin columns also when `margin_overflow_at` is.
pub struct LineRow {
    pub n: i32,
    pub gross: Option<Decimal>,
    pub discount: Option<Decimal>,
    pub excl: Option<Decimal>,
    pub line_vat: Option<Decimal>,
    pub rate: Decimal,
    pub rate_first: i32,
    pub rate_base: Option<Decimal>,
    pub rate_vat: Option<Decimal>,
    pub gross_total: Option<Decimal>,
    pub discount_total: Option<Decimal>,
    pub excl_total: Option<Decimal>,
    pub vat_total: Option<Decimal>,
    pub total: Option<Decimal>,
    pub cost: Option<Decimal>,
    pub revenue: Option<Decimal>,
    pub margin_amount: Option<Decimal>,
    pub margin_percentage: Option<f64>,
    pub overflow_at: Option<i32>,
    pub margin_overflow_at: Option<i32>,
}

/// `price_lines` over `lines` at `dp`, in line order. Pure maths: no workspace data read.
pub async fn price_lines(
    db: impl PgExecutor<'_>,
    dp: u32,
    lines: &[super::engine::Line],
) -> Result<Vec<LineRow>, sqlx::Error> {
    let col = |f: fn(&super::engine::Line) -> Decimal| lines.iter().map(f).collect::<Vec<_>>();
    let purchase: Vec<Option<Decimal>> = lines.iter().map(|l| l.purchase_price).collect();
    let set_child: Vec<bool> = lines.iter().map(|l| l.set_child).collect();
    sqlx::query_as!(
        LineRow,
        r#"SELECT n AS "n!", gross, discount, excl, line_vat, rate AS "rate!",
                  rate_first AS "rate_first!", rate_base, rate_vat, gross_total, discount_total,
                  excl_total, vat_total, total, cost, revenue, margin_amount, margin_percentage,
                  overflow_at, margin_overflow_at
           FROM price_lines($1, $2, $3, $4, $5, $6, $7) ORDER BY n"#,
        dp as i32,
        &col(|l| l.quantity),
        &col(|l| l.price),
        &col(|l| l.vat),
        &col(|l| l.discount),
        &purchase as &[Option<Decimal>],
        &set_child,
    )
    .fetch_all(db)
    .await
}

/// `price_margin` row: margin of lines whose revenue (excl per line) is given.
pub struct MarginRow {
    pub cost: Option<Decimal>,
    pub revenue: Option<Decimal>,
    pub margin_amount: Option<Decimal>,
    pub margin_percentage: Option<f64>,
    pub margin_overflow_at: Option<i32>,
}

/// `price_margin` of `lines` with `revenues` (same order) at `dp`.
pub async fn price_margin(
    db: impl PgExecutor<'_>,
    dp: u32,
    lines: &[super::engine::Line],
    revenues: &[Decimal],
) -> Result<MarginRow, sqlx::Error> {
    let quantity: Vec<Decimal> = lines.iter().map(|l| l.quantity).collect();
    let purchase: Vec<Option<Decimal>> = lines.iter().map(|l| l.purchase_price).collect();
    let set_child: Vec<bool> = lines.iter().map(|l| l.set_child).collect();
    sqlx::query_as!(
        MarginRow,
        "SELECT cost, revenue, margin_amount, margin_percentage, margin_overflow_at
         FROM price_margin($1, $2, $3, $4, $5)",
        dp as i32,
        &quantity,
        &purchase as &[Option<Decimal>],
        &set_child,
        revenues,
    )
    .fetch_one(db)
    .await
}
