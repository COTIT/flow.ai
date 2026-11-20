//! Item line maths (Laravel HasItemPricing + BuildItemsSummary), exact decimals. Each line's
//! amounts round to the currency's decimals; VAT is computed once per rate on the summed line
//! amounts (user decision 2026-10-21), so total = excl + VAT always.

use rust_decimal::Decimal;

use crate::money::round;

/// One validated item line.
#[derive(Debug, Clone, Copy, Default)]
pub struct Line {
    pub quantity: Decimal,
    /// Unit price excl VAT.
    pub price: Decimal,
    /// VAT percent, e.g. 21.
    pub vat: Decimal,
    /// Discount percent, 0..=100.
    pub discount: Decimal,
    pub purchase_price: Option<Decimal>,
    /// Child line of a multi-product set: margin skips it (Laravel).
    pub set_child: bool,
}

/// One line's rounded amounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Amounts {
    /// quantity × price.
    pub gross: Decimal,
    pub discount: Decimal,
    /// gross − discount.
    pub excl: Decimal,
    /// This line's own VAT, rounded on its own: for per-line display only. Document VAT is
    /// per rate (`Summary::vats`); the sum of these can differ from it by half a cent per line.
    pub line_vat: Decimal,
}

/// Rounded `Amounts` of `line` at `dp` decimals. None: a value left Decimal's range.
pub fn amounts(line: &Line, dp: u32) -> Option<Amounts> {
    let percent = |v: Decimal, rate: Decimal| {
        let r = v.checked_mul(rate)?.checked_div(Decimal::ONE_HUNDRED)?;
        Some(round(r, dp))
    };
    let gross = round(line.quantity.checked_mul(line.price)?, dp);
    let discount = percent(gross, line.discount)?;
    let excl = gross.checked_sub(discount)?;
    let line_vat = percent(excl, line.vat)?;
    Some(Amounts {
        gross,
        discount,
        excl,
        line_vat,
    })
}

/// VAT of one rate: `round(base × rate / 100)` over the summed line amounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateTotal {
    pub rate: Decimal,
    /// Σ line excl at this rate.
    pub base: Decimal,
    pub vat: Decimal,
}

/// Totals over all lines, VAT grouped by exact rate in first-seen order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    pub gross: Decimal,
    pub discount: Decimal,
    pub excl: Decimal,
    /// Σ VAT per rate.
    pub vat: Decimal,
    /// excl + vat.
    pub total: Decimal,
    /// One entry per distinct rate, first-seen order.
    pub vats: Vec<RateTotal>,
    /// Per line, same order as the input.
    pub lines: Vec<Amounts>,
}

/// Sum `lines`, then VAT once per rate. Err(index) names the first line whose amounts or
/// running totals overflowed (the last line when a rate's VAT does).
pub fn summarise(lines: &[Line], dp: u32) -> Result<Summary, usize> {
    let mut s = Summary::default();
    for (i, line) in lines.iter().enumerate() {
        let a = amounts(line, dp).ok_or(i)?;
        let add = |acc: Decimal, v: Decimal| acc.checked_add(v).ok_or(i);
        s.gross = add(s.gross, a.gross)?;
        s.discount = add(s.discount, a.discount)?;
        s.excl = add(s.excl, a.excl)?;
        match s.vats.iter_mut().find(|v| v.rate == line.vat) {
            Some(v) => v.base = add(v.base, a.excl)?,
            None => s.vats.push(RateTotal {
                rate: line.vat,
                base: a.excl,
                vat: Decimal::ZERO,
            }),
        }
        s.lines.push(a);
    }
    let last = lines.len().saturating_sub(1);
    for v in &mut s.vats {
        let raw = v.base.checked_mul(v.rate).ok_or(last)?;
        v.vat = round(raw.checked_div(Decimal::ONE_HUNDRED).ok_or(last)?, dp);
        s.vat = s.vat.checked_add(v.vat).ok_or(last)?;
    }
    s.total = s.excl.checked_add(s.vat).ok_or(last)?;
    Ok(s)
}

/// Margin of the item lines (Laravel BuildItemsMargeCalculationSummary, `items` part).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Margin {
    /// Σ purchase price × quantity, each rounded.
    pub cost: Decimal,
    /// Σ line excl.
    pub revenue: Decimal,
    /// revenue − cost.
    pub amount: Decimal,
    /// amount / revenue × 100, unrounded. Laravel divides by 1 when revenue is 0.
    pub percentage: Decimal,
}

/// Margin over `lines` priced as `amounts` (same order). Set children are skipped (their
/// set line carries the price). Err(index) names the line whose cost or running sum
/// overflowed; an overflow in the final amount or percentage names the last line counted.
pub fn margin(lines: &[Line], amounts: &[Amounts], dp: u32) -> Result<Margin, usize> {
    let mut m = Margin::default();
    let mut last = 0;
    for (i, (line, a)) in lines.iter().zip(amounts).enumerate() {
        if line.set_child {
            continue;
        }
        last = i;
        let purchase = line.purchase_price.unwrap_or_default();
        let cost = purchase.checked_mul(line.quantity).ok_or(i)?;
        m.cost = m.cost.checked_add(round(cost, dp)).ok_or(i)?;
        m.revenue = m.revenue.checked_add(a.excl).ok_or(i)?;
    }
    m.amount = m.revenue.checked_sub(m.cost).ok_or(last)?;
    let divisor = if m.revenue.is_zero() {
        Decimal::ONE
    } else {
        m.revenue
    };
    m.percentage = m
        .amount
        .checked_div(divisor)
        .and_then(|r| r.checked_mul(Decimal::ONE_HUNDRED))
        .ok_or(last)?;
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    fn line(q: &str, p: &str, vat: &str, disc: &str) -> Line {
        Line {
            quantity: d(q),
            price: d(p),
            vat: d(vat),
            discount: d(disc),
            ..Line::default()
        }
    }

    fn totals(lines: &[Line]) -> (String, String, String) {
        let s = summarise(lines, 2).unwrap();
        (s.excl.to_string(), s.vat.to_string(), s.total.to_string())
    }

    #[test]
    fn three_at_9_99() {
        let t = totals(&[line("3", "9.99", "21", "0")]);
        assert_eq!(t, ("29.97".into(), "6.29".into(), "36.26".into()));
    }

    /// Laravel says 32.64 here; 26.97 + 5.66 is 32.63 (user decision: totals add up).
    #[test]
    fn discount_rounds_per_line() {
        let a = amounts(&line("3", "9.99", "21", "10"), 2).unwrap();
        assert_eq!(
            (a.gross, a.discount, a.excl),
            (d("29.97"), d("3.00"), d("26.97"))
        );
        let t = totals(&[line("3", "9.99", "21", "10")]);
        assert_eq!(t, ("26.97".into(), "5.66".into(), "32.63".into()));
    }

    /// VAT once per rate on the summed lines (user decision 2026-10-21): 0.21 × 21 % = 0.04,
    /// where three lines rounded on their own gave 0.03. Laravel's total also says 0.25.
    #[test]
    fn vat_rounds_once_per_rate() {
        let l = line("1", "0.07", "21", "0");
        let t = totals(&[l, l, l]);
        assert_eq!(t, ("0.21".into(), "0.04".into(), "0.25".into()));
        let cents = line("1", "0.01", "21", "0");
        let t = totals(&[cents; 1000]);
        assert_eq!(t, ("10.00".into(), "2.10".into(), "12.10".into()));
    }

    #[test]
    fn credit_mirrors_debit() {
        let debit = summarise(&[line("3", "9.99", "21", "10")], 2).unwrap();
        let credit = summarise(&[line("-3", "9.99", "21", "10")], 2).unwrap();
        assert_eq!(credit.total, -debit.total);
        assert_eq!(credit.vat, -debit.vat);
        assert_eq!(credit.discount, -debit.discount);
    }

    #[test]
    fn ties_round_away_from_zero() {
        // 0.5 × 21% = 0.105 -> 0.11 (and -0.11 on a credit); 3 × 0.335 = 1.005 -> 1.01.
        let vat = |q: &str| summarise(&[line(q, "0.5", "21", "0")], 2).unwrap().vat;
        assert_eq!((vat("1"), vat("-1")), (d("0.11"), d("-0.11")));
        assert_eq!(
            amounts(&line("3", "0.335", "21", "0"), 2).unwrap().gross,
            d("1.01")
        );
        assert_eq!(
            amounts(&line("-3", "0.335", "21", "0"), 2).unwrap().gross,
            d("-1.01")
        );
    }

    #[test]
    fn full_discount_is_free() {
        let t = totals(&[line("2", "50", "21", "100")]);
        assert_eq!(t, ("0".into(), "0".into(), "0".into()));
    }

    #[test]
    fn rates_group_exactly_in_first_seen_order() {
        let s = summarise(
            &[
                line("1", "100", "5.5", "0"),
                line("1", "100", "5", "0"),
                line("1", "10", "5.5", "0"),
                line("1", "10", "5.50", "0"),
            ],
            2,
        )
        .unwrap();
        let got: Vec<_> = s.vats.iter().map(|v| (v.rate, v.base, v.vat)).collect();
        assert_eq!(
            got,
            vec![
                (d("5.5"), d("120"), d("6.60")),
                (d("5"), d("100"), d("5.00"))
            ]
        );
        assert_eq!(s.total, s.excl + s.vat);
    }

    /// One line per rate: per-line and per-rate rounding agree exactly.
    #[test]
    fn one_line_per_rate_matches_line_vat() {
        let lines = [line("3", "9.99", "21", "10"), line("2", "4.35", "9", "0")];
        let s = summarise(&lines, 2).unwrap();
        let by_line: Decimal = s.lines.iter().map(|a| a.line_vat).sum();
        assert_eq!(s.vat, by_line);
    }

    /// A rate's base can pass every line check and still overflow when taxed.
    #[test]
    fn rate_vat_overflow_names_the_last_line() {
        let big = Line {
            quantity: d("1"),
            price: d("30000000000000000000000000000"),
            vat: d("2"),
            ..Line::default()
        };
        assert!(amounts(&big, 2).is_some(), "each line fits");
        assert_eq!(summarise(&[big, big], 2).map(|s| s.vat), Err(1));
    }

    #[test]
    fn currency_decimals_drive_rounding() {
        let a = amounts(&line("3", "9.99", "21", "0"), 0).unwrap();
        assert_eq!((a.gross, a.line_vat), (d("30"), d("6")));
    }

    #[test]
    fn overflow_names_the_line() {
        let max = crate::money::MAX;
        let big = Line {
            quantity: max,
            price: max,
            ..Line::default()
        };
        assert_eq!(amounts(&big, 2), None);
        assert_eq!(summarise(&[line("1", "1", "0", "0"), big], 2), Err(1));
    }

    /// Field of a fixture item: number or numeric string, blank/null/missing = 0
    /// (CalculateItemsRequest::validated()).
    fn fixture_dec(v: Option<&serde_json::Value>) -> Decimal {
        match v {
            Some(serde_json::Value::Number(n)) => d(&n.to_string()),
            Some(serde_json::Value::String(s)) if !s.is_empty() => d(s),
            _ => Decimal::ZERO,
        }
    }

    fn fixture_number(v: &serde_json::Value) -> Decimal {
        d(&v.as_f64().unwrap().to_string())
    }

    /// Every case in `pricing_parity.json` (PHP 8.4 running vimdesk-api's real
    /// BuildItemsSummary). Line rounding moves each sum by at most half a cent per line, VAT per
    /// rate by half a cent per rate. Whole-cent inputs (2-dp prices, whole quantities, no
    /// discount) match the line sums exactly, and with one VAT rate also Laravel's total (it
    /// rounds Σ incl once, as one rate does).
    #[test]
    fn matches_laravel_within_rounding() {
        let mut vat_differs = Vec::new();
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/pricing_parity.json"))
                .unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(cases.len() >= 20);
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let items = case["items"].as_array().unwrap();
            let lines: Vec<Line> = items
                .iter()
                .map(|i| Line {
                    quantity: fixture_dec(i.get("quantity")),
                    price: fixture_dec(i.get("price")),
                    vat: fixture_dec(i.get("vat")),
                    discount: fixture_dec(i.get("productDiscount")),
                    ..Line::default()
                })
                .collect();
            let whole_cents = lines.iter().all(|l| {
                l.discount.is_zero() && l.quantity.fract().is_zero() && l.price.scale() <= 2
            });
            let s = summarise(&lines, 2).unwrap();
            let tolerance = d("0.005") * Decimal::from(lines.len() + s.vats.len() + 1);
            let laravel = &case["summary"];
            for (field, ours) in [
                ("sum_total", s.gross),
                ("total_discount", s.discount),
                ("total_excl", s.excl),
                ("total_vat", s.vat),
                ("total", s.total),
            ] {
                let theirs = fixture_number(&laravel[field]);
                let diff = (ours - theirs).abs();
                assert!(diff <= tolerance, "{name} {field}: {ours} vs {theirs}");
                match field {
                    "total" if whole_cents && s.vats.len() == 1 => {
                        assert_eq!(ours, crate::money::round(theirs, 2), "{name} total");
                    }
                    "total_vat" if ours != crate::money::round(theirs, 2) => vat_differs.push(name),
                    "total" | "total_vat" => {}
                    _ if whole_cents => {
                        assert_eq!(ours, crate::money::round(theirs, 2), "{name} {field}")
                    }
                    _ => {}
                }
            }
            assert_eq!(s.total, s.excl + s.vat, "{name}: total must add up");
        }
        // Per-rate VAT shows where Laravel's lines undercount: 0.04 here, Laravel's lines 0.03.
        assert!(vat_differs.contains(&"three_lines_0_07_vat21"));
    }

    fn margin_of(lines: &[Line]) -> Margin {
        let s = summarise(lines, 2).unwrap();
        margin(lines, &s.lines, 2).unwrap()
    }

    fn bought(q: &str, p: &str, disc: &str, purchase: &str) -> Line {
        Line {
            purchase_price: Some(d(purchase)),
            ..line(q, p, "21", disc)
        }
    }

    #[test]
    fn margin_uses_excl_after_discount() {
        let m = margin_of(&[bought("2", "25", "0", "15"), bought("1", "10", "10", "4.5")]);
        assert_eq!(
            (m.cost, m.revenue, m.amount),
            (d("34.5"), d("59"), d("24.5"))
        );
        assert_eq!(m.percentage.round_dp(6), d("41.525424"));
    }

    #[test]
    fn margin_without_revenue_divides_by_one() {
        let m = margin_of(&[bought("1", "0", "0", "5")]);
        assert_eq!((m.amount, m.percentage), (d("-5"), d("-500")));
    }

    #[test]
    fn margin_skips_set_children_and_missing_purchase() {
        let child = Line {
            set_child: true,
            ..bought("1", "30", "0", "10")
        };
        let m = margin_of(&[
            bought("1", "30", "0", "10"),
            child,
            line("1", "5", "21", "0"),
        ]);
        assert_eq!((m.cost, m.revenue), (d("10"), d("35")));
    }

    #[test]
    fn margin_overflow_names_the_line() {
        let max = crate::money::MAX;
        let big = Line {
            quantity: max,
            purchase_price: Some(max),
            ..Line::default()
        };
        let lines = [line("1", "1", "0", "0"), big];
        let amounts = [Amounts::default(), Amounts::default()];
        assert_eq!(margin(&lines, &amounts, 2), Err(1));
    }

    /// Margin side of `pricing_parity.json`: cost, revenue and amount within per-line
    /// rounding of Laravel, percentage within 0.01 points.
    #[test]
    fn margin_matches_laravel() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/pricing_parity.json"))
                .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let lines: Vec<Line> = case["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| Line {
                    quantity: fixture_dec(i.get("quantity")),
                    price: fixture_dec(i.get("price")),
                    vat: fixture_dec(i.get("vat")),
                    discount: fixture_dec(i.get("productDiscount")),
                    purchase_price: i.get("purchase_price").map(|v| fixture_dec(Some(v))),
                    set_child: i.get("multi_set_item_id").is_some_and(|v| !v.is_null()),
                })
                .collect();
            let tolerance = d("0.005") * Decimal::from(lines.len() + 1);
            let m = margin_of(&lines);
            let items = &case["marge"]["extra_data"]["components"]["items"];
            for (field, ours) in [
                ("cost_excl", m.cost),
                ("revenue_excl", m.revenue),
                ("margin_amount", m.amount),
            ] {
                let theirs = fixture_number(&items[field]);
                assert!(
                    (ours - theirs).abs() <= tolerance,
                    "{name} {field}: {ours} vs {theirs}"
                );
            }
            let theirs = fixture_number(&items["margin_percentage"]);
            assert!(
                (m.percentage - theirs).abs() <= d("0.01"),
                "{name} margin_percentage: {} vs {theirs}",
                m.percentage
            );
        }
    }

    #[test]
    fn running_total_overflow_is_caught() {
        let near = Line {
            quantity: d("7000000000000"),
            price: crate::money::MAX,
            ..Line::default()
        };
        assert!(amounts(&near, 2).is_some());
        assert_eq!(summarise(&[near, near], 2), Err(1));
    }

    /// One golden document: name, currency decimals, lines.
    type GoldenDoc = (String, u32, Vec<Line>);

    fn golden_line(q: &str, p: &str, vat: &str, disc: &str, purchase: Option<&str>) -> Line {
        Line {
            purchase_price: purchase.map(d),
            ..line(q, p, vat, disc)
        }
    }

    /// Wide case set for `price_lines` (SQL): half-cent ties, credits, discounts, exact rate
    /// grouping, 0/2/3/8 decimals, set children, overflow, the Laravel parity fixture.
    fn golden_docs() -> Vec<GoldenDoc> {
        let mut docs: Vec<GoldenDoc> = Vec::new();
        let dps = [0, 2, 3, 8];
        let qs = ["1", "3", "-3", "0.125", "2.33333333", "100000000"];
        let ps = ["0.005", "0.335", "9.99", "1.00000005", "-4.35"];
        let vats = ["0", "5.5", "5.50", "21", "100.12345678"];
        let discs = ["0", "10", "33.33333333", "100"];
        for dp in dps {
            for q in qs {
                let mut lines = Vec::new();
                for p in ps {
                    for vat in vats {
                        for disc in discs {
                            let n = lines.len();
                            let purchase = (n % 3 == 0).then_some("0.333");
                            lines.push(Line {
                                set_child: n % 7 == 6,
                                ..golden_line(q, p, vat, disc, purchase)
                            });
                        }
                    }
                }
                docs.push((format!("grid_dp{dp}_q{q}"), dp, lines));
                // Largest numeric(24,8) price: alone, so a lossy Rust product stays in its doc.
                for vat in ["21", "100.12345678"] {
                    for disc in ["0", "33.33333333"] {
                        let l = golden_line(q, "9999999999999999.99999999", vat, disc, Some("1"));
                        docs.push((format!("huge_dp{dp}_q{q}_v{vat}_d{disc}"), dp, vec![l]));
                    }
                }
            }
        }
        let max = crate::money::MAX;
        let big = |q: Decimal, p: Decimal| Line {
            quantity: q,
            price: p,
            ..Line::default()
        };
        let mut named = |name: &str, dp: u32, lines: Vec<Line>| docs.push((name.into(), dp, lines));
        named("empty", 2, vec![]);
        named("three_at_9_99", 2, vec![line("3", "9.99", "21", "0")]);
        named("discount_per_line", 2, vec![line("3", "9.99", "21", "10")]);
        named("tie_vat", 2, vec![line("1", "0.5", "21", "0")]);
        named("tie_vat_credit", 2, vec![line("-1", "0.5", "21", "0")]);
        named("tie_gross", 2, vec![line("3", "0.335", "21", "0")]);
        named("tie_gross_credit", 2, vec![line("-3", "0.335", "21", "0")]);
        named(
            "vat_once_per_rate",
            2,
            vec![line("1", "0.07", "21", "0"); 3],
        );
        named(
            "thousand_cents",
            2,
            vec![line("1", "0.01", "21", "0"); 1000],
        );
        named(
            "rates_first_seen",
            2,
            vec![
                line("1", "100", "5.5", "0"),
                line("1", "100", "5", "0"),
                line("1", "10", "5.5", "0"),
                line("1", "10", "5.50", "0"),
            ],
        );
        named("full_discount", 2, vec![line("2", "50", "21", "100")]);
        named("decimals_0", 0, vec![line("3", "9.99", "21", "0")]);
        named(
            "margin_no_revenue",
            2,
            vec![golden_line("1", "0", "21", "0", Some("5"))],
        );
        named("overflow_line", 2, vec![big(max, max)]);
        named(
            "overflow_second",
            2,
            vec![line("1", "1", "0", "0"), big(max, max)],
        );
        let rate = Line {
            vat: d("2"),
            ..big(d("1"), d("30000000000000000000000000000"))
        };
        named("overflow_rate_vat", 2, vec![rate, rate]);
        let near = big(d("7000000000000"), max);
        named("overflow_running", 2, vec![near, near]);
        let costly = Line {
            purchase_price: Some(max),
            ..big(max, Decimal::ZERO)
        };
        named("overflow_margin", 2, vec![line("1", "1", "0", "0"), costly]);
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/pricing_parity.json"))
                .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let lines = case["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| Line {
                    quantity: fixture_dec(i.get("quantity")),
                    price: fixture_dec(i.get("price")),
                    vat: fixture_dec(i.get("vat")),
                    discount: fixture_dec(i.get("productDiscount")),
                    purchase_price: i.get("purchase_price").map(|v| fixture_dec(Some(v))),
                    set_child: i.get("multi_set_item_id").is_some_and(|v| !v.is_null()),
                })
                .collect();
            docs.push((
                format!("parity_{}", case["name"].as_str().unwrap()),
                2,
                lines,
            ));
        }
        docs
    }

    /// rust_decimal keeps 28 digits: a product needing more drops scale (lossy); SQL is exact.
    fn lossy_mul(a: Decimal, b: Decimal) -> bool {
        let exact = a.mantissa().checked_mul(b.mantissa());
        a.scale() + b.scale() > 28 || exact.is_none_or(|m| m.unsigned_abs() > (1u128 << 96) - 1)
    }

    fn lossy(lines: &[Line], s: &Summary) -> bool {
        let line = lines.iter().zip(&s.lines).any(|(l, a)| {
            lossy_mul(l.quantity, l.price)
                || lossy_mul(a.gross, l.discount)
                || lossy_mul(a.excl, l.vat)
                || lossy_mul(l.purchase_price.unwrap_or_default(), l.quantity)
        });
        line || s.vats.iter().any(|v| lossy_mul(v.base, v.rate))
    }

    fn golden_out(lines: &[Line], dp: u32) -> (serde_json::Value, bool) {
        use rust_decimal::prelude::ToPrimitive;
        use serde_json::json;
        let n = |v: Decimal| v.normalize().to_string();
        let s = match summarise(lines, dp) {
            Ok(s) => s,
            Err(i) => return (json!({ "err": i }), false),
        };
        let margin = match margin(lines, &s.lines, dp) {
            Ok(m) => json!([n(m.cost), n(m.revenue), n(m.amount), m.percentage.to_f64()]),
            Err(i) => json!({ "err": i }),
        };
        let out = json!({
            "lines": s.lines.iter()
                .map(|a| [n(a.gross), n(a.discount), n(a.excl), n(a.line_vat)])
                .collect::<Vec<_>>(),
            // Rate as first seen (scale kept): "5.5" vs "5.50" shows which line set it.
            "vats": s.vats.iter()
                .map(|v| [v.rate.to_string(), n(v.base), n(v.vat)])
                .collect::<Vec<_>>(),
            "totals": [n(s.gross), n(s.discount), n(s.excl), n(s.vat), n(s.total)],
            "margin": margin,
        });
        (out, lossy(lines, &s))
    }

    /// Freezes this engine's results into `tests/fixtures/pricing_golden.json`, which
    /// `tests/api/pricing_sql.rs` checks `price_lines` (SQL) against. `GOLDEN_WRITE=1`
    /// rewrites the file; otherwise it must match.
    #[test]
    fn golden_fixture_matches_engine() {
        let cases: Vec<String> = golden_docs()
            .iter()
            .map(|(name, dp, lines)| {
                let (out, lossy) = golden_out(lines, *dp);
                let input: Vec<_> = lines
                    .iter()
                    .map(|l| {
                        serde_json::json!([
                            l.quantity.to_string(),
                            l.price.to_string(),
                            l.vat.to_string(),
                            l.discount.to_string(),
                            l.purchase_price.map(|p| p.to_string()),
                            l.set_child
                        ])
                    })
                    .collect();
                serde_json::json!({ "name": name, "dp": dp, "lossy": lossy, "lines": input, "out": out })
                    .to_string()
            })
            .collect();
        let got = format!("{{\"cases\":[\n{}\n]}}\n", cases.join(",\n"));
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/pricing_golden.json"
        );
        if std::env::var_os("GOLDEN_WRITE").is_some() {
            std::fs::write(path, &got).unwrap();
        }
        assert_eq!(got, std::fs::read_to_string(path).unwrap());
    }
}
