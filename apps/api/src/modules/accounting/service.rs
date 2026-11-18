//! Book setup, chart of accounts and fiscal calendar. Every write runs as a permanent
//! accounting command (replayed by `client_ref`); setup and calendar take the exclusive gate.

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{list::Page, workspace::WorkspaceAccess},
    idempotency::{self, Answer, Command},
    modules::accounting::{
        dto::{
            AccountChange, AccountPatch, AccountPayload, AccountResource, Acted, Action,
            BankAccountPayload, BankAccountResource, Create, Created, NewAccount, NewBankAccount,
            NewPeriod, PeriodPayload, PeriodResource, ROLES, Range, RevaluationRef, SetupInput,
            SetupPayload, SetupResource, YearPayload, YearResource,
        },
        gate,
        repo::{self, AccountRow, RangeRow},
    },
};

/// 409 `stale_document`: the caller's version or review no longer matches.
pub fn stale() -> AppError {
    AppError::Reason {
        status: StatusCode::CONFLICT,
        code: "stale_document",
        message: "This record changed since it was loaded. Reload it and try again.".into(),
    }
}

/// 422 `accounting_not_configured`.
pub fn not_configured() -> AppError {
    AppError::Reason {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code: "accounting_not_configured",
        message: "Accounting is not set up and active for this workspace yet.".into(),
    }
}

/// 422 `accounting_period_closed`.
pub fn period_closed() -> AppError {
    AppError::Reason {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code: "accounting_period_closed",
        message: "No open accounting period covers this date.".into(),
    }
}

/// 422 `unsupported_active_erp_operation` with a client-safe explanation.
pub fn unsupported(message: &str) -> AppError {
    AppError::Reason {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code: "unsupported_active_erp_operation",
        message: message.into(),
    }
}

/// The book's setup as the caller sees it.
pub async fn setup(conn: &mut PgConnection, ws: Uuid) -> Result<SetupResource, AppError> {
    let s = repo::settings(&mut *conn, ws, false).await?;
    let mappings = repo::mappings(&mut *conn, ws).await?;
    let currency = match s.functional_currency_id {
        Some(id) => repo::currency(&mut *conn, ws, id).await?,
        None => None,
    };
    let missing_roles: Vec<&'static str> = ROLES
        .into_iter()
        .filter(|role| !mappings.iter().any(|m| m.role == *role && m.active))
        .collect();
    let configured = currency.is_some() && s.timezone.is_some() && missing_roles.is_empty();
    let setup = s.state == "setup";
    Ok(SetupResource {
        state: s.state,
        version: s.version,
        functional_currency: currency,
        timezone: s.timezone,
        cutover_at: s.cutover_at,
        activated_at: s.activated_at,
        account_mappings: mappings
            .into_iter()
            .map(|m| (m.role, m.account_id))
            .collect(),
        missing_roles,
        configured,
        allowed_actions: if setup { vec!["update"] } else { vec![] },
    })
}

/// PUT /accounting/setup: setup state only, version-checked, exclusive gate.
pub async fn update_setup(
    state: &AppState,
    access: &WorkspaceAccess,
    cmd: Command<Action<SetupPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "accounting_setup_update";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    gate::exclusive(&mut tx, ws).await?;
    let current = repo::settings(&mut tx, ws, true).await?;
    let Acted {
        expected_version,
        payload:
            SetupInput {
                currency,
                timezone,
                mappings,
            },
        ..
    } = &cmd.input;
    if current.state != "setup" {
        return Err(unsupported(
            "Accounting setup can only change before the book is activated.",
        ));
    }
    if current.version != *expected_version {
        return Err(stale());
    }
    let mut r = crate::http::json::Rules::default();
    if let Some(id) = currency {
        r.check(
            "payload.functional_currency_id",
            repo::currency(&mut *tx, ws, *id).await?.is_some(),
            "The selected functional currency id is invalid.",
        );
    }
    if let Some(tz) = timezone {
        r.check(
            "payload.timezone",
            repo::timezone_known(&mut *tx, tz).await?,
            "The timezone must be a valid IANA zone name.",
        );
    }
    for (role, account) in mappings {
        let ok = matches!(
            repo::account_lock(&mut tx, ws, *account, false).await?,
            Some((_, true, _))
        );
        r.check(
            &format!("payload.account_mappings.{role}"),
            ok,
            "The selected account is invalid or inactive.",
        );
    }
    r.into_result()?;
    repo::update_settings(
        &mut tx,
        ws,
        currency.or(current.functional_currency_id),
        timezone.as_deref().or(current.timezone.as_deref()),
    )
    .await?;
    for (role, account) in mappings {
        repo::set_mapping(&mut tx, ws, role, *account).await?;
    }
    let answer = ok(&setup(&mut tx, ws).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[ws]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// 200 `{data}`.
pub fn ok<T: serde::Serialize>(data: &T) -> Answer {
    Answer {
        status: 200,
        body: json!({ "data": data }).to_string().into_bytes(),
    }
}

/// 201 `{data}`.
pub fn created<T: serde::Serialize>(data: &T) -> Answer {
    Answer::created(&json!({ "data": data }))
}

fn account_resource(r: AccountRow, ws: Uuid) -> AccountResource {
    AccountResource {
        id: r.id,
        workspace_id: ws,
        code: r.code,
        name: r.name,
        kind: r.kind,
        parent_id: r.parent_id,
        active: r.active,
        roles: r.roles,
        version: r.version,
        created_by: r.created_by,
        created_at: r.created_at,
        updated_at: r.updated_at,
        allowed_actions: vec!["update"],
    }
}

/// One page of the chart of accounts, by code; plus the total.
pub async fn accounts(
    state: &AppState,
    ws: Uuid,
    page: Page,
) -> Result<(Vec<AccountResource>, i64), AppError> {
    let rows = repo::accounts(&state.db, ws, None, page.size, page.offset()).await?;
    let total = repo::account_count(&state.db, ws).await?;
    Ok((
        rows.into_iter().map(|r| account_resource(r, ws)).collect(),
        total,
    ))
}

async fn account(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<AccountResource, AppError> {
    let row = repo::accounts(conn, ws, Some(id), 1, 0)
        .await?
        .pop()
        .ok_or(AppError::NotFound)?;
    Ok(account_resource(row, ws))
}

/// POST /accounting/accounts.
pub async fn create_account(
    state: &AppState,
    access: &WorkspaceAccess,
    cmd: Command<Create<AccountPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "accounting_accounts_create";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let Created {
        payload:
            NewAccount {
                code,
                name,
                kind,
                parent,
            },
        ..
    } = &cmd.input;
    if let Some(p) = parent {
        let same_type = matches!(repo::account_lock(&mut tx, ws, *p, false).await?, Some((t, _, _)) if t == *kind);
        if !same_type {
            return Err(AppError::field(
                "payload.parent_id",
                "The parent must be an account of the same type.",
            ));
        }
    }
    let id = repo::insert_account(&mut tx, ws, code, name, kind, *parent, access.user_id)
        .await
        .map_err(|e| {
            AppError::unique(
                e,
                "accounts_workspace_code_key",
                "payload.code",
                "The code has already been taken.",
            )
        })?;
    let answer = created(&account(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// PATCH /accounting/accounts/{id}: name/active; a mapped account stays active.
pub async fn update_account(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<AccountPatch>>,
) -> Result<Answer, AppError> {
    const OP: &str = "accounting_accounts_update";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let (_, _, version) = repo::account_lock(&mut tx, ws, id, true)
        .await?
        .ok_or(AppError::NotFound)?;
    let Acted {
        expected_version,
        payload: AccountChange { name, active },
        ..
    } = &cmd.input;
    if version != *expected_version {
        return Err(stale());
    }
    if *active == Some(false) && repo::is_mapped(&mut *tx, ws, id).await? {
        return Err(AppError::field(
            "payload.active",
            "A mapped account can't be deactivated. Map its role to another account first.",
        ));
    }
    repo::update_account(&mut tx, ws, id, name.as_deref(), *active).await?;
    let answer = ok(&account(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

fn year_resource(r: RangeRow, ws: Uuid) -> YearResource {
    let actions = if r.state == "open" {
        vec!["close"]
    } else {
        vec![]
    };
    YearResource {
        id: r.id,
        workspace_id: ws,
        start_date: r.start_date,
        end_date: r.end_date,
        state: r.state,
        version: r.version,
        created_by: r.created_by,
        created_at: r.created_at,
        updated_at: r.updated_at,
        allowed_actions: actions,
    }
}

pub(crate) fn period_resource(
    r: RangeRow,
    ws: Uuid,
    last: Option<RevaluationRef>,
) -> PeriodResource {
    let open = r.state == "open";
    // The final period closes only through the year-end close.
    let actions = match (open, r.final_period) {
        (true, false) => vec!["revalue", "close"],
        (true, true) => vec!["revalue"],
        _ => vec![],
    };
    PeriodResource {
        id: r.id,
        workspace_id: ws,
        fiscal_year_id: r.fiscal_year_id.unwrap_or_default(),
        start_date: r.start_date,
        end_date: r.end_date,
        state: r.state,
        version: r.version,
        created_by: r.created_by,
        created_at: r.created_at,
        updated_at: r.updated_at,
        last_revaluation: last,
        allowed_actions: actions,
    }
}

/// One page of fiscal years by start; plus the total.
pub async fn years(
    state: &AppState,
    ws: Uuid,
    page: Page,
) -> Result<(Vec<YearResource>, i64), AppError> {
    let rows = repo::years(&state.db, ws, None, page.size, page.offset()).await?;
    let total = repo::range_count(&state.db, ws, false).await?;
    Ok((
        rows.into_iter().map(|r| year_resource(r, ws)).collect(),
        total,
    ))
}

/// One page of periods by start; plus the total.
pub async fn periods(
    state: &AppState,
    ws: Uuid,
    page: Page,
) -> Result<(Vec<PeriodResource>, i64), AppError> {
    let rows = repo::periods(&state.db, ws, None, page.size, page.offset()).await?;
    let total = repo::range_count(&state.db, ws, true).await?;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let mut conn = state.db.acquire().await?;
    let mut last = super::revaluation::latest(&mut conn, ws, &ids).await?;
    Ok((
        rows.into_iter()
            .map(|r| {
                let run = last
                    .iter()
                    .position(|(p, _)| *p == r.id)
                    .map(|i| last.swap_remove(i).1);
                period_resource(r, ws, run)
            })
            .collect(),
        total,
    ))
}

/// POST /accounting/fiscal-years: at most 24 months, no overlap.
pub async fn create_year(
    state: &AppState,
    access: &WorkspaceAccess,
    cmd: Command<Create<YearPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "accounting_years_create";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    gate::exclusive(&mut tx, ws).await?;
    let Range { start, end } = cmd.input.payload;
    if start
        .checked_add_months(chrono::Months::new(24))
        .is_none_or(|max| end > max)
    {
        return Err(AppError::field(
            "payload.end_date",
            "A fiscal year is at most 24 months long.",
        ));
    }
    if repo::overlaps(&mut *tx, ws, false, start, end).await? {
        return Err(AppError::field(
            "payload.start_date",
            "The fiscal year overlaps another fiscal year.",
        ));
    }
    let id = repo::insert_year(&mut tx, ws, start, end, access.user_id).await?;
    let row = repo::years(&mut *tx, ws, Some(id), 1, 0)
        .await?
        .pop()
        .ok_or(AppError::NotFound)?;
    let answer = created(&year_resource(row, ws));
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// POST /accounting/periods: inside an open fiscal year, no overlap.
pub async fn create_period(
    state: &AppState,
    access: &WorkspaceAccess,
    cmd: Command<Create<PeriodPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "accounting_periods_create";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    gate::exclusive(&mut tx, ws).await?;
    let NewPeriod {
        year,
        range: Range { start, end },
    } = cmd.input.payload;
    let Some(y) = repo::years(&mut *tx, ws, Some(year), 1, 0).await?.pop() else {
        return Err(AppError::field(
            "payload.fiscal_year_id",
            "The selected fiscal year id is invalid.",
        ));
    };
    if y.state != "open" {
        return Err(period_closed());
    }
    if start < y.start_date || end > y.end_date {
        return Err(AppError::field(
            "payload.start_date",
            "The period must lie inside its fiscal year.",
        ));
    }
    if repo::overlaps(&mut *tx, ws, true, start, end).await? {
        return Err(AppError::field(
            "payload.start_date",
            "The period overlaps another period.",
        ));
    }
    let id = repo::insert_period(&mut tx, ws, year, start, end, access.user_id).await?;
    let row = repo::periods(&mut *tx, ws, Some(id), 1, 0)
        .await?
        .pop()
        .ok_or(AppError::NotFound)?;
    let answer = created(&period_resource(row, ws, None));
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Bank accounts of the book (a handful; capped).
pub async fn bank_accounts(
    state: &AppState,
    ws: Uuid,
) -> Result<Vec<BankAccountResource>, AppError> {
    Ok(sqlx::query_as(
        "SELECT id, name, currency_id, account_id, active, version, created_at
         FROM bank_accounts WHERE workspace_id = $1 ORDER BY name, id LIMIT 500",
    )
    .bind(ws)
    .fetch_all(&state.db)
    .await?)
}

/// POST /accounting/bank-accounts: a bank account on an active asset account of the book.
pub async fn create_bank_account(
    state: &AppState,
    access: &WorkspaceAccess,
    cmd: Command<Create<BankAccountPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "accounting_bank_accounts_create";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let NewBankAccount {
        name,
        currency_id,
        account_id,
    } = &cmd.input.payload;
    let asset = matches!(
        repo::account_lock(&mut tx, ws, *account_id, false).await?,
        Some((t, active, _)) if t == "asset" && active
    );
    if !asset {
        return Err(AppError::field(
            "payload.account_id",
            "The account must be an active asset account.",
        ));
    }
    if repo::currency(&mut *tx, ws, *currency_id).await?.is_none() {
        return Err(AppError::field(
            "payload.currency_id",
            "The selected currency is invalid.",
        ));
    }
    let row: BankAccountResource = sqlx::query_as(
        "INSERT INTO bank_accounts (workspace_id, name, currency_id, account_id, created_by)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, name, currency_id, account_id, active, version, created_at",
    )
    .bind(ws)
    .bind(name)
    .bind(currency_id)
    .bind(account_id)
    .bind(access.user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        AppError::unique(
            e,
            "bank_accounts_name_key",
            "payload.name",
            "The name has already been taken.",
        )
    })?;
    let answer = created(&row);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[row.id]).await?;
    tx.commit().await?;
    Ok(answer)
}
