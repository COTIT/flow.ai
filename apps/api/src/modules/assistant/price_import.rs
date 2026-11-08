//! Price-list import rows: exact product match per workspace and strict price reading; writes nothing.

use super::{
    dto::PriceImportInput,
    repo::{self, Owner, PriceMatch},
    service,
};
use crate::AppError;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{collections::HashSet, str::FromStr};
use uuid::Uuid;

/// Resolve each row to `matched` or a reason it stays out of the review.
pub async fn resolve(
    db: &PgPool,
    o: Owner,
    session: Uuid,
    input: PriceImportInput,
) -> Result<Vec<Value>, AppError> {
    service::session(db, o, session).await?;
    let raw: Vec<String> = input.rows.iter().map(|r| r.key.trim().to_owned()).collect();
    let norm: Vec<String> = raw.iter().map(|k| normalise(k)).collect();
    let found = repo::price_matches(db, o.workspace_id, &raw, &norm).await?;
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(input.rows.len());
    for (index, row) in input.rows.iter().enumerate() {
        let ord = index as i64 + 1;
        let mine: Vec<&PriceMatch> = found.iter().filter(|m| m.ord == ord).collect();
        let best = mine.iter().map(|m| m.rank).min();
        let top: Vec<&&PriceMatch> = mine.iter().filter(|m| Some(m.rank) == best).collect();
        let line = row.line;
        let value = match top.as_slice() {
            [] => json!({"line":line,"status":"not_found"}),
            [one] => {
                let current = shown(one.price, one.decimals);
                let base =
                    json!({"line":line,"product_id":one.id,"name":one.name,"current":current});
                let status = if !seen.insert(one.id) {
                    "duplicate"
                } else {
                    match read_price(&row.price, one.decimals) {
                        Err(reason) => reason,
                        Ok(price) if price == one.price => "unchanged",
                        Ok(_) => "matched",
                    }
                };
                let mut base = base;
                base["status"] = json!(status);
                if status == "matched" {
                    base["price"] = json!(row.price.trim().replace(',', "."));
                }
                base
            }
            many => {
                json!({"line":line,"status":"ambiguous","candidates":many.iter().take(5).map(|m| m.name.clone()).collect::<Vec<_>>()})
            }
        };
        out.push(value);
    }
    Ok(out)
}

fn normalise(key: &str) -> String {
    key.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Exact decimal, one `.` or `,` mark, no sign or grouping; more decimals than the currency is never rounded.
fn read_price(text: &str, decimals: i16) -> Result<Decimal, &'static str> {
    let text = text.trim();
    let (whole, fraction) = match text.find(['.', ',']) {
        Some(at) => (&text[..at], &text[at + 1..]),
        None => (text, ""),
    };
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if whole.is_empty()
        || whole.len() > 16
        || !digits(whole)
        || !digits(fraction)
        || (text.len() > whole.len() && fraction.is_empty())
    {
        return Err("unreadable");
    }
    if fraction.len() > decimals.max(0) as usize {
        return Err("unclear_precision");
    }
    Decimal::from_str(&text.replace(',', ".")).map_err(|_| "unreadable")
}

/// Stored price at currency decimals when exact, else every stored digit.
fn shown(price: Decimal, decimals: i16) -> String {
    let mut value = price.normalize();
    let decimals = decimals.max(0) as u32;
    if value.scale() < decimals {
        value.rescale(decimals);
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_read_strictly() {
        assert_eq!(read_price(" 12,5 ", 2), Ok(Decimal::new(125, 1)));
        assert_eq!(read_price("0", 0), Ok(Decimal::ZERO));
        assert_eq!(read_price("12.50", 0), Err("unclear_precision"));
        assert_eq!(read_price("1,500", 2), Err("unclear_precision"));
        for bad in [
            "", "12.", ".5", "-1", "+1", "1 000", "1.2.3", "1e3", "€5", "١٢",
        ] {
            assert_eq!(read_price(bad, 2), Err("unreadable"), "{bad:?}");
        }
    }

    #[test]
    fn stored_prices_show_at_currency_decimals() {
        assert_eq!(shown(Decimal::from_str("3.00000000").unwrap(), 2), "3.00");
        assert_eq!(shown(Decimal::from_str("0.50500000").unwrap(), 2), "0.505");
        assert_eq!(shown(Decimal::from_str("1500").unwrap(), 0), "1500");
    }

    #[test]
    fn keys_normalise_whitespace_and_case() {
        assert_eq!(normalise("  RICE 5\t kg "), "rice 5 kg");
    }
}
