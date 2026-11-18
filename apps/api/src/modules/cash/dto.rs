//! Cash request/response bodies: denomination counts, shifts, vaults and transfers. Money is
//! integer minor units.

use chrono::{DateTime, NaiveDate, Utc};
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
    /// Currency of every amount here (the register's): upper-case ISO 4217 code.
    #[schema(example = "EUR")]
    currency: String,
    /// Minor-unit exponent: amounts / 10^decimals = major units.
    decimals: i16,
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
            currency: s.currency,
            decimals: s.decimals,
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

/// A cash place in a request: `{kind: drawer|vault, id}`. Drawer id = register id.
#[derive(Deserialize, ToSchema)]
pub struct PlaceRequest {
    #[schema(example = "drawer")]
    kind: Option<String>,
    #[schema(value_type = Uuid)]
    id: Option<String>,
}

/// A validated cash place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// A register's drawer.
    Drawer(Uuid),
    Vault(Uuid),
}

impl Place {
    /// `drawer` or `vault`.
    pub fn kind(self) -> &'static str {
        match self {
            Self::Drawer(_) => "drawer",
            Self::Vault(_) => "vault",
        }
    }

    /// The register or vault id.
    pub fn id(self) -> Uuid {
        match self {
            Self::Drawer(id) | Self::Vault(id) => id,
        }
    }
}

pub(crate) fn place(r: &mut Rules, field: &str, value: Option<PlaceRequest>) -> Option<Place> {
    let Some(p) = value else {
        r.missing(field);
        return None;
    };
    let kind = r.required(&format!("{field}.kind"), p.kind);
    let id = r
        .required(&format!("{field}.id"), p.id)
        .and_then(|v| Uuid::parse_str(&v).ok());
    r.check(
        &format!("{field}.id"),
        id.is_some(),
        "The place id must be a valid UUID.",
    );
    match (kind.as_deref(), id) {
        (Some("drawer"), Some(id)) => Some(Place::Drawer(id)),
        (Some("vault"), Some(id)) => Some(Place::Vault(id)),
        (Some(_), _) => {
            r.add(
                &format!("{field}.kind"),
                "The place kind must be drawer or vault.",
            );
            None
        }
        _ => None,
    }
}

/// A counted amount: Office decimal text in major units (SQL turns it into minor units at the
/// place's decimals), or Till integer minor units.
#[derive(Clone)]
pub enum CountedAmount {
    Major(Decimal),
    Minor(i64),
}

fn major(r: &mut Rules, value: Option<Value>) -> Option<CountedAmount> {
    let v = r.required_value("counted_amount", value)?;
    // Range and format only: the conversion to minor units happens in SQL.
    r.decimal("counted_amount", &v, Decimal::ZERO, money::MAX)
        .map(CountedAmount::Major)
}

fn minor(r: &mut Rules, value: Option<Value>) -> Option<CountedAmount> {
    let v = r.required_value("counted_amount", value)?;
    r.integer("counted_amount", &v, 0, 1_000_000_000_000_000)
        .map(CountedAmount::Minor)
}

fn date(r: &mut Rules, value: Option<String>) -> Option<NaiveDate> {
    let v = r.required("accounting_date", value)?;
    let d = NaiveDate::parse_from_str(&v, "%Y-%m-%d").ok();
    r.check(
        "accounting_date",
        d.is_some(),
        "The accounting date must be a date (YYYY-MM-DD).",
    );
    d
}

/// What a cash adjustment review checks: place, count and (Office) date; None = book today.
pub struct ReviewInput {
    pub place: Place,
    pub counted: CountedAmount,
    pub date: Option<NaiveDate>,
}

/// A validated adjustment command payload.
pub struct AdjustInput {
    pub review: ReviewInput,
    pub evidence: String,
    pub reason: String,
    /// Hex sha256 from the review.
    pub fingerprint: String,
}

/// POST /accounting/cash-adjustments/review body.
#[derive(Deserialize, ToSchema)]
pub struct OfficeReviewRequest {
    cash_place: Option<PlaceRequest>,
    /// Decimal text in the place's currency, major units (`"50.00"`).
    #[schema(value_type = String, example = "50.00")]
    counted_amount: Option<Value>,
    #[schema(value_type = String, format = Date)]
    accounting_date: Option<String>,
}

impl Validate for OfficeReviewRequest {
    type Valid = ReviewInput;

    fn validate(self, r: &mut Rules) -> Option<ReviewInput> {
        let place = place(r, "cash_place", self.cash_place);
        let counted = major(r, self.counted_amount);
        let date = date(r, self.accounting_date);
        Some(ReviewInput {
            place: place?,
            counted: counted?,
            date: Some(date?),
        })
    }
}

/// POST /till/cash-adjustments/review body.
#[derive(Deserialize, ToSchema)]
pub struct TillReviewRequest {
    target_cash_place: Option<PlaceRequest>,
    /// Integer minor units (5000 = €50.00).
    #[schema(value_type = i64, minimum = 0, example = 5000)]
    counted_amount: Option<Value>,
}

impl Validate for TillReviewRequest {
    type Valid = ReviewInput;

    fn validate(self, r: &mut Rules) -> Option<ReviewInput> {
        let place = place(r, "target_cash_place", self.target_cash_place);
        let counted = minor(r, self.counted_amount);
        Some(ReviewInput {
            place: place?,
            counted: counted?,
            date: None,
        })
    }
}

/// Evidence, reason and review fingerprint of an adjustment post.
#[derive(Deserialize, ToSchema)]
pub struct AdjustFields {
    #[schema(max_length = 255)]
    count_evidence_ref: Option<String>,
    #[schema(max_length = 1000)]
    reason: Option<String>,
    /// `review_fingerprint` from the review.
    review_fingerprint: Option<String>,
}

fn fields(r: &mut Rules, f: AdjustFields, review: Option<ReviewInput>) -> Option<AdjustInput> {
    let evidence = r.text("count_evidence_ref", f.count_evidence_ref, 255);
    let reason = r.text("reason", f.reason, 1000);
    let fingerprint = r.required("review_fingerprint", f.review_fingerprint);
    Some(AdjustInput {
        review: review?,
        evidence,
        reason,
        fingerprint: fingerprint?,
    })
}

/// POST /accounting/cash-adjustments payload.
#[derive(Deserialize, ToSchema)]
pub struct OfficeAdjustPayload {
    #[serde(flatten)]
    review: OfficeReviewRequest,
    #[serde(flatten)]
    fields: AdjustFields,
}

impl Validate for OfficeAdjustPayload {
    type Valid = AdjustInput;

    fn validate(self, r: &mut Rules) -> Option<AdjustInput> {
        let review = self.review.validate(r);
        fields(r, self.fields, review)
    }
}

/// POST /till/cash-adjustments payload.
#[derive(Deserialize, ToSchema)]
pub struct TillAdjustPayload {
    #[serde(flatten)]
    review: TillReviewRequest,
    #[serde(flatten)]
    fields: AdjustFields,
}

impl Validate for TillAdjustPayload {
    type Valid = AdjustInput;

    fn validate(self, r: &mut Rules) -> Option<AdjustInput> {
        let review = self.review.validate(r);
        fields(r, self.fields, review)
    }
}

/// A place as shown in responses.
#[derive(Serialize, ToSchema)]
pub struct PlaceRef {
    #[schema(example = "drawer")]
    pub kind: &'static str,
    pub id: Uuid,
}

/// A currency beside minor-unit amounts.
#[derive(Serialize, ToSchema)]
pub struct PlaceCurrency {
    pub id: Uuid,
    #[schema(example = "EUR")]
    pub code: String,
    pub decimals: i16,
}

/// A drawer or vault with its active book cash.
#[derive(Serialize, ToSchema)]
pub struct CashPlaceResource {
    #[schema(example = "vault")]
    pub kind: String,
    pub id: Uuid,
    pub name: String,
    pub currency: PlaceCurrency,
    /// Minor units: reviewed adjustments + posted movements; counts excluded.
    pub balance: i64,
    /// Drawers: the open shift, if any.
    pub open_shift_id: Option<Uuid>,
}

/// Journal a posted adjustment writes in an active book.
#[derive(Serialize, ToSchema)]
pub struct JournalPlan {
    #[schema(example = "cash")]
    pub debit_role: &'static str,
    #[schema(example = "cash_over_short")]
    pub credit_role: &'static str,
    /// Functional minor units.
    pub amount: i64,
}

/// Server facts for a cash adjustment; post with `review_fingerprint`.
#[derive(Serialize, ToSchema)]
pub struct AdjustReviewResource {
    pub cash_place: PlaceRef,
    pub currency: PlaceCurrency,
    /// Active book cash before, minor units.
    pub expected: i64,
    pub counted: i64,
    /// counted − expected; the place movement.
    pub difference: i64,
    #[schema(value_type = String, format = Date)]
    pub accounting_date: NaiveDate,
    /// `setup` (place movement only) or `active` (plus the journal).
    pub book: String,
    pub journal: Option<JournalPlan>,
    pub review_fingerprint: String,
}

/// Who approved an adjustment.
#[derive(Serialize, ToSchema)]
pub struct AdjustActor {
    #[schema(example = "member")]
    pub kind: &'static str,
    pub user_id: Uuid,
}

/// A posted cash adjustment.
#[derive(Serialize, ToSchema)]
pub struct CashAdjustmentResource {
    pub id: Uuid,
    pub cash_place: PlaceRef,
    pub currency: PlaceCurrency,
    pub expected: i64,
    pub counted: i64,
    pub difference: i64,
    #[schema(value_type = String, format = Date)]
    pub accounting_date: NaiveDate,
    pub count_evidence_ref: String,
    pub reason: String,
    pub actor: AdjustActor,
    /// Cashier context of a privileged Till approval.
    pub till_user_id: Option<Uuid>,
    /// Active books only.
    pub journal_entry_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}
