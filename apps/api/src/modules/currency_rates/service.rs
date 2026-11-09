//! Rate rules: both currencies live in this workspace, no future dates, history only grows.
//! Convert uses the latest rate on or before the day, direct first, else the inverse of the
//! reverse pair; never a third currency.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::json::Rules,
    modules::currency_rates::{
        convert::convert as apply,
        dto::{Conversion, Convert, Direction, NewRate, invalid},
        repo::{self, RateRow, Side},
    },
};

/// Both currencies of a pair, live in `workspace_id`, else 422 per unknown side.
pub async fn pair(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    from: Uuid,
    to: Uuid,
) -> Result<(Side, Side), AppError> {
    let found = repo::sides(&mut *conn, workspace_id, &[from, to]).await?;
    let side = |id: Uuid| {
        found.iter().find(|s| s.id == id).map(|s| Side {
            id: s.id,
            code: s.code.clone(),
            decimals: s.decimals,
        })
    };
    let (f, t) = (side(from), side(to));
    let mut r = Rules::default();
    if f.is_none() {
        invalid(&mut r, "from_currency_id");
    }
    if t.is_none() {
        invalid(&mut r, "to_currency_id");
    }
    r.into_result()?;
    f.zip(t).ok_or(AppError::NotFound)
}

/// The native create rules (live pair, no future day), shared with the review card.
pub async fn checked(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    input: &NewRate,
) -> Result<(Side, Side), AppError> {
    let sides = pair(&mut *conn, workspace_id, input.from, input.to).await?;
    if input.effective_on > repo::today(&mut *conn, workspace_id).await? {
        return Err(AppError::field(
            "effective_on",
            "The effective on must not be in the future.",
        ));
    }
    Ok(sides)
}

/// Store a rate; returns its id.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    input: NewRate,
) -> Result<Uuid, AppError> {
    let mut conn = state.db.acquire().await?;
    checked(&mut conn, workspace_id, &input).await?;
    Ok(repo::insert(&mut *conn, workspace_id, user_id, &input).await?)
}

/// One history page plus the total.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    from: Option<Uuid>,
    to: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<RateRow>, i64), AppError> {
    let (rows, mut total) = repo::list(&state.db, workspace_id, from, to, limit, offset).await?;
    if rows.is_empty() && offset > 0 {
        total = repo::count(&state.db, workspace_id, from, to).await?;
    }
    Ok((rows, total))
}

/// Convert at the workspace's own rate in force on `on` (default today).
pub async fn conversion(
    state: &AppState,
    workspace_id: Uuid,
    q: Convert,
) -> Result<Conversion, AppError> {
    let mut conn = state.db.acquire().await?;
    let (from, to) = pair(&mut conn, workspace_id, q.from, q.to).await?;
    let on = match q.on {
        Some(d) => d,
        None => repo::today(&mut *conn, workspace_id).await?,
    };
    let (rate, effective_on, direction) = if from.id == to.id {
        (Decimal::ONE, None, Direction::Same)
    } else if let Some(r) = repo::in_force(&mut *conn, workspace_id, from.id, to.id, on).await? {
        (r.rate, Some(r.effective_on), Direction::Direct)
    } else if let Some(r) = repo::in_force(&mut *conn, workspace_id, to.id, from.id, on).await? {
        (r.rate, Some(r.effective_on), Direction::Inverse)
    } else {
        return Err(no_rate(&from, &to, on));
    };
    let decimals = |s: &Side| {
        u32::try_from(s.decimals)
            .map_err(|_| AppError::Internal("negative currency decimals".into()))
    };
    let converted = apply(
        q.amount,
        rate,
        decimals(&from)?,
        decimals(&to)?,
        direction == Direction::Inverse,
    )
    .ok_or_else(|| AppError::field("amount_minor", "The amount minor is too large to convert."))?;
    Ok(Conversion {
        amount_minor: q.amount,
        from: (&from).into(),
        to: (&to).into(),
        converted_minor: converted,
        rate: rate.normalize().to_string(),
        rate_effective_on: effective_on,
        direction,
        rounding: "half_up",
        source: "workspace",
    })
}

fn no_rate(from: &Side, to: &Side, on: NaiveDate) -> AppError {
    AppError::field(
        "to_currency_id",
        format!(
            "No exchange rate from {} to {} on or before {on}. An admin can set one.",
            from.code, to.code
        ),
    )
}
