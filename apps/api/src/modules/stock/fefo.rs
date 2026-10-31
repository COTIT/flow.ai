//! First expired, first out: the order a hold or take uses a product's locked stock rows in.
//! No-lot stock goes first (it has no date, and selling it first clears untracked stock), then
//! sellable lots by their earliest date (expiry or best-before), undated lots last.

use chrono::NaiveDate;
use uuid::Uuid;

use crate::modules::stock::repo::StockRow;

/// A lot's status and dates, as the pick needs them.
pub struct LotInfo {
    pub id: Uuid,
    pub status: String,
    pub best_before: Option<NaiveDate>,
    pub expires_on: Option<NaiveDate>,
}

impl LotInfo {
    /// Released and not past its expiry date.
    pub fn sellable(&self, today: NaiveDate) -> bool {
        self.status == "released" && self.expires_on.is_none_or(|e| e >= today)
    }

    /// The date FEFO sorts on: the earlier of expiry and best-before.
    fn due(&self) -> Option<NaiveDate> {
        match (self.expires_on, self.best_before) {
            (Some(e), Some(b)) => Some(e.min(b)),
            (e, b) => e.or(b),
        }
    }
}

/// Use order of a row: lot rows after the no-lot row, undated after dated, then date, lot.
type UseKey = (bool, bool, Option<NaiveDate>, Option<Uuid>);

/// The rows a hold or take may use, in use order. Rows of lots that aren't sellable today
/// (or unknown) are left out.
pub fn order(rows: Vec<StockRow>, lots: &[LotInfo], today: NaiveDate) -> Vec<StockRow> {
    let mut keyed: Vec<(UseKey, StockRow)> = rows
        .into_iter()
        .filter_map(|row| match row.lot_id {
            None => Some(((false, false, None, None), row)),
            Some(id) => {
                let lot = lots.iter().find(|l| l.id == id && l.sellable(today))?;
                let due = lot.due();
                Some(((true, due.is_none(), due, Some(id)), row))
            }
        })
        .collect();
    keyed.sort_by_key(|(k, _)| *k);
    keyed.into_iter().map(|(_, row)| row).collect()
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::*;

    fn row(lot: Option<Uuid>) -> StockRow {
        StockRow {
            id: Uuid::now_v7(),
            product_id: Uuid::nil(),
            warehouse_id: Uuid::nil(),
            shelf_id: None,
            bin_id: None,
            lot_id: lot,
            stock_lvl: Decimal::ONE,
            reserved: Decimal::ZERO,
            minimal_stock_level: 0,
            maximum_stock_level: 0,
        }
    }

    fn d(s: &str) -> Option<NaiveDate> {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
    }

    fn lot(status: &str, best_before: Option<NaiveDate>, expires_on: Option<NaiveDate>) -> LotInfo {
        LotInfo {
            id: Uuid::now_v7(),
            status: status.to_owned(),
            best_before,
            expires_on,
        }
    }

    #[test]
    fn no_lot_first_then_earliest_date_then_undated() {
        let today = d("2026-11-01").unwrap();
        let late = lot("released", None, d("2027-06-01"));
        let soon = lot("released", d("2026-12-01"), d("2027-09-01"));
        let undated = lot("released", None, None);
        let mid = lot("released", d("2027-01-01"), None);
        let lots = [late, soon, undated, mid];
        let rows: Vec<StockRow> = lots
            .iter()
            .map(|l| row(Some(l.id)))
            .chain([row(None)])
            .collect();

        let order: Vec<Option<Uuid>> = order(rows, &lots, today).iter().map(|r| r.lot_id).collect();

        assert_eq!(
            order,
            [
                None,
                Some(lots[1].id),
                Some(lots[3].id),
                Some(lots[0].id),
                Some(lots[2].id)
            ]
        );
    }

    #[test]
    fn unsellable_and_unknown_lots_are_left_out() {
        let today = d("2026-11-01").unwrap();
        let lots = [
            lot("recalled", None, None),
            lot("quarantine", None, None),
            lot("released", None, d("2026-10-31")),
            lot("released", None, d("2026-11-01")),
        ];
        let mut rows: Vec<StockRow> = lots.iter().map(|l| row(Some(l.id))).collect();
        rows.push(row(Some(Uuid::now_v7())));

        let kept: Vec<Option<Uuid>> = order(rows, &lots, today).iter().map(|r| r.lot_id).collect();

        assert_eq!(kept, [Some(lots[3].id)], "expiring today is still sellable");
    }
}
