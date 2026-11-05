//! Cash request/response bodies: denomination counts, shifts, vaults and transfers. Money is
//! integer minor units.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    error::AppError,
    http::json::{Rules, Validate},
    modules::cash::repo::{ShiftFilter, ShiftRow, VaultRow},
    money,
};

const COUNT_MAX: usize = 50;
const DENOMINATION_MAX: i64 = 1_000_000_000;
const PIECES_MAX: i64 = 1_000_000;

/// One line of a cash count.
#[derive(Deserialize, ToSchema)]
#[allow(dead_code)]
pub struct CountLine {
    /// Minor units per note or coin (500 = €5.00), as the register's `accepted_cash` lists
    /// them. Send this or `value`.
    #[schema(minimum = 1, maximum = 1_000_000_000, example = 500)]
    denomination: Option<i64>,
    /// A typed note or coin as decimal text in the register's currency (`"0.50"`); the API
    /// converts it to minor units. Send this or `denomination`.
    #[schema(example = "0.50")]
    value: Option<String>,
    /// How many of them.
    #[schema(minimum = 0, maximum = 1_000_000, example = 3)]
    count: i64,
}

/// A line's note or coin: minor units, or decimal text the register's decimals turn into them.
pub enum Denomination {
    Minor(i64),
    Value(Decimal),
}

/// A count as sent: `(note or coin, pieces)` per line. `resolve` gives minor units.
pub struct Count {
    pub lines: Vec<(Denomination, i64)>,
}

/// A count in minor units: `(denomination, pieces)` pairs and their total.
pub struct Counted {
    pub lines: Vec<(i64, i64)>,
    pub total: i64,
}

impl Count {
    /// Minor units at the register's `dp`, each denomination once; 422 on the line's
    /// `denomination` / `value` (too many decimals, out of range, counted twice).
    pub fn resolve(self, dp: u32) -> Result<Counted, AppError> {
        let mut out = Counted {
            lines: Vec::new(),
            total: 0,
        };
        for (n, (d, pieces)) in self.lines.into_iter().enumerate() {
            let (field, d) = match d {
                Denomination::Minor(d) => (format!("count.{n}.denomination"), d),
                Denomination::Value(v) => {
                    let field = format!("count.{n}.value");
                    let d = money::minor(v, dp).map_err(|_| {
                        AppError::field(
                            &field,
                            format!("The value must have at most {dp} decimal places."),
                        )
                    })?;
                    if !(1..=DENOMINATION_MAX).contains(&d) {
                        return Err(AppError::field(&field, "The value must be greater than 0."));
                    }
                    (field, d)
                }
            };
            if out.lines.iter().any(|(seen, _)| *seen == d) {
                return Err(AppError::field(
                    &field,
                    "The denomination is counted twice.",
                ));
            }
            out.lines.push((d, pieces));
            // 50 × 1e9 × 1e6 stays far below i64::MAX.
            out.total += d * pieces;
        }
        Ok(out)
    }
}

/// 0..=50 lines; each a `denomination` (minor units, each once) or a `value` (decimal text),
/// not both.
fn count(r: &mut Rules, v: Option<Value>) -> Count {
    let mut out = Count { lines: Vec::new() };
    let Some(v) = r.required_value("count", v) else {
        return out;
    };
    let Some(list) = v.as_array().filter(|l| l.len() <= COUNT_MAX) else {
        r.add("count", "The count must be a list of at most 50 lines.");
        return out;
    };
    for (n, line) in list.iter().enumerate() {
        let c = r.integer(&format!("count.{n}.count"), &line["count"], 0, PIECES_MAX);
        let d = match (line.get("denomination"), line.get("value")) {
            (Some(d), None) => r
                .integer(&format!("count.{n}.denomination"), d, 1, DENOMINATION_MAX)
                .map(Denomination::Minor),
            (None, Some(v)) => r
                .decimal(&format!("count.{n}.value"), v, -money::MAX, money::MAX)
                .and_then(|v| {
                    if v > Decimal::ZERO {
                        Some(Denomination::Value(v))
                    } else {
                        r.add(
                            &format!("count.{n}.value"),
                            "The value must be greater than 0.",
                        );
                        None
                    }
                }),
            (Some(_), Some(_)) => {
                r.add(
                    &format!("count.{n}.value"),
                    "Send the denomination or the value, not both.",
                );
                None
            }
            (None, None) => {
                r.missing(&format!("count.{n}.denomination"));
                None
            }
        };
        let (Some(d), Some(c)) = (d, c) else { continue };
        if let Denomination::Minor(m) = d {
            let twice = out
                .lines
                .iter()
                .any(|(seen, _)| matches!(seen, Denomination::Minor(s) if *s == m));
            if twice {
                r.add(
                    &format!("count.{n}.denomination"),
                    "The denomination is counted twice.",
                );
                continue;
            }
        }
        out.lines.push((d, c));
    }
    out
}

/// POST /till/shift/open and /till/shift/close.
#[derive(Deserialize, ToSchema)]
#[schema(description = "A count of the drawer, one line per denomination.")]
pub struct CountRequest {
    #[schema(value_type = Vec<CountLine>)]
    count: Option<Value>,
}

impl Validate for CountRequest {
    type Valid = Count;

    fn validate(self, r: &mut Rules) -> Option<Count> {
        Some(count(r, self.count))
    }
}

/// POST /till/transfers.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Cash moved between this register's drawer and a vault, counted by denomination."
)]
pub struct TransferRequest {
    #[schema(value_type = Uuid)]
    vault_id: Option<String>,
    /// `to_vault` (a drop) or `from_vault` (more float).
    #[schema(value_type = String, example = "to_vault")]
    direction: Option<String>,
    #[schema(value_type = Vec<CountLine>)]
    count: Option<Value>,
}

/// Validated transfer.
pub struct Transfer {
    pub vault_id: Uuid,
    pub to_vault: bool,
    pub count: Count,
}

impl Validate for TransferRequest {
    type Valid = Transfer;

    fn validate(self, r: &mut Rules) -> Option<Transfer> {
        let vault_id = r
            .required("vault_id", self.vault_id)
            .map(|v| Uuid::parse_str(&v).unwrap_or(Uuid::nil()))
            .unwrap_or_default();
        let direction = r
            .required("direction", self.direction.clone())
            .and_then(|_| {
                r.one_of(
                    "direction",
                    self.direction,
                    &["to_vault", "from_vault"],
                    None,
                )
            });
        let sent = self.count.as_ref().is_some_and(Value::is_array);
        let count = count(r, self.count);
        // Every denomination is at least 1, so nothing moves only when no piece is counted.
        if sent && count.lines.iter().all(|(_, pieces)| *pieces == 0) {
            r.add("count", "The count must move some cash.");
        }
        Some(Transfer {
            vault_id,
            to_vault: direction.as_deref() == Some("to_vault"),
            count,
        })
    }
}

/// A count's figures in minor units, from `POST /till/shift/count-quote`.
#[derive(Serialize, ToSchema)]
pub struct CountQuote {
    /// The count's total.
    pub total: i64,
    /// Each line's denomination × pieces, in the order sent.
    pub lines: Vec<i64>,
    /// The open shift's expected cash; null without an open shift.
    pub expected: Option<i64>,
    /// total − expected (negative = short); null without an open shift.
    pub difference: Option<i64>,
}

/// A drawer shift. Open: `expected` is the drawer's cash so far; the close fields are null.
#[derive(Serialize, ToSchema)]
pub struct ShiftResource {
    id: Uuid,
    register_id: Uuid,
    opened_by: Uuid,
    opened_at: DateTime<Utc>,
    /// Counted float at open, minor units.
    opening: i64,
    /// Cash the drawer should hold: opening + sales ± transfers, minor units.
    expected: i64,
    #[schema(required = true)]
    closed_by: Option<Uuid>,
    #[schema(required = true)]
    closed_at: Option<DateTime<Utc>>,
    /// Counted at close, minor units.
    #[schema(required = true)]
    counted: Option<i64>,
    /// counted − expected: negative = short.
    #[schema(required = true)]
    difference: Option<i64>,
}

impl From<ShiftRow> for ShiftResource {
    fn from(s: ShiftRow) -> Self {
        Self {
            id: s.id,
            register_id: s.register_id,
            opened_by: s.opened_by,
            opened_at: s.opened_at,
            opening: s.opening,
            expected: s.expected,
            closed_by: s.closed_by,
            closed_at: s.closed_at,
            counted: s.counted,
            difference: s.difference,
        }
    }
}

/// A vault and the cash it holds.
#[derive(Serialize, ToSchema)]
pub struct VaultResource {
    id: Uuid,
    name: String,
    currency_id: Uuid,
    /// Minor units.
    balance: i64,
    created_at: DateTime<Utc>,
}

impl From<VaultRow> for VaultResource {
    fn from(v: VaultRow) -> Self {
        Self {
            id: v.id,
            name: v.name,
            currency_id: v.currency_id,
            balance: v.balance,
            created_at: v.created_at,
        }
    }
}

/// POST /till/vaults.
#[derive(Deserialize, ToSchema)]
#[schema(description = "New vault (safe). The currency defaults to the main one.")]
pub struct CreateVaultRequest {
    #[schema(value_type = String, max_length = 100, example = "Safe")]
    name: Option<String>,
    #[schema(value_type = Option<Uuid>)]
    currency_id: Option<String>,
}

/// Validated vault input.
pub struct NewVault {
    pub name: String,
    pub currency_id: Option<Uuid>,
}

impl Validate for CreateVaultRequest {
    type Valid = NewVault;

    fn validate(self, r: &mut Rules) -> Option<NewVault> {
        Some(NewVault {
            name: r.text("name", self.name, 100),
            currency_id: crate::http::json::optional(self.currency_id)
                .map(|c| Uuid::parse_str(&c).unwrap_or(Uuid::nil())),
        })
    }
}

/// A transfer as booked.
#[derive(Serialize, ToSchema)]
pub struct TransferResource {
    pub transfer_id: Uuid,
    pub vault_id: Uuid,
    #[schema(example = "to_vault")]
    pub direction: &'static str,
    /// Minor units moved.
    pub amount: i64,
}

/// GET /till/shifts query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ShiftQuery {
    /// `true`: only shifts still open; `false`: only closed ones.
    #[param(value_type = Option<bool>)]
    pub open: Option<String>,
    /// Only this register's shifts.
    #[param(value_type = Option<Uuid>)]
    pub register_id: Option<String>,
    /// Opened at or after this time (RFC 3339).
    #[param(value_type = Option<String>, format = DateTime, example = "2026-11-02T00:00:00+01:00")]
    pub from: Option<String>,
    /// Opened before this time (RFC 3339).
    #[param(value_type = Option<String>, format = DateTime)]
    pub to: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

impl ShiftQuery {
    /// The filter; 422 on a malformed `open`, `register_id`, `from` or `to`.
    pub fn filter(&self) -> Result<ShiftFilter, crate::error::AppError> {
        use crate::http::list;
        let mut r = Rules::default();
        let f = ShiftFilter {
            open: list::boolean(&mut r, "open", self.open.as_deref()),
            register_id: list::id(&mut r, "register_id", self.register_id.as_deref()),
            from: list::time(&mut r, "from", self.from.as_deref()),
            to: list::time(&mut r, "to", self.to.as_deref()),
        };
        r.into_result()?;
        Ok(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn run(v: Value) -> (Count, Rules) {
        let mut r = Rules::default();
        let c = count(&mut r, Some(v));
        (c, r)
    }

    #[test]
    fn total_is_denomination_times_pieces_in_minor_units() {
        let (c, r) = run(json!([
            { "denomination": 2000, "count": 2 },
            { "denomination": 5, "count": 3 },
            { "denomination": 100, "count": 0 }
        ]));
        assert!(r.into_result().is_ok());
        let c = c.resolve(2).ok().unwrap();
        assert_eq!(c.total, 4015);
        assert_eq!(c.lines.len(), 3);
    }

    #[test]
    fn the_largest_count_does_not_overflow() {
        let lines: Vec<Value> = (0..50)
            .map(|n| json!({ "denomination": DENOMINATION_MAX - n, "count": PIECES_MAX }))
            .collect();
        let (c, r) = run(Value::Array(lines));
        assert!(r.into_result().is_ok());
        assert!(c.resolve(2).ok().unwrap().total > 0);
    }
}
