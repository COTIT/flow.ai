//! `price_lines` (Postgres) against the former Rust pricing engine: every case of
//! `tests/fixtures/pricing_golden.json` (its frozen output, written before the Rust maths was
//! removed; generator in git history at cbc07f4d9) must come out the same, field by field,
//! out-of-range index included. Margin % compares as f64.

use serde_json::{Value, json};
use sqlx::{PgPool, Row};

const SQL: &str = r#"SELECT n, rate_first, rate::text AS rate,
       trim_scale(gross)::text AS gross, trim_scale(discount)::text AS discount,
       trim_scale(excl)::text AS excl, trim_scale(line_vat)::text AS line_vat,
       trim_scale(rate_base)::text AS rate_base, trim_scale(rate_vat)::text AS rate_vat,
       trim_scale(gross_total)::text AS gross_total,
       trim_scale(discount_total)::text AS discount_total,
       trim_scale(excl_total)::text AS excl_total, trim_scale(vat_total)::text AS vat_total,
       trim_scale(total)::text AS total, trim_scale(cost)::text AS cost,
       trim_scale(revenue)::text AS revenue, trim_scale(margin_amount)::text AS margin_amount,
       margin_percentage, overflow_at, margin_overflow_at
FROM price_lines($1, $2::text[]::numeric[], $3::text[]::numeric[], $4::text[]::numeric[],
                 $5::text[]::numeric[], $6::text[]::numeric[], $7)
ORDER BY n"#;

/// SQL result of one case, shaped like the fixture's `out`.
async fn priced(pool: &PgPool, case: &Value) -> Value {
    let lines = case["lines"].as_array().unwrap();
    let col = |i: usize| -> Vec<Option<String>> {
        lines
            .iter()
            .map(|l| l[i].as_str().map(str::to_owned))
            .collect()
    };
    let set_child: Vec<bool> = lines.iter().map(|l| l[5].as_bool().unwrap()).collect();
    let rows = sqlx::query(SQL)
        .bind(case["dp"].as_i64().unwrap() as i32)
        .bind(col(0))
        .bind(col(1))
        .bind(col(2))
        .bind(col(3))
        .bind(col(4))
        .bind(set_child)
        .fetch_all(pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), lines.len());
    let Some(first) = rows.first() else {
        return json!({ "empty": true });
    };
    let s = |r: &sqlx::postgres::PgRow, c: &str| r.get::<Option<String>, _>(c);
    if let Some(i) = first.get::<Option<i32>, _>("overflow_at") {
        return json!({ "err": i });
    }
    let margin = match first.get::<Option<i32>, _>("margin_overflow_at") {
        Some(i) => json!({ "err": i }),
        None => json!([
            s(first, "cost"),
            s(first, "revenue"),
            s(first, "margin_amount"),
            first.get::<Option<f64>, _>("margin_percentage")
        ]),
    };
    let totals = [
        "gross_total",
        "discount_total",
        "excl_total",
        "vat_total",
        "total",
    ]
    .map(|c| s(first, c));
    json!({
        "lines": rows.iter()
            .map(|r| [s(r, "gross"), s(r, "discount"), s(r, "excl"), s(r, "line_vat")])
            .collect::<Vec<_>>(),
        "vats": rows.iter()
            .filter(|r| r.get::<i32, _>("n") == r.get::<i32, _>("rate_first"))
            .map(|r| [s(r, "rate"), s(r, "rate_base"), s(r, "rate_vat")])
            .collect::<Vec<_>>(),
        "totals": totals,
        "margin": margin,
    })
}

/// Margin % as f64 within rounding of the last bits (Rust divides at 28 digits, SQL exactly).
fn same_margin(want: &Value, got: &Value) -> bool {
    match (want.as_array(), got.as_array()) {
        (Some(w), Some(g)) => {
            let (a, b) = (w[3].as_f64().unwrap(), g[3].as_f64().unwrap());
            w[..3] == g[..3] && (a - b).abs() <= 1e-13 * a.abs().max(1.0)
        }
        _ => want == got,
    }
}

fn same(want: &Value, got: &Value) -> bool {
    match (want.get("margin"), got.get("margin")) {
        (Some(wm), Some(gm)) => {
            let strip = |v: &Value| {
                let mut v = v.clone();
                v.as_object_mut().unwrap().remove("margin");
                v
            };
            strip(want) == strip(got) && same_margin(wm, gm)
        }
        _ => want == got,
    }
}

#[sqlx::test]
async fn price_lines_matches_the_engine_on_every_golden_case(pool: PgPool) {
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/pricing_golden.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert!(cases.len() > 150, "golden set too small: {}", cases.len());
    let mut lossy_differs = Vec::new();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let got = priced(&pool, case).await;
        let want = &case["out"];
        if case["lines"].as_array().unwrap().is_empty() {
            assert_eq!(got, json!({ "empty": true }), "{name}");
            continue;
        }
        if same(want, &got) {
            continue;
        }
        // Only where rust_decimal itself dropped digits (> 28) may SQL differ.
        assert!(
            case["lossy"].as_bool().unwrap(),
            "{name}: engine {want} vs SQL {got}"
        );
        lossy_differs.push(name.to_owned());
    }
    // The documented divergence is real (see the next test), so the set is never empty.
    assert!(
        lossy_differs
            .iter()
            .any(|n| n == "huge_dp8_q100000000_v100.12345678_d0"),
        "{lossy_differs:?}"
    );
}

/// VAT on 999999999999999999999999 at 100.12345678 % needs 35 digits at 8 decimals:
/// rust_decimal keeps 28 (…998.9988); Postgres keeps every digit, the right answer.
#[sqlx::test]
async fn sql_is_exact_where_rust_decimal_ran_out_of_digits(pool: PgPool) {
    let case = json!({
        "dp": 8,
        "lines": [["100000000", "9999999999999999.99999999", "100.12345678", "0", "1", false]],
    });
    let got = priced(&pool, &case).await;
    assert_eq!(got["lines"][0][3], "1001234567799999999999998.99876543");
}

/// Rates group by value (5.5 = 5.50); the group shows the first line's spelling.
#[sqlx::test]
async fn equal_rates_group_and_keep_the_first_spelling(pool: PgPool) {
    let case = json!({
        "dp": 2,
        "lines": [
            ["1", "10", "5.50", "0", null, false],
            ["1", "10", "5.5", "0", null, false],
        ],
    });
    let got = priced(&pool, &case).await;
    assert_eq!(got["vats"], json!([["5.50", "20", "1.1"]]));
}

/// One unit incl VAT in minor units (shop, Till, checkout) is a one-line `price_lines` total,
/// over half-cent ties, credits, odd rates and 0/2/3/8 decimals; NULL past range.
#[sqlx::test]
async fn unit_gross_is_a_one_line_document_total(pool: PgPool) {
    let differs: i64 = sqlx::query_scalar(
        r#"SELECT count(*) FROM
               unnest('{0,1,5,50,99,105,250,333,999,1000005,-50,-105,4611686018427387903}'::bigint[]) u,
               unnest('{0,5.5,9,21,33.33333333,100.12345678}'::numeric[]) v,
               unnest('{0,2,3,8}'::int[]) dp,
               LATERAL (SELECT p.total * 10::numeric ^ dp AS want
                        FROM price_lines(dp, '{1}', ARRAY[round(u / 10::numeric ^ dp, dp)], ARRAY[v],
                                         '{0}', '{NULL}', '{f}') p) w
           WHERE money_unit_gross(u, v, dp) IS DISTINCT FROM
                 CASE WHEN w.want BETWEEN -9223372036854775808 AND 9223372036854775807
                      THEN w.want::bigint END"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(differs, 0);
    let row: (Option<i64>, Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT money_unit_gross(105, 21, 2), money_unit_gross(-105, 21, 2),
                money_unit_gross(9223372036854775807, 21, 2)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    // 1.05 + 0.2205 VAT -> 1.27 (tie-free); credit mirrors; past bigint -> NULL.
    assert_eq!(row, (Some(127), Some(-127), None));
}
