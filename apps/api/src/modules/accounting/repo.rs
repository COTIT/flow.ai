//! Accounting SQL. Every statement filters by `workspace_id`.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::modules::accounting::dto::CurrencyRef;

/// `accounting_settings` row.
pub struct SettingsRow {
    pub state: String,
    pub version: i64,
    pub functional_currency_id: Option<Uuid>,
    pub timezone: Option<String>,
    pub cutover_at: Option<DateTime<Utc>>,
    pub activated_at: Option<DateTime<Utc>>,
}

/// The book's settings; `lock` takes the row FOR UPDATE.
pub async fn settings(
    conn: &mut PgConnection,
    ws: Uuid,
    lock: bool,
) -> Result<SettingsRow, sqlx::Error> {
    if lock {
        sqlx::query_as!(
            SettingsRow,
            "SELECT state, version, functional_currency_id, timezone, cutover_at, activated_at
             FROM accounting_settings WHERE workspace_id = $1 FOR UPDATE",
            ws
        )
        .fetch_one(conn)
        .await
    } else {
        sqlx::query_as!(
            SettingsRow,
            "SELECT state, version, functional_currency_id, timezone, cutover_at, activated_at
             FROM accounting_settings WHERE workspace_id = $1",
            ws
        )
        .fetch_one(conn)
        .await
    }
}

/// Store a setup change and bump the version.
pub async fn update_settings(
    conn: &mut PgConnection,
    ws: Uuid,
    currency: Option<Uuid>,
    timezone: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE accounting_settings
         SET functional_currency_id = $2, timezone = $3, version = version + 1, updated_at = now()
         WHERE workspace_id = $1",
        ws,
        currency,
        timezone
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// One role mapping with its account's state.
pub struct MappingRow {
    pub role: String,
    pub account_id: Uuid,
    pub active: bool,
}

/// Every mapping of the book.
pub async fn mappings(db: impl PgExecutor<'_>, ws: Uuid) -> Result<Vec<MappingRow>, sqlx::Error> {
    sqlx::query_as!(
        MappingRow,
        "SELECT m.role, m.account_id, a.active
         FROM account_mappings m JOIN accounts a ON a.workspace_id = m.workspace_id AND a.id = m.account_id
         WHERE m.workspace_id = $1 ORDER BY m.role",
        ws
    )
    .fetch_all(db)
    .await
}

/// Map `role` to `account` (replacing any earlier mapping).
pub async fn set_mapping(
    conn: &mut PgConnection,
    ws: Uuid,
    role: &str,
    account: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO account_mappings (workspace_id, role, account_id) VALUES ($1, $2, $3)
         ON CONFLICT (workspace_id, role) DO UPDATE SET account_id = EXCLUDED.account_id",
        ws,
        role,
        account
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// A live currency of the workspace.
pub async fn currency(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<CurrencyRef>, sqlx::Error> {
    sqlx::query_as!(
        CurrencyRef,
        r#"SELECT id, upper(asset_type) AS "code!", decimals FROM currencies
           WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL"#,
        ws,
        id
    )
    .fetch_optional(db)
    .await
}

/// True when Postgres knows the IANA zone `name`.
pub async fn timezone_known(db: impl PgExecutor<'_>, name: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = $1) AS "known!""#,
        name
    )
    .fetch_one(db)
    .await
}

/// `accounts` row plus its mapped roles.
pub struct AccountRow {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub kind: String,
    pub parent_id: Option<Uuid>,
    pub active: bool,
    pub version: i64,
    pub roles: Vec<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One page of accounts by code, or the one with `id`; plus the total.
pub async fn accounts(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<AccountRow>, sqlx::Error> {
    sqlx::query_as!(
        AccountRow,
        r#"SELECT a.id, a.code, a.name, a.type AS kind, a.parent_id, a.active, a.version,
                  ARRAY(SELECT m.role FROM account_mappings m
                        WHERE m.workspace_id = a.workspace_id AND m.account_id = a.id
                        ORDER BY m.role) AS "roles!",
                  a.created_by, a.created_at, a.updated_at
           FROM accounts a
           WHERE a.workspace_id = $1 AND ($2::uuid IS NULL OR a.id = $2)
           ORDER BY a.code LIMIT $3 OFFSET $4"#,
        ws,
        id,
        limit,
        offset
    )
    .fetch_all(db)
    .await
}

/// How many accounts the book has.
pub async fn account_count(db: impl PgExecutor<'_>, ws: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM accounts WHERE workspace_id = $1"#,
        ws
    )
    .fetch_one(db)
    .await
}

/// Type and state of an account, FOR SHARE (or FOR UPDATE when `update`).
pub async fn account_lock(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    update: bool,
) -> Result<Option<(String, bool, i64)>, sqlx::Error> {
    let row = if update {
        sqlx::query!(
            "SELECT type, active, version FROM accounts WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
            ws,
            id
        )
        .fetch_optional(conn)
        .await?
        .map(|r| (r.r#type, r.active, r.version))
    } else {
        sqlx::query!(
            "SELECT type, active, version FROM accounts WHERE workspace_id = $1 AND id = $2 FOR SHARE",
            ws,
            id
        )
        .fetch_optional(conn)
        .await?
        .map(|r| (r.r#type, r.active, r.version))
    };
    Ok(row)
}

/// Insert an account; its id.
pub async fn insert_account(
    conn: &mut PgConnection,
    ws: Uuid,
    code: &str,
    name: &str,
    kind: &str,
    parent: Option<Uuid>,
    by: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO accounts (workspace_id, code, name, type, parent_id, created_by)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        ws,
        code,
        name,
        kind,
        parent,
        by
    )
    .fetch_one(conn)
    .await
}

/// Change name/active and bump the version.
pub async fn update_account(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    name: Option<&str>,
    active: Option<bool>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE accounts SET name = coalesce($3, name), active = coalesce($4, active),
                version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        name,
        active
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// True when the account is mapped to any role.
pub async fn is_mapped(db: impl PgExecutor<'_>, ws: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM account_mappings WHERE workspace_id = $1 AND account_id = $2) AS "m!""#,
        ws,
        id
    )
    .fetch_one(db)
    .await
}

/// A fiscal year or period row.
pub struct RangeRow {
    pub id: Uuid,
    pub fiscal_year_id: Option<Uuid>,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub state: String,
    pub version: i64,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One page of fiscal years by start, or the one with `id`.
pub async fn years(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<RangeRow>, sqlx::Error> {
    sqlx::query_as!(
        RangeRow,
        r#"SELECT id, NULL::uuid AS fiscal_year_id, start_date, end_date, state, version,
                  created_by, created_at, updated_at
           FROM accounting_fiscal_years
           WHERE workspace_id = $1 AND ($2::uuid IS NULL OR id = $2)
           ORDER BY start_date LIMIT $3 OFFSET $4"#,
        ws,
        id,
        limit,
        offset
    )
    .fetch_all(db)
    .await
}

/// One page of periods by start, or the one with `id`.
pub async fn periods(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<RangeRow>, sqlx::Error> {
    sqlx::query_as!(
        RangeRow,
        r#"SELECT id, fiscal_year_id AS "fiscal_year_id?", start_date, end_date, state, version,
                  created_by, created_at, updated_at
           FROM accounting_periods
           WHERE workspace_id = $1 AND ($2::uuid IS NULL OR id = $2)
           ORDER BY start_date LIMIT $3 OFFSET $4"#,
        ws,
        id,
        limit,
        offset
    )
    .fetch_all(db)
    .await
}

/// How many years (`periods` false) or periods the book has.
pub async fn range_count(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    periods: bool,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT CASE WHEN $2 THEN (SELECT count(*) FROM accounting_periods WHERE workspace_id = $1)
                       ELSE (SELECT count(*) FROM accounting_fiscal_years WHERE workspace_id = $1)
                  END AS "n!""#,
        ws,
        periods
    )
    .fetch_one(db)
    .await
}

/// True when `[start, end)` overlaps a year (`periods` false) or a period of the book.
pub async fn overlaps(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    periods: bool,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT CASE WHEN $2
                  THEN EXISTS (SELECT 1 FROM accounting_periods
                               WHERE workspace_id = $1 AND start_date < $4 AND $3 < end_date)
                  ELSE EXISTS (SELECT 1 FROM accounting_fiscal_years
                               WHERE workspace_id = $1 AND start_date < $4 AND $3 < end_date)
                  END AS "o!""#,
        ws,
        periods,
        start,
        end
    )
    .fetch_one(db)
    .await
}

/// Insert a fiscal year; its id.
pub async fn insert_year(
    conn: &mut PgConnection,
    ws: Uuid,
    start: NaiveDate,
    end: NaiveDate,
    by: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO accounting_fiscal_years (workspace_id, start_date, end_date, created_by)
         VALUES ($1, $2, $3, $4) RETURNING id",
        ws,
        start,
        end,
        by
    )
    .fetch_one(conn)
    .await
}

/// Insert a period; its id.
pub async fn insert_period(
    conn: &mut PgConnection,
    ws: Uuid,
    year: Uuid,
    start: NaiveDate,
    end: NaiveDate,
    by: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO accounting_periods (workspace_id, fiscal_year_id, start_date, end_date, created_by)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
        ws,
        year,
        start,
        end,
        by
    )
    .fetch_one(conn)
    .await
}

/// The open period (in an open year) containing `day`, both rows FOR SHARE so no close can
/// slip in before commit.
pub async fn open_period(
    conn: &mut PgConnection,
    ws: Uuid,
    day: NaiveDate,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT p.id FROM accounting_periods p
         JOIN accounting_fiscal_years y ON y.workspace_id = p.workspace_id AND y.id = p.fiscal_year_id
         WHERE p.workspace_id = $1 AND p.start_date <= $2 AND $2 < p.end_date
           AND p.state = 'open' AND y.state = 'open'
         FOR SHARE OF p, y",
        ws,
        day
    )
    .fetch_optional(conn)
    .await
}

/// `journal_entries` row with its currency.
pub struct EntryRow {
    pub id: Uuid,
    pub number: Option<i64>,
    pub kind: String,
    pub state: String,
    pub accounting_date: NaiveDate,
    pub currency_id: Uuid,
    pub currency_code: String,
    pub currency_decimals: i16,
    pub reason: String,
    pub evidence_ref: Option<String>,
    pub reversal_of: Option<Uuid>,
    pub reversed_by: Option<Uuid>,
    pub period_id: Option<Uuid>,
    pub posted_by: Option<Uuid>,
    pub posted_at: Option<DateTime<Utc>>,
    pub version: i64,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One page of entries, newest date first, or the one with `id`.
pub async fn entries(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<EntryRow>, sqlx::Error> {
    sqlx::query_as!(
        EntryRow,
        r#"SELECT e.id, e.number, e.kind, e.state, e.accounting_date, e.currency_id,
                  upper(c.asset_type) AS "currency_code!", c.decimals AS currency_decimals,
                  e.reason, e.evidence_ref, e.reversal_of,
                  (SELECT r.id FROM journal_entries r WHERE r.reversal_of = e.id) AS reversed_by,
                  e.period_id, e.posted_by, e.posted_at, e.version, e.created_by,
                  e.created_at, e.updated_at
           FROM journal_entries e
           JOIN currencies c ON c.workspace_id = e.workspace_id AND c.id = e.currency_id
           WHERE e.workspace_id = $1 AND ($2::uuid IS NULL OR e.id = $2)
           ORDER BY e.accounting_date DESC, e.id DESC LIMIT $3 OFFSET $4"#,
        ws,
        id,
        limit,
        offset
    )
    .fetch_all(db)
    .await
}

/// How many entries the book has.
pub async fn entry_count(db: impl PgExecutor<'_>, ws: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM journal_entries WHERE workspace_id = $1"#,
        ws
    )
    .fetch_one(db)
    .await
}

/// Lock an entry row FOR UPDATE; its kind, state and version.
pub async fn lock_entry(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<(String, String, i64)>, sqlx::Error> {
    Ok(sqlx::query!(
        "SELECT kind, state, version FROM journal_entries WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
        ws,
        id
    )
    .fetch_optional(conn)
    .await?
    .map(|r| (r.kind, r.state, r.version)))
}

/// `journal_lines` row with its account.
#[derive(Clone)]
pub struct LineRow {
    pub entry_id: Uuid,
    pub line_no: i16,
    pub account_id: Uuid,
    pub account_code: String,
    pub relation_id: Option<Uuid>,
    pub source_ref: Option<String>,
    pub debit: i64,
    pub credit: i64,
    pub functional_debit: i64,
    pub functional_credit: i64,
    pub rate: Decimal,
    pub rate_inverse: bool,
    pub rate_source: String,
    pub rate_date: Option<NaiveDate>,
}

/// Lines of `entries`, by entry then line number.
pub async fn lines(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    entries: &[Uuid],
) -> Result<Vec<LineRow>, sqlx::Error> {
    sqlx::query_as!(
        LineRow,
        "SELECT l.entry_id, l.line_no, l.account_id, a.code AS account_code, l.relation_id,
                l.source_ref, l.debit, l.credit, l.functional_debit, l.functional_credit,
                l.rate, l.rate_inverse, l.rate_source, l.rate_date
         FROM journal_lines l
         JOIN accounts a ON a.workspace_id = l.workspace_id AND a.id = l.account_id
         WHERE l.workspace_id = $1 AND l.entry_id = ANY($2)
         ORDER BY l.entry_id, l.line_no",
        ws,
        entries
    )
    .fetch_all(db)
    .await
}

/// Fields of a new or replaced draft entry.
pub struct DraftFields<'a> {
    pub kind: &'a str,
    pub date: NaiveDate,
    pub currency: Uuid,
    pub reason: &'a str,
    pub evidence_ref: Option<&'a str>,
    pub reversal_of: Option<Uuid>,
}

/// Insert a draft entry; its id.
pub async fn insert_entry(
    conn: &mut PgConnection,
    ws: Uuid,
    f: &DraftFields<'_>,
    by: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO journal_entries (workspace_id, kind, accounting_date, currency_id, reason,
                                      evidence_ref, reversal_of, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
        ws,
        f.kind,
        f.date,
        f.currency,
        f.reason,
        f.evidence_ref,
        f.reversal_of,
        by
    )
    .fetch_one(conn)
    .await
}

/// Replace a draft's fields and bump its version.
pub async fn update_entry(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    f: &DraftFields<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE journal_entries SET accounting_date = $3, currency_id = $4, reason = $5,
                evidence_ref = $6, version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        f.date,
        f.currency,
        f.reason,
        f.evidence_ref
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Drop a draft's lines (posted lines are refused by trigger).
pub async fn delete_lines(
    conn: &mut PgConnection,
    ws: Uuid,
    entry: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM journal_lines WHERE workspace_id = $1 AND entry_id = $2",
        ws,
        entry
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// A line to insert; functional amounts and rate snapshot set at posting.
pub struct NewLine {
    pub line_no: i16,
    pub account_id: Uuid,
    pub relation_id: Option<Uuid>,
    pub source_ref: Option<String>,
    pub debit: i64,
    pub credit: i64,
    pub functional_debit: i64,
    pub functional_credit: i64,
    pub rate: Decimal,
    pub rate_inverse: bool,
    pub rate_source: &'static str,
    pub rate_date: Option<NaiveDate>,
}

/// Insert lines of a draft entry in one statement.
pub async fn insert_lines(
    conn: &mut PgConnection,
    ws: Uuid,
    entry: Uuid,
    lines: &[NewLine],
) -> Result<(), sqlx::Error> {
    let col = |f: fn(&NewLine) -> i64| lines.iter().map(f).collect::<Vec<_>>();
    let no: Vec<i16> = lines.iter().map(|l| l.line_no).collect();
    let account: Vec<Uuid> = lines.iter().map(|l| l.account_id).collect();
    let relation: Vec<Option<Uuid>> = lines.iter().map(|l| l.relation_id).collect();
    let source: Vec<Option<String>> = lines.iter().map(|l| l.source_ref.clone()).collect();
    let rate: Vec<Decimal> = lines.iter().map(|l| l.rate).collect();
    let inverse: Vec<bool> = lines.iter().map(|l| l.rate_inverse).collect();
    let rate_source: Vec<String> = lines.iter().map(|l| l.rate_source.to_owned()).collect();
    let rate_date: Vec<Option<NaiveDate>> = lines.iter().map(|l| l.rate_date).collect();
    sqlx::query!(
        r#"INSERT INTO journal_lines (workspace_id, entry_id, line_no, account_id, relation_id,
                source_ref, debit, credit, functional_debit, functional_credit, rate, rate_inverse,
                rate_source, rate_date)
           SELECT $1, $2, * FROM UNNEST($3::smallint[], $4::uuid[], $5::uuid[], $6::text[],
                $7::bigint[], $8::bigint[], $9::bigint[], $10::bigint[], $11::numeric[],
                $12::bool[], $13::text[], $14::date[])"#,
        ws,
        entry,
        &no,
        &account,
        &relation as &[Option<Uuid>],
        &source as &[Option<String>],
        &col(|l| l.debit),
        &col(|l| l.credit),
        &col(|l| l.functional_debit),
        &col(|l| l.functional_credit),
        &rate,
        &inverse,
        &rate_source,
        &rate_date as &[Option<NaiveDate>],
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Accounts of `ids` that exist in the book: id, active, mapped roles. Locked FOR SHARE in
/// id order so a concurrent deactivate waits for this posting.
pub async fn line_accounts(
    conn: &mut PgConnection,
    ws: Uuid,
    ids: &[Uuid],
) -> Result<Vec<(Uuid, bool, Vec<String>)>, sqlx::Error> {
    Ok(sqlx::query!(
        r#"SELECT a.id, a.active,
                  ARRAY(SELECT m.role FROM account_mappings m
                        WHERE m.workspace_id = a.workspace_id AND m.account_id = a.id) AS "roles!"
           FROM accounts a WHERE a.workspace_id = $1 AND a.id = ANY($2)
           ORDER BY a.id FOR SHARE OF a"#,
        ws,
        ids
    )
    .fetch_all(conn)
    .await?
    .into_iter()
    .map(|r| (r.id, r.active, r.roles))
    .collect())
}

/// Relations of `ids` that exist in the book.
pub async fn relations_found(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM relations WHERE workspace_id = $1 AND id = ANY($2)",
        ws,
        ids
    )
    .fetch_all(db)
    .await
}

/// Record an economic effect; its id. A second effect with the same identity is refused by
/// `accounting_effects_identity_key`.
pub async fn insert_effect(
    conn: &mut PgConnection,
    ws: Uuid,
    source: Uuid,
    effect_kind: &str,
    fingerprint: &[u8],
    by: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO accounting_effects (workspace_id, source_kind, source_id, effect_kind,
                                         fingerprint, created_by)
         VALUES ($1, 'manual_journal', $2, $3, $4, $5) RETURNING id",
        ws,
        source,
        effect_kind,
        fingerprint,
        by
    )
    .fetch_one(conn)
    .await
}

/// Write functional amounts and rate snapshot onto a draft's line.
pub async fn set_functional(
    conn: &mut PgConnection,
    ws: Uuid,
    entry: Uuid,
    line: &NewLine,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE journal_lines SET functional_debit = $4, functional_credit = $5, rate = $6,
                rate_inverse = $7, rate_source = $8, rate_date = $9
         WHERE workspace_id = $1 AND entry_id = $2 AND line_no = $3",
        ws,
        entry,
        line.line_no,
        line.functional_debit,
        line.functional_credit,
        line.rate,
        line.rate_inverse,
        line.rate_source,
        line.rate_date
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Flip a draft to posted with its number, period and effect.
pub async fn mark_posted(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    number: i64,
    period: Uuid,
    effect: Uuid,
    by: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE journal_entries SET state = 'posted', number = $3, period_id = $4, effect_id = $5,
                posted_by = $6, posted_at = now(), version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        number,
        period,
        effect,
        by
    )
    .execute(conn)
    .await?;
    Ok(())
}
