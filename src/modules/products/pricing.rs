//! Product price rules from Laravel ProductPriceTrait + Product saving hook, in exact decimals.
//! No special offers, price corrections or discount groups yet (final batch F3), so the
//! consumer price is the stored net price.

use rust_decimal::Decimal;

use crate::{
    error::AppError,
    http::json::human,
    money::{MAX, Zero, format_decimals, round},
};

/// Which input wins when gross, price and impact all changed (Laravel `$priority`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Price,
    Impact,
}

/// Laravel `Product::normalizePriceImpact`: with a gross price > 0, price and markup
/// (`impact`, %) are derived from each other. Returns `(price, impact)`, or the field that
/// drove a derived value out of range (price outside `0..=MAX`, impact outside `±MAX`).
pub fn normalise(
    gross: Option<Decimal>,
    price: Option<Decimal>,
    impact: Option<Decimal>,
    priority: Priority,
) -> Result<(Option<Decimal>, Option<Decimal>), &'static str> {
    let Some(gross) = gross.filter(|g| *g > Decimal::ZERO) else {
        return Ok((price, impact));
    };
    let hundred = Decimal::ONE_HUNDRED;
    let from_impact = |i: Decimal| {
        let p = round(gross.checked_mul(i / hundred + Decimal::ONE)?, 2);
        (Decimal::ZERO..=MAX).contains(&p).then_some(p)
    };
    let to_impact = |p: Decimal| {
        let i = round(p.checked_div(gross)?.checked_mul(hundred)? - hundred, 4);
        (-MAX..=MAX).contains(&i).then_some(i)
    };
    match (price, impact, priority) {
        (None, None, _) => Ok((None, None)),
        (_, Some(i), Priority::Impact) => {
            let p = from_impact(i).ok_or("impact_on_price")?;
            Ok((Some(p), Some(i)))
        }
        (Some(p), _, _) => {
            let p = Some(round(p, 2)).filter(|p| *p <= MAX).ok_or("price")?;
            Ok((Some(p), Some(to_impact(p).ok_or("price")?)))
        }
        (None, Some(i), _) => {
            let i = round(i, 4);
            Ok((Some(from_impact(i).ok_or("impact_on_price")?), Some(i)))
        }
    }
}

/// 422 for the field [`normalise`] named.
pub fn out_of_range(field: &str) -> AppError {
    AppError::field(field, format!("The {} is out of range.", human(field)))
}

/// Price incl VAT: `net × (100 + vat) / 100` (Laravel `buildPriceIncl`).
pub fn incl(net: Decimal, vat_pct: Decimal) -> Decimal {
    net * (Decimal::ONE_HUNDRED + vat_pct) / Decimal::ONE_HUNDRED
}

/// Laravel `buildPurchasePrice` without supplier discounts: wholesale when it has a markup,
/// else wholesale when > 0, else the supplier's wholesale price.
pub fn purchase_price(
    impact_on_wholesale: Option<Decimal>,
    wholesale: Option<Decimal>,
    supplier_wholesale: Option<Decimal>,
) -> Option<Decimal> {
    let positive = |v: Option<Decimal>| v.is_some_and(|v| v > Decimal::ZERO);
    if positive(impact_on_wholesale) || positive(wholesale) {
        wholesale
    } else {
        supplier_wholesale
    }
}

/// A `buildPrice` value: PHP `(string) $float`, so an exact zero is falsy -> `"0.00"`.
pub fn computed(v: Decimal, dp: u32) -> String {
    format_decimals(Some(v), dp, true, Zero::Falsy)
}

/// A raw decimal column: an uncast DB string, so zero prints `"0"`; null prints `"0.00"`.
pub fn column(v: Option<Decimal>, dp: u32) -> String {
    format_decimals(v, dp, true, Zero::Column)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn impact_drives_price_like_laravel_update_test() {
        let (p, i) = normalise(
            Some(d("200")),
            Some(d("100")),
            Some(d("10")),
            Priority::Impact,
        )
        .unwrap();
        assert_eq!((p, i), (Some(d("220")), Some(d("10"))));
    }

    #[test]
    fn price_drives_impact_like_laravel_update_test() {
        let (p, i) = normalise(
            Some(d("250")),
            Some(d("275")),
            Some(d("3")),
            Priority::Price,
        )
        .unwrap();
        assert_eq!((p, i), (Some(d("275")), Some(d("10"))));
    }

    #[test]
    fn price_rounds_to_cents_and_impact_to_four_places() {
        let (p, i) = normalise(Some(d("3")), Some(d("1.005")), None, Priority::Price).unwrap();
        assert_eq!(p, Some(d("1.01")));
        assert_eq!(i, Some(d("-66.3333")));
    }

    #[test]
    fn negative_markup_is_a_discount() {
        let (p, _) = normalise(Some(d("100")), None, Some(d("-12.5")), Priority::Impact).unwrap();
        assert_eq!(p, Some(d("87.5")));
    }

    #[test]
    fn no_gross_leaves_both_untouched() {
        let raw = (Some(d("743.057847")), Some(d("5")));
        assert_eq!(normalise(None, raw.0, raw.1, Priority::Price), Ok(raw));
        assert_eq!(
            normalise(Some(Decimal::ZERO), raw.0, raw.1, Priority::Price),
            Ok(raw)
        );
    }

    #[test]
    fn derived_values_outside_the_column_name_their_driver() {
        let max = "9999999999999999";
        let n = |g, p: Option<&str>, i: Option<&str>, prio| {
            normalise(Some(d(g)), p.map(d), i.map(d), prio)
        };
        assert_eq!(
            n(max, None, Some(max), Priority::Impact),
            Err("impact_on_price")
        );
        assert_eq!(
            n("100", None, Some("-200"), Priority::Impact),
            Err("impact_on_price")
        );
        assert_eq!(
            n("0.00000001", Some(max), None, Priority::Price),
            Err("price")
        );
        assert_eq!(
            n("1", Some("9999999999999999.999"), None, Priority::Price),
            Err("price")
        );
        assert_eq!(
            n("100", None, Some("-100"), Priority::Impact).unwrap().0,
            Some(Decimal::ZERO)
        );
    }

    #[test]
    fn vat_oracle_from_laravel_consistency_test() {
        let net = d("743.057847");
        assert_eq!(computed(net, 4), "743.0578");
        assert_eq!(computed(incl(net, d("21")), 2), "899.1");
    }

    #[test]
    fn zero_prints_by_where_laravel_held_it() {
        assert_eq!(computed(Decimal::ZERO, 4), "0.00");
        assert_eq!(column(Some(Decimal::ZERO), 4), "0");
        assert_eq!(column(None, 4), "0.00");
        assert_eq!(column(Some(d("220.50000000")), 4), "220.5");
    }

    /// PHP MoneyHelper at 4 dp: a computed float zero prints "0.00", a DB string zero "0".
    #[test]
    fn zero_rules_match_php_oracle() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/format_decimals.json"))
                .unwrap();
        let zero_case = |kind: &str| {
            fixture["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| {
                    c["kind"] == kind
                        && c["dp"] == 4
                        && c["rtrim"] == true
                        && c["value"].as_str().is_some_and(|v| d(v) == Decimal::ZERO)
                })
                .unwrap()["expected"]
                .clone()
        };
        assert_eq!(computed(Decimal::ZERO, 4), zero_case("float"));
        assert_eq!(column(Some(Decimal::ZERO), 4), zero_case("db_string"));
    }

    #[test]
    fn purchase_price_without_discounts() {
        let some = |s| Some(d(s));
        assert_eq!(purchase_price(some("5"), some("10"), some("8")), some("10"));
        assert_eq!(purchase_price(None, some("10"), some("8")), some("10"));
        assert_eq!(purchase_price(None, some("0"), some("8")), some("8"));
        assert_eq!(purchase_price(None, None, None), None);
    }
}
