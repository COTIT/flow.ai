//! Journals: manual drafts, server-computed post/reverse reviews, posting and manual-only
//! reversal. Lock order on post/reverse: command row → shared book gate → entry FOR UPDATE →
//! period + year FOR SHARE → accounts FOR SHARE by id → effect insert → journal number last.

use std::collections::HashMap;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{json::Rules, list::Page, workspace::WorkspaceAccess},
    idempotency::{self, Answer, Command},
    modules::{
        accounting::{
            dto::{
                Acted, Action, CONTROL_ROLES, Create, CurrencyRef, JournalInput, JournalPayload,
                JournalResource, LineResource, PostPayload, ROLES, ReverseInput, ReversePayload,
                ReviewResource, Totals,
            },
            gate,
            repo::{self, DraftFields, EntryRow, LineRow, NewLine, SumsRow},
            service::{created, not_configured, ok, period_closed, stale, unsupported},
        },
        currency_rates::repo::in_force,
    },
    numbering::{self, Series},
};

/// Effect source kind of manual journals.
const MANUAL: &str = "manual_journal";

const NOT_MANUAL: &str = "Only manual journals can be changed or reversed here. Correct source entries through their own workflow.";

/// Minor units as an exact decimal string at `dp` places.
fn amount(minor: i64, dp: i16) -> String {
    Decimal::new(minor, u32::try_from(dp).unwrap_or(0)).to_string()
}

fn line_resource(l: &LineRow, dp: i16, fdp: i16) -> LineResource {
    LineResource {
        line_no: l.line_no,
        account_id: l.account_id,
        account_code: l.account_code.clone(),
        relation_id: l.relation_id,
        source_ref: l.source_ref.clone(),
        debit: amount(l.debit, dp),
        credit: amount(l.credit, dp),
        functional_debit: amount(l.functional_debit, fdp),
        functional_credit: amount(l.functional_credit, fdp),
        rate: l.rate.normalize().to_string(),
        rate_inverse: l.rate_inverse,
        rate_source: l.rate_source.clone(),
        rate_date: l.rate_date,
    }
}

/// Line sums (computed in SQL) as decimal strings; no lines → zeros.
fn totals(s: Option<&SumsRow>, dp: i16, fdp: i16) -> Totals {
    let s = |f: fn(&SumsRow) -> i64| s.map_or(0, f);
    Totals {
        debit: amount(s(|s| s.debit), dp),
        credit: amount(s(|s| s.credit), dp),
        functional_debit: amount(s(|s| s.functional_debit), fdp),
        functional_credit: amount(s(|s| s.functional_credit), fdp),
    }
}

/// Entries with their lines (one lines query for the whole page).
async fn resources(
    conn: &mut PgConnection,
    ws: Uuid,
    rows: Vec<EntryRow>,
) -> Result<Vec<JournalResource>, AppError> {
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let mut by_entry: HashMap<Uuid, Vec<LineRow>> = HashMap::new();
    for l in repo::lines(&mut *conn, ws, &ids).await? {
        by_entry.entry(l.entry_id).or_default().push(l);
    }
    let sums = repo::sums(&mut *conn, ws, &ids).await?;
    let functional = functional(conn, ws).await?;
    let fdp = functional.as_ref().map_or(2, |c| c.decimals);
    Ok(rows
        .into_iter()
        .map(|e| {
            let lines = by_entry.remove(&e.id).unwrap_or_default();
            let dp = e.currency_decimals;
            let allowed_actions = match (e.kind.as_str(), e.state.as_str()) {
                ("manual", "draft") => vec!["update", "post"],
                ("manual", "posted") if e.reversed_by.is_none() => vec!["reverse"],
                _ => vec![],
            };
            JournalResource {
                id: e.id,
                workspace_id: ws,
                number: e.number,
                kind: e.kind,
                state: e.state,
                accounting_date: e.accounting_date,
                currency: CurrencyRef {
                    id: e.currency_id,
                    code: e.currency_code,
                    decimals: dp,
                },
                functional_currency: functional.clone(),
                reason: e.reason,
                evidence_ref: e.evidence_ref,
                reversal_of: e.reversal_of,
                reversed_by: e.reversed_by,
                period_id: e.period_id,
                posted_by: e.posted_by,
                posted_at: e.posted_at,
                version: e.version,
                created_by: e.created_by,
                created_at: e.created_at,
                updated_at: e.updated_at,
                totals: totals(sums.iter().find(|s| s.entry_id == e.id), dp, fdp),
                lines: lines.iter().map(|l| line_resource(l, dp, fdp)).collect(),
                allowed_actions,
            }
        })
        .collect())
}

async fn functional(conn: &mut PgConnection, ws: Uuid) -> Result<Option<CurrencyRef>, AppError> {
    let s = repo::settings(&mut *conn, ws, false).await?;
    Ok(match s.functional_currency_id {
        Some(id) => repo::currency(&mut *conn, ws, id).await?,
        None => None,
    })
}

/// One journal; 404 outside the workspace.
pub async fn get(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<JournalResource, AppError> {
    let rows = repo::entries(&mut *conn, ws, Some(id), 1, 0).await?;
    resources(conn, ws, rows)
        .await?
        .pop()
        .ok_or(AppError::NotFound)
}

/// One page of journals, newest date first; plus the total.
pub async fn list(
    state: &AppState,
    ws: Uuid,
    page: Page,
) -> Result<(Vec<JournalResource>, i64), AppError> {
    let mut conn = state.db.acquire().await?;
    let rows = repo::entries(&mut *conn, ws, None, page.size, page.offset()).await?;
    let total = repo::entry_count(&mut *conn, ws).await?;
    Ok((resources(&mut conn, ws, rows).await?, total))
}

/// Draft lines checked against the book: live accounts and relations of this workspace, no
/// control-account role, amounts exact at the currency's decimals.
async fn draft_lines(
    conn: &mut PgConnection,
    ws: Uuid,
    input: &JournalInput,
) -> Result<Vec<NewLine>, AppError> {
    let Some(currency) = repo::currency(&mut *conn, ws, input.currency).await? else {
        return Err(AppError::field(
            "payload.currency_id",
            "The selected currency id is invalid.",
        ));
    };
    let dp = u32::try_from(currency.decimals).unwrap_or(0);
    let account_ids: Vec<Uuid> = input.lines.iter().map(|l| l.account).collect();
    let accounts = repo::line_accounts(&mut *conn, ws, &account_ids).await?;
    let relation_ids: Vec<Uuid> = input.lines.iter().filter_map(|l| l.relation).collect();
    let relations = repo::relations_found(&mut *conn, ws, &relation_ids).await?;
    let mut r = Rules::default();
    let mut out = Vec::with_capacity(input.lines.len());
    for (i, l) in input.lines.iter().enumerate() {
        let field = |f: &str| format!("payload.lines.{i}.{f}");
        account_rule(
            &mut r,
            &field("account_id"),
            accounts.iter().find(|a| a.0 == l.account),
        );
        if l.relation.is_some_and(|id| !relations.contains(&id)) {
            r.add(
                &field("relation_id"),
                "The selected relation id is invalid.",
            );
        }
        let minor = |r: &mut Rules, f: &str, v: Decimal| match crate::money::minor(v, dp) {
            Ok(m) => Some(m),
            Err(_) => {
                r.add(
                    &field(f),
                    format!("The {f} must have at most {dp} decimal places."),
                );
                None
            }
        };
        let (debit, credit) = (
            minor(&mut r, "debit", l.debit),
            minor(&mut r, "credit", l.credit),
        );
        if let (Some(debit), Some(credit)) = (debit, credit) {
            out.push(NewLine {
                line_no: i16::try_from(i + 1).unwrap_or(i16::MAX),
                account_id: l.account,
                relation_id: l.relation,
                source_ref: l.source_ref.clone(),
                debit,
                credit,
                functional_debit: 0,
                functional_credit: 0,
                rate: Decimal::ONE,
                rate_inverse: false,
                rate_source: "identity",
                rate_date: None,
            });
        }
    }
    r.into_result()?;
    Ok(out)
}

/// Line account rule: in the book, active, not a control account.
fn account_rule(r: &mut Rules, field: &str, account: Option<&(Uuid, bool, Vec<String>)>) {
    match account {
        None => r.add(field, "The selected account id is invalid."),
        Some((_, false, _)) => r.add(field, "The selected account is inactive."),
        Some((_, true, roles)) => {
            if let Some(role) = roles
                .iter()
                .find(|role| CONTROL_ROLES.contains(&role.as_str()))
            {
                r.add(
                    field,
                    format!("Manual journals can't post to the {role} control account; use its own workflow."),
                );
            }
        }
    }
}

/// POST /accounting/journals: a manual draft.
pub async fn create(
    state: &AppState,
    access: &WorkspaceAccess,
    cmd: Command<Create<JournalPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "accounting_journals_create";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let input = &cmd.input.payload;
    let lines = draft_lines(&mut tx, ws, input).await?;
    let fields = DraftFields {
        kind: "manual",
        date: input.date,
        currency: input.currency,
        reason: &input.reason,
        evidence_ref: input.evidence_ref.as_deref(),
        reversal_of: None,
    };
    let id = repo::insert_entry(&mut tx, ws, &fields, access.user_id).await?;
    repo::insert_lines(&mut tx, ws, id, &lines).await?;
    let answer = created(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// PATCH /accounting/journals/{id}: replace a manual draft's fields and lines.
pub async fn update(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<JournalPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "accounting_journals_update";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let (kind, entry_state, version) = repo::lock_entry(&mut tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let Acted {
        expected_version,
        payload: input,
        ..
    } = &cmd.input;
    if version != *expected_version {
        return Err(stale());
    }
    if kind != "manual" {
        return Err(unsupported(NOT_MANUAL));
    }
    if entry_state != "draft" {
        return Err(unsupported(
            "Posted journals never change. Reverse them instead.",
        ));
    }
    let lines = draft_lines(&mut tx, ws, input).await?;
    let fields = DraftFields {
        kind: "manual",
        date: input.date,
        currency: input.currency,
        reason: &input.reason,
        evidence_ref: input.evidence_ref.as_deref(),
        reversal_of: None,
    };
    repo::update_entry(&mut tx, ws, id, &fields).await?;
    repo::delete_lines(&mut tx, ws, id).await?;
    repo::insert_lines(&mut tx, ws, id, &lines).await?;
    let answer = ok(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// What a post or reverse would write, computed from current facts.
struct Prepared {
    entry: EntryRow,
    period: Uuid,
    currency: CurrencyRef,
    functional: CurrencyRef,
    date: NaiveDate,
    lines: Vec<NewLine>,
    sums: SumsRow,
    fingerprint: [u8; 32],
}

impl Prepared {
    fn review(&self, action: &'static str, accounts: &[LineRow]) -> ReviewResource {
        let rows: Vec<LineRow> = self
            .lines
            .iter()
            .map(|l| LineRow {
                entry_id: self.entry.id,
                line_no: l.line_no,
                account_id: l.account_id,
                account_code: accounts
                    .iter()
                    .find(|a| a.account_id == l.account_id)
                    .map(|a| a.account_code.clone())
                    .unwrap_or_default(),
                relation_id: l.relation_id,
                source_ref: l.source_ref.clone(),
                debit: l.debit,
                credit: l.credit,
                functional_debit: l.functional_debit,
                functional_credit: l.functional_credit,
                rate: l.rate,
                rate_inverse: l.rate_inverse,
                rate_source: l.rate_source.to_owned(),
                rate_date: l.rate_date,
            })
            .collect();
        let (dp, fdp) = (self.currency.decimals, self.functional.decimals);
        ReviewResource {
            journal_id: self.entry.id,
            version: self.entry.version,
            action,
            accounting_date: self.date,
            period_id: self.period,
            currency: self.currency.clone(),
            functional_currency: self.functional.clone(),
            totals: totals(Some(&self.sums), dp, fdp),
            lines: rows.iter().map(|l| line_resource(l, dp, fdp)).collect(),
            review_fingerprint: hex::encode(self.fingerprint),
        }
    }
}

/// The book's functional currency when it is active and fully configured.
async fn active_book(conn: &mut PgConnection, ws: Uuid) -> Result<CurrencyRef, AppError> {
    let s = repo::settings(&mut *conn, ws, false).await?;
    let mappings = repo::mappings(&mut *conn, ws).await?;
    let mapped = ROLES
        .iter()
        .all(|role| mappings.iter().any(|m| m.role == *role && m.active));
    let currency = match s.functional_currency_id {
        Some(id) => repo::currency(&mut *conn, ws, id).await?,
        None => None,
    };
    match currency {
        Some(c) if s.state == "active" && s.timezone.is_some() && mapped => Ok(c),
        _ => Err(not_configured()),
    }
}

fn fingerprint(value: &serde_json::Value) -> [u8; 32] {
    Sha256::digest(value.to_string().as_bytes()).into()
}

fn lines_json(lines: &[NewLine]) -> serde_json::Value {
    lines
        .iter()
        .map(|l| {
            json!([
                l.line_no,
                l.account_id,
                l.relation_id,
                l.source_ref,
                l.debit,
                l.credit,
                l.functional_debit,
                l.functional_credit,
                l.rate.normalize().to_string(),
                l.rate_inverse,
                l.rate_source,
                l.rate_date
            ])
        })
        .collect()
}

/// Post facts for a draft, rechecked under locks (period and accounts FOR SHARE).
async fn prepare_post(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<(Prepared, Vec<LineRow>), AppError> {
    let functional = active_book(&mut *conn, ws).await?;
    let entry = repo::entries(&mut *conn, ws, Some(id), 1, 0)
        .await?
        .pop()
        .ok_or(AppError::NotFound)?;
    if entry.kind != "manual" {
        return Err(unsupported(NOT_MANUAL));
    }
    if entry.state != "draft" {
        return Err(stale());
    }
    let stored = repo::lines(&mut *conn, ws, &[id]).await?;
    if stored.len() < 2 || !repo::balanced(&mut *conn, ws, id).await? {
        return Err(AppError::field("lines", "Debits and credits must balance."));
    }
    let ids: Vec<Uuid> = stored.iter().map(|l| l.account_id).collect();
    let accounts = repo::line_accounts(&mut *conn, ws, &ids).await?;
    let mut r = Rules::default();
    for (i, l) in stored.iter().enumerate() {
        account_rule(
            &mut r,
            &format!("lines.{i}.account_id"),
            accounts.iter().find(|a| a.0 == l.account_id),
        );
    }
    r.into_result()?;
    let period = repo::open_period(&mut *conn, ws, entry.accounting_date)
        .await?
        .ok_or_else(period_closed)?;
    let currency = CurrencyRef {
        id: entry.currency_id,
        code: entry.currency_code.clone(),
        decimals: entry.currency_decimals,
    };
    let (rate, inverse, source, rate_date) = if currency.id == functional.id {
        (Decimal::ONE, false, "identity", None)
    } else if let Some(r) = in_force(
        &mut *conn,
        ws,
        currency.id,
        functional.id,
        entry.accounting_date,
    )
    .await?
    {
        (r.rate, false, "workspace", Some(r.effective_on))
    } else if let Some(r) = in_force(
        &mut *conn,
        ws,
        functional.id,
        currency.id,
        entry.accounting_date,
    )
    .await?
    {
        (r.rate, true, "workspace", Some(r.effective_on))
    } else {
        return Err(AppError::field(
            "currency_id",
            format!(
                "No {} → {} rate is in force on {}.",
                currency.code, functional.code, entry.accounting_date
            ),
        ));
    };
    let unconvertible = || {
        AppError::field(
            "lines",
            "The lines can't be converted to the functional currency exactly.",
        )
    };
    let dps = (i32::from(currency.decimals), i32::from(functional.decimals));
    let converted = match repo::converted(&mut *conn, ws, id, rate, inverse, dps).await {
        Ok(Some(rows)) => rows,
        Ok(None) => return Err(unconvertible()),
        // 22003: a converted amount past bigint.
        Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("22003") => {
            return Err(unconvertible());
        }
        Err(e) => return Err(e.into()),
    };
    let sums = converted
        .first()
        .map(|r| r.sums)
        .ok_or_else(unconvertible)?;
    let lines: Vec<NewLine> = stored
        .iter()
        .map(|l| NewLine {
            line_no: l.line_no,
            account_id: l.account_id,
            relation_id: l.relation_id,
            source_ref: l.source_ref.clone(),
            debit: l.debit,
            credit: l.credit,
            functional_debit: converted
                .iter()
                .find(|c| c.line_no == l.line_no)
                .map_or(0, |c| c.functional_debit),
            functional_credit: converted
                .iter()
                .find(|c| c.line_no == l.line_no)
                .map_or(0, |c| c.functional_credit),
            rate,
            rate_inverse: inverse,
            rate_source: source,
            rate_date,
        })
        .collect();
    let fingerprint = fingerprint(&json!({
        "action": "post", "entry": entry.id, "version": entry.version, "period": period,
        "date": entry.accounting_date, "currency": currency.id, "functional": functional.id,
        "lines": lines_json(&lines),
    }));
    let date = entry.accounting_date;
    Ok((
        Prepared {
            entry,
            period,
            currency,
            functional,
            date,
            lines,
            sums,
            fingerprint,
        },
        stored,
    ))
}

/// POST /accounting/journals/{id}/post/review: no writes.
pub async fn post_review(state: &AppState, ws: Uuid, id: Uuid) -> Result<ReviewResource, AppError> {
    let mut tx = state.db.begin().await?;
    let (p, stored) = prepare_post(&mut tx, ws, id).await?;
    tx.rollback().await?;
    Ok(p.review("post", &stored))
}

/// POST /accounting/journals/{id}/post.
pub async fn post(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<PostPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "accounting_journals_post";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    gate::shared(&mut tx, &[ws]).await?;
    let (_, _, version) = repo::lock_entry(&mut tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if version != cmd.input.expected_version {
        return Err(stale());
    }
    let (p, _) = prepare_post(&mut tx, ws, id).await?;
    if hex::encode(p.fingerprint) != cmd.input.payload {
        return Err(stale());
    }
    for l in &p.lines {
        repo::set_functional(&mut tx, ws, id, l).await?;
    }
    let effect = repo::insert_effect(
        &mut tx,
        ws,
        MANUAL,
        id,
        "posted",
        &p.fingerprint,
        access.user_id,
    )
    .await?;
    let number = numbering::next(&mut tx, ws, Series::Journal, 1).await?;
    repo::mark_posted(&mut tx, ws, id, number, p.period, effect, access.user_id).await?;
    let answer = ok(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Reversal facts for a posted manual entry: same lines, sides swapped, same functional
/// amounts and rate snapshot, dated into an open period.
async fn prepare_reverse(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    input: &ReverseInput,
) -> Result<(Prepared, Vec<LineRow>), AppError> {
    let functional = active_book(&mut *conn, ws).await?;
    let entry = repo::entries(&mut *conn, ws, Some(id), 1, 0)
        .await?
        .pop()
        .ok_or(AppError::NotFound)?;
    if entry.kind != "manual" {
        return Err(unsupported(NOT_MANUAL));
    }
    if entry.state != "posted" {
        return Err(unsupported("Only posted journals can be reversed."));
    }
    if entry.reversed_by.is_some() {
        return Err(AppError::Conflict("This journal is already reversed."));
    }
    if input.date < entry.accounting_date {
        return Err(AppError::field(
            "payload.accounting_date",
            "The reversal date can't be before the journal's date.",
        ));
    }
    let period = repo::open_period(&mut *conn, ws, input.date)
        .await?
        .ok_or_else(period_closed)?;
    let stored = repo::lines(&mut *conn, ws, &[id]).await?;
    let lines: Vec<NewLine> = stored
        .iter()
        .map(|l| NewLine {
            line_no: l.line_no,
            account_id: l.account_id,
            relation_id: l.relation_id,
            source_ref: l.source_ref.clone(),
            debit: l.credit,
            credit: l.debit,
            functional_debit: l.functional_credit,
            functional_credit: l.functional_debit,
            rate: l.rate,
            rate_inverse: l.rate_inverse,
            rate_source: if l.rate_source == "workspace" {
                "workspace"
            } else {
                "identity"
            },
            rate_date: l.rate_date,
        })
        .collect();
    let currency = CurrencyRef {
        id: entry.currency_id,
        code: entry.currency_code.clone(),
        decimals: entry.currency_decimals,
    };
    // Same lines with sides swapped: the stored sums, swapped.
    let sums = repo::sums(&mut *conn, ws, &[id])
        .await?
        .pop()
        .map(|s| SumsRow {
            entry_id: id,
            debit: s.credit,
            credit: s.debit,
            functional_debit: s.functional_credit,
            functional_credit: s.functional_debit,
        })
        .ok_or_else(|| AppError::Internal("posted journal without lines".into()))?;
    let fingerprint = fingerprint(&json!({
        "action": "reverse", "entry": entry.id, "version": entry.version, "period": period,
        "date": input.date, "reason": input.reason, "lines": lines_json(&lines),
    }));
    Ok((
        Prepared {
            entry,
            period,
            currency,
            functional,
            date: input.date,
            lines,
            sums,
            fingerprint,
        },
        stored,
    ))
}

/// POST /accounting/journals/{id}/reverse/review: no writes.
pub async fn reverse_review(
    state: &AppState,
    ws: Uuid,
    id: Uuid,
    input: &ReverseInput,
) -> Result<ReviewResource, AppError> {
    let mut tx = state.db.begin().await?;
    let (p, stored) = prepare_reverse(&mut tx, ws, id, input).await?;
    tx.rollback().await?;
    Ok(p.review("reverse", &stored))
}

/// POST /accounting/journals/{id}/reverse: manual originals only; returns the reversal.
pub async fn reverse(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<ReversePayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "accounting_journals_reverse";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    gate::shared(&mut tx, &[ws]).await?;
    let (_, _, version) = repo::lock_entry(&mut tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let (input, review) = &cmd.input.payload;
    if version != cmd.input.expected_version {
        return Err(stale());
    }
    let (p, _) = prepare_reverse(&mut tx, ws, id, input).await?;
    if hex::encode(p.fingerprint) != *review {
        return Err(stale());
    }
    let fields = DraftFields {
        kind: "reversal",
        date: input.date,
        currency: p.currency.id,
        reason: &input.reason,
        evidence_ref: None,
        reversal_of: Some(id),
    };
    let reversal = repo::insert_entry(&mut tx, ws, &fields, access.user_id).await?;
    repo::insert_lines(&mut tx, ws, reversal, &p.lines).await?;
    let effect = repo::insert_effect(
        &mut tx,
        ws,
        MANUAL,
        id,
        "reversed",
        &p.fingerprint,
        access.user_id,
    )
    .await
    .map_err(
        |e| match e.as_database_error().and_then(|d| d.constraint()) {
            Some("accounting_effects_identity_key") => {
                AppError::Conflict("This journal is already reversed.")
            }
            _ => e.into(),
        },
    )?;
    let number = numbering::next(&mut tx, ws, Series::Journal, 1).await?;
    repo::mark_posted(
        &mut tx,
        ws,
        reversal,
        number,
        p.period,
        effect,
        access.user_id,
    )
    .await?;
    let answer = ok(&get(&mut tx, ws, reversal).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id, reversal]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// A source posting in the functional currency: one debit and one credit line on mapped roles.
pub struct SourceJournal<'a> {
    pub source_kind: &'a str,
    pub source_id: Uuid,
    pub effect_kind: &'a str,
    pub date: NaiveDate,
    pub reason: &'a str,
    pub debit: &'a str,
    pub credit: &'a str,
    /// Functional minor units, > 0.
    pub amount: i64,
    pub fingerprint: &'a [u8; 32],
    pub by: Uuid,
}

/// Effect identity, date and actor of a multi-line source posting.
pub struct SourceHeader<'a> {
    pub source_kind: &'a str,
    pub source_id: Uuid,
    pub effect_kind: &'a str,
    pub date: NaiveDate,
    pub reason: &'a str,
    pub fingerprint: &'a [u8; 32],
    pub by: Uuid,
}

/// One line of a source posting on a mapped role, functional minor units (one side > 0).
pub struct SourceLine<'a> {
    pub role: &'a str,
    pub debit: i64,
    pub credit: i64,
    /// Customer or supplier of a control-account line.
    pub relation_id: Option<Uuid>,
}

/// Post `j` as a two-line `source` journal; its id. See [`post_source_lines`].
pub async fn post_source(
    conn: &mut PgConnection,
    ws: Uuid,
    j: &SourceJournal<'_>,
) -> Result<Uuid, AppError> {
    let header = SourceHeader {
        source_kind: j.source_kind,
        source_id: j.source_id,
        effect_kind: j.effect_kind,
        date: j.date,
        reason: j.reason,
        fingerprint: j.fingerprint,
        by: j.by,
    };
    let lines = [
        SourceLine {
            role: j.debit,
            debit: j.amount,
            credit: 0,
            relation_id: None,
        },
        SourceLine {
            role: j.credit,
            debit: 0,
            credit: j.amount,
            relation_id: None,
        },
    ];
    Ok(post_source_lines(conn, ws, &header, &lines).await?.0)
}

/// Post `lines` as a `source` journal in the caller's transaction; (entry, effect). 422
/// `accounting_not_configured` without functional currency or an active mapped account per
/// role (accounts locked FOR SHARE), `accounting_period_closed` without an open period on the
/// date. Balance is checked at commit (deferred trigger). The effect identity makes a second
/// posting of the same source a database error.
pub async fn post_source_lines(
    conn: &mut PgConnection,
    ws: Uuid,
    j: &SourceHeader<'_>,
    lines: &[SourceLine<'_>],
) -> Result<(Uuid, Uuid), AppError> {
    let currency = repo::settings(&mut *conn, ws, false)
        .await?
        .functional_currency_id
        .ok_or_else(not_configured)?;
    let mappings = repo::mappings(&mut *conn, ws).await?;
    let accounts = lines
        .iter()
        .map(|l| {
            mappings
                .iter()
                .find(|m| m.role == l.role)
                .map(|m| m.account_id)
                .ok_or_else(not_configured)
        })
        .collect::<Result<Vec<Uuid>, AppError>>()?;
    // As manual posting: FOR SHARE in id order, so a deactivation waits for this posting or
    // this posting sees it; active read under the lock.
    let locked = repo::line_accounts(&mut *conn, ws, &accounts).await?;
    if !accounts
        .iter()
        .all(|id| locked.iter().any(|a| a.0 == *id && a.1))
    {
        return Err(not_configured());
    }
    let period = repo::open_period(&mut *conn, ws, j.date)
        .await?
        .ok_or_else(period_closed)?;
    let fields = DraftFields {
        kind: "source",
        date: j.date,
        currency,
        reason: j.reason,
        evidence_ref: None,
        reversal_of: None,
    };
    let entry = repo::insert_entry(&mut *conn, ws, &fields, j.by).await?;
    let rows: Vec<NewLine> = lines
        .iter()
        .zip(&accounts)
        .zip(1i16..)
        .map(|((l, account_id), line_no)| NewLine {
            line_no,
            account_id: *account_id,
            relation_id: l.relation_id,
            source_ref: None,
            debit: l.debit,
            credit: l.credit,
            functional_debit: l.debit,
            functional_credit: l.credit,
            rate: Decimal::ONE,
            rate_inverse: false,
            rate_source: "identity",
            rate_date: None,
        })
        .collect();
    repo::insert_lines(&mut *conn, ws, entry, &rows).await?;
    let effect = repo::insert_effect(
        &mut *conn,
        ws,
        j.source_kind,
        j.source_id,
        j.effect_kind,
        j.fingerprint,
        j.by,
    )
    .await?;
    let number = numbering::next(&mut *conn, ws, Series::Journal, 1).await?;
    repo::mark_posted(&mut *conn, ws, entry, number, period, effect, j.by).await?;
    Ok((entry, effect))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Book with EUR functional, roles unbilled_receivables/revenue/tax_payable mapped, an
    /// open period around today; (ws, user).
    async fn book(pool: &sqlx::PgPool) -> (Uuid, Uuid) {
        let row: (Uuid, Uuid) = sqlx::query_as(
            "WITH u AS (INSERT INTO users (email, first_name, last_name, password_hash)
                        VALUES ('o@x.io', 'O', 'W', 'h') RETURNING id),
                  w AS (INSERT INTO workspaces (name, created_by_user_id)
                        SELECT 'acme', id FROM u RETURNING id, created_by_user_id),
                  c AS (INSERT INTO currencies (workspace_id, name, asset_type, decimals, active)
                        SELECT id, 'Euro', 'EUR', 2, true FROM w RETURNING id)
             SELECT w.id, w.created_by_user_id FROM w, c",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let (ws, user) = row;
        for (code, ty, role) in [
            ("1210", "asset", "unbilled_receivables"),
            ("4000", "income", "revenue"),
            ("2200", "liability", "tax_payable"),
        ] {
            sqlx::query(
                "WITH a AS (INSERT INTO accounts (workspace_id, code, name, type, created_by)
                            VALUES ($1, $2, $2, $3, $4) RETURNING id)
                 INSERT INTO account_mappings (workspace_id, role, account_id)
                 SELECT $1, $5, id FROM a",
            )
            .bind(ws)
            .bind(code)
            .bind(ty)
            .bind(user)
            .bind(role)
            .execute(pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "WITH y AS (INSERT INTO accounting_fiscal_years (workspace_id, start_date, end_date, created_by)
                        VALUES ($1, current_date - 10, current_date + 10, $2) RETURNING id)
             INSERT INTO accounting_periods (workspace_id, fiscal_year_id, start_date, end_date, created_by)
             SELECT $1, id, current_date - 10, current_date + 10, $2 FROM y",
        )
        .bind(ws)
        .bind(user)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE accounting_settings SET functional_currency_id =
                 (SELECT id FROM currencies WHERE workspace_id = $1), timezone = 'UTC'
             WHERE workspace_id = $1",
        )
        .bind(ws)
        .execute(pool)
        .await
        .unwrap();
        row
    }

    #[sqlx::test]
    async fn source_lines_post_once(pool: sqlx::PgPool) {
        let (ws, user) = book(&pool).await;
        let source = Uuid::now_v7();
        let header = SourceHeader {
            source_kind: "shipment",
            source_id: source,
            effect_kind: "fulfilled",
            date: chrono::Utc::now().date_naive(),
            reason: "sale",
            fingerprint: &[7; 32],
            by: user,
        };
        let line = |role, debit, credit, relation_id| SourceLine {
            role,
            debit,
            credit,
            relation_id,
        };
        let lines = [
            line("unbilled_receivables", 121, 0, None),
            line("revenue", 0, 100, None),
            line("tax_payable", 0, 21, None),
        ];
        let mut tx = pool.begin().await.unwrap();
        let (entry, effect) = post_source_lines(&mut tx, ws, &header, &lines)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let got: Vec<(i16, String, i64, i64, Option<Uuid>)> = sqlx::query_as(
            "SELECT l.line_no, m.role, l.functional_debit, l.functional_credit, l.relation_id
             FROM journal_lines l
             JOIN account_mappings m ON m.workspace_id = l.workspace_id AND m.account_id = l.account_id
             WHERE l.entry_id = $1 ORDER BY l.line_no",
        )
        .bind(entry)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            got,
            vec![
                (1, "unbilled_receivables".into(), 121, 0, None),
                (2, "revenue".into(), 0, 100, None),
                (3, "tax_payable".into(), 0, 21, None),
            ]
        );
        let posted: (String, Uuid) =
            sqlx::query_as("SELECT state, effect_id FROM journal_entries WHERE id = $1")
                .bind(entry)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(posted, ("posted".into(), effect));
        // Same effect identity again: refused by the database.
        let mut tx = pool.begin().await.unwrap();
        assert!(
            post_source_lines(&mut tx, ws, &header, &lines)
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
        // An unmapped role: accounting_not_configured, nothing written.
        let mut tx = pool.begin().await.unwrap();
        let other = SourceHeader {
            source_id: Uuid::now_v7(),
            ..header
        };
        let err = post_source_lines(
            &mut tx,
            ws,
            &other,
            &[line("cash", 5, 0, None), line("revenue", 0, 5, None)],
        )
        .await
        .unwrap_err();
        assert!(matches!(
            err,
            AppError::Reason {
                code: "accounting_not_configured",
                ..
            }
        ));
    }
}
