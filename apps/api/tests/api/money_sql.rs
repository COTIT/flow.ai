//! Money math in SQL: `money_round_div` rounds n/d half away from zero, exactly.

use sqlx::PgPool;

async fn div(pool: &PgPool, n: &str, d: &str) -> String {
    sqlx::query_scalar("SELECT money_round_div($1::numeric, $2::numeric)::text")
        .bind(n)
        .bind(d)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test]
async fn ties_round_away_from_zero(pool: PgPool) {
    assert_eq!(div(&pool, "5", "2").await, "3");
    assert_eq!(div(&pool, "-5", "2").await, "-3");
    assert_eq!(div(&pool, "1", "2").await, "1");
    assert_eq!(div(&pool, "-1", "2").await, "-1");
    assert_eq!(div(&pool, "5", "-2").await, "-3");
    assert_eq!(div(&pool, "-5", "-2").await, "3");
    assert_eq!(div(&pool, "1.5", "1").await, "2");
}

#[sqlx::test]
async fn non_ties_round_to_nearest(pool: PgPool) {
    assert_eq!(div(&pool, "1", "3").await, "0");
    assert_eq!(div(&pool, "2", "3").await, "1");
    assert_eq!(div(&pool, "-2", "3").await, "-1");
    assert_eq!(div(&pool, "0", "7").await, "0");
    // moving-average share: 24000 × 5 / 20, 100 × 2 / 3
    assert_eq!(div(&pool, "120000.00", "20.00").await, "6000");
    assert_eq!(div(&pool, "200", "3").await, "67");
}

/// Plain `round(n / d)` rounds the quotient to ~20 digits first and turns this into 1.
#[sqlx::test]
async fn just_under_a_half_stays_down(pool: PgPool) {
    let d = "1000000000000000000000000";
    assert_eq!(div(&pool, "499999999999999999999999", d).await, "0");
    assert_eq!(div(&pool, "-499999999999999999999999", d).await, "0");
    assert_eq!(div(&pool, "500000000000000000000001", d).await, "1");
    assert_eq!(
        div(&pool, "1234567890123456749999999999999999999999", "100").await,
        "12345678901234567500000000000000000000"
    );
}

#[sqlx::test]
async fn fx_conversion_matches_integer_convert(pool: PgPool) {
    // 0.01 EUR × 150 → 2 JPY; × 149.99 → 1; TZS 1000 / 2950.5 → 34 cents; −3 / 2 → −2.
    assert_eq!(div(&pool, "150.00000000", "100").await, "2");
    assert_eq!(div(&pool, "149.99000000", "100").await, "1");
    assert_eq!(div(&pool, "100000", "2950.50000000").await, "34");
    assert_eq!(div(&pool, "-3", "2.00000000").await, "-2");
}

#[sqlx::test]
async fn null_in_null_out(pool: PgPool) {
    let v: Option<String> =
        sqlx::query_scalar("SELECT money_round_div(NULL::numeric, 2::numeric)::text")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(v, None);
}
