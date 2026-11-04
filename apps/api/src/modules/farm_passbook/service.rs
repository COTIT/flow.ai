//! Passbook and consent rules: a farmer reads and sets their own; staff of the person's
//! village read and set anyone's there. Staff elsewhere get 404, other farmers 403.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{farm::FarmAccess, workspace::NOT_ALLOWED},
    modules::{
        farm_passbook::repo::{self, ConsentRow, EntryRow},
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

async fn passbook(
    state: &AppState,
    access: &FarmAccess,
    id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Passbook, AppError> {
    let ws = access.workspace_id;
    let (rows, total) = repo::entries(&state.db, ws, id, limit, offset).await?;
    Ok((rows, total, repo::balances(&state.db, ws, id).await?))
}

/// One page of the caller's own passbook; 404 when the account is no registered person.
pub async fn mine(
    state: &AppState,
    access: &FarmAccess,
    limit: i64,
    offset: i64,
) -> Result<Passbook, AppError> {
    let id = access.person_id.ok_or(AppError::NotFound)?;
    passbook(state, access, id, limit, offset).await
}

/// One page of a person's passbook.
pub async fn of_person(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
    limit: i64,
    offset: i64,
) -> Result<Passbook, AppError> {
    let id = person(state, access, id).await?;
    passbook(state, access, id, limit, offset).await
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
