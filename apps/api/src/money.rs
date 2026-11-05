//! Money and decimal output: vimdesk-api `MoneyHelper::formatDecimals`, done in exact decimals.
//! Convention for every module: rates, unit prices, percentages are `numeric(24,8)` read as
//! `Decimal`; rounding is half away from zero (PHP `round`); floats never touch money.

use rust_decimal::{Decimal, RoundingStrategy};

use crate::error::AppError;

/// Largest `numeric(24,8)` value: 9999999999999999.99999999 (lo/mid/hi words of 10^24 - 1).
pub const MAX: Decimal = Decimal::from_parts(2_701_131_775, 466_537_709, 54_210, false, 8);

/// An `amount` the client typed, in minor units at `dp`, exact: 422 on `amount` past the
/// currency's decimals or past i64. Trailing zeros are no extra decimals.
pub fn minor(amount: Decimal, dp: u32) -> Result<i64, AppError> {
    if amount.normalize().scale() > dp {
        return Err(AppError::field(
            "amount",
            format!("The amount must have at most {dp} decimal places."),
        ));
    }
    10_i64
        .checked_pow(dp)
        .and_then(|scale| amount.checked_mul(Decimal::from(scale)))
        .and_then(|m| i64::try_from(m).ok())
        .ok_or_else(|| AppError::field("amount", "The amount is out of range."))
}

/// How Laravel held an exact zero. A `float` cast or a computed value is PHP-falsy and prints
/// `"0.00"`; an uncast DB string like `"0.00000000"` is truthy and prints `"0"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zero {
    Falsy,
    Column,
}

/// `MoneyHelper::formatDecimals($value, $dp, $rtrim)`: `None` -> `"0.00"`, else round to `dp`,
/// print `dp` digits, drop trailing zeros when `rtrim`, then a trailing `.`. One deliberate
/// difference: with `dp = 0` PHP trims "100" to "1"; this keeps "100".
pub fn format_decimals(value: Option<Decimal>, dp: u32, rtrim: bool, zero: Zero) -> String {
    let Some(v) = value else {
        return "0.00".into();
    };
    if v.is_zero() && zero == Zero::Falsy {
        return "0.00".into();
    }
    let mut r = round(v, dp);
    if r.is_zero() {
        // PHP number_format never prints "-0".
        r = Decimal::ZERO;
    }
    r.rescale(dp);
    let mut s = r.to_string();
    // dp = 0 has no point; trimming zeros there would turn "100" into "1".
    if rtrim && s.contains('.') {
        s.truncate(s.trim_end_matches('0').len());
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

/// A PHP float as `json_encode` printed it: whole numbers without a fraction (`5`, not `5.0`).
/// A whole number past i64 prints as a float, the way PHP held it.
pub fn php_number(d: Decimal) -> serde_json::Value {
    use rust_decimal::prelude::ToPrimitive;
    let whole = d.fract().is_zero().then(|| d.to_i64()).flatten();
    whole.map(serde_json::Value::from).unwrap_or_else(|| {
        d.to_f64()
            .map_or(serde_json::Value::Null, serde_json::Value::from)
    })
}

/// PHP `round($v, $dp)`: half away from zero.
pub fn round(v: Decimal, dp: u32) -> Decimal {
    v.round_dp_with_strategy(dp, RoundingStrategy::MidpointAwayFromZero)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::str::FromStr;

    #[test]
    fn amounts_turn_into_minor_units_exactly() {
        let m = |s: &str, dp| minor(Decimal::from_str(s).unwrap(), dp).ok();
        assert_eq!(m("39.98", 2), Some(3_998));
        assert_eq!(m("100", 2), Some(10_000));
        assert_eq!(
            m("1.50", 2),
            Some(150),
            "trailing zeros are no extra decimals"
        );
        assert_eq!(m("1.234", 2), None);
        assert_eq!(m("1.234", 3), Some(1_234));
        assert_eq!(m("12", 0), Some(12));
        assert_eq!(m("99999999999999999", 2), None, "past i64");
    }

    #[derive(Deserialize)]
    struct Case {
        value: Option<String>,
        kind: String,
        dp: u32,
        rtrim: bool,
        expected: String,
    }

    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }

    /// Every case PHP 8.4 produced from the real MoneyHelper.php, including the ones past
    /// 15 significant digits (PHP floats happen to agree on those too).
    #[test]
    fn matches_php_oracle() {
        let fixture: Fixture =
            serde_json::from_str(include_str!("../tests/fixtures/format_decimals.json")).unwrap();
        assert!(fixture.cases.len() > 200);
        for c in fixture.cases {
            let value = c.value.as_deref().map(|v| Decimal::from_str(v).unwrap());
            let zero = if c.kind == "float" {
                Zero::Falsy
            } else {
                Zero::Column
            };
            assert_eq!(
                format_decimals(value, c.dp, c.rtrim, zero),
                c.expected,
                "{:?} ({}) dp={} rtrim={}",
                c.value,
                c.kind,
                c.dp,
                c.rtrim
            );
        }
    }

    #[test]
    fn negative_zero_after_rounding_has_no_sign() {
        let v = Decimal::from_str("-0.004").unwrap();
        assert_eq!(format_decimals(Some(v), 2, false, Zero::Column), "0.00");
    }

    #[test]
    fn php_number_never_prints_null() {
        let whole = Decimal::from_str("100000000000000000000").unwrap();
        assert_eq!(php_number(whole).as_f64(), Some(1e20));
        assert_eq!(php_number(Decimal::from(5)), serde_json::json!(5));
        assert_eq!(
            php_number(Decimal::from_str("0.1").unwrap()),
            serde_json::json!(0.1)
        );
    }

    #[test]
    fn max_is_numeric_24_8_ceiling() {
        assert_eq!(MAX.to_string(), "9999999999999999.99999999");
    }

    #[test]
    fn keeps_eight_decimals_when_asked() {
        let v = Decimal::from_str("9999999999999999.99999999").unwrap();
        assert_eq!(
            format_decimals(Some(v), 8, true, Zero::Column),
            "9999999999999999.99999999"
        );
    }
}
