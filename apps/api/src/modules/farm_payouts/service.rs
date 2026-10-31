//! Payout desk rules. Staff of the farmer's village pay; whoever registered or verified the
//! household, or belongs to it, may not (voucher four eyes); above the workspace's officer
//! limit only ops or admin pay. The farmer's row is locked, then the balance read: a payout
//! never passes it, and never comes out of a negative one. Voids (ops, admin) and cash moves
//! are new rows. A client_ref replays.

use chrono::{FixedOffset, NaiveDate, Offset, Utc};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{farm::FarmAccess, workspace::NOT_ALLOWED},
    modules::{
        currencies::repo as currencies,
        farm_payouts::{
            dto::{CashInput, CashMovementResource, NewPayoutInput},
            repo::{self, DayRow, Movement, NewPayout, PayoutRow},
        },
        farm_vouchers::rules::{Conflict, household_conflict},
        settings::service as settings,
    },
};

const REF_TAKEN: &str = "This client_ref belongs to another payout.";
const ALREADY_VOID: &str = "This payout is already void.";
const OVER_LIMIT: &str = "this payout is above the officer limit: ops or admin must pay it";
const REGISTERED: &str =
    "you registered this household, so another staff member must pay this farmer";
const VERIFIED: &str = "you verified this household, so another staff member must pay this farmer";
const OWN: &str = "you cannot pay your own household";

/// Today in Tanzania (UTC+3 all year), the cash ledger's day.
fn today() -> NaiveDate {
    let eat = FixedOffset::east_opt(3 * 3600).unwrap_or_else(|| Utc.fix());
    Utc::now().with_timezone(&eat).date_naive()
}

/// The stored payout for a replay, or 409 when another member or body used the key.
fn replay(p: PayoutRow, access: &FarmAccess, hash: &[u8; 32]) -> Result<PayoutRow, AppError> {
    if p.paid_by == access.user_id && p.request_hash == hash {
        Ok(p)
    } else {
        Err(AppError::Conflict(REF_TAKEN))
    }
}

/// POST /payouts. True = paid now, false = replayed.
pub async fn pay(
    state: &AppState,
    access: &FarmAccess,
    input: NewPayoutInput,
) -> Result<(PayoutRow, bool), AppError> {
    if !access.is_staff() {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let ws = access.workspace_id;
    let hash = input.hash();
    if let Some(p) = repo::find(&state.db, ws, None, Some(input.client_ref)).await? {
        return replay(p, access, &hash).map(|p| (p, false));
    }
    let mut tx = access.begin(&state.db).await?;
    let village = repo::lock_person(&mut tx, ws, input.person_id)
        .await?
        .filter(|v| access.staff_in(*v))
        .ok_or(AppError::NotFound)?;
    let currency = currencies::find(&mut *tx, ws, input.currency_id).await?;
    if !currency.is_some_and(|c| c.active) {
        return Err(AppError::field(
            "currency_id",
            "The selected currency id is invalid.",
        ));
    }
    // A copy that held the lock first has committed: answer with it.
    if let Some(p) = repo::find(&mut *tx, ws, None, Some(input.client_ref)).await? {
        return replay(p, access, &hash).map(|p| (p, false));
    }
    let facts = repo::household_facts(
        &mut *tx,
        ws,
        input.person_id,
        access.user_id,
        access.person_id,
    )
    .await?;
    let me_registered = facts.registered.then_some(access.user_id);
    let me_verified = facts.verified.then_some(access.user_id);
    match household_conflict(me_registered, me_verified, access.user_id, facts.own) {
        Some(Conflict::Registered) => return Err(AppError::Forbidden(REGISTERED)),
        Some(Conflict::Verified) => return Err(AppError::Forbidden(VERIFIED)),
        Some(Conflict::OwnHousehold) => return Err(AppError::Forbidden(OWN)),
        None => {}
    }
    if !access.manages() {
        let farm = settings::section_or_default(&mut *tx, ws, "farm").await?;
        let limit = farm["payout_officer_limit"].as_i64().unwrap_or(0);
        if input.amount > limit {
            return Err(AppError::Forbidden(OVER_LIMIT));
        }
    }
    let balance = repo::balance(&mut tx, ws, input.person_id, input.currency_id).await?;
    if input.amount > balance {
        return Err(AppError::field(
            "amount",
            format!("The amount must not be greater than the balance of {balance}."),
        ));
    }
    let new = NewPayout {
        village_id: village,
        person_id: input.person_id,
        currency_id: input.currency_id,
        amount: input.amount,
        id_type: &input.id_type,
        client_ref: input.client_ref,
        request_hash: &hash,
        paid_by: access.user_id,
    };
    let id = repo::insert(&mut tx, ws, &new).await.map_err(|e| {
        match e.as_database_error().and_then(|d| d.constraint()) {
            Some("farm_payouts_client_ref_key") => AppError::Conflict(REF_TAKEN),
            _ => e.into(),
        }
    })?;
    let p = repo::find(&mut *tx, ws, Some(id), None)
        .await?
        .ok_or_else(|| AppError::Internal("payout gone".into()))?;
    repo::passbook_line(&mut tx, ws, &p, "payout", -p.amount, access.user_id).await?;
    let cash = Movement {
        village_id: p.village_id,
        staff_user_id: access.user_id,
        currency_id: p.currency_id,
        day: today(),
        kind: "payout",
        amount: -p.amount,
        payout_id: Some(p.id),
        created_by: access.user_id,
    };
    repo::move_cash(&mut *tx, ws, &cash).await?;
    tx.commit().await?;
    Ok((p, true))
}

/// POST /payouts/{payout}/void: ops or admin; reversing passbook and cash rows, never edits.
pub async fn void(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
    reason: String,
) -> Result<PayoutRow, AppError> {
    if !access.manages() {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let ws = access.workspace_id;
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    let p = repo::find(&state.db, ws, Some(id), None)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut tx = access.begin(&state.db).await?;
    if !repo::insert_void(&mut tx, ws, p.id, &reason, access.user_id).await? {
        return Err(AppError::Conflict(ALREADY_VOID));
    }
    repo::passbook_line(&mut tx, ws, &p, "payout_void", p.amount, access.user_id).await?;
    // The cash went back to whoever paid it out.
    let cash = Movement {
        village_id: p.village_id,
        staff_user_id: p.paid_by,
        currency_id: p.currency_id,
        day: today(),
        kind: "payout_void",
        amount: p.amount,
        payout_id: Some(p.id),
        created_by: access.user_id,
    };
    repo::move_cash(&mut *tx, ws, &cash).await?;
    let p = repo::find(&mut *tx, ws, Some(p.id), None)
        .await?
        .ok_or_else(|| AppError::Internal("payout gone".into()))?;
    tx.commit().await?;
    Ok(p)
}

/// POST /cash/movements: ops or admin hand floats out and take them back; an officer counts
/// their own cash (ops and admin may record any count).
pub async fn move_cash(
    state: &AppState,
    access: &FarmAccess,
    input: CashInput,
) -> Result<CashMovementResource, AppError> {
    let own_count = input.kind == "count" && input.staff_user_id == access.user_id;
    if !access.manages() && !(own_count && access.staff_in(input.village_id)) {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let ws = access.workspace_id;
    if !repo::is_staff_of(&state.db, ws, input.staff_user_id, input.village_id).await? {
        return Err(AppError::field(
            "staff_user_id",
            "The selected staff user id is not staff of this village.",
        ));
    }
    let currency = currencies::find(&state.db, ws, input.currency_id).await?;
    if !currency.is_some_and(|c| c.active) {
        return Err(AppError::field(
            "currency_id",
            "The selected currency id is invalid.",
        ));
    }
    let (kind, amount) = match input.kind.as_str() {
        "float_out" => ("float_out", input.amount),
        "float_back" => ("float_back", -input.amount),
        _ => ("count", input.amount),
    };
    let m = Movement {
        village_id: input.village_id,
        staff_user_id: input.staff_user_id,
        currency_id: input.currency_id,
        day: today(),
        kind,
        amount,
        payout_id: None,
        created_by: access.user_id,
    };
    let id = repo::move_cash(&state.db, ws, &m).await?;
    Ok(CashMovementResource {
        id,
        kind: kind.to_owned(),
        staff_user_id: m.staff_user_id,
        village_id: m.village_id,
        currency_id: m.currency_id,
        amount,
        day: m.day,
    })
}

/// GET /cash/reconciliation: every officer's day for ops and admin, an officer's own else.
pub async fn reconciliation(
    state: &AppState,
    access: &FarmAccess,
    day: Option<String>,
    village_id: Option<String>,
) -> Result<Vec<DayRow>, AppError> {
    if !access.is_staff() {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let day = day
        .as_deref()
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .ok_or_else(|| AppError::field("day", "The day must be a date (YYYY-MM-DD)."))?;
    let village =
        match village_id.as_deref().filter(|v| !v.is_empty()) {
            Some(v) => Some(Uuid::parse_str(v).map_err(|_| {
                AppError::field("village_id", "The village id must be a valid UUID.")
            })?),
            None => None,
        };
    let staff = (!access.manages()).then_some(access.user_id);
    Ok(repo::day(&state.db, access.workspace_id, day, village, staff).await?)
}
