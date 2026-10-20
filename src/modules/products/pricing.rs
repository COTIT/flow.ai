//! Product price rules from Laravel ProductPriceTrait + Product saving hook, in exact decimals.
//! No special offers, price corrections or discount groups yet (final batch F3), so the
//! consumer price is the stored net price.

use rust_decimal::Decimal;

use crate::money::{Zero, format_decimals, round};

/// Which input wins when gross, price and impact all changed (Laravel `$priority`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Price,
    Impact,
}

/// Laravel `Product::normalizePriceImpact`: with a gross price > 0, price and markup
/// (`impact`, %) are derived from each other. Returns `(price, impact)`.
pub fn normalise(
    gross: Option<Decimal>,
    price: Option<Decimal>,
    impact: Option<Decimal>,
    priority: Priority,
) -> (Option<Decimal>, Option<Decimal>) {
    let Some(gross) = gross.filter(|g| *g > Decimal::ZERO) else {
        return (price, impact);
    };
    let hundred = Decimal::ONE_HUNDRED;
    let from_impact = |i: Decimal| round(gross * (i / hundred + Decimal::ONE), 2);
    match (price, impact, priority) {
        (None, None, _) => (None, None),
        (_, Some(i), Priority::Impact) => (Some(from_impact(i)), Some(i)),
        (Some(p), _, _) => {
            let p = round(p, 2);
            (Some(p), Some(round(p / gross * hundred - hundred, 4)))
        }
        (None, Some(i), _) => {
            let i = round(i, 4);
            (Some(from_impact(i)), Some(i))
        }
    }
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
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn impact_drives_price_like_laravel_update_test() {
        let (p, i) = normalise(Some(d("200")), Some(d("100")), Some(d("10")), Priority::Impact);
        assert_eq!((p, i), (Some(d("220")), Some(d("10"))));
    }

    #[test]
    fn price_drives_impact_like_laravel_update_test() {
        let (p, i) = normalise(Some(d("250")), Some(d("275")), Some(d("3")), Priority::Price);
        assert_eq!((p, i), (Some(d("275")), Some(d("10"))));
    }

    #[test]
    fn price_rounds_to_cents_and_impact_to_four_places() {
        let (p, i) = normalise(Some(d("3")), Some(d("1.005")), None, Priority::Price);
        assert_eq!(p, Some(d("1.01")));
        assert_eq!(i, Some(d("-66.3333")));
    }

    #[test]
    fn negative_markup_is_a_discount() {
        let (p, _) = normalise(Some(d("100")), None, Some(d("-12.5")), Priority::Impact);
        assert_eq!(p, Some(d("87.5")));
    }

    #[test]
    fn no_gross_leaves_both_untouched() {
        let raw = (Some(d("743.057847")), Some(d("5")));
        assert_eq!(normalise(None, raw.0, raw.1, Priority::Price), raw);
        assert_eq!(normalise(Some(Decimal::ZERO), raw.0, raw.1, Priority::Price), raw);
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

    #[test]
    fn purchase_price_without_discounts() {
        let some = |s| Some(d(s));
        assert_eq!(purchase_price(some("5"), some("10"), some("8")), some("10"));
        assert_eq!(purchase_price(None, some("10"), some("8")), some("10"));
        assert_eq!(purchase_price(None, some("0"), some("8")), some("8"));
        assert_eq!(purchase_price(None, None, None), None);
    }
}
