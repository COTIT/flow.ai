//! Passbook and consent rules: a farmer reads and sets their own; staff of the person's
//! village read and set anyone's there. Staff elsewhere get 404, other farmers 403.

use chrono::NaiveDate;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{farm::FarmAccess, workspace::NOT_ALLOWED},
    modules::{
        farm_passbook::repo::{self, ConsentRow, Days, EntryRow, OwedRow},
        farm_surveys::dto::Money,
    },
};

/// The person if the caller may act on them: themself, or staff of their village.
async fn person(state: &AppState, access: &FarmAccess, id: &str) -> Result<Uuid, AppError> {
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    let village = repo::person_village(&state.db, access.workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if access.person_id == Some(id) || access.staff_in(village) {
        Ok(id)
    } else if access.is_staff() {
        Err(AppError::NotFound)
    } else {
        Err(AppError::Forbidden(NOT_ALLOWED))
    }
}

/// A page of a passbook, its total and the balance per currency.
pub type Passbook = (Vec<EntryRow>, i64, Vec<Money>);

/// `from` / `to` as inclusive Tanzania days; blank means no bound. 422 on a bad date or
/// `from` after `to`.
pub fn days(from: Option<&str>, to: Option<&str>) -> Result<Days, AppError> {
    let parse = |field: &str, v: Option<&str>| match v.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(v) => NaiveDate::parse_from_str(v, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| {
                AppError::field(field, format!("The {field} must be a date (YYYY-MM-DD)."))
            }),
    };
    let (from, to) = (parse("from", from)?, parse("to", to)?);
    if let (Some(f), Some(t)) = (from, to)
        && f > t
    {
        return Err(AppError::field(
            "to",
            "The to date must be on or after the from date.",
        ));
    }
    Ok((from, to))
}

async fn passbook(
    state: &AppState,
    access: &FarmAccess,
    id: Uuid,
    days: Days,
    limit: i64,
    offset: i64,
) -> Result<Passbook, AppError> {
    let ws = access.workspace_id;
    let (rows, total) = repo::entries(&state.db, ws, id, days, limit, offset).await?;
    Ok((rows, total, repo::balances(&state.db, ws, id).await?))
}

/// One page of the caller's own passbook; 404 when the account is no registered person.
pub async fn mine(
    state: &AppState,
    access: &FarmAccess,
    days: Days,
    limit: i64,
    offset: i64,
) -> Result<Passbook, AppError> {
    let id = access.person_id.ok_or(AppError::NotFound)?;
    passbook(state, access, id, days, limit, offset).await
}

/// One page of a person's passbook.
pub async fn of_person(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
    days: Days,
    limit: i64,
    offset: i64,
) -> Result<Passbook, AppError> {
    let id = person(state, access, id).await?;
    passbook(state, access, id, days, limit, offset).await
}

/// A person's passport consent: themself or staff of their village.
pub async fn consent_of(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
) -> Result<ConsentRow, AppError> {
    let id = person(state, access, id).await?;
    Ok(repo::consent(&state.db, access.workspace_id, id).await?)
}

/// Who is owed money in the caller's villages, and the total per currency: staff only.
pub async fn owed(
    state: &AppState,
    access: &FarmAccess,
    village: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<OwedRow>, i64, Vec<Money>), AppError> {
    if !access.is_staff() {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let village =
        match village.filter(|v| !v.is_empty()) {
            Some(v) => Some(Uuid::parse_str(v).map_err(|_| {
                AppError::field("village_id", "The village id must be a valid UUID.")
            })?),
            None => None,
        };
    let scope = access.staff_scope();
    let ws = access.workspace_id;
    Ok(repo::owed(
        &state.db,
        ws,
        scope.all,
        &scope.villages,
        village,
        limit,
        offset,
    )
    .await?)
}

/// Record or withdraw a person's passport consent (audited by the farm trail).
pub async fn consent(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
    consent: bool,
) -> Result<ConsentRow, AppError> {
    let id = person(state, access, id).await?;
    let mut tx = access.begin(&state.db).await?;
    let row = repo::set_consent(&mut *tx, access.workspace_id, id, consent, access.user_id).await?;
    tx.commit().await?;
    Ok(row)
}
