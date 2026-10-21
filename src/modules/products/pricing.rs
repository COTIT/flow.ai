//! Product price rules from Laravel ProductPriceTrait + Product saving hook, in exact decimals:
//! net/impact normalisation, purchase price with supplier discounts, and price corrections
//! (bands). No special offers yet (F3b), so the consumer price starts from the stored net price.

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

/// Laravel `buildPurchasePrice`. `discount` is the best live internal supplier discount
/// (`discounts::repo::purchase_discounts`). A wholesale markup above it wins; without a
/// discount: wholesale when > 0, else the supplier's wholesale price; with one: gross price
/// minus the discount, or the supplier's wholesale price when that comes to 0.
pub fn purchase_price(
    impact_on_wholesale: Option<Decimal>,
    wholesale: Option<Decimal>,
    supplier_wholesale: Option<Decimal>,
    discount: Option<Decimal>,
    gross: Decimal,
) -> Option<Decimal> {
    let positive = |v: Option<Decimal>| v.is_some_and(|v| v > Decimal::ZERO);
    let pct = discount.filter(|d| *d > Decimal::ZERO);
    if positive(impact_on_wholesale) && impact_on_wholesale > Some(pct.unwrap_or_default()) {
        return wholesale;
    }
    match pct {
        None if positive(wholesale) => wholesale,
        None => supplier_wholesale,
        Some(d) => Some(discounted(gross, d))
            .filter(|p| !p.is_zero())
            .or(supplier_wholesale),
    }
}

/// Laravel CalculateProductPurchasePrice with a discount: gross × (1 − d/100). `d` is a
/// stored percentage (0..=100), so this stays inside the gross price's range.
pub fn discounted(gross: Decimal, d: Decimal) -> Decimal {
    gross * (Decimal::ONE - d / Decimal::ONE_HUNDRED)
}

/// One live price correction band (Laravel PriceCorrection).
pub struct Band {
    pub value_from: Decimal,
    pub value_to: Decimal,
    pub round_on: Option<Decimal>,
    pub remove_amount: Option<Decimal>,
}

/// Laravel GetPriceCorrectionForValue: the band holding `net` (both ends included) with the
/// lowest `value_from`. `bands` come sorted by `value_from`, then id.
pub fn band_for(bands: &[Band], net: Decimal) -> Option<&Band> {
    bands
        .iter()
        .find(|b| b.value_from <= net && net <= b.value_to)
}

/// Laravel buildPrice with a band: round the incl-VAT price to the nearest `round_on` step (a
/// tie goes down, GetRoundedPrice), back to excl, then take off `remove_amount` (incl VAT).
/// None only on overflow, which stored values can't reach.
pub fn correct(net: Decimal, vat_pct: Decimal, b: &Band) -> Option<Decimal> {
    let mult = (Decimal::ONE_HUNDRED + vat_pct).checked_div(Decimal::ONE_HUNDRED)?;
    let rounded = match b.round_on.filter(|r| !r.is_zero()) {
        None => net,
        Some(step) => {
            let gross = net.checked_mul(mult)?;
            let steps = gross.checked_div(step)?;
            let up = steps.ceil().checked_mul(step)?;
            let down = steps.floor().checked_mul(step)?;
            let nearest = if (gross - up).abs() < (gross - down).abs() {
                up
            } else {
                down
            };
            nearest.checked_div(mult)?
        }
    };
    rounded.checked_sub(b.remove_amount.unwrap_or_default().checked_div(mult)?)
}

/// Laravel's four net prices before special offers (F3b).
pub struct Prices {
    /// `price` (getConsumerPrice) and `original_price` (getOriginalPrice): equal until offers.
    pub consumer: Decimal,
    /// `business_price` (getBusinessPrice).
    pub business: Decimal,
    /// `price_without_correction` (getPriceWithoutCorrection).
    pub uncorrected: Decimal,
}

/// Laravel ProductPriceTrait getters. With `business_from_gross` (setting
/// `product.disable_double_discount_for_business`) a product priced below gross starts its
/// business price from gross.
pub fn prices(
    net: Decimal,
    gross: Decimal,
    impact_on_price: Option<Decimal>,
    excluded: bool,
    business_from_gross: bool,
    vat_pct: Decimal,
    bands: &[Band],
) -> Prices {
    let build = |v: Decimal| {
        band_for(bands, v)
            .filter(|_| !excluded)
            .and_then(|b| correct(v, vat_pct, b))
            .unwrap_or(v)
    };
    let below_gross = impact_on_price.is_some_and(|i| i < Decimal::ZERO);
    let business_base = if below_gross && business_from_gross {
        gross
    } else {
        net
    };
    Prices {
        consumer: build(net),
        business: build(business_base),
        uncorrected: business_base,
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

    /// Every case in `price_correction_parity.json` (PHP 8.4 running vimdesk-api's real
    /// GetRoundedPrice + MoneyHelper, `tests/fixtures/price_correction_oracle.php`).
    #[test]
    fn corrections_match_laravel() {
        let cases: Vec<Value> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/price_correction_parity.json"
        ))
        .unwrap();
        assert_eq!(cases.len(), 14);
        let opt = |v: &Value| v.as_str().map(d);
        for c in cases {
            let (net, vat) = (
                d(c["amount"].as_str().unwrap()),
                d(c["vat"].as_str().unwrap()),
            );
            let band = Band {
                value_from: Decimal::ZERO,
                value_to: MAX,
                round_on: opt(&c["round_on"]),
                remove_amount: opt(&c["remove_amount"]),
            };
            let got = correct(net, vat, &band).unwrap();
            assert_eq!(computed(got, 4), c["price"], "{}", c["name"]);
            assert_eq!(
                computed(incl(got, vat), 2),
                c["price_incl"],
                "{}",
                c["name"]
            );
        }
    }

    #[test]
    fn band_pick_and_opt_out() {
        let b = |from: &str, to: &str, step: &str| Band {
            value_from: d(from),
            value_to: d(to),
            round_on: Some(d(step)),
            remove_amount: None,
        };
        let bands = [b("0", "10", "1"), b("5", "20", "0.5"), b("30", "40", "1")];
        assert_eq!(
            band_for(&bands, d("10")).map(|b| b.value_from),
            Some(d("0"))
        );
        assert_eq!(
            band_for(&bands, d("10.01")).map(|b| b.value_from),
            Some(d("5"))
        );
        assert!(band_for(&bands, d("25")).is_none());
        let p = prices(
            d("10.3"),
            d("10.3"),
            None,
            false,
            false,
            Decimal::ZERO,
            &bands,
        );
        // 10.3 is in the second band only: 0.5 steps, 10.5 is nearer than 10.
        assert_eq!(
            (p.consumer, p.business, p.uncorrected),
            (d("10.5"), d("10.5"), d("10.3"))
        );
        let p = prices(
            d("10.3"),
            d("10.3"),
            None,
            true,
            false,
            Decimal::ZERO,
            &bands,
        );
        assert_eq!(p.consumer, d("10.3"), "excluded");
        // Below gross with the setting: business from gross, consumer from net.
        let p = prices(
            d("9.4"),
            d("12.2"),
            Some(d("-23")),
            false,
            true,
            Decimal::ZERO,
            &bands,
        );
        assert_eq!(
            (p.consumer, p.business, p.uncorrected),
            (d("9"), d("12"), d("12.2"))
        );
        // Overflow keeps the price instead of panicking.
        let huge = Band {
            value_from: Decimal::ZERO,
            value_to: Decimal::MAX,
            round_on: Some(d("0.00000001")),
            remove_amount: None,
        };
        assert!(correct(Decimal::MAX, d("21"), &huge).is_none());
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
        assert_eq!(
            purchase_price(some("5"), some("10"), some("8"), None, Decimal::ZERO),
            some("10")
        );
        assert_eq!(
            purchase_price(None, some("10"), some("8"), None, Decimal::ZERO),
            some("10")
        );
        assert_eq!(
            purchase_price(None, some("0"), some("8"), None, Decimal::ZERO),
            some("8")
        );
        assert_eq!(purchase_price(None, None, None, None, Decimal::ZERO), None);
        // With a supplier discount: gross minus it, unless a bigger wholesale markup wins.
        let gross = d("100");
        assert_eq!(
            purchase_price(None, some("70"), some("95"), some("10"), gross),
            some("90")
        );
        assert_eq!(
            purchase_price(some("20"), some("70"), some("95"), some("10"), gross),
            some("70")
        );
        assert_eq!(
            purchase_price(some("5"), some("70"), some("95"), some("10"), gross),
            some("90")
        );
        assert_eq!(
            purchase_price(None, None, some("95"), some("100"), gross),
            some("95"),
            "0 falls back"
        );
    }
}
