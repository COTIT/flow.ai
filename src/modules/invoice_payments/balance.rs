//! Laravel UpdateInvoiceBalanceAfterPayment: the status an invoice's payments give it, in
//! minor units. Laravel compared at the nearest 0.05 for every payment; here only when the
//! invoice has a cash payment (cash rounding), else exactly.

use rust_decimal::{Decimal, RoundingStrategy};

/// What an invoice's paid, unvoided payments add up to, in minor units at `dp` decimals.
#[derive(Clone, Copy, Debug)]
pub struct Balance {
    /// The invoice total (negative for a credit note).
    pub total: i64,
    pub paid: i64,
    /// A paid cash payment is among them.
    pub cash: bool,
    pub dp: u32,
}

/// The status after a payment write; None keeps `current`. Only open and settled statuses
/// move (Laravel also set a cancelled or draft invoice back to `pending`).
pub fn status(current: &str, is_credit: bool, p: Balance) -> Option<&'static str> {
    let cmp = |minor: i64| {
        if p.cash {
            nearest(minor, p.dp).unwrap_or(minor)
        } else {
            minor
        }
    };
    if is_credit {
        if !matches!(current, "pending" | "credited") {
            return None;
        }
        // Both sides are refunds: credited once as much went back as the note is worth.
        return Some(if cmp(p.paid) <= cmp(p.total) {
            "credited"
        } else {
            "pending"
        });
    }
    if !matches!(current, "pending" | "partially_paid" | "paid") {
        return None;
    }
    if p.total == 0 || (p.paid > 0 && p.total > 0 && cmp(p.paid) >= cmp(p.total)) {
        Some("paid")
    } else if p.paid == 0 {
        Some("pending")
    } else if p.paid < p.total {
        Some("partially_paid")
    } else {
        None
    }
}

/// Laravel MoneyHelper::nearestAmount in minor units: the nearest 0.05, half away from zero.
/// None below 2 decimals (no 0.05 step exists).
fn nearest(minor: i64, dp: u32) -> Option<i64> {
    let step = 10_i64.checked_pow(dp.checked_sub(2)?)?.checked_mul(5)?;
    let steps = (Decimal::from(minor) / Decimal::from(step))
        .round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero);
    i64::try_from(steps).ok()?.checked_mul(step)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paid(total: i64, paid: i64, cash: bool) -> Balance {
        Balance {
            total,
            paid,
            cash,
            dp: 2,
        }
    }

    #[test]
    fn invoices_move_between_pending_partly_and_paid() {
        let s = |p| status("pending", false, p);
        assert_eq!(s(paid(10_000, 0, false)), Some("pending"));
        assert_eq!(s(paid(10_000, 6_000, false)), Some("partially_paid"));
        assert_eq!(s(paid(10_000, 10_000, false)), Some("paid"));
        assert_eq!(
            s(paid(10_000, 12_000, false)),
            Some("paid"),
            "overpaid stays paid (Laravel)"
        );
        assert_eq!(s(paid(0, 0, false)), Some("paid"), "nothing to pay");
        assert_eq!(
            status("paid", false, paid(10_000, 6_000, false)),
            Some("partially_paid"),
            "a void moves it back"
        );
    }

    #[test]
    fn the_005_tolerance_needs_a_cash_payment() {
        // EUR 100.00 with 99.98 paid: cash rounds to 100.00, a transfer leaves 0.02 open.
        assert_eq!(
            status("partially_paid", false, paid(10_000, 9_998, true)),
            Some("paid")
        );
        assert_eq!(
            status("partially_paid", false, paid(10_000, 9_998, false)),
            Some("partially_paid")
        );
        for under in [1, 2] {
            assert_eq!(
                status("pending", false, paid(10_000, 10_000 - under, true)),
                Some("paid"),
                "{under} cent under rounds up"
            );
        }
        for under in [3, 4] {
            assert_eq!(
                status("pending", false, paid(10_000, 10_000 - under, true)),
                Some("partially_paid"),
                "{under} cents under rounds to 99.95"
            );
        }
    }

    #[test]
    fn credit_notes_are_credited_once_refunded() {
        let s = |p| status("pending", true, p);
        assert_eq!(s(paid(-5_000, 0, false)), Some("pending"));
        assert_eq!(s(paid(-5_000, -5_000, false)), Some("credited"));
        assert_eq!(s(paid(-5_000, -2_000, false)), Some("pending"));
        assert_eq!(
            status("credited", true, paid(-5_000, 0, false)),
            Some("pending"),
            "a void reopens it (Laravel left it credited)"
        );
    }

    #[test]
    fn other_statuses_are_left_alone() {
        for current in ["draft", "cancelled", "awaiting_pos_payment", "planned"] {
            assert_eq!(
                status(current, false, paid(10_000, 0, false)),
                None,
                "{current}"
            );
        }
        assert_eq!(status("partially_paid", true, paid(-5_000, 0, false)), None);
        // A negative invoice that is not a credit note: Laravel kept its status once paid.
        assert_eq!(status("pending", false, paid(-1_000, 500, false)), None);
    }

    #[test]
    fn nearest_follows_php_rounding() {
        assert_eq!(nearest(9_998, 2), Some(10_000));
        assert_eq!(nearest(9_997, 2), Some(9_995));
        assert_eq!(nearest(-9_998, 2), Some(-10_000));
        assert_eq!(nearest(12, 3), Some(0), "0.012 at 3 decimals");
        assert_eq!(nearest(25, 3), Some(50), "half away from zero");
        assert_eq!(nearest(7, 0), None);
    }
}
