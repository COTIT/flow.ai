//! Accounting request envelopes (C/A, masterplan §5), payloads and response resources.
//! Amounts travel as exact decimal strings in major units; the service turns them into
//! integer minor units at the currency's decimals.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, optional},
    idempotency::ClientRef,
};

/// Every role the book maps to an account (§8 "Required mapped roles").
pub const ROLES: [&str; 26] = [
    "receivables",
    "unbilled_receivables",
    "payables",
    "inventory",
    "grni",
    "revenue",
    "sales_returns",
    "cogs",
    "tax_payable",
    "recoverable_tax",
    "customer_deposits",
    "supplier_advances",
    "bank",
    "cash",
    "provider_clearing",
    "inventory_adjustment",
    "pending_purchase_cost_variance",
    "purchase_return_variance",
    "cash_over_short",
    "realized_fx",
    "unrealized_fx",
    "rounding",
    "opening_equity",
    "retained_earnings",
    "financial_discrepancy",
    "financial_loss",
];

/// Roles a manual journal may not touch: they move only through their own source workflows
/// (sales, purchasing, cash places, banking, provider settlement, opening, close checks).
pub const CONTROL_ROLES: [&str; 13] = [
    "receivables",
    "unbilled_receivables",
    "payables",
    "inventory",
    "grni",
    "customer_deposits",
    "supplier_advances",
    "cash",
    "bank",
    "provider_clearing",
    "financial_discrepancy",
    "pending_purchase_cost_variance",
    "opening_equity",
];

const ACCOUNT_TYPES: [&str; 5] = ["asset", "liability", "equity", "income", "expense"];
/// §5 LIMITS.
pub const MAX_LINES: usize = 100;
const REF_MAX: usize = 255;
const REASON_MAX: usize = 1000;

fn uuid(r: &mut Rules, field: &str, value: Option<String>) -> Option<Uuid> {
    let v = r.required(field, value)?;
    let id = Uuid::parse_str(&v).ok();
    r.check(
        field,
        id.is_some(),
        format!("The {} must be a valid UUID.", field.replace('_', " ")),
    );
    id
}

fn opt_uuid(r: &mut Rules, field: &str, value: Option<String>) -> Option<Option<Uuid>> {
    match optional(value) {
        None => Some(None),
        Some(v) => uuid(r, field, Some(v)).map(Some),
    }
}

fn date(r: &mut Rules, field: &str, value: Option<String>) -> Option<NaiveDate> {
    let v = r.required(field, value)?;
    let d = NaiveDate::parse_from_str(&v, "%Y-%m-%d").ok();
    r.check(
        field,
        d.is_some(),
        format!(
            "The {} must be a date (YYYY-MM-DD).",
            field.replace('_', " ")
        ),
    );
    d
}

/// Validate `payload` on its own and report its errors under `payload.`.
fn nested<P: Validate>(r: &mut Rules, payload: Option<P>) -> Option<P::Valid> {
    let Some(p) = payload else {
        r.missing("payload");
        return None;
    };
    let mut inner = Rules::default();
    let valid = p.validate(&mut inner);
    if inner.merge_into_prefixed(r, "payload.") {
        return None;
    }
    valid
}

/// C envelope: create with a caller-chosen `client_ref` (replay identity).
#[derive(Deserialize, ToSchema)]
pub struct Create<P> {
    #[schema(value_type = Uuid)]
    pub client_ref: Option<String>,
    pub payload: Option<P>,
}

/// A validated C envelope.
pub struct Created<V> {
    pub client_ref: Uuid,
    pub payload: V,
}

impl<P: Validate> Validate for Create<P> {
    type Valid = Created<P::Valid>;

    fn validate(self, r: &mut Rules) -> Option<Self::Valid> {
        let client_ref = uuid(r, "client_ref", self.client_ref);
        let payload = nested(r, self.payload);
        Some(Created {
            client_ref: client_ref?,
            payload: payload?,
        })
    }
}

impl<V> ClientRef for Created<V> {
    fn client_ref(&self) -> Uuid {
        self.client_ref
    }
}

/// A envelope: action on an existing resource at `expected_version`.
#[derive(Deserialize, ToSchema)]
pub struct Action<P> {
    #[schema(value_type = Uuid)]
    pub client_ref: Option<String>,
    #[schema(value_type = i64, minimum = 1)]
    pub expected_version: Option<Value>,
    pub payload: Option<P>,
}

/// A validated A envelope.
pub struct Acted<V> {
    pub client_ref: Uuid,
    pub expected_version: i64,
    pub payload: V,
}

impl<P: Validate> Validate for Action<P> {
    type Valid = Acted<P::Valid>;

    fn validate(self, r: &mut Rules) -> Option<Self::Valid> {
        let client_ref = uuid(r, "client_ref", self.client_ref);
        let version = match self.expected_version {
            None | Some(Value::Null) => {
                r.missing("expected_version");
                None
            }
            Some(v) => r.integer("expected_version", &v, 1, i64::MAX),
        };
        let payload = nested(r, self.payload);
        Some(Acted {
            client_ref: client_ref?,
            expected_version: version?,
            payload: payload?,
        })
    }
}

impl<V> ClientRef for Acted<V> {
    fn client_ref(&self) -> Uuid {
        self.client_ref
    }
}

/// PUT /accounting/setup payload; absent fields stay as they are.
#[derive(Deserialize, ToSchema)]
pub struct SetupPayload {
    #[schema(value_type = Option<Uuid>)]
    pub functional_currency_id: Option<String>,
    /// IANA zone name, e.g. `Africa/Dar_es_Salaam`.
    pub timezone: Option<String>,
    /// role → account id. Replaces only the roles sent.
    #[schema(value_type = Option<BTreeMap<String, Uuid>>)]
    pub account_mappings: Option<BTreeMap<String, String>>,
}

/// Validated setup change.
pub struct SetupInput {
    pub currency: Option<Uuid>,
    pub timezone: Option<String>,
    pub mappings: Vec<(String, Uuid)>,
}

impl Validate for SetupPayload {
    type Valid = SetupInput;

    fn validate(self, r: &mut Rules) -> Option<SetupInput> {
        let currency = opt_uuid(r, "functional_currency_id", self.functional_currency_id);
        let timezone = r.opt_text("timezone", self.timezone, 64);
        let mut mappings = Vec::new();
        for (role, account) in self.account_mappings.unwrap_or_default() {
            let field = format!("account_mappings.{role}");
            if !ROLES.contains(&role.as_str()) {
                r.add(&field, "The role is not an accounting role.");
                continue;
            }
            if let Some(id) = uuid(r, &field, Some(account)) {
                mappings.push((role, id));
            }
        }
        Some(SetupInput {
            currency: currency?,
            timezone,
            mappings,
        })
    }
}

/// POST /accounting/accounts payload.
#[derive(Deserialize, ToSchema)]
pub struct AccountPayload {
    /// 1-32 letters, digits, `.` or `-`, starting with a letter or digit; unique per book.
    pub code: Option<String>,
    pub name: Option<String>,
    /// asset, liability, equity, income or expense.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// Parent of the same type.
    #[schema(value_type = Option<Uuid>)]
    pub parent_id: Option<String>,
}

/// Validated new account.
pub struct NewAccount {
    pub code: String,
    pub name: String,
    pub kind: String,
    pub parent: Option<Uuid>,
}

impl Validate for AccountPayload {
    type Valid = NewAccount;

    fn validate(self, r: &mut Rules) -> Option<NewAccount> {
        let code = r.required("code", self.code);
        if let Some(c) = &code {
            let ok = c.len() <= 32
                && c.starts_with(|ch: char| ch.is_ascii_alphanumeric())
                && c.chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-');
            r.check(
                "code",
                ok,
                "The code must be 1 to 32 letters, digits, dots or dashes.",
            );
        }
        let name = r.required("name", self.name);
        if let Some(n) = &name {
            r.max_chars("name", n, REF_MAX);
        }
        if self.kind.is_none() {
            r.missing("type");
        }
        let kind = r.one_of("type", self.kind, &ACCOUNT_TYPES, None);
        let parent = opt_uuid(r, "parent_id", self.parent_id);
        Some(NewAccount {
            code: code?,
            name: name?,
            kind: kind?,
            parent: parent?,
        })
    }
}

/// PATCH /accounting/accounts/{id} payload.
#[derive(Deserialize, ToSchema)]
pub struct AccountPatch {
    pub name: Option<String>,
    pub active: Option<bool>,
}

/// Validated account change.
pub struct AccountChange {
    pub name: Option<String>,
    pub active: Option<bool>,
}

impl Validate for AccountPatch {
    type Valid = AccountChange;

    fn validate(self, r: &mut Rules) -> Option<AccountChange> {
        let name = match self.name {
            None => None,
            Some(n) => {
                let n = r.required("name", Some(n));
                if let Some(n) = &n {
                    r.max_chars("name", n, REF_MAX);
                }
                n
            }
        };
        Some(AccountChange {
            name,
            active: self.active,
        })
    }
}

/// POST /accounting/fiscal-years payload: `[start_date, end_date)`.
#[derive(Deserialize, ToSchema)]
pub struct YearPayload {
    #[schema(value_type = NaiveDate)]
    pub start_date: Option<String>,
    /// Exclusive.
    #[schema(value_type = NaiveDate)]
    pub end_date: Option<String>,
}

/// Validated `[start, end)` range.
pub struct Range {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

fn range(r: &mut Rules, start: Option<String>, end: Option<String>) -> Option<Range> {
    let (start, end) = (date(r, "start_date", start)?, date(r, "end_date", end)?);
    r.check(
        "end_date",
        end > start,
        "The end date must be after the start date.",
    );
    Some(Range { start, end })
}

impl Validate for YearPayload {
    type Valid = Range;

    fn validate(self, r: &mut Rules) -> Option<Range> {
        range(r, self.start_date, self.end_date)
    }
}

/// POST /accounting/periods payload: `[start_date, end_date)` inside its fiscal year.
#[derive(Deserialize, ToSchema)]
pub struct PeriodPayload {
    #[schema(value_type = Uuid)]
    pub fiscal_year_id: Option<String>,
    #[schema(value_type = NaiveDate)]
    pub start_date: Option<String>,
    /// Exclusive.
    #[schema(value_type = NaiveDate)]
    pub end_date: Option<String>,
}

/// Validated new period.
pub struct NewPeriod {
    pub year: Uuid,
    pub range: Range,
}

impl Validate for PeriodPayload {
    type Valid = NewPeriod;

    fn validate(self, r: &mut Rules) -> Option<NewPeriod> {
        let year = uuid(r, "fiscal_year_id", self.fiscal_year_id);
        let range = range(r, self.start_date, self.end_date);
        Some(NewPeriod {
            year: year?,
            range: range?,
        })
    }
}

/// One manual journal line: exactly one of `debit` / `credit` above zero.
#[derive(Deserialize, ToSchema)]
pub struct LinePayload {
    #[schema(value_type = Uuid)]
    pub account_id: Option<String>,
    /// Exact decimal string in major units, `"0"` for the other side.
    #[schema(value_type = String, example = "12.50")]
    pub debit: Option<Value>,
    #[schema(value_type = String, example = "0")]
    pub credit: Option<Value>,
    #[schema(value_type = Option<Uuid>)]
    pub relation_id: Option<String>,
    /// Not supported until bank accounts exist; sending one is refused.
    #[schema(value_type = Option<Uuid>)]
    pub bank_account_id: Option<String>,
    pub source_ref: Option<String>,
}

/// Validated line; amounts still in major units.
pub struct LineInput {
    pub account: Uuid,
    pub debit: Decimal,
    pub credit: Decimal,
    pub relation: Option<Uuid>,
    pub source_ref: Option<String>,
}

/// POST/PATCH journal payload (PATCH replaces every draft field).
#[derive(Deserialize, ToSchema)]
pub struct JournalPayload {
    #[schema(value_type = NaiveDate)]
    pub accounting_date: Option<String>,
    #[schema(value_type = Uuid)]
    pub currency_id: Option<String>,
    pub reason: Option<String>,
    pub evidence_ref: Option<String>,
    pub lines: Option<Vec<LinePayload>>,
}

/// Validated draft journal.
pub struct JournalInput {
    pub date: NaiveDate,
    pub currency: Uuid,
    pub reason: String,
    pub evidence_ref: Option<String>,
    pub lines: Vec<LineInput>,
}

impl Validate for JournalPayload {
    type Valid = JournalInput;

    fn validate(self, r: &mut Rules) -> Option<JournalInput> {
        let date = date(r, "accounting_date", self.accounting_date);
        let currency = uuid(r, "currency_id", self.currency_id);
        let reason = r.required("reason", self.reason);
        if let Some(t) = &reason {
            r.max_chars("reason", t, REASON_MAX);
        }
        let evidence_ref = r.opt_text("evidence_ref", self.evidence_ref, REF_MAX);
        let raw = self.lines.unwrap_or_default();
        r.check(
            "lines",
            (2..=MAX_LINES).contains(&raw.len()),
            format!("A journal needs 2 to {MAX_LINES} lines."),
        );
        let mut lines = Vec::with_capacity(raw.len());
        for (i, l) in raw.into_iter().take(MAX_LINES).enumerate() {
            let mut lr = Rules::default();
            let account = uuid(&mut lr, "account_id", l.account_id);
            let amount = |lr: &mut Rules, f: &str, v: Option<Value>| {
                lr.decimal(
                    f,
                    &v.unwrap_or(Value::String("0".into())),
                    Decimal::ZERO,
                    crate::money::MAX,
                )
            };
            let debit = amount(&mut lr, "debit", l.debit);
            let credit = amount(&mut lr, "credit", l.credit);
            if let (Some(d), Some(c)) = (debit, credit) {
                lr.check(
                    "debit",
                    d.is_zero() != c.is_zero(),
                    "Exactly one of debit or credit must be above zero.",
                );
            }
            let relation = opt_uuid(&mut lr, "relation_id", l.relation_id);
            if optional(l.bank_account_id).is_some() {
                lr.add("bank_account_id", "Bank lines are not supported yet.");
            }
            let source_ref = lr.opt_text("source_ref", l.source_ref, REF_MAX);
            let ok = !lr.merge_into_prefixed(r, &format!("lines.{i}."));
            if let (true, Some(account), Some(debit), Some(credit), Some(relation)) =
                (ok, account, debit, credit, relation)
            {
                lines.push(LineInput {
                    account,
                    debit,
                    credit,
                    relation,
                    source_ref,
                });
            }
        }
        Some(JournalInput {
            date: date?,
            currency: currency?,
            reason: reason?,
            evidence_ref,
            lines,
        })
    }
}

/// POST /journals/{id}/post payload.
#[derive(Deserialize, ToSchema)]
pub struct PostPayload {
    /// From `POST /journals/{id}/post/review`.
    pub review_fingerprint: Option<String>,
}

impl Validate for PostPayload {
    type Valid = String;

    fn validate(self, r: &mut Rules) -> Option<String> {
        r.required("review_fingerprint", self.review_fingerprint)
    }
}

/// POST /journals/{id}/reverse/review body (and the reverse payload without fingerprint).
#[derive(Deserialize, ToSchema)]
pub struct ReverseReviewBody {
    #[schema(value_type = NaiveDate)]
    pub accounting_date: Option<String>,
    pub reason: Option<String>,
}

/// Validated reversal request.
pub struct ReverseInput {
    pub date: NaiveDate,
    pub reason: String,
}

fn reverse_input(r: &mut Rules, d: Option<String>, reason: Option<String>) -> Option<ReverseInput> {
    let date = date(r, "accounting_date", d);
    let reason = r.required("reason", reason);
    if let Some(t) = &reason {
        r.max_chars("reason", t, REASON_MAX);
    }
    Some(ReverseInput {
        date: date?,
        reason: reason?,
    })
}

impl Validate for ReverseReviewBody {
    type Valid = ReverseInput;

    fn validate(self, r: &mut Rules) -> Option<ReverseInput> {
        reverse_input(r, self.accounting_date, self.reason)
    }
}

/// POST /journals/{id}/reverse payload.
#[derive(Deserialize, ToSchema)]
pub struct ReversePayload {
    #[schema(value_type = NaiveDate)]
    pub accounting_date: Option<String>,
    pub reason: Option<String>,
    pub review_fingerprint: Option<String>,
}

impl Validate for ReversePayload {
    type Valid = (ReverseInput, String);

    fn validate(self, r: &mut Rules) -> Option<(ReverseInput, String)> {
        let input = reverse_input(r, self.accounting_date, self.reason);
        let fp = r.required("review_fingerprint", self.review_fingerprint);
        Some((input?, fp?))
    }
}

/// `?page=&page_size=` on list routes.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    pub page: Option<String>,
    pub page_size: Option<String>,
}

/// A currency as shown beside amounts.
#[derive(Serialize, ToSchema, Clone)]
pub struct CurrencyRef {
    pub id: Uuid,
    /// ISO 4217, upper case.
    pub code: String,
    pub decimals: i16,
}

/// The workspace book's setup.
#[derive(Serialize, ToSchema)]
pub struct SetupResource {
    /// `setup` until activation (opening conversion), then `active`.
    pub state: String,
    pub version: i64,
    pub functional_currency: Option<CurrencyRef>,
    pub timezone: Option<String>,
    pub cutover_at: Option<DateTime<Utc>>,
    pub activated_at: Option<DateTime<Utc>>,
    /// role → account id.
    pub account_mappings: BTreeMap<String, Uuid>,
    /// Required roles without an active mapped account.
    pub missing_roles: Vec<&'static str>,
    /// Currency, timezone and every role mapped.
    pub configured: bool,
    pub allowed_actions: Vec<&'static str>,
}

/// A ledger account.
#[derive(Serialize, ToSchema)]
pub struct AccountResource {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub code: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub parent_id: Option<Uuid>,
    pub active: bool,
    /// Roles this account is mapped to.
    pub roles: Vec<String>,
    pub version: i64,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub allowed_actions: Vec<&'static str>,
}

/// A fiscal year `[start_date, end_date)`.
#[derive(Serialize, ToSchema)]
pub struct YearResource {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub state: String,
    pub version: i64,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub allowed_actions: Vec<&'static str>,
}

/// A posting period `[start_date, end_date)` inside a fiscal year.
#[derive(Serialize, ToSchema)]
pub struct PeriodResource {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub fiscal_year_id: Uuid,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub state: String,
    pub version: i64,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub allowed_actions: Vec<&'static str>,
}

/// One journal line; amounts are exact decimal strings in major units.
#[derive(Serialize, ToSchema, Clone, PartialEq)]
pub struct LineResource {
    pub line_no: i16,
    pub account_id: Uuid,
    pub account_code: String,
    pub relation_id: Option<Uuid>,
    pub source_ref: Option<String>,
    pub debit: String,
    pub credit: String,
    /// In the functional currency; zero on unposted drafts.
    pub functional_debit: String,
    pub functional_credit: String,
    /// 1 entry-currency unit = `rate` functional units (or the inverse when `rate_inverse`).
    pub rate: String,
    pub rate_inverse: bool,
    /// `identity` (same currency) or `workspace` (workspace currency rate).
    pub rate_source: String,
    pub rate_date: Option<NaiveDate>,
}

/// Sums of a journal's lines.
#[derive(Serialize, ToSchema, Clone, PartialEq)]
pub struct Totals {
    pub debit: String,
    pub credit: String,
    pub functional_debit: String,
    pub functional_credit: String,
}

/// A journal entry with its lines.
#[derive(Serialize, ToSchema)]
pub struct JournalResource {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// Assigned at posting.
    pub number: Option<i64>,
    /// manual, source, opening, reversal, fx_revaluation or year_end_close.
    pub kind: String,
    /// draft or posted.
    pub state: String,
    pub accounting_date: NaiveDate,
    pub currency: CurrencyRef,
    pub functional_currency: Option<CurrencyRef>,
    pub reason: String,
    pub evidence_ref: Option<String>,
    pub reversal_of: Option<Uuid>,
    /// The reversal entry, when this entry was reversed.
    pub reversed_by: Option<Uuid>,
    pub period_id: Option<Uuid>,
    pub posted_by: Option<Uuid>,
    pub posted_at: Option<DateTime<Utc>>,
    pub version: i64,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub totals: Totals,
    pub lines: Vec<LineResource>,
    pub allowed_actions: Vec<&'static str>,
}

/// Server-computed preview of a post or reverse: what would be written, nothing stored.
#[derive(Serialize, ToSchema)]
pub struct ReviewResource {
    pub journal_id: Uuid,
    /// The journal version the review was computed from.
    pub version: i64,
    /// `post` or `reverse`.
    pub action: &'static str,
    pub accounting_date: NaiveDate,
    pub period_id: Uuid,
    pub currency: CurrencyRef,
    pub functional_currency: CurrencyRef,
    pub lines: Vec<LineResource>,
    pub totals: Totals,
    /// Send back on the action; a changed fact makes it stale (409 `stale_document`).
    pub review_fingerprint: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn journal(v: Value) -> Result<JournalInput, Vec<String>> {
        let mut r = Rules::default();
        let p: JournalPayload = serde_json::from_value(v).unwrap();
        let out = p.validate(&mut r);
        match r.into_result() {
            Ok(()) => Ok(out.unwrap()),
            Err(crate::error::AppError::Validation(e)) => Err(e.into_keys().collect()),
            Err(e) => panic!("{e}"),
        }
    }

    #[test]
    fn journal_lines_need_exactly_one_side_and_no_bank() {
        let base = |lines: Value| {
            json!({"accounting_date": "2027-01-15", "currency_id": Uuid::nil(), "reason": "r",
                   "lines": lines})
        };
        let a = Uuid::nil().to_string();
        let ok = journal(base(json!([
            {"account_id": a, "debit": "1.50"}, {"account_id": a, "credit": "1.5"}
        ])))
        .unwrap();
        assert_eq!(ok.lines[0].debit, Decimal::new(150, 2));
        assert_eq!(ok.lines[1].debit, Decimal::ZERO);
        let Err(errs) = journal(base(json!([
            {"account_id": a, "debit": "1", "credit": "1"},
            {"account_id": a, "credit": "-1", "bank_account_id": a}
        ]))) else {
            panic!("expected errors")
        };
        assert!(errs.contains(&"lines.0.debit".to_owned()), "{errs:?}");
        assert!(errs.contains(&"lines.1.credit".to_owned()), "{errs:?}");
        assert!(
            errs.contains(&"lines.1.bank_account_id".to_owned()),
            "{errs:?}"
        );
        let Err(one) = journal(base(json!([{"account_id": a, "debit": "1"}]))) else {
            panic!("one line is refused")
        };
        assert_eq!(one, vec!["lines".to_owned()]);
    }

    #[test]
    fn envelopes_need_client_ref_version_and_payload() {
        let mut r = Rules::default();
        let a: Action<PostPayload> = serde_json::from_value(
            json!({"client_ref": "x", "expected_version": 0, "payload": {}}),
        )
        .unwrap();
        assert!(a.validate(&mut r).is_none());
        let Err(crate::error::AppError::Validation(e)) = r.into_result() else {
            panic!("expected 422")
        };
        let keys: Vec<_> = e.into_keys().collect();
        assert_eq!(
            keys,
            [
                "client_ref",
                "expected_version",
                "payload.review_fingerprint"
            ]
        );
    }
}
