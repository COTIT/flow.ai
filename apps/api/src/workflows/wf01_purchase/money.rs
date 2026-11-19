//! WF-01 money rules (facts O6, PLAN P-3, decision A strict): exact minor units, Odoo's verified
//! untaxed formulas, and the pre-write checks P1 (`rate_precision`) / P2 (`untotalled_lines`).
//! Pure: no I/O. C4b reuses `odoo_untaxed` and `to_minor`.

use rust_decimal::{Decimal, RoundingStrategy};

use crate::integrations::capability::PurchaseRequest;

/// `res.company.tax_calculation_rounding_method` (facts O6 L186).
// shortcut: intake uses Globally only (strict rule A); PerLine serves C4b's V2 check.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rounding {
    PerLine,
    Globally,
}

/// One line's inputs to Odoo's base amount.
#[derive(Debug, Clone, Copy)]
pub struct LineInput {
    pub price_unit: Decimal,
    pub quantity: Decimal,
    /// Percent, 0 when none.
    pub discount: Decimal,
}

/// `currency.round`: HALF-UP, ties away from zero (facts O2 L123). The float epsilon Odoo adds
/// only repairs float artefacts; exact decimals need none.
pub fn currency_round(d: Decimal, decimals: u8) -> Decimal {
    d.round_dp_with_strategy(u32::from(decimals), RoundingStrategy::MidpointAwayFromZero)
}

/// `d` in minor units when exact at `decimals`; None when it carries finer digits or overflows.
pub fn to_minor(d: Decimal, decimals: u8) -> Option<i64> {
    let scaled = d.checked_mul(Decimal::from(10_i64.checked_pow(u32::from(decimals))?))?;
    if !scaled.fract().is_zero() {
        return None;
    }
    i64::try_from(scaled.trunc()).ok()
}

/// Raw base `price_unit × (1 − discount/100) × qty` (facts O6 L201-L204). None on overflow.
fn raw(l: &LineInput) -> Option<Decimal> {
    let factor = Decimal::ONE.checked_sub(l.discount.checked_div(Decimal::ONE_HUNDRED)?)?;
    l.price_unit.checked_mul(factor)?.checked_mul(l.quantity)
}

/// Odoo's `amount_untaxed` for untaxed or price-excluded lines in one currency (facts O6
/// L241-L243): per line Σ round(raw); globally round(Σ raw). None on overflow.
pub fn odoo_untaxed(lines: &[LineInput], method: Rounding, decimals: u8) -> Option<Decimal> {
    let mut sum = Decimal::ZERO;
    for l in lines {
        let r = raw(l)?;
        sum = sum.checked_add(match method {
            Rounding::PerLine => currency_round(r, decimals),
            Rounding::Globally => r,
        })?;
    }
    Some(currency_round(sum, decimals))
}

/// Minor-unit amounts of a request plus the money blocking codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// Per line, in request line order; rounded when not exact (then a reason is set).
    pub subtotals_minor: Vec<i64>,
    pub untaxed_minor: i64,
    pub tax_minor: i64,
    pub total_minor: i64,
    /// `amount_not_exact`, `rate_precision`, `untotalled_lines`, each at most once.
    pub reasons: Vec<String>,
}

/// Minor units, rounded and flagged when not exact or negative.
fn minor(d: Decimal, decimals: u8, reasons: &mut Vec<String>) -> i64 {
    match to_minor(d, decimals).filter(|m| *m >= 0) {
        Some(m) => m,
        None => {
            push(reasons, "amount_not_exact");
            to_minor(currency_round(d, decimals), decimals)
                .unwrap_or(0)
                .max(0)
        }
    }
}

fn push(reasons: &mut Vec<String>, code: &str) {
    if !reasons.iter().any(|r| r == code) {
        reasons.push(code.into());
    }
}

// shortcut: strict rule A. The adapter reads no discount, no company rounding method, no tax
// price_include flag and no currency rounding step, so discounts, price-included taxes, 0.05-step
// currencies and sub-cent price × qty block as `rate_precision` / `untotalled_lines`. Upgrade
// once the adapter reads those facts and the rule is verified on a sandbox (gate L1).
/// P1/P2 for one request: every raw line base exact in minor units (`rate_precision`), every
/// `price_subtotal` equal to it, and `amount_untaxed` equal to Odoo's untaxed total
/// (`untotalled_lines`). With every line exact both rounding methods give Σ subtotals.
pub fn check(req: &PurchaseRequest) -> Checked {
    let d = req.currency_decimals;
    let mut reasons = Vec::new();
    let mut inputs = Vec::with_capacity(req.lines.len());
    let mut subtotals_minor = Vec::with_capacity(req.lines.len());
    for l in &req.lines {
        subtotals_minor.push(minor(l.subtotal, d, &mut reasons));
        let input = LineInput {
            price_unit: l.price_unit,
            quantity: l.quantity,
            discount: Decimal::ZERO,
        };
        match raw(&input).and_then(|r| to_minor(r, d).map(|m| (r, m))) {
            Some((r, _)) if r != l.subtotal => push(&mut reasons, "untotalled_lines"),
            Some(_) => {}
            None => push(&mut reasons, "rate_precision"),
        }
        inputs.push(input);
    }
    let untaxed_minor = minor(req.amount_untaxed, d, &mut reasons);
    let tax_minor = minor(req.amount_tax, d, &mut reasons);
    let total_minor = minor(req.amount_total, d, &mut reasons);
    // P2 only means something once every line base is exact (then both methods agree).
    let lines_exact = !reasons.iter().any(|r| r == "rate_precision");
    if lines_exact && odoo_untaxed(&inputs, Rounding::Globally, d) != Some(req.amount_untaxed) {
        push(&mut reasons, "untotalled_lines");
    }
    Checked {
        subtotals_minor,
        untaxed_minor,
        tax_minor,
        total_minor,
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::integrations::capability::{RequestLine, SourceState};

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    fn li(price: &str, qty: &str) -> LineInput {
        LineInput {
            price_unit: d(price),
            quantity: d(qty),
            discount: Decimal::ZERO,
        }
    }

    fn req(lines: &[(&str, &str, &str)], untaxed: &str, decimals: u8) -> PurchaseRequest {
        PurchaseRequest {
            id: "17".into(),
            state: SourceState::Draft,
            company_id: "1".into(),
            vendor_id: "42".into(),
            currency_id: "1".into(),
            currency_ref: "EUR".into(),
            currency_decimals: decimals,
            buyer_id: None,
            location_id: None,
            date_order: None,
            lines: lines
                .iter()
                .enumerate()
                .map(|(i, (p, q, s))| RequestLine {
                    id: i.to_string(),
                    product_id: "9".into(),
                    uom_id: "1".into(),
                    quantity: d(q),
                    price_unit: d(p),
                    subtotal: d(s),
                    tax_ids: vec![],
                    date_planned: None,
                    downpayment: false,
                })
                .collect(),
            amount_untaxed: d(untaxed),
            amount_tax: d("0"),
            amount_total: d(untaxed),
            labels: Default::default(),
        }
    }

    #[test]
    fn to_minor_is_exact_or_none() {
        assert_eq!(to_minor(d("1250.00"), 2), Some(125_000));
        assert_eq!(to_minor(d("0.999"), 2), None);
        assert_eq!(to_minor(d("1500"), 0), Some(1500));
        assert_eq!(to_minor(d("0.5"), 0), None);
        assert_eq!(to_minor(d("1.234"), 3), Some(1234));
        assert_eq!(to_minor(d("-1.10"), 2), Some(-110));
    }

    #[test]
    fn currency_round_is_half_away_from_zero() {
        assert_eq!(currency_round(d("0.125"), 2), d("0.13"));
        assert_eq!(currency_round(d("-0.125"), 2), d("-0.13"));
        assert_eq!(currency_round(d("0.124999"), 2), d("0.12"));
        assert_eq!(currency_round(d("2.5"), 0), d("3"));
    }

    // facts O6 L240: Odoo docstring (account_tax.py#L2036-L2040), two lines of 12.12 × 12.12:
    // per line 146.89 + 146.89 = 293.78; globally round(2 × 146.8944) = 293.79.
    #[test]
    fn odoo_untaxed_golden_values_from_the_docstring() {
        let lines = [li("12.12", "12.12"), li("12.12", "12.12")];
        assert_eq!(
            odoo_untaxed(&lines, Rounding::PerLine, 2),
            Some(d("293.78"))
        );
        assert_eq!(
            odoo_untaxed(&lines, Rounding::Globally, 2),
            Some(d("293.79"))
        );
    }

    #[test]
    fn odoo_untaxed_applies_discount_and_currency_decimals() {
        let disc = LineInput {
            discount: d("10"),
            ..li("100", "3")
        };
        assert_eq!(
            odoo_untaxed(&[disc], Rounding::PerLine, 2),
            Some(d("270.00"))
        );
        assert_eq!(
            odoo_untaxed(&[li("0.333", "3")], Rounding::Globally, 2),
            Some(d("1.00"))
        );
        assert_eq!(
            odoo_untaxed(&[li("0.4", "3"), li("0.4", "3")], Rounding::PerLine, 0),
            Some(d("2"))
        );
        assert_eq!(
            odoo_untaxed(&[li("0.4", "3"), li("0.4", "3")], Rounding::Globally, 0),
            Some(d("2"))
        );
        assert_eq!(odoo_untaxed(&[], Rounding::Globally, 2), Some(d("0")));
    }

    #[test]
    fn check_passes_exact_lines() {
        let c = check(&req(
            &[("150", "2", "300.00"), ("1.25", "4", "5.00")],
            "305.00",
            2,
        ));
        assert_eq!(c.reasons, Vec::<String>::new());
        assert_eq!(c.subtotals_minor, vec![30_000, 500]);
        assert_eq!((c.untaxed_minor, c.total_minor), (30_500, 30_500));
    }

    // T30 rewritten per O6: Odoo stores 0.333 × 3 as subtotal 1.00 (currency.round), but the
    // raw base 0.999 has finer digits than the currency → rate_precision, never a guess.
    #[test]
    fn t30_sub_cent_line_base_blocks_rate_precision() {
        let c = check(&req(&[("0.333", "3", "1.00")], "1.00", 2));
        assert_eq!(c.reasons, vec!["rate_precision"]);
        assert_eq!(c.subtotals_minor, vec![100]);
    }

    // Golden pair from the docstring under strict rule A: each line base 146.8944 is sub-cent.
    #[test]
    fn docstring_order_blocks_under_both_methods() {
        for untaxed in ["293.78", "293.79"] {
            let c = check(&req(
                &[("12.12", "12.12", "146.89"), ("12.12", "12.12", "146.89")],
                untaxed,
                2,
            ));
            assert_eq!(c.reasons, vec!["rate_precision"], "untaxed {untaxed}");
        }
    }

    #[test]
    fn discount_or_price_included_tax_shows_as_untotalled_line() {
        // 10 % discount or an included tax: Odoo subtotal 90.00 ≠ 100 × 1.
        let c = check(&req(&[("100", "1", "90.00")], "90.00", 2));
        assert_eq!(c.reasons, vec!["untotalled_lines"]);
    }

    #[test]
    fn untaxed_not_matching_lines_is_untotalled() {
        let c = check(&req(&[("150", "2", "300.00")], "300.01", 2));
        assert_eq!(c.reasons, vec!["untotalled_lines"]);
    }

    #[test]
    fn amounts_with_finer_digits_or_negative_are_not_exact() {
        let mut r = req(&[("1", "1", "1.00")], "1.00", 2);
        r.amount_tax = d("0.215");
        r.amount_total = d("1.215");
        let c = check(&r);
        assert_eq!(c.reasons, vec!["amount_not_exact"]);
        assert_eq!((c.tax_minor, c.total_minor), (22, 122));
        let neg = check(&req(&[("-5", "1", "-5.00")], "-5.00", 2));
        assert!(neg.reasons.contains(&"amount_not_exact".to_string()));
        assert_eq!(neg.untaxed_minor, 0);
    }

    #[test]
    fn zero_decimal_currency() {
        let c = check(&req(&[("1500", "2", "3000")], "3000", 0));
        assert_eq!(c.reasons, Vec::<String>::new());
        assert_eq!(c.untaxed_minor, 3000);
        let c = check(&req(&[("1500.5", "1", "1501")], "1501", 0));
        assert_eq!(c.reasons, vec!["rate_precision"]);
    }
}
