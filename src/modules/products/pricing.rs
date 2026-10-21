//! Product price rules from Laravel ProductPriceTrait + Product saving hook, in exact decimals:
//! net/impact normalisation, purchase price with supplier discounts, price corrections (bands)
//! and the product's active special offer.

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

/// What a product's active special offer needs for its price (Laravel SpecialOffer).
#[derive(Clone, Copy)]
pub struct Offer<'a> {
    pub kind: &'a str,
    pub value: Decimal,
    pub base_price: Option<Decimal>,
    pub rule: Option<&'a str>,
    pub discount_value: Option<Decimal>,
    pub discount_type: Option<&'a str>,
    pub condition_quantity: Option<i32>,
}

/// Laravel `calculateSpecialPrice` without a cart, for the product's own active offer: `base`
/// is the price being built, `raw` the stored net price (cart-rule amounts use it). None on
/// overflow.
pub fn special(base: Decimal, raw: Decimal, o: &Offer) -> Option<Decimal> {
    let hundred = Decimal::ONE_HUNDRED;
    let off = |b: Decimal, pct: Decimal| {
        b.checked_mul(hundred.checked_sub(pct)?)?
            .checked_div(hundred)
    };
    // Laravel's fallback for rule-type offers: `type` against `value`.
    let by_type = || match o.kind {
        "percentage" => off(base, o.value),
        "exact" => Some(o.value),
        _ => base.checked_sub(o.value),
    };
    let Some(rule) = o.rule else {
        // calculateLegacyPromotionalPrice: percentage and fixed use the frozen base price.
        return match (o.kind, o.base_price) {
            ("percentage", frozen) => off(frozen.unwrap_or(base), o.value),
            ("fixed", frozen) => frozen.unwrap_or(base).checked_sub(o.value),
            _ => Some(o.value),
        };
    };
    if !matches!(rule, "global_discount" | "cross_product_discount" | "bogo") {
        return match (o.kind, o.discount_value, o.discount_type) {
            ("percentage", ..) => off(base, o.value),
            ("exact", ..) => Some(o.value),
            (_, Some(dv), Some("percentage")) => off(base, dv),
            (_, Some(dv), Some("fixed")) => Some(base.checked_sub(dv)?.max(Decimal::ZERO)),
            _ => base.checked_sub(o.value),
        };
    }
    // Cart rules: CalculateSpecialOfferDiscount's amount for this offer (a 0..100 value
    // without a discount type counts as a percentage).
    let kind = o.discount_type.or_else(|| {
        o.discount_value
            .filter(|v| (Decimal::ZERO..=hundred).contains(v))
            .map(|_| "percentage")
    });
    let amount = match (o.discount_value, kind) {
        (Some(dv), Some("percentage")) => raw.checked_mul(dv)?.checked_div(hundred)?,
        (Some(dv), Some("fixed")) => dv,
        (Some(_), Some("free")) => raw,
        (Some(dv), Some("bundle_price")) => match o.condition_quantity.filter(|q| *q > 0) {
            Some(q) => raw.checked_sub(dv.checked_div(Decimal::from(q))?)?,
            None => Decimal::ZERO,
        },
        (None, Some("percentage")) => raw.checked_mul(o.value)?.checked_div(hundred)?,
        (dv, _) => dv.unwrap_or(o.value),
    };
    if amount > Decimal::ZERO {
        Some(base.checked_sub(amount)?.max(Decimal::ZERO))
    } else {
        by_type()
    }
}

/// Laravel's four net prices.
pub struct Prices {
    /// `price` (getConsumerPrice).
    pub consumer: Decimal,
    /// `original_price` (getOriginalPrice): no special offer.
    pub original: Decimal,
    /// `business_price` (getBusinessPrice).
    pub business: Decimal,
    /// `price_without_correction` (getPriceWithoutCorrection).
    pub uncorrected: Decimal,
}

/// What `prices` reads from a product and its workspace.
pub struct PriceInputs<'a> {
    pub net: Decimal,
    pub gross: Decimal,
    pub impact_on_price: Option<Decimal>,
    /// `exclude_price_corrections`.
    pub excluded: bool,
    /// Setting `product.disable_double_discount_for_business`.
    pub business_from_gross: bool,
    pub vat_pct: Decimal,
    pub bands: &'a [Band],
    /// The product's active special offer.
    pub offer: Option<Offer<'a>>,
}

/// Laravel ProductPriceTrait getters. With `business_from_gross`, a product priced below gross
/// starts its business price from gross, and its consumer price too while an offer is active.
pub fn prices(i: &PriceInputs) -> Prices {
    let correct_ = |v: Decimal| {
        band_for(i.bands, v)
            .filter(|_| !i.excluded)
            .and_then(|b| correct(v, i.vat_pct, b))
            .unwrap_or(v)
    };
    // A special price never goes below 0 (Laravel could print a negative one).
    let special_ = |v: Decimal| {
        i.offer
            .as_ref()
            .and_then(|o| special(v, i.net, o))
            .map_or(v, |s| s.max(Decimal::ZERO))
    };
    let from_gross = i.impact_on_price.is_some_and(|v| v < Decimal::ZERO) && i.business_from_gross;
    let business_base = if from_gross { i.gross } else { i.net };
    let consumer_base = if from_gross && i.offer.is_some() {
        i.gross
    } else {
        i.net
    };
    Prices {
        consumer: correct_(special_(consumer_base)),
        original: correct_(i.net),
        business: correct_(special_(business_base)),
        uncorrected: special_(business_base),
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
        let p = prices(&PriceInputs {
            net: d("10.3"),
            gross: d("10.3"),
            impact_on_price: None,
            excluded: false,
            business_from_gross: false,
            vat_pct: Decimal::ZERO,
            bands: &bands,
            offer: None,
        });
        // 10.3 is in the second band only: 0.5 steps, 10.5 is nearer than 10.
        assert_eq!(
            (p.consumer, p.business, p.uncorrected),
            (d("10.5"), d("10.5"), d("10.3"))
        );
        let p = prices(&PriceInputs {
            net: d("10.3"),
            gross: d("10.3"),
            impact_on_price: None,
            excluded: true,
            business_from_gross: false,
            vat_pct: Decimal::ZERO,
            bands: &bands,
            offer: None,
        });
        assert_eq!(p.consumer, d("10.3"), "excluded");
        // Below gross with the setting: business from gross, consumer from net.
        let p = prices(&PriceInputs {
            net: d("9.4"),
            gross: d("12.2"),
            impact_on_price: Some(d("-23")),
            excluded: false,
            business_from_gross: true,
            vat_pct: Decimal::ZERO,
            bands: &bands,
            offer: None,
        });
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

    /// Laravel calculateSpecialPrice branches (ProductPriceTrait) for an offer on the product.
    #[test]
    fn special_price_branches() {
        let offer = |kind, value: &str, rule, dv: Option<&str>, dt| Offer {
            kind,
            value: d(value),
            base_price: None,
            rule,
            discount_value: dv.map(d),
            discount_type: dt,
            condition_quantity: Some(3),
        };
        let at = |o: &Offer| special(d("100"), d("100"), o).unwrap();
        // Legacy.
        assert_eq!(at(&offer("percentage", "10", None, None, None)), d("90"));
        assert_eq!(at(&offer("fixed", "10", None, None, None)), d("90"));
        assert_eq!(at(&offer("exact", "70", None, None, None)), d("70"));
        let frozen = Offer {
            base_price: Some(d("50")),
            ..offer("percentage", "10", None, None, None)
        };
        assert_eq!(at(&frozen), d("45"), "frozen base price");
        // Rules without a cart: type first, then discount_value / discount_type.
        assert_eq!(
            at(&offer(
                "percentage",
                "25",
                Some("brand_discount"),
                None,
                None
            )),
            d("75")
        );
        assert_eq!(
            at(&offer(
                "fixed",
                "15",
                Some("brand_discount"),
                Some("15"),
                Some("fixed")
            )),
            d("85")
        );
        assert_eq!(
            at(&offer(
                "fixed",
                "500",
                Some("brand_discount"),
                Some("500"),
                Some("fixed")
            )),
            d("0")
        );
        assert_eq!(
            at(&offer(
                "free",
                "5",
                Some("brand_discount"),
                Some("5"),
                Some("free")
            )),
            d("95"),
            "else: price - value"
        );
        // Cart rules: the offer's own amount from the stored price.
        assert_eq!(
            at(&offer(
                "percentage",
                "20",
                Some("global_discount"),
                Some("20"),
                None
            )),
            d("80")
        );
        assert_eq!(
            at(&offer("free", "1", Some("bogo"), Some("1"), Some("free"))),
            d("0")
        );
        assert_eq!(
            at(&offer(
                "bundle_price",
                "150",
                Some("bogo"),
                Some("150"),
                Some("bundle_price")
            )),
            d("50"),
            "100 - 150/3"
        );
        assert_eq!(
            at(&offer(
                "fixed",
                "250",
                Some("global_discount"),
                Some("250"),
                None
            )),
            d("0"),
            "over 100: an amount"
        );
        // Amount 0 falls back to type/value.
        assert_eq!(
            at(&offer(
                "exact",
                "60",
                Some("global_discount"),
                Some("0"),
                Some("fixed")
            )),
            d("60")
        );
        // Overflow: None (the caller keeps the price).
        assert!(
            special(
                MAX,
                MAX,
                &offer(
                    "percentage",
                    "0",
                    Some("global_discount"),
                    Some(&MAX.to_string()),
                    Some("percentage")
                )
            )
            .is_none()
        );
    }

    #[test]
    fn offer_feeds_consumer_business_and_uncorrected() {
        let bands: [Band; 0] = [];
        let o = Offer {
            kind: "percentage",
            value: d("10"),
            base_price: None,
            rule: Some("brand_discount"),
            discount_value: Some(d("10")),
            discount_type: Some("percentage"),
            condition_quantity: None,
        };
        let i = |bfg| PriceInputs {
            net: d("90"),
            gross: d("100"),
            impact_on_price: Some(d("-10")),
            excluded: false,
            business_from_gross: bfg,
            vat_pct: Decimal::ZERO,
            bands: &bands,
            offer: Some(o),
        };
        let p = prices(&i(false));
        assert_eq!(
            (p.consumer, p.original, p.business, p.uncorrected),
            (d("81"), d("90"), d("81"), d("81"))
        );
        let p = prices(&i(true));
        assert_eq!(
            (p.consumer, p.original, p.business, p.uncorrected),
            (d("90"), d("90"), d("90"), d("90"))
        );
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
