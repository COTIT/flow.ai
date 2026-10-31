//! Farmer credit math, integers only. A sale of jars from a supply lot owes
//! `price_per_kg × kg sold`, rounded once to a minor unit; that total is split over the
//! batch's farmers by their kg with the largest-remainder rule, so the credits always sum to
//! the total exactly.

use rust_decimal::{Decimal, prelude::ToPrimitive};

/// A quantity with at most 2 decimals as hundredths (`12.50` -> 1250). None past 2 decimals
/// or out of range.
pub fn hundredths(q: Decimal) -> Option<i128> {
    let scaled = q.checked_mul(Decimal::ONE_HUNDRED)?;
    if scaled.fract().is_zero() {
        scaled.to_i128()
    } else {
        None
    }
}

/// `price_per_kg × jars × raw_kg ÷ units`, rounded half up to a minor unit: the kg in `jars`
/// packed `units` jars from `raw_kg` kg (1:1 for a raw lot: `raw_kg == units`). None when a
/// quantity has more than 2 decimals, `units` is not positive, or the result passes `i64`.
pub fn credit_total(
    price_per_kg: i64,
    jars: Decimal,
    raw_kg: Decimal,
    units: Decimal,
) -> Option<i64> {
    let (j, r, u) = (hundredths(jars)?, hundredths(raw_kg)?, hundredths(units)?);
    if u <= 0 || j < 0 || r < 0 {
        return None;
    }
    // total = P·(J/100)·(R/100) / (U/100) = P·J·R / (100·U)
    let num = i128::from(price_per_kg).checked_mul(j)?.checked_mul(r)?;
    let den = u.checked_mul(100)?;
    let rounded = num.checked_mul(2)?.checked_add(den)? / den.checked_mul(2)?;
    i64::try_from(rounded).ok()
}

/// Split `total` over `weights` (key, weight ≥ 0) by the largest-remainder rule: each gets
/// `floor(total·w / W)`, then the leftover units go one each to the largest remainders, ties
/// to the smaller key. Same order as `weights`; sums to `total` exactly. All zero (or empty)
/// weights -> all zeros.
pub fn split<K: Ord + Copy>(total: i64, weights: &[(K, i128)]) -> Vec<i64> {
    let sum: i128 = weights.iter().map(|w| w.1).sum();
    if sum <= 0 {
        return vec![0; weights.len()];
    }
    let t = i128::from(total);
    let mut shares: Vec<i128> = weights.iter().map(|w| (t * w.1).div_euclid(sum)).collect();
    let mut leftover = t - shares.iter().sum::<i128>();
    let mut order: Vec<usize> = (0..weights.len()).collect();
    order.sort_by(|&a, &b| {
        let ra = (t * weights[a].1).rem_euclid(sum);
        let rb = (t * weights[b].1).rem_euclid(sum);
        rb.cmp(&ra).then(weights[a].0.cmp(&weights[b].0))
    });
    for i in order {
        if leftover == 0 {
            break;
        }
        if weights[i].1 > 0 {
            shares[i] += 1;
            leftover -= 1;
        }
    }
    // Each share is at most |total|, which fits i64.
    shares.into_iter().map(|s| s as i64).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn hundredths_of_quantities() {
        assert_eq!(hundredths(d("12.5")), Some(1250));
        assert_eq!(hundredths(d("12.50")), Some(1250));
        assert_eq!(hundredths(d("0.01")), Some(1));
        assert_eq!(hundredths(d("3")), Some(300));
        assert_eq!(hundredths(d("1.234")), None);
    }

    #[test]
    fn credit_total_is_price_times_kg_rounded_half_up_once() {
        // 3 jars of 10 packed from 7.50 kg = 2.25 kg at TZS 8,000.00 = 18,000.00.
        assert_eq!(
            credit_total(800_000, d("3"), d("7.50"), d("10")),
            Some(1_800_000)
        );
        // Raw lot sold by the kg: 1:1.
        assert_eq!(
            credit_total(800_000, d("2.5"), d("1"), d("1")),
            Some(2_000_000)
        );
        // 10 kg into 3 jars, one jar = 3.333… kg: 1 × 333.33… = 333.33 -> 333; 2 jars 666.67 -> 667.
        assert_eq!(credit_total(100, d("1"), d("10"), d("3")), Some(333));
        assert_eq!(credit_total(100, d("2"), d("10"), d("3")), Some(667));
        // Exactly half rounds up: 1 × 0.5 kg × 1 = 0.5 -> 1.
        assert_eq!(credit_total(1, d("1"), d("1"), d("2")), Some(1));
        assert_eq!(credit_total(1, d("0"), d("1"), d("2")), Some(0));
    }

    #[test]
    fn credit_total_refuses_bad_or_huge_input() {
        assert_eq!(credit_total(1, d("1"), d("1"), d("0")), None);
        assert_eq!(credit_total(1, d("1.001"), d("1"), d("1")), None);
        assert_eq!(credit_total(i64::MAX, d("2"), d("1"), d("1")), None);
    }

    #[test]
    fn split_sums_exactly_and_gives_leftovers_to_largest_remainders() {
        // 100 over 1:1:1 -> 33.33 each; one leftover to the first key (ties by key).
        assert_eq!(split(100, &[(1, 1), (2, 1), (3, 1)]), [34, 33, 33]);
        // Order of input does not change who gets the unit.
        assert_eq!(split(100, &[(3, 1), (1, 1), (2, 1)]), [33, 34, 33]);
        // 1,800,000 over 250 / 120.5 / 30 kg (hundredths).
        let w = [(1, 25_000), (2, 12_050), (3, 3_000)];
        let s = split(1_800_000, &w);
        assert_eq!(s.iter().sum::<i64>(), 1_800_000);
        // 1,123,595.51 / 541,573.03 / 134,831.46: the one leftover unit goes to the first.
        assert_eq!(s, [1_123_596, 541_573, 134_831]);
        // Remainders 0.59 / 0.71 / 0.70 -> the leftover goes to 2 and 3, not 1.
        assert_eq!(split(10, &[(1, 59), (2, 271), (3, 670)]), [0, 3, 7]);
    }

    #[test]
    fn split_edges() {
        assert_eq!(split(1, &[(1, 5), (2, 5)]), [1, 0]);
        assert_eq!(split(0, &[(1, 5), (2, 5)]), [0, 0]);
        assert_eq!(split(7, &[(1, 0), (2, 3)]), [0, 7]);
        assert_eq!(split::<u8>(7, &[]), Vec::<i64>::new());
        assert_eq!(split(7, &[(1, 0), (2, 0)]), [0, 0]);
        // Negative totals (reversals) mirror the positive split.
        assert_eq!(split(-100, &[(1, 1), (2, 1), (3, 1)]), [-33, -33, -34]);
        let many: Vec<(u32, i128)> = (0..500).map(|i| (i, i128::from(i % 7 + 1))).collect();
        assert_eq!(split(123_457, &many).iter().sum::<i64>(), 123_457);
    }
}
