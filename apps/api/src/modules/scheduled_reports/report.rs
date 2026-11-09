//! Report mail text: gathers one report's data through the existing reads (sales summary,
//! attention) and renders plain text. Money stays integer minor units until display.

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::{
    error::AppError,
    modules::{
        attention::{dto::Attention, repo as attention_repo, service as attention},
        orders::repo as orders,
        scheduled_reports::repo::{self, Due},
        settings,
        storefront::dto::LOW_STOCK_DEFAULT,
    },
};

/// Most products a stock mail lists.
const STOCK_ITEMS: i64 = 20;

/// A rendered report: title for the subject and the text before the footer.
pub struct Rendered {
    pub title: &'static str,
    pub text: String,
}

/// Human name of a report kind.
pub fn title(report: &str) -> &'static str {
    match report {
        "sales_summary" => "Sales summary",
        "attention" => "To-do",
        "overdue_invoices" => "Overdue invoices",
        _ => "Low stock",
    }
}

/// `minor` as a decimal amount with its currency code ("1234.50 EUR"), integer math only.
pub fn money(minor: i64, decimals: i16, code: &str) -> String {
    let d = u32::try_from(decimals.clamp(0, 8)).unwrap_or(0);
    if d == 0 {
        return format!("{minor} {code}");
    }
    let unit = 10i128.pow(d);
    let m = i128::from(minor);
    let sign = if m < 0 { "-" } else { "" };
    let (whole, frac) = (m.abs() / unit, m.abs() % unit);
    format!("{sign}{whole}.{frac:0w$} {code}", w = d as usize)
}

/// "every Monday at 07:00 (Africa/Nairobi)".
pub fn schedule_words(due: &Due, zone: &str) -> String {
    const DAYS: [&str; 7] = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];
    let when = match (due.frequency.as_str(), due.weekday, due.day_of_month) {
        ("weekly", Some(w), _) => format!(
            "every {}",
            DAYS.get(usize::try_from(w - 1).unwrap_or(0))
                .unwrap_or(&"week")
        ),
        ("monthly", _, Some(d)) => format!("on day {d} of every month"),
        _ => "every day".to_owned(),
    };
    format!("{when} around {:02}:00 ({zone})", due.hour)
}

/// The report `due` asks for, read with `may`'s permissions; None when an alert report has
/// nothing to say.
pub async fn build(
    db: &PgPool,
    due: &Due,
    may: impl Fn(&str) -> bool,
    now: DateTime<Utc>,
) -> Result<Option<Rendered>, AppError> {
    let ws = due.workspace_id;
    let text = match due.report.as_str() {
        "sales_summary" => Some(sales(db, due, now).await?),
        "attention" => Some(todo(&attention::sections(db, ws, may).await?)),
        "overdue_invoices" => overdue(db, ws).await?,
        _ => stock(db, ws).await?,
    };
    Ok(text.map(|text| Rendered {
        title: title(&due.report),
        text,
    }))
}

async fn sales(db: &PgPool, due: &Due, now: DateTime<Utc>) -> Result<String, AppError> {
    let p = repo::period(db, due.workspace_id, &due.frequency, now).await?;
    let rows = orders::sales_summary(db, due.workspace_id, p.from, p.to, None).await?;
    let mut out = if p.first_day == p.last_day {
        format!("Sales on {}:\n", p.first_day)
    } else {
        format!("Sales from {} to {}:\n", p.first_day, p.last_day)
    };
    if rows.is_empty() {
        out.push_str("No sales in this period.\n");
    }
    for c in rows {
        out.push_str(&format!(
            "- {}: {} orders, {} incl VAT",
            c.currency,
            c.orders,
            money(c.gross_total, c.decimals, &c.currency)
        ));
        if c.refunded_total != 0 {
            out.push_str(&format!(
                ", {} refunded",
                money(c.refunded_total, c.decimals, &c.currency)
            ));
        }
        out.push('\n');
    }
    Ok(out)
}

fn todo(a: &Attention) -> String {
    let mut out = String::from("What needs attention today:\n");
    if let Some(o) = &a.orders {
        out.push_str(&format!(
            "- Orders: {} draft, {} to fulfil, {} awaiting payment\n",
            o.draft, o.open, o.awaiting_payment
        ));
    }
    if let Some(i) = &a.invoices {
        out.push_str(&format!(
            "- Invoices: {} to send, {} unpaid, {} overdue\n",
            i.draft, i.unpaid, i.overdue
        ));
    }
    if let Some(s) = &a.stock {
        out.push_str(&format!(
            "- Stock: {} sold out, {} rows below zero\n",
            s.out_of_stock, s.negative
        ));
    }
    if let Some(t) = &a.till_shifts {
        out.push_str(&format!("- Till: {} shifts open\n", t.open));
    }
    out.push_str(&format!(
        "- Lots: {} in quarantine, {} orders held\n",
        a.lots.quarantined, a.lots.held_orders
    ));
    if let Some(w) = &a.webhooks {
        out.push_str(&format!("- Webhooks: {} failed deliveries\n", w.failed));
    }
    out
}

async fn overdue(db: &PgPool, ws: uuid::Uuid) -> Result<Option<String>, AppError> {
    let i = attention::invoices(db, ws).await?;
    if i.overdue == 0 {
        return Ok(None);
    }
    let mut out = format!("{} invoices are overdue. Oldest:\n", i.overdue);
    for v in i.items.iter().filter(|v| v.overdue) {
        out.push_str(&format!(
            "- {} {}: {} open, due {}\n",
            v.number.as_deref().unwrap_or("(draft)"),
            v.customer_name.as_deref().unwrap_or(""),
            money(v.open, v.decimals, &v.currency),
            v.due_date
                .map(|d| d.date_naive().to_string())
                .unwrap_or_default(),
        ));
    }
    Ok(Some(out))
}

async fn stock(db: &PgPool, ws: uuid::Uuid) -> Result<Option<String>, AppError> {
    let threshold = settings::repo::section(db, ws, "product")
        .await?
        .and_then(|s| s.get("low_stock_threshold")?.as_i64())
        .unwrap_or(LOW_STOCK_DEFAULT);
    let (items, total) = attention_repo::sold_out(db, ws, threshold, STOCK_ITEMS).await?;
    if total == 0 {
        return Ok(None);
    }
    let mut out = format!("{total} products are sold out or at {threshold} or fewer left:\n");
    for i in items {
        out.push_str(&format!("- {}: {} left\n", i.product_name, i.available));
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_is_exact() {
        assert_eq!(money(123_450, 2, "EUR"), "1234.50 EUR");
        assert_eq!(money(-5, 2, "EUR"), "-0.05 EUR");
        assert_eq!(money(1500, 0, "TZS"), "1500 TZS");
        assert_eq!(money(i64::MIN, 2, "X"), "-92233720368547758.08 X");
    }
}
