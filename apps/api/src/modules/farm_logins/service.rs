//! Farmer login rules (RUAHA `app_farmer_login_issue`, `app_password_changed`, sign-in by phone).
//! One login per person and per phone; a phone that signs someone else in (any workspace) is
//! refused, so an officer can never take over another account. A reset ends every session.

use crate::{
    AppState,
    error::AppError,
    http::{
        auth::AuthUser,
        farm::{FARMER, FarmAccess},
        workspace::NOT_ALLOWED,
    },
    modules::{
        auth::{
            password::{self, Password},
            repo as tokens,
            service::{MAX_LOGIN_FAILURES, issue_token},
        },
        farm_logins::{
            dto::{
                FarmLogin, FarmMe, FarmSession, IssuedLogin, IssuerRef, LoginIssue, MeUser,
                PasswordChange,
            },
            repo, rules,
        },
        farm_memberships::repo as memberships,
        farm_records::{phone::normalize_phone, service::path_id},
    },
};

const PHONE_TAKEN: &str = "this phone number already has an app login";
/// Crockford base32, lower case: no i, l, o, u to misread aloud.
const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// 8 random Crockford characters (40 bits; online guessing is capped by the lockout).
fn temp_password() -> Result<String, AppError> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).map_err(|e| AppError::Internal(format!("os rng: {e}")))?;
    // 256 is a multiple of 32: no bias.
    Ok(bytes
        .iter()
        .map(|b| char::from(ALPHABET[usize::from(b % 32)]))
        .collect())
}

/// Issue (or reset) the farmer login of a registered person.
pub async fn issue(
    state: &AppState,
    access: &FarmAccess,
    person: &str,
) -> Result<IssuedLogin, AppError> {
    if !access.is_staff() {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let person = path_id(person)?;
    let scope = access.staff_scope();
    let ws = access.workspace_id;
    let mut tx = access.begin(&state.db).await?;
    let p = repo::lock_person(&mut *tx, ws, scope.all, &scope.villages, person)
        .await?
        .ok_or(AppError::NotFound)?;
    let phone = p.phone.as_deref().ok_or_else(|| {
        AppError::Unprocessable("add a phone number before issuing a login".into())
    })?;
    if let Some(user) = p.user_id
        && repo::is_staff_account(&mut *tx, user).await?
    {
        return Err(AppError::Unprocessable(
            "this person signs in as staff: their password cannot be reset here".into(),
        ));
    }
    if let Some(owner) = repo::user_by_phone(&mut *tx, phone).await?
        && Some(owner) != p.user_id
    {
        return Err(AppError::Unprocessable(PHONE_TAKEN.into()));
    }

    let plain = temp_password()?;
    let hash = password::hash(Password::generated(plain.clone())).await?;
    let taken = |e: sqlx::Error| match e.as_database_error().and_then(|d| d.constraint()) {
        Some("users_phone_unique") => AppError::Unprocessable(PHONE_TAKEN.into()),
        _ => AppError::Database(e),
    };
    let (user, kind) = match p.user_id {
        None => {
            let user = repo::insert_user(&mut *tx, phone, &p.given_name, &p.family_name, &hash)
                .await
                .map_err(taken)?;
            repo::link_person(&mut *tx, p.id, user).await?;
            (user, "initial")
        }
        Some(user) => {
            repo::reset_user(&mut *tx, user, phone, &hash)
                .await
                .map_err(taken)?;
            tokens::delete_all_for_user(&mut *tx, user).await?;
            (user, "reset")
        }
    };
    // A farmer role in the person's village; restored if it was revoked.
    memberships::grant(
        &mut *tx,
        ws,
        user,
        FARMER,
        Some(p.village_id),
        access.user_id,
    )
    .await?;
    tokens::clear_login_attempts(&mut *tx, phone).await?;
    repo::insert_issue(&mut *tx, ws, user, p.id, access.user_id, kind, &hash).await?;
    tx.commit().await?;
    Ok(IssuedLogin {
        phone: phone.to_owned(),
        temp_password: plain,
        kind: kind.to_owned(),
    })
}

/// A person's login history, for staff of their village.
pub async fn history(
    state: &AppState,
    access: &FarmAccess,
    person: &str,
) -> Result<Vec<LoginIssue>, AppError> {
    if !access.is_staff() {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let person = path_id(person)?;
    let scope = access.staff_scope();
    let ws = access.workspace_id;
    let rows = repo::history(&state.db, ws, scope.all, &scope.villages, person).await?;
    if rows.is_empty()
        && repo::lock_person(&state.db, ws, scope.all, &scope.villages, person)
            .await?
            .is_none()
    {
        return Err(AppError::NotFound);
    }
    Ok(rows
        .into_iter()
        .map(|r| LoginIssue {
            issued_at: r.issued_at,
            kind: r.kind,
            issued_by: IssuerRef {
                id: r.issued_by,
                name: r.issued_by_name,
            },
            must_change_password: r.must_change_password,
        })
        .collect())
}

/// Phone + password -> session. Unknown phone, junk and a wrong password answer alike, in body
/// and timing; ten failures in 15 minutes lock the phone until an officer issues a new password.
pub async fn login(state: &AppState, input: FarmLogin) -> Result<FarmSession, AppError> {
    let phone = normalize_phone(&input.phone).ok();
    // Junk is counted under its own text, so it locks like any unknown phone.
    let key = phone
        .clone()
        .unwrap_or_else(|| input.phone.trim().to_owned());
    tokens::record_login_attempt(&state.db, &key).await?;
    if tokens::recent_login_attempts(&state.db, &key).await? > MAX_LOGIN_FAILURES {
        let secs = tokens::lock_seconds_left(&state.db, &key, MAX_LOGIN_FAILURES).await?;
        return Err(AppError::Locked(secs));
    }
    let found = match &phone {
        // Staff sign in by email; a phone here is a farmer account.
        Some(p) => repo::credentials_by_phone(&state.db, p)
            .await?
            .filter(|c| !c.has_email),
        None => None,
    };
    let matches = match &found {
        Some(c) => password::verify(input.password, c.password_hash.clone()).await?,
        None => password::verify_dummy(input.password).await?,
    };
    let Some(user) = found.filter(|_| matches) else {
        return Err(AppError::InvalidCredentials);
    };
    tokens::clear_login_attempts(&state.db, &key).await?;
    let token = issue_token(&state.db, user.id, state.token_ttl).await?;
    Ok(FarmSession {
        access_token: token.expose().to_owned(),
        token_type: "Bearer",
        must_change_password: user.must_change_password,
        memberships: repo::memberships(&state.db, user.id).await?,
    })
}

/// The farmer replaces the password they signed in with.
pub async fn change_password(
    state: &AppState,
    user: AuthUser,
    input: PasswordChange,
) -> Result<(), AppError> {
    let account = repo::credentials_by_id(&state.db, user.user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    // Staff passwords follow the staff rule, not this one.
    if account.has_email {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    if !password::verify(input.current, account.password_hash.clone()).await? {
        return Err(AppError::field(
            "current_password",
            "The current password is incorrect.",
        ));
    }
    if let Some(temp) = repo::latest_temp_hash(&state.db, account.id).await?
        && password::verify(Password::generated(input.new.expose().to_owned()), temp).await?
    {
        return Err(AppError::field("password", "choose a new password first"));
    }
    if let Some(problem) = rules::problem(input.new.expose(), account.phone.as_deref()) {
        return Err(AppError::field("password", problem));
    }
    let hash = password::hash(input.new).await?;
    repo::set_password(&state.db, account.id, &hash).await?;
    Ok(())
}

/// The account behind any token, phone or email, with its farm roles.
pub async fn me(state: &AppState, user: AuthUser) -> Result<FarmMe, AppError> {
    let row = repo::me(&state.db, user.user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    Ok(FarmMe {
        must_change_password: row.must_change_password,
        memberships: repo::memberships(&state.db, row.id).await?,
        locale: Some(row.locale),
        user: MeUser {
            id: row.id,
            name: format!("{} {}", row.first_name, row.last_name)
                .trim()
                .to_owned(),
            email: row.email,
            phone: row.phone,
        },
    })
}

/// Save the account's language.
pub async fn set_locale(state: &AppState, user: AuthUser, locale: String) -> Result<(), AppError> {
    Ok(repo::set_locale(&state.db, user.user_id, &locale).await?)
}
