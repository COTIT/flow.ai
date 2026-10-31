//! Cash request/response bodies: denomination counts, shifts, vaults and transfers. Money is
//! integer minor units.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::cash::repo::{ShiftRow, VaultRow},
};

const COUNT_MAX: usize = 50;
const DENOMINATION_MAX: i64 = 1_000_000_000;
const PIECES_MAX: i64 = 1_000_000;

/// One line of a cash count.
#[derive(Deserialize, ToSchema)]
#[allow(dead_code)]
pub struct CountLine {
    /// Minor units per note or coin (500 = €5.00).
    #[schema(minimum = 1, maximum = 1_000_000_000, example = 500)]
    denomination: i64,
    /// How many of them.
    #[schema(minimum = 0, maximum = 1_000_000, example = 3)]
    count: i64,
}

/// Validated count: `(denomination, pieces)` pairs and their total in minor units.
pub struct Count {
    pub lines: Vec<(i64, i64)>,
    pub total: i64,
}

/// 0..=50 lines, each denomination once.
fn count(r: &mut Rules, v: Option<Value>) -> Count {
    let mut out = Count {
        lines: Vec::new(),
        total: 0,
    };
    let Some(v) = r.required_value("count", v) else {
        return out;
    };
    let Some(list) = v.as_array().filter(|l| l.len() <= COUNT_MAX) else {
        r.add("count", "The count must be a list of at most 50 lines.");
        return out;
    };
    for (n, line) in list.iter().enumerate() {
        let d = r.integer(
            &format!("count.{n}.denomination"),
            &line["denomination"],
            1,
            DENOMINATION_MAX,
        );
        let c = r.integer(&format!("count.{n}.count"), &line["count"], 0, PIECES_MAX);
        let (Some(d), Some(c)) = (d, c) else { continue };
        if out.lines.iter().any(|(seen, _)| *seen == d) {
            r.add(
                &format!("count.{n}.denomination"),
                "The denomination is counted twice.",
            );
            continue;
        }
        out.lines.push((d, c));
        // 50 × 1e9 × 1e6 stays far below i64::MAX.
        out.total += d * c;
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
        if sent && count.total == 0 {
            r.add("count", "The count must move some cash.");
        }
        Some(Transfer {
            vault_id,
            to_vault: direction.as_deref() == Some("to_vault"),
            count,
        })
    }
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
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
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
        assert!(c.total > 0);
    }
}
