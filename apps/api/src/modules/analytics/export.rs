//! Invoice export CSV: one row per invoice line, money as exact decimal text from minor units,
//! spreadsheet-safe text cells.

use std::collections::BTreeMap;

use rust_decimal::Decimal;

use crate::{
    modules::analytics::{
        dto::{Bounds, When},
        repo::ExportLine,
    },
    numbering::Appendix,
};

const HEADER: &str = "invoice_number,invoice_date,due_date,type,status,customer,\
customer_vat_number,currency,line,quantity,unit_price_excl,vat_rate,discount_pct,line_excl,\
line_vat,line_total,invoice_excl,invoice_vat,invoice_total";

/// The built file and what it holds.
pub struct Csv {
    pub bytes: Vec<u8>,
    pub invoices: i64,
    pub lines: i64,
    /// (currency, decimals) -> (excl, vat, total) over the invoices, each invoice once.
    pub totals: BTreeMap<(String, i16), (i64, i64, i64)>,
}

/// `invoices-<first day>-to-<last day>.csv`; an instant names its UTC day, an exclusive day
/// bound names the day before.
pub fn file_name(b: Bounds) -> String {
    let first = match b.from {
        When::Day(d) => d,
        When::At(t) => t.date_naive(),
    };
    let last = match b.to {
        When::Day(d) => d.pred_opt().unwrap_or(d),
        When::At(t) => (t - chrono::Duration::nanoseconds(1)).date_naive(),
    };
    format!("invoices-{first}-to-{last}.csv")
}

/// UTF-8 CSV with a BOM (Excel reads it as UTF-8), CRLF line ends (RFC 4180).
pub fn csv(rows: &[ExportLine]) -> Csv {
    let mut out = String::from("\u{feff}");
    out.push_str(HEADER);
    out.push_str("\r\n");
    let (mut invoices, mut lines) = (0, 0);
    let mut totals: BTreeMap<(String, i16), (i64, i64, i64)> = BTreeMap::new();
    let mut last = None;
    for r in rows {
        if last != Some(r.invoice_id) {
            last = Some(r.invoice_id);
            invoices += 1;
            let t = totals.entry((r.currency.clone(), r.decimals)).or_default();
            t.0 += r.invoice_excl;
            t.1 += r.invoice_total - r.invoice_excl;
            t.2 += r.invoice_total;
        }
        if r.title.is_some() {
            lines += 1;
        }
        let dp = u32::try_from(r.decimals).unwrap_or(0);
        let money = |m: Option<i64>| {
            m.map(|m| Decimal::new(m, dp).to_string())
                .unwrap_or_default()
        };
        let number =
            Appendix::from_section(r.appendix.as_ref().unwrap_or(&serde_json::Value::Null))
                .format(r.number);
        let line_total = r.line_excl.zip(r.line_vat).map(|(e, v)| e + v);
        let cells = [
            text(&number),
            r.invoice_date.to_string(),
            r.due_date.map(|d| d.to_string()).unwrap_or_default(),
            (if r.is_credit {
                "credit_note"
            } else {
                "invoice"
            })
            .to_owned(),
            text(&r.status),
            text(&r.customer),
            text(r.vat_number.as_deref().unwrap_or_default()),
            text(&r.currency),
            text(r.title.as_deref().unwrap_or_default()),
            r.quantity.clone().unwrap_or_default(),
            money(r.unit_price),
            r.vat_rate.clone().unwrap_or_default(),
            r.discount.clone().unwrap_or_default(),
            money(r.line_excl),
            money(r.line_vat),
            money(line_total),
            money(Some(r.invoice_excl)),
            money(Some(r.invoice_total - r.invoice_excl)),
            money(Some(r.invoice_total)),
        ];
        out.push_str(&cells.join(","));
        out.push_str("\r\n");
    }
    Csv {
        bytes: out.into_bytes(),
        invoices,
        lines,
        totals,
    }
}

/// A text cell: a leading `= + - @` tab or CR gets a `'` so a spreadsheet shows it instead of
/// running it as a formula; quoted when it holds a comma, quote or line break.
fn text(v: &str) -> String {
    let v = if v.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{v}")
    } else {
        v.to_owned()
    };
    if v.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn text_cells_are_quoted_and_never_formulas() {
        assert_eq!(text("Plain"), "Plain");
        assert_eq!(text("A, B"), "\"A, B\"");
        assert_eq!(text("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(text("=1+2"), "'=1+2");
        assert_eq!(text("@SUM(A1)"), "'@SUM(A1)");
        assert_eq!(text("-cmd"), "'-cmd");
        assert_eq!(text("line\nbreak"), "\"line\nbreak\"");
    }

    #[test]
    fn file_name_names_the_last_included_day() {
        let day = |m, d| When::Day(NaiveDate::from_ymd_opt(2026, m, d).unwrap());
        let b = Bounds {
            from: day(10, 1),
            to: day(11, 1),
        };
        assert_eq!(file_name(b), "invoices-2026-10-01-to-2026-10-31.csv");
        let at = |s: &str| When::At(s.parse().unwrap());
        let b = Bounds {
            from: at("2026-10-01T00:00:00Z"),
            to: at("2026-11-01T00:00:00Z"),
        };
        assert_eq!(file_name(b), "invoices-2026-10-01-to-2026-10-31.csv");
    }
}
