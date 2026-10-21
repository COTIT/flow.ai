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
    /// per rate (`Summary::vats`) and can differ from the sum of these by a cent per rate.
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
            ],
            2,
        )
        .unwrap();
        let got: Vec<_> = s.vats.iter().map(|v| (v.rate, v.base, v.vat)).collect();
        assert_eq!(
            got,
            vec![
                (d("5.5"), d("110"), d("6.05")),
                (d("5"), d("100"), d("5.00"))
            ]
        );
        assert_eq!(s.total, s.excl + s.vat);
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
}
