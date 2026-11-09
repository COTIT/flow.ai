//! Conversion math, integers only. `rate` = major units of `to` per major unit of `from`;
//! the result is rounded half up once (away from zero for negatives) to a minor unit of
//! `to`. Same rule as farmer credits (`farm_supply::credit`).

use rust_decimal::Decimal;

/// `amount` minor units of a `from_dec` currency in minor units of a `to_dec` currency.
/// `inverse`: the rate is stored the other way round (1 `to` = `rate` `from`), so divide.
/// None when the rate is not positive, decimals pass 18, or the result passes `i64`.
pub fn convert(
    amount: i64,
    rate: Decimal,
    from_dec: u32,
    to_dec: u32,
    inverse: bool,
) -> Option<i64> {
    let (m, scale) = (rate.mantissa(), rate.scale());
    if m <= 0 || from_dec > 18 || to_dec > 18 {
        return None;
    }
    let p = |e: u32| 10_i128.checked_pow(e);
    // direct:  a · m · 10^to / (10^scale · 10^from)
    // inverse: a · 10^scale · 10^to / (m · 10^from)
    let (num, den) = if inverse {
        (
            p(scale)?.checked_mul(p(to_dec)?)?,
            m.checked_mul(p(from_dec)?)?,
        )
    } else {
        (
            m.checked_mul(p(to_dec)?)?,
            p(scale)?.checked_mul(p(from_dec)?)?,
        )
    };
    let num = i128::from(amount).checked_mul(num)?;
    let rounded = (num
        .unsigned_abs()
        .checked_mul(2)?
        .checked_add(den.unsigned_abs())?)
        / den.unsigned_abs().checked_mul(2)?;
    let rounded = i128::try_from(rounded).ok()?;
    i64::try_from(if num < 0 { -rounded } else { rounded }).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn direct_shifts_decimals_and_rounds_half_up_once() {
        // EUR 100.00 at 2950.5 TZS (0 dec) = 295,050.
        assert_eq!(convert(10_000, d("2950.5"), 2, 0, false), Some(295_050));
        // Same at TZS with 2 decimals.
        assert_eq!(convert(10_000, d("2950.5"), 2, 2, false), Some(29_505_000));
        // 0.01 EUR × 150 = 1.5 JPY -> 2 (exact half up); 0.01 × 149.99 = 1.4999 -> 1.
        assert_eq!(convert(1, d("150"), 2, 0, false), Some(2));
        assert_eq!(convert(1, d("149.99"), 2, 0, false), Some(1));
        assert_eq!(convert(0, d("150"), 2, 0, false), Some(0));
    }

    #[test]
    fn negatives_mirror_positives() {
        assert_eq!(convert(-1, d("150"), 2, 0, false), Some(-2));
        assert_eq!(convert(-1, d("149.99"), 2, 0, false), Some(-1));
        assert_eq!(convert(-10_000, d("2950.5"), 2, 0, false), Some(-295_050));
    }

    #[test]
    fn inverse_divides_by_the_stored_rate() {
        // TZS 295,050 / 2950.5 = EUR 100.00.
        assert_eq!(convert(295_050, d("2950.5"), 0, 2, true), Some(10_000));
        // TZS 1,000 / 2950.5 = 0.338925… EUR -> 34 cents.
        assert_eq!(convert(1_000, d("2950.5"), 0, 2, true), Some(34));
        // JPY 74 / 150 = 0.49333 EUR -> 49 cents; JPY 1 / 150 = 0.667 cents -> 1.
        assert_eq!(convert(74, d("150"), 0, 2, true), Some(49));
        assert_eq!(convert(1, d("150"), 0, 2, true), Some(1));
        // Exact half: 3 / 2 = 1.5 -> 2.
        assert_eq!(convert(3, d("2"), 0, 0, true), Some(2));
        assert_eq!(convert(-3, d("2"), 0, 0, true), Some(-2));
    }

    #[test]
    fn tiny_and_huge_rates() {
        assert_eq!(convert(10_000, d("0.0004"), 0, 2, false), Some(400));
        assert_eq!(convert(100, d("0.00000001"), 2, 2, false), Some(0));
        assert_eq!(
            convert(100, d("1000000000"), 2, 2, false),
            Some(100_000_000_000)
        );
    }

    #[test]
    fn refuses_overflow_and_bad_rates() {
        assert_eq!(convert(i64::MAX, d("1000000000"), 2, 2, false), None);
        assert_eq!(convert(i64::MIN, d("2"), 2, 2, false), None);
        assert_eq!(convert(1, d("0"), 2, 2, false), None);
        assert_eq!(convert(1, d("-1"), 2, 2, false), None);
        assert_eq!(convert(1, d("0"), 2, 2, true), None);
        // Largest i64 that still fits after × 1.
        assert_eq!(convert(i64::MAX, d("1"), 2, 2, false), Some(i64::MAX));
    }
}
