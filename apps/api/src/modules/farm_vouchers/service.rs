//! Voucher rules (RUAHA `app_voucher_lookup`, `app_voucher_redeem`, `app_voucher_void`,
//! `app_voucher_timeline`, the redemption log). Staff of the voucher's village only; the code
//! is looked up, never listed to staff; every scan leaves an event, a scan outside the
//! officer's villages a `refused` one.

use crate::{
    AppState,
    error::AppError,
    http::{farm::FarmAccess, workspace::NOT_ALLOWED},
    modules::{
        farm_records::service::path_id,
        farm_surveys::{dto::Money, repo::insert_event},
        farm_vouchers::{
            dto::{LookupResult, Redeem, Redeemed, StaffRef},
            repo::{self, LogRow, MineRow, TotalRow, TrailRow, VoucherRow},
            rules::{Redeemer, VoucherFacts, block_reason, normalize},
        },
    },
};
use chrono::{NaiveDate, Utc};
use serde_json::json;

/// Lookups per account per minute before 429.
const LOOKUPS_PER_MINUTE: i64 = 30;
/// ID documents a payer may check (vouchers and payouts).
pub const ID_TYPES: &[&str] = &["nida", "voter", "driving_licence", "village_letter"];

fn require_staff(access: &FarmAccess) -> Result<(), AppError> {
    access
        .is_staff()
        .then_some(())
        .ok_or(AppError::Forbidden(NOT_ALLOWED))
}

fn money(v: &VoucherRow) -> Money {
    Money {
        minor: v.amount_minor,
        currency: v.currency.clone(),
        decimals: v.decimals,
        currency_id: v.currency_id,
    }
}

fn facts(v: &VoucherRow) -> VoucherFacts<'_> {
    VoucherFacts {
        status: &v.status,
        redeemed_at: v.redeemed_at,
        redeemer_name: v.redeemer_name.as_deref(),
        void_reason: v.void_reason.as_deref(),
        expires_at: v.expires_at,
        audit_required: v.audit_required,
        household_captured_by: v.household_captured_by,
        household_verified_by: v.household_verified_by,
    }
}

async fn redeemer(
    db: impl sqlx::PgExecutor<'_>,
    access: &FarmAccess,
    v: &VoucherRow,
) -> Result<Redeemer, AppError> {
    let in_household = match access.person_id {
        Some(p) => repo::in_household(db, v.household_id, p).await?,
        None => false,
    };
    Ok(Redeemer {
        id: access.user_id,
        in_household,
        manages: access.manages(),
    })
}

/// The household's own vouchers, with codes.
pub async fn mine(
    state: &AppState,
    access: &FarmAccess,
    limit: i64,
    offset: i64,
) -> Result<(Vec<MineRow>, i64), AppError> {
    match access.person_id {
        Some(p) => Ok(repo::mine(&state.db, access.workspace_id, p, limit, offset).await?),
        None => Ok((Vec::new(), 0)),
    }
}

/// Scan a code: what it is and whether this officer may redeem it. Always logged.
pub async fn lookup(
    state: &AppState,
    access: &FarmAccess,
    code: &str,
) -> Result<LookupResult, AppError> {
    require_staff(access)?;
    if repo::count_lookup(&state.db, access.user_id).await? > LOOKUPS_PER_MINUTE {
        return Err(AppError::TooManyRequests);
    }
    let ws = access.workspace_id;
    let Some(v) = repo::by_code(&state.db, ws, &normalize(code)).await? else {
        return Ok(LookupResult::default());
    };
    if !access.staff_in(v.village_id) {
        // A real code outside this officer's villages: the same answer as an unknown one, but
        // ops sees the attempt.
        let detail = json!({ "reason": "outside your villages" });
        insert_event(
            &state.db,
            ws,
            v.id,
            "refused",
            access.user_id,
            access.role_label(),
            &detail,
        )
        .await?;
        return Ok(LookupResult::default());
    }
    let block = block_reason(
        &facts(&v),
        &redeemer(&state.db, access, &v).await?,
        Utc::now(),
    );
    let detail = json!({ "status_seen": v.status, "can_redeem": block.is_none(),
                         "blocked_reason": block, "needs_ops": v.audit_required });
    insert_event(
        &state.db,
        ws,
        v.id,
        "scanned",
        access.user_id,
        access.role_label(),
        &detail,
    )
    .await?;
    Ok(LookupResult {
        found: true,
        voucher_id: Some(v.id),
        expired: Some(v.status == "issued" && v.expires_at <= Utc::now()),
        amount: Some(money(&v)),
        status: Some(v.status),
        issued_at: Some(v.issued_at),
        expires_at: Some(v.expires_at),
        redeemed_at: v.redeemed_at,
        survey_title_en: Some(v.survey_title_en),
        survey_title_sw: v.survey_title_sw,
        household_label: Some(v.household_label),
        head_name: v.head_name,
        respondent_name: Some(v.respondent_name),
        needs_ops: Some(v.audit_required),
        can_redeem: Some(block.is_none()),
        blocked_reason: block,
    })
}

/// Pay out a voucher face to face. The row lock makes two officers scanning at once pay once;
/// the same officer retrying gets the first redemption back.
pub async fn redeem(
    state: &AppState,
    access: &FarmAccess,
    input: Redeem,
) -> Result<Redeemed, AppError> {
    require_staff(access)?;
    let ws = access.workspace_id;
    let mut tx = access.begin(&state.db).await?;
    let v = repo::lock_by_code(&mut *tx, ws, &normalize(&input.code))
        .await?
        .filter(|v| access.staff_in(v.village_id))
        .ok_or(AppError::NotFound)?;
    let me = StaffRef {
        id: access.user_id,
        name: v.redeemer_name.clone().unwrap_or_default(),
    };
    if v.status == "redeemed" && v.redeemed_by == Some(access.user_id) {
        return Ok(Redeemed {
            voucher_id: v.id,
            status: v.status.clone(),
            redeemed_at: v.redeemed_at.unwrap_or_else(Utc::now),
            amount: money(&v),
            redeemed_by: me,
            replayed: true,
        });
    }
    if let Some(reason) = block_reason(
        &facts(&v),
        &redeemer(&mut *tx, access, &v).await?,
        Utc::now(),
    ) {
        return Err(AppError::Unprocessable(reason));
    }
    let id_type = input
        .id_type
        .filter(|t| ID_TYPES.contains(&t.as_str()))
        .ok_or_else(|| AppError::Unprocessable("record which ID document you checked".into()))?;
    if !input.name_confirmed {
        return Err(AppError::Unprocessable(
            "confirm that the name on the ID matches the household".into(),
        ));
    }
    let redeemed_at = repo::redeem(&mut *tx, v.id, access.user_id, &id_type).await?;
    let detail = json!({ "id_type_seen": id_type, "audit_required": v.audit_required,
                         "amount_minor": v.amount_minor, "currency": v.currency });
    insert_event(
        &mut *tx,
        ws,
        v.id,
        "redeemed",
        access.user_id,
        access.role_label(),
        &detail,
    )
    .await?;
    tx.commit().await?;
    let name = repo::by_id(&state.db, ws, v.id)
        .await?
        .and_then(|r| r.redeemer_name)
        .unwrap_or_default();
    Ok(Redeemed {
        voucher_id: v.id,
        status: "redeemed".into(),
        redeemed_at,
        amount: money(&v),
        redeemed_by: StaffRef {
            id: access.user_id,
            name,
        },
        replayed: false,
    })
}

/// Void an unredeemed voucher, with a reason: ops and admin.
pub async fn void(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
    reason: String,
) -> Result<(), AppError> {
    let id = path_id(id)?;
    let ws = access.workspace_id;
    let mut tx = access.begin(&state.db).await?;
    let v = repo::lock_by_id(&mut *tx, ws, id)
        .await?
        .filter(|v| access.staff_in(v.village_id))
        .ok_or(AppError::NotFound)?;
    if !access.manages() {
        return Err(AppError::Forbidden("only ops or admin may void a voucher"));
    }
    if v.status != "issued" {
        return Err(AppError::Unprocessable(
            "only an unredeemed voucher can be voided".into(),
        ));
    }
    if reason.is_empty() {
        return Err(AppError::Unprocessable(
            "give a reason for voiding this voucher".into(),
        ));
    }
    repo::void(&mut *tx, v.id, access.user_id, &reason).await?;
    insert_event(
        &mut *tx,
        ws,
        v.id,
        "voided",
        access.user_id,
        access.role_label(),
        &json!({ "reason": reason }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// The voucher's trail for whoever asks: ops/admin everything; field staff of the village
/// without refusals and audit flags; the household its own steps.
pub async fn timeline(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
) -> Result<Vec<TrailRow>, AppError> {
    let id = path_id(id)?;
    let ws = access.workspace_id;
    let v = repo::by_id(&state.db, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let level = if access.manages() {
        "full"
    } else if access.staff_in(v.village_id) {
        "staff"
    } else if match access.person_id {
        Some(p) => repo::in_household(&state.db, v.household_id, p).await?,
        None => false,
    } {
        "farmer"
    } else {
        return Err(AppError::NotFound);
    };
    Ok(repo::trail(&state.db, ws, id, level).await?)
}

fn days(from: Option<String>, to: Option<String>) -> Result<(NaiveDate, NaiveDate), AppError> {
    let parse = |field: &str, v: Option<String>| {
        v.and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
            .ok_or_else(|| {
                AppError::field(field, format!("The {field} must be a date (YYYY-MM-DD)."))
            })
    };
    Ok((parse("from", from)?, parse("to", to)?))
}

/// Every redemption in the days: ops and admin.
pub async fn log(
    state: &AppState,
    access: &FarmAccess,
    from: Option<String>,
    to: Option<String>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<LogRow>, i64), AppError> {
    if !access.manages() {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let (from, to) = days(from, to)?;
    Ok(repo::log(&state.db, access.workspace_id, from, to, limit, offset).await?)
}

/// Redemptions per officer per day: ops and admin.
pub async fn totals(
    state: &AppState,
    access: &FarmAccess,
    from: Option<String>,
    to: Option<String>,
) -> Result<Vec<TotalRow>, AppError> {
    if !access.manages() {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let (from, to) = days(from, to)?;
    Ok(repo::totals(&state.db, access.workspace_id, from, to).await?)
}
