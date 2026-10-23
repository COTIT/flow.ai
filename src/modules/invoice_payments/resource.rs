//! Laravel InvoicePaymentResource, and the invoice `balance` figures. Amounts are stored in
//! minor units; Laravel printed its decimal(40,8) columns as strings with 8 places.

use rust_decimal::Decimal;
use serde_json::{Value, json};

use crate::modules::invoice_payments::repo::PaymentRow;

/// `minor` at `dp` decimals as currency units (0 past Decimal's 28 places, which a currency's
/// 0..8 never reach).
pub fn units(minor: i64, dp: u32) -> Decimal {
    Decimal::try_new(minor, dp).unwrap_or_default()
}

/// Laravel's decimal column text: 8 places.
fn column(minor: i64, dp: u32) -> String {
    format!("{:.8}", units(minor, dp))
}

/// `balance.paid` as Laravel printed `getPaidAmount()`: 0 without paid payments, else the
/// summed column text.
pub fn paid(paid_total: i64, dp: u32) -> Value {
    if paid_total == 0 {
        Value::from(0)
    } else {
        Value::from(column(paid_total, dp))
    }
}

/// One payment.
pub fn payment(p: &PaymentRow, dp: u32) -> Value {
    json!({
        "id": p.id,
        "description": p.description,
        "date": p.date.format("%Y-%m-%dT00:00:00.000000Z").to_string(),
        "amount": column(p.amount, dp),
        // Laravel set it only on till and settle rows; its API payments printed null.
        "paid_amount": null,
        "is_paid": i32::from(p.is_paid),
        "provider": p.provider,
        "provider_id": p.provider_id,
        "status": p.status,
        "payment_method": p.payment_method,
        "created_at": p.created_at,
        "last_updated_at": p.updated_at,
        "meta": p.meta,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_prints_as_laravel_columns() {
        assert_eq!(column(9_998, 2), "99.98000000");
        assert_eq!(column(-3_000, 2), "-30.00000000");
        assert_eq!(column(1_234, 3), "1.23400000");
        assert_eq!(paid(0, 2), Value::from(0));
        assert_eq!(paid(6_000, 2), Value::from("60.00000000"));
    }
}
